//! mpv, driven through its JSON IPC.
//!
//! In embedded mode mpv draws into a pane using the kitty graphics protocol;
//! hidden tabs switch to a null video output, so coming back is quick.

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use serde_json::{Value, json};

use crate::app::PlayerCommand;
use crate::config::{Language, PlayerConfig, PlayerMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Embedded,
    Window,
    /// A borderless mpv window over the pane (X11).
    Overlay,
}

impl Mode {
    /// The video shows inside the app's pane (drawn there or laid over it).
    pub fn in_pane(self) -> bool {
        self != Self::Window
    }
}

/// Terminal multiplexers relay keys and images with a delay, or drop images.
const MULTIPLEXERS: [&str; 3] = ["TMUX", "ZELLIJ", "HERDR_ENV"];

pub fn in_multiplexer(env: impl Fn(&str) -> Option<String>) -> bool {
    MULTIPLEXERS.iter().any(|var| env(var).is_some())
}

/// `Auto` embeds only in kitty itself, not inside a multiplexer.
pub fn resolve_mode(mode: PlayerMode, env: impl Fn(&str) -> Option<String>) -> Mode {
    match mode {
        PlayerMode::Embedded => Mode::Embedded,
        PlayerMode::Window => Mode::Window,
        // Needs X11 and the terminal's window (kitty and most terminals export WINDOWID).
        PlayerMode::Overlay => {
            let x11 = env("DISPLAY").is_some() && env("XDG_SESSION_TYPE").as_deref() != Some("wayland");
            if x11 && env("WINDOWID").is_some() && !in_multiplexer(&env) { Mode::Overlay } else { Mode::Window }
        }
        PlayerMode::Auto => {
            let kitty = env("KITTY_WINDOW_ID").is_some() || env("TERM").as_deref() == Some("xterm-kitty");
            if kitty && !in_multiplexer(&env) { Mode::Embedded } else { Mode::Window }
        }
    }
}

/// Where the video goes, in the units mpv's kitty output expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    /// 1-based cell column.
    pub left: u16,
    /// 1-based cell row.
    pub top: u16,
    pub cols: u16,
    pub rows: u16,
    pub width_px: u32,
    pub height_px: u32,
    /// Overlay: the area's top-left pixel on the screen.
    pub screen: Option<(i32, i32)>,
}

impl Geometry {
    /// `cell` is the size of one terminal cell in pixels.
    pub fn new(area: Rect, cell: (u16, u16)) -> Self {
        Self {
            left: area.x + 1,
            top: area.y + 1,
            cols: area.width,
            rows: area.height,
            width_px: u32::from(area.width) * u32::from(cell.0),
            height_px: u32::from(area.height) * u32::from(cell.1),
            screen: None,
        }
    }

    /// Places the area on the screen, given where the terminal's cell grid starts.
    pub fn on_screen(self, grid: (i32, i32)) -> Self {
        let cell = |px: u32, n: u16| if n == 0 { 0 } else { px / u32::from(n) };
        let x = grid.0 + (i32::from(self.left) - 1) * cell(self.width_px, self.cols) as i32;
        let y = grid.1 + (i32::from(self.top) - 1) * cell(self.height_px, self.rows) as i32;
        Self { screen: Some((x, y)), ..self }
    }

    /// mpv's `--geometry`: `WxH+X+Y`.
    fn x11(&self) -> Option<String> {
        let (x, y) = self.screen?;
        Some(format!("{}x{}+{x}+{y}", self.width_px, self.height_px))
    }

    fn options(&self) -> [(&'static str, u32); 6] {
        [
            ("left", self.left.into()),
            ("top", self.top.into()),
            ("cols", self.cols.into()),
            ("rows", self.rows.into()),
            ("width", self.width_px),
            ("height", self.height_px),
        ]
    }
}

/// Pixel size of a terminal cell, with a sane fallback when unknown.
pub fn cell_size() -> (u16, u16) {
    match ratatui::crossterm::terminal::window_size() {
        Ok(ws) if ws.columns > 0 && ws.rows > 0 && ws.width > 0 && ws.height > 0 => {
            (ws.width / ws.columns, ws.height / ws.rows)
        }
        _ => (10, 20),
    }
}

/// yt-dlp format selector: preferred codec first, then anything that fits.
/// Dubbed audio prefers the `sub_lang` track and falls back to the original.
pub fn ytdl_format(config: &PlayerConfig, mode: Mode) -> String {
    let fits = format!("[height<={}]", config.max_height);
    // Each choice must be a whole `video+audio`: a bare `ba` between slashes
    // would be a choice of its own, sound without picture.
    let audios = match config.language {
        Language::Dubbed => vec![format!("ba[language^={}]", config.sub_lang), "ba".into()],
        Language::Subtitles | Language::Original => vec!["ba".into()],
    };
    let codecs: Vec<String> = match config.prefer_codec.as_str() {
        "" => vec![String::new()],
        codec => vec![format!("[vcodec^={codec}]"), String::new()],
    };
    // Embedded frames are decoded and scaled on the CPU: 60 fps falls behind the sound.
    let rates: &[&str] = if mode == Mode::Embedded { &["[fps<=30]", ""] } else { &[""] };
    let mut choices = Vec::new();
    for rate in rates {
        for codec in &codecs {
            for audio in &audios {
                choices.push(format!("bv*{fits}{rate}{codec}+{audio}"));
            }
        }
    }
    choices.extend([format!("b{fits}"), "b".into()]);
    choices.join("/")
}

/// yt-dlp format selector for listening only: the sound, in the dubbing
/// language when that is the mode.
pub fn audio_format(config: &PlayerConfig) -> String {
    match config.language {
        Language::Dubbed => format!("ba[language^={}]/ba/b", config.sub_lang),
        Language::Subtitles | Language::Original => "ba/b".into(),
    }
}

fn format_for(config: &PlayerConfig, mode: Mode, audio_only: bool) -> String {
    if audio_only { audio_format(config) } else { ytdl_format(config, mode) }
}

/// The address of a live stream's best quality up to `max_height`, as
/// streamlink finds it; `None` if it fails or takes too long (yt-dlp then plays it).
pub fn streamlink_url(streamlink: &std::path::Path, url: &str, max_height: u32) -> Option<String> {
    let mut child = Command::new(streamlink)
        .args(["--stream-url", &format!("--stream-sorting-excludes=>{max_height}p"), url, "best"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while child.try_wait().ok()?.is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
    parse_stream_url(&out)
}

/// The one address `streamlink --stream-url` prints.
fn parse_stream_url(output: &str) -> Option<String> {
    let line = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    line.starts_with("http").then(|| line.to_owned())
}

/// Everything about one playback except where it is drawn.
pub struct Launch<'a> {
    pub config: &'a PlayerConfig,
    pub mode: Mode,
    pub cookies: Option<&'a std::path::Path>,
    /// URLs or paths, played in order.
    pub queue: &'a [String],
    /// Music: no video output at all.
    pub audio_only: bool,
    /// Second to start at.
    pub start: Option<u32>,
}

pub fn mpv_args(launch: &Launch, geometry: Option<Geometry>, ipc: &str) -> Vec<String> {
    let Launch { config, mode, cookies, queue, audio_only, start } = *launch;
    let lang = &config.sub_lang;
    let raw = raw_options(config, cookies);
    let mut args = vec![
        format!("--input-ipc-server={ipc}"),
        format!("--ytdl-format={}", format_for(config, mode, audio_only)),
        format!("--ytdl-raw-options={raw}"),
        // Subtitles are always fetched so `j` can turn them on later.
        format!("--slang={lang}"),
        "--sub-auto=fuzzy".into(),
        format!("--sid={}", if config.language == Language::Subtitles { "auto" } else { "no" }),
        "--really-quiet".into(),
        "--input-terminal=no".into(),
    ];
    if let Some(start) = start {
        args.push(format!("--start={start}"));
    }
    // Embedded frames are scaled on the CPU. Copying hardware-decoded frames
    // back is slower than decoding them in software: measured on Intel, the
    // picture fell behind the sound (1.7 s after 20 s); in software it stays in sync.
    args.push(match (config.hwdec, mode) {
        (true, Mode::Window | Mode::Overlay) => "--hwdec=auto".into(),
        _ => "--hwdec=no".into(),
    });
    match mode {
        _ if audio_only => args.push("--vid=no".into()),
        Mode::Window => args.push("--force-window=immediate".into()),
        Mode::Overlay => {
            // Clicks go through to the terminal, and so does the keyboard
            // (window managers that ignore `focus-on` get it back, see `give_back_focus`).
            args.extend(
                [
                    "--force-window=immediate",
                    "--no-border",
                    "--ontop",
                    "--focus-on=never",
                    "--input-cursor-passthrough=yes",
                    "--cursor-autohide=always",
                    "--osc=no",
                    "--keepaspect-window=no",
                ]
                .map(String::from),
            );
            args.push(format!("--x11-name={OVERLAY_NAME}"));
            if let Some(geometry) = geometry.and_then(|g| g.x11()) {
                args.push(format!("--geometry={geometry}"));
            }
        }
        Mode::Embedded => {
            args.extend(
                ["--vo=kitty", "--vo-kitty-alt-screen=no", "--vo-kitty-config-clear=no", "--sws-scaler=fast-bilinear"]
                    .map(String::from),
            );
            // When the CPU falls behind, skip decoding frames too: dropping them
            // only at the output still lets the picture drift from the sound.
            args.extend(["--framedrop=decoder+vo", "--vd-lavc-skiploopfilter=all", "--vd-lavc-fast"].map(String::from));
            if cfg!(unix) {
                args.push("--vo-kitty-use-shm=yes".into());
            }
            if let Some(g) = geometry {
                args.extend(g.options().iter().map(|(name, value)| format!("--vo-kitty-{name}={value}")));
            }
        }
    }
    args.push("--".into());
    args.extend(queue.iter().cloned());
    args
}

/// X11 instance name of overlay windows, to tell them apart.
const OVERLAY_NAME: &str = "tubetui-overlay";

/// Where the terminal's cell grid starts on the screen, from its X11 window
/// (`$WINDOWID`): the grid is centered in the window, so the left margin
/// (padding and the leftover pixels) is taken for the top one too.
pub fn grid_origin(cell: (u16, u16)) -> Option<(i32, i32)> {
    let window = std::env::var("WINDOWID").ok()?;
    let out = Command::new("xwininfo").args(["-id", &window]).stderr(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let field = |name: &str| -> Option<i32> {
        let line = text.lines().find(|l| l.trim_start().starts_with(name))?;
        line.split(':').nth(1)?.trim().parse().ok()
    };
    let (x, y, width) = (field("Absolute upper-left X")?, field("Absolute upper-left Y")?, field("Width")?);
    let columns = ratatui::crossterm::terminal::size().ok()?.0;
    Some(grid_from_window((x, y, width), columns, cell.0))
}

fn grid_from_window((x, y, width): (i32, i32, i32), columns: u16, cell_width: u16) -> (i32, i32) {
    let margin = (width - i32::from(columns) * i32::from(cell_width)).max(0) / 2;
    (x + margin, y + margin)
}

/// Window managers that ignore mpv's `focus-on=never` focus every new or
/// restored overlay: give the keyboard back to the terminal as soon as that
/// happens (for up to `wait`).
fn give_back_focus(wait: Duration) {
    let Ok(terminal) = std::env::var("WINDOWID") else { return };
    std::thread::spawn(move || {
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            if let Some(overlay) = focused_overlay() {
                let _ = Command::new("xdotool").args(["windowactivate", &terminal]).stderr(Stdio::null()).status();
                // It is part of the app: keep it out of the taskbar (best effort).
                let _ = Command::new("wmctrl")
                    .args(["-i", "-r", &overlay, "-b", "add,skip_taskbar,skip_pager"])
                    .stderr(Stdio::null())
                    .status();
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
}

/// The focused window is one of our overlays.
pub fn overlay_has_focus() -> bool {
    focused_overlay().is_some()
}

/// The X11 id of the focused window, if it is one of our overlays.
fn focused_overlay() -> Option<String> {
    let run = |args: &[&str]| {
        let out = Command::new(args[0]).args(&args[1..]).stderr(Stdio::null()).output().ok()?;
        Some(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    };
    let id = run(&["xdotool", "getwindowfocus"])?;
    run(&["xprop", "-id", &id, "WM_CLASS"]).filter(|class| class.contains(OVERLAY_NAME)).map(|_| id)
}

/// Deletes every image drawn with the kitty graphics protocol. mpv stops
/// drawing when its video is hidden, but its last frame stays on screen.
pub fn clear_images() {
    let mut out = std::io::stdout();
    // A killed mpv may leave a graphics command unterminated, which would
    // swallow ours: close any open one first.
    let _ = out.write_all(b"\x1b\\\x1b_Ga=d,d=a\x1b\\");
    let _ = out.flush();
}

/// Puts the terminal window itself in fullscreen, where the platform allows
/// it (X11 with `wmctrl`; kitty and most terminals export `WINDOWID`).
pub fn set_window_fullscreen(on: bool) {
    // Inside a multiplexer the window holds more than this app.
    if in_multiplexer(|var| std::env::var(var).ok()) {
        return;
    }
    let Ok(window) = std::env::var("WINDOWID") else { return };
    let action = if on { "add,fullscreen" } else { "remove,fullscreen" };
    let _ = Command::new("wmctrl")
        .args(["-i", "-r", &window, "-b", action])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Reopens `path` at `seconds`, so new format/subtitle options take effect.
pub fn reload_line(path: &str, seconds: f64) -> String {
    json!({ "command": ["loadfile", path, "replace", -1, format!("start={seconds:.1}")] }).to_string()
}

/// yt-dlp options passed through mpv: subtitles to fetch and the login.
fn raw_options(config: &PlayerConfig, cookies: Option<&std::path::Path>) -> String {
    let mut raw = format!("sub-langs=[{}],write-subs=,write-auto-subs=", config.sub_lang);
    if let Some(cookies) = cookies {
        raw.push_str(&format!(",cookies={}", cookies.display()));
    }
    raw
}

/// IPC lines for a command. Most are JSON; reloading uses mpv's text
/// command form, the only one that expands `${time-pos}`.
/// `config` must already reflect `Language` / `SubLang` / `Quality` / `Codec` changes (see [`Player::send`]).
pub fn command_lines(
    config: &PlayerConfig,
    mode: Mode,
    audio_only: bool,
    cookies: Option<&std::path::Path>,
    command: PlayerCommand,
) -> Vec<String> {
    let line = |value: Value| vec![value.to_string()];
    match command {
        PlayerCommand::TogglePause => line(json!({ "command": ["cycle", "pause"] })),
        PlayerCommand::Seek(seconds) => line(json!({ "command": ["seek", seconds, "relative"] })),
        PlayerCommand::SeekTo(second) => line(json!({ "command": ["seek", second, "absolute"] })),
        PlayerCommand::Chapter(delta) => line(json!({ "command": ["add", "chapter", delta] })),
        PlayerCommand::Volume(delta) => line(json!({ "command": ["add", "volume", delta] })),
        PlayerCommand::ToggleMute => line(json!({ "command": ["cycle", "mute"] })),
        PlayerCommand::CycleSubtitles => line(json!({ "command": ["cycle", "sub"] })),
        PlayerCommand::NextTrack => line(json!({ "command": ["playlist-next"] })),
        PlayerCommand::Fullscreen(on) => line(set_property("fullscreen", on.into())),
        PlayerCommand::PrevTrack => line(json!({ "command": ["playlist-prev"] })),
        PlayerCommand::Language(_)
        | PlayerCommand::SubLang(_)
        | PlayerCommand::Quality(_)
        | PlayerCommand::Codec(_) => {
            let sid = if config.language == Language::Subtitles { "auto" } else { "no" };
            vec![
                set_property("ytdl-format", format_for(config, mode, audio_only).into()).to_string(),
                set_property("ytdl-raw-options", raw_options(config, cookies).into()).to_string(),
                set_property("slang", config.sub_lang.clone().into()).to_string(),
                set_property("sid", sid.into()).to_string(),
            ]
        }
    }
}

fn set_property(name: &str, value: Value) -> Value {
    json!({ "command": ["set_property", name, value] })
}

/// What the video tab shows about the playback, as mpv reports it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Info {
    pub volume: Option<f64>,
    pub muted: bool,
    /// Height of the video being played.
    pub height: Option<i64>,
    /// mpv's codec name: `h264`, `vp9`, `av1` …
    pub codec: Option<String>,
    /// Language of the audio track playing (`en`, `es` …), when the video says.
    pub audio_lang: Option<String>,
    /// Seconds played and in total.
    pub position: Option<f64>,
    pub duration: Option<f64>,
    /// Up to which second the video is already downloaded.
    pub buffered: Option<f64>,
    pub paused: bool,
    /// Where each chapter starts (seconds) and its title.
    pub chapters: Vec<(f64, String)>,
    /// The chapter playing, as an index of `chapters`.
    pub chapter: Option<usize>,
}

impl Info {
    /// Title of the chapter playing.
    pub fn chapter_title(&self) -> Option<&str> {
        self.chapters.get(self.chapter?).map(|(_, title)| title.as_str()).filter(|t| !t.is_empty())
    }
}

/// Properties mpv reports to [`Info`] whenever they change.
const OBSERVED: [&str; 11] = [
    "volume",
    "mute",
    "height",
    "video-format",
    "current-tracks/audio/lang",
    "time-pos",
    "duration",
    "pause",
    "demuxer-cache-time",
    "chapter-list",
    "chapter",
];

impl Info {
    /// Takes a new value; whether what is shown changed (the position only
    /// shows whole seconds, and every redraw competes with the video).
    fn apply(&mut self, name: &str, data: &Value) -> bool {
        let before = self.clone();
        match name {
            "volume" => self.volume = data.as_f64(),
            "mute" => self.muted = data.as_bool().unwrap_or(false),
            "height" => self.height = data.as_i64(),
            "video-format" => self.codec = data.as_str().map(str::to_owned),
            "current-tracks/audio/lang" => self.audio_lang = data.as_str().map(str::to_owned),
            "time-pos" => self.position = data.as_f64(),
            "duration" => self.duration = data.as_f64(),
            "pause" => self.paused = data.as_bool().unwrap_or(false),
            "demuxer-cache-time" => self.buffered = data.as_f64(),
            "chapter-list" => {
                self.chapters = (data.as_array().into_iter().flatten())
                    .filter_map(|c| Some((c["time"].as_f64()?, c["title"].as_str().unwrap_or_default().to_owned())))
                    .collect();
            }
            "chapter" => self.chapter = data.as_i64().and_then(|i| usize::try_from(i).ok()),
            _ => {}
        }
        let second = |p: Option<f64>| p.map(|p| p as i64);
        let coarse = |i: &Self| Self {
            position: second(i.position).map(|s| s as f64),
            buffered: second(i.buffered).map(|s| s as f64),
            ..i.clone()
        };
        coarse(self) != coarse(&before)
    }
}

/// Shared between the player and the thread that listens to mpv.
#[derive(Default)]
struct Observed {
    info: Mutex<Info>,
    changed: AtomicBool,
}

/// One running mpv instance, killed on drop.
pub struct Player {
    child: Child,
    ipc_path: String,
    ipc: Option<Box<dyn Write + Send>>,
    mode: Mode,
    /// Sound only: reloads keep asking for audio formats.
    audio_only: bool,
    config: PlayerConfig,
    cookies: Option<std::path::PathBuf>,
    /// Where the video is drawn; `None` while hidden or in window mode.
    shown_at: Option<Geometry>,
    /// We paused it because another video took over; a user pause is left alone.
    paused_by_us: bool,
    opened_at: Instant,
    /// mpv reports a playback position: the video is loaded.
    started: Arc<AtomicBool>,
    observed: Arc<Observed>,
}

impl Player {
    pub fn spawn(launch: &Launch, geometry: Option<Geometry>) -> std::io::Result<Self> {
        let ipc_path = ipc_path();
        // Music has no picture to place: treat it like a window that never shows.
        let mode = if launch.audio_only { Mode::Window } else { launch.mode };
        let stdout = match mode {
            Mode::Embedded => Stdio::inherit(), // frames go to our terminal
            Mode::Window | Mode::Overlay => Stdio::null(),
        };
        let mut command = Command::new(crate::doctor::program("mpv"));
        // mpv asks yt-dlp for the streams: tell it where yt-dlp is if it is not on PATH.
        if let Some(yt_dlp) = crate::doctor::find("yt-dlp") {
            command.arg(format!("--script-opts=ytdl_hook-ytdl_path={}", yt_dlp.display()));
        }
        let child = command
            .args(mpv_args(launch, geometry, &ipc_path))
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(Stdio::null())
            .spawn()?;
        let shown_at = if mode.in_pane() { geometry } else { None };
        if mode == Mode::Overlay {
            give_back_focus(Duration::from_secs(30));
        }
        let (config, cookies) = (launch.config.clone(), launch.cookies.map(Into::into));
        let started = Arc::new(AtomicBool::new(false));
        let observed = Arc::new(Observed::default());
        watch_start(ipc_path.clone(), Arc::clone(&started), Arc::clone(&observed));
        let opened_at = Instant::now();
        Ok(Self {
            child,
            ipc_path,
            ipc: None,
            mode,
            audio_only: launch.audio_only,
            config,
            cookies,
            shown_at,
            paused_by_us: false,
            opened_at,
            started,
            observed,
        })
    }

    /// How long it has been loading, until it starts playing.
    pub fn opening(&self) -> Option<Duration> {
        (!self.started.load(Ordering::Relaxed)).then(|| self.opened_at.elapsed())
    }

    /// Volume, quality and codec as mpv reports them.
    pub fn info(&self) -> Info {
        self.observed.info.lock().map(|i| i.clone()).unwrap_or_default()
    }

    /// Whether [`Player::info`] changed since the last call.
    pub fn info_changed(&self) -> bool {
        self.observed.changed.swap(false, Ordering::Relaxed)
    }

    pub fn send(&mut self, command: PlayerCommand) {
        match command {
            PlayerCommand::Language(language) => self.config.language = language,
            PlayerCommand::SubLang(code) => self.config.sub_lang = code.into(),
            PlayerCommand::Quality(height) => self.config.max_height = height,
            PlayerCommand::Codec(codec) => self.config.prefer_codec = codec.into(),
            _ => {}
        }
        for line in command_lines(&self.config, self.mode, self.audio_only, self.cookies.as_deref(), command) {
            self.write(&line);
        }
        if matches!(
            command,
            PlayerCommand::Language(_)
                | PlayerCommand::SubLang(_)
                | PlayerCommand::Quality(_)
                | PlayerCommand::Codec(_)
        ) {
            // Same video, new format/audio/subtitles, same second.
            let path = self.query("path").and_then(|v| v.as_str().map(str::to_owned));
            let seconds = self.query("time-pos").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if let Some(path) = path {
                self.write(&reload_line(&path, seconds));
            }
        }
    }

    fn query(&self, property: &str) -> Option<Value> {
        query(&self.ipc_path, property)
    }

    /// Shows the video at `geometry`, resuming it if we had paused it.
    /// No-op in window mode.
    pub fn show(&mut self, geometry: Geometry) {
        if self.mode == Mode::Overlay {
            if self.shown_at != Some(geometry) {
                if let Some(place) = geometry.x11() {
                    self.write(&set_property("geometry", place.into()).to_string());
                }
                if self.shown_at.is_none() {
                    self.write(&set_property("window-minimized", false.into()).to_string());
                    give_back_focus(Duration::from_secs(2));
                }
                self.shown_at = Some(geometry);
            }
            self.resume();
            return;
        }
        if self.mode != Mode::Embedded {
            return;
        }
        if self.shown_at != Some(geometry) {
            // The old placement would stay on screen at the old size/position.
            clear_images();
            for (name, value) in geometry.options() {
                self.write(&set_property(&format!("options/vo-kitty-{name}"), value.into()).to_string());
            }
            // kitty's output reads its geometry only when created, so recreate
            // the output alone. Toggling the video track instead makes mpv seek
            // the stream again: measured 6 s on YouTube, against 1.4 s for this.
            if self.shown_at.is_some() {
                self.write(&set_property("vo", "null".into()).to_string());
            }
            self.write(&set_property("vo", "kitty".into()).to_string());
            self.shown_at = Some(geometry);
        }
        self.resume();
    }

    /// Stops drawing video (it keeps decoding, so showing it again is quick).
    /// With `keep_audio` it keeps playing
    /// (background listening); otherwise it is paused until shown again.
    /// Returns whether it was on screen (its last frame must be cleared).
    pub fn hide(&mut self, keep_audio: bool) -> bool {
        if !self.mode.in_pane() {
            return false;
        }
        let was_shown = self.shown_at.take().is_some();
        if was_shown && self.mode == Mode::Overlay {
            self.write(&set_property("window-minimized", true.into()).to_string());
        } else if was_shown {
            self.write(&set_property("vo", "null".into()).to_string());
        }
        if keep_audio {
            self.resume();
        } else if !self.paused_by_us {
            self.write(&set_property("pause", true.into()).to_string());
            self.paused_by_us = true;
        }
        // An overlay leaves nothing drawn in the terminal.
        was_shown && self.mode == Mode::Embedded
    }

    fn resume(&mut self) {
        if std::mem::take(&mut self.paused_by_us) {
            self.write(&set_property("pause", false.into()).to_string());
        }
    }

    /// It was started with the login cookies.
    pub fn used_cookies(&self) -> bool {
        self.cookies.is_some()
    }

    pub fn has_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    /// Best effort: if mpv is not listening yet the message is dropped.
    fn write(&mut self, line: &str) {
        if self.ipc.is_none() {
            self.ipc = connect(&self.ipc_path).ok();
            // We never read replies or events; keep mpv from queueing them.
            if let Some(ipc) = self.ipc.as_mut() {
                let _ = writeln!(ipc, "{}", json!({ "command": ["disable_event", "all"] }));
            }
        }
        if let Some(ipc) = self.ipc.as_mut()
            && writeln!(ipc, "{line}").is_err()
        {
            self.ipc = None;
        }
    }
}

impl Drop for Player {
    /// Asks mpv to quit so it finishes its current frame and removes its
    /// image; kills it if it does not exit promptly.
    fn drop(&mut self) {
        self.write(&json!({ "command": ["quit"] }).to_string());
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline && !self.has_exited() {
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.ipc_path);
    }
}

/// Reads a property on a separate connection; `None` if mpv does not answer quickly.
fn query(ipc_path: &str, property: &str) -> Option<Value> {
    use std::io::{BufRead, BufReader};
    let mut stream = connect_stream(ipc_path).ok()?;
    #[cfg(unix)]
    stream.set_read_timeout(Some(Duration::from_millis(500))).ok()?;
    writeln!(stream, "{}", json!({ "command": ["get_property", property], "request_id": 1 })).ok()?;
    BufReader::new(stream)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .find(|reply| reply["request_id"] == 1)
        .map(|reply| reply["data"].clone())
}

/// Sets `started` once mpv has a playback position, then listens to the
/// [`OBSERVED`] properties until mpv quits. Gives up when the player is
/// dropped or after two minutes without starting.
fn watch_start(ipc_path: String, started: Arc<AtomicBool>, observed: Arc<Observed>) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(120);
        while Instant::now() < deadline && Arc::strong_count(&started) > 1 {
            if query(&ipc_path, "time-pos").is_some_and(|v| v.is_f64()) {
                started.store(true, Ordering::Relaxed);
                observe(&ipc_path, &observed);
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
}

/// Keeps `observed` up to date; returns when mpv closes the connection.
fn observe(ipc_path: &str, observed: &Observed) {
    use std::io::{BufRead, BufReader};
    let Ok(mut stream) = connect_stream(ipc_path) else { return };
    for (id, name) in OBSERVED.iter().enumerate() {
        if writeln!(stream, "{}", json!({ "command": ["observe_property", id + 1, name] })).is_err() {
            return;
        }
    }
    for line in BufReader::new(stream).lines().map_while(Result::ok) {
        let Ok(event) = serde_json::from_str::<Value>(&line) else { continue };
        if event["event"] == "property-change"
            && let Ok(mut info) = observed.info.lock()
            && info.apply(event["name"].as_str().unwrap_or_default(), &event["data"])
        {
            observed.changed.store(true, Ordering::Relaxed);
        }
    }
}

fn ipc_path() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = format!("tubetui-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    if cfg!(windows) {
        format!(r"\\.\pipe\{name}")
    } else {
        std::env::temp_dir().join(format!("{name}.sock")).display().to_string()
    }
}

/// Write side of the IPC connection. mpv answers every command; a thread
/// discards the answers, otherwise the socket fills up and writes block.
fn connect(path: &str) -> std::io::Result<Box<dyn Write + Send>> {
    let stream = connect_stream(path)?;
    let mut replies = stream.try_clone()?;
    std::thread::spawn(move || std::io::copy(&mut replies, &mut std::io::sink()));
    Ok(Box::new(stream))
}

#[cfg(unix)]
fn connect_stream(path: &str) -> std::io::Result<std::os::unix::net::UnixStream> {
    std::os::unix::net::UnixStream::connect(path)
}

#[cfg(windows)]
fn connect_stream(path: &str) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new().read(true).write(true).open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| vars.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string())
    }

    /// A fake mpv that answers every `get_property` with `data`.
    #[cfg(unix)]
    fn fake_mpv(data: Value) -> String {
        use std::io::{BufRead, BufReader};
        let path = ipc_path();
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().map_while(Result::ok) {
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                writeln!(stream, "{}", json!({ "data": data, "request_id": 1, "error": "success" })).unwrap();
            }
        });
        path
    }

    #[cfg(unix)]
    #[test]
    fn streamlink_prints_one_address() {
        assert_eq!(parse_stream_url("https://x/live.m3u8\n").as_deref(), Some("https://x/live.m3u8"));
        assert_eq!(parse_stream_url("error: no playable streams\n"), None);
        assert_eq!(parse_stream_url(""), None);
    }

    #[test]
    fn start_is_detected_when_mpv_reports_a_position() {
        let loading = Arc::new(AtomicBool::new(false));
        watch_start(fake_mpv(Value::Null), Arc::clone(&loading), Arc::default());
        let playing = Arc::new(AtomicBool::new(false));
        watch_start(fake_mpv(json!(0.5)), Arc::clone(&playing), Arc::default());
        std::thread::sleep(Duration::from_millis(300));
        assert!(playing.load(Ordering::Relaxed));
        assert!(!loading.load(Ordering::Relaxed), "no position yet: still loading");
    }

    #[test]
    fn info_follows_property_changes() {
        let mut info = Info::default();
        assert!(info.apply("volume", &json!(85.0)));
        info.apply("mute", &json!(true));
        info.apply("height", &json!(480));
        info.apply("video-format", &json!("h264"));
        assert!(info.apply("current-tracks/audio/lang", &json!("es")));
        info.apply("duration", &json!(300.5));
        assert!(info.apply("time-pos", &json!(3.0)));
        assert!(!info.apply("time-pos", &json!(3.4)), "same second: nothing to redraw");
        assert!(info.apply("time-pos", &json!(4.1)));
        assert!(!info.apply("estimated-frame-count", &json!(1)), "not observed");
        assert!(info.apply("chapter-list", &json!([{"title": "Intro", "time": 0.0}, {"title": "Fin", "time": 200.0}])));
        assert!(info.apply("chapter", &json!(1)));
        assert_eq!(info.chapter_title(), Some("Fin"));
        assert!(info.apply("chapter", &json!(-1)));
        assert_eq!(info.chapter_title(), None, "before the first chapter");
        assert!(info.apply("demuxer-cache-time", &json!(60.2)));
        assert!(!info.apply("demuxer-cache-time", &json!(60.7)));
        assert_eq!(
            info,
            Info {
                volume: Some(85.0),
                muted: true,
                height: Some(480),
                codec: Some("h264".into()),
                audio_lang: Some("es".into()),
                position: Some(4.1),
                duration: Some(300.5),
                buffered: Some(60.7),
                paused: false,
                chapters: vec![(0.0, "Intro".into()), (200.0, "Fin".into())],
                chapter: None,
            }
        );
        info.apply("height", &Value::Null);
        assert_eq!(info.height, None, "unloaded while reloading");
    }

    #[test]
    fn explicit_modes_win() {
        assert_eq!(resolve_mode(PlayerMode::Window, env(&[("KITTY_WINDOW_ID", "1")])), Mode::Window);
        assert_eq!(resolve_mode(PlayerMode::Embedded, env(&[])), Mode::Embedded);
    }

    #[test]
    fn auto_embeds_only_in_kitty_without_multiplexer() {
        assert_eq!(resolve_mode(PlayerMode::Auto, env(&[("KITTY_WINDOW_ID", "1")])), Mode::Embedded);
        assert_eq!(resolve_mode(PlayerMode::Auto, env(&[("TERM", "xterm-kitty")])), Mode::Embedded);
        assert_eq!(resolve_mode(PlayerMode::Auto, env(&[("TERM", "xterm-256color")])), Mode::Window);
        for multiplexer in MULTIPLEXERS {
            let vars = [("KITTY_WINDOW_ID", "1"), (multiplexer, "1")];
            assert_eq!(resolve_mode(PlayerMode::Auto, env(&vars)), Mode::Window, "{multiplexer}");
            assert_eq!(resolve_mode(PlayerMode::Embedded, env(&vars)), Mode::Embedded, "explicit wins");
        }
    }

    #[test]
    fn geometry_is_one_based_with_pixel_size() {
        let g = Geometry::new(Rect::new(25, 2, 80, 20), (10, 22));
        assert_eq!(g, Geometry { left: 26, top: 3, cols: 80, rows: 20, width_px: 800, height_px: 440, screen: None });
    }

    const QUEUE: &[String] = &[];

    fn launch(config: &PlayerConfig, mode: Mode) -> Launch<'_> {
        Launch { config, mode, cookies: None, queue: QUEUE, audio_only: false, start: None }
    }

    #[test]
    fn queue_goes_last_after_a_separator() {
        let config = PlayerConfig::default();
        let queue = ["/m/a.mp3".to_string(), "-raro.mp3".to_string()];
        let args = mpv_args(&Launch { queue: &queue, audio_only: true, ..launch(&config, Mode::Embedded) }, None, "s");
        assert_eq!(&args[args.len() - 3..], ["--", "/m/a.mp3", "-raro.mp3"]);
        assert!(args.contains(&"--vid=no".into()));
        assert!(!args.iter().any(|a| a.starts_with("--vo=")), "music needs no video output");
    }

    #[test]
    fn resuming_starts_at_the_saved_second() {
        let config = PlayerConfig::default();
        let args = mpv_args(&Launch { start: Some(125), ..launch(&config, Mode::Window) }, None, "s");
        assert!(args.contains(&"--start=125".into()));
        assert!(!mpv_args(&launch(&config, Mode::Window), None, "s").iter().any(|a| a.starts_with("--start")));
    }

    #[test]
    fn format_prefers_codec_then_falls_back() {
        assert_eq!(
            ytdl_format(&PlayerConfig::default(), Mode::Window),
            "bv*[height<=720][vcodec^=avc1]+ba/bv*[height<=720]+ba/b[height<=720]/b"
        );
        let any = PlayerConfig { max_height: 480, prefer_codec: String::new(), ..Default::default() };
        assert_eq!(ytdl_format(&any, Mode::Window), "bv*[height<=480]+ba/b[height<=480]/b");
    }

    #[test]
    fn embedded_prefers_30_fps() {
        let any = PlayerConfig { max_height: 480, prefer_codec: String::new(), ..Default::default() };
        assert_eq!(
            ytdl_format(&any, Mode::Embedded),
            "bv*[height<=480][fps<=30]+ba/bv*[height<=480]+ba/b[height<=480]/b"
        );
        let format = ytdl_format(&PlayerConfig::default(), Mode::Embedded);
        assert!(
            format.starts_with("bv*[height<=720][fps<=30][vcodec^=avc1]+ba/bv*[height<=720][fps<=30]+ba/"),
            "{format}"
        );
    }

    #[test]
    fn dubbed_prefers_audio_in_sub_lang() {
        let config = PlayerConfig { language: Language::Dubbed, prefer_codec: String::new(), ..Default::default() };
        assert_eq!(
            ytdl_format(&config, Mode::Window),
            "bv*[height<=720]+ba[language^=es]/bv*[height<=720]+ba/b[height<=720]/b",
            "never a bare `ba`: that would be sound without picture"
        );
    }

    #[test]
    fn subtitles_are_fetched_and_shown_only_in_subtitles_mode() {
        let config = PlayerConfig::default();
        let args = mpv_args(&launch(&config, Mode::Window), None, "s");
        assert!(args.contains(&"--sid=auto".into()));
        assert!(args.contains(&"--slang=es".into()));
        assert!(args.contains(&"--ytdl-raw-options=sub-langs=[es],write-subs=,write-auto-subs=".into()));
        let original = PlayerConfig { language: Language::Original, ..Default::default() };
        assert!(mpv_args(&launch(&original, Mode::Window), None, "s").contains(&"--sid=no".into()));
    }

    #[test]
    fn cookies_go_to_yt_dlp() {
        let config = PlayerConfig::default();
        let with = Launch { cookies: Some(std::path::Path::new("/c.txt")), ..launch(&config, Mode::Window) };
        let args = mpv_args(&with, None, "s");
        assert!(
            args.iter().any(|a| a.starts_with("--ytdl-raw-options=") && a.ends_with(",cookies=/c.txt")),
            "{args:?}"
        );
    }

    #[test]
    fn overlay_needs_x11_and_the_terminal_window() {
        let x11 = [("DISPLAY", ":0"), ("WINDOWID", "42")];
        assert_eq!(resolve_mode(PlayerMode::Overlay, env(&x11)), Mode::Overlay);
        let wayland = [("DISPLAY", ":0"), ("WINDOWID", "42"), ("XDG_SESSION_TYPE", "wayland")];
        assert_eq!(resolve_mode(PlayerMode::Overlay, env(&wayland)), Mode::Window);
        assert_eq!(resolve_mode(PlayerMode::Overlay, env(&[("DISPLAY", ":0")])), Mode::Window);
        let tmux = [("DISPLAY", ":0"), ("WINDOWID", "42"), ("TMUX", "1")];
        assert_eq!(resolve_mode(PlayerMode::Overlay, env(&tmux)), Mode::Window);
        assert!(Mode::Overlay.in_pane() && Mode::Embedded.in_pane() && !Mode::Window.in_pane());
    }

    #[test]
    fn overlay_is_placed_in_screen_pixels() {
        // 10×20 px cells; the pane starts at column 29, row 4 (1-based).
        let g = Geometry::new(Rect::new(28, 3, 50, 20), (10, 20));
        assert_eq!(g.x11(), None, "not placed yet");
        let placed = g.on_screen((118, 153));
        assert_eq!(placed.screen, Some((118 + 280, 153 + 60)));
        assert_eq!(placed.x11().as_deref(), Some("500x400+398+213"));
        // A 1000 px window with 98 columns of 10 px: 10 px each side.
        assert_eq!(grid_from_window((100, 50, 1000), 98, 10), (110, 60));
        assert_eq!(grid_from_window((0, 0, 100), 50, 10), (0, 0), "never negative");
    }

    #[test]
    fn overlay_args_make_a_borderless_window_that_lets_input_through() {
        let config = PlayerConfig::default();
        let g = Geometry::new(Rect::new(28, 3, 50, 20), (10, 20)).on_screen((118, 153));
        let args = mpv_args(&launch(&config, Mode::Overlay), Some(g), "/tmp/s");
        for arg in
            ["--no-border", "--ontop", "--input-cursor-passthrough=yes", "--hwdec=auto", "--geometry=500x400+398+213"]
        {
            assert!(args.contains(&arg.to_string()), "{arg}: {args:?}");
        }
        assert!(!args.iter().any(|a| a.starts_with("--vo=")), "mpv's own video output");
        assert!(ytdl_format(&config, Mode::Overlay).starts_with("bv*[height<=720][vcodec^=avc1]"), "no 30 fps cap");
    }

    #[test]
    fn window_args() {
        let config = PlayerConfig::default();
        let args = mpv_args(&launch(&config, Mode::Window), None, "/tmp/s");
        assert!(args.contains(&"--hwdec=auto".into()));
        assert!(args.contains(&"--force-window=immediate".into()));
        assert!(!args.iter().any(|a| a.starts_with("--vo")));
        assert_eq!(args.last().unwrap(), "--");
    }

    #[test]
    fn embedded_args_place_video_and_leave_screen_to_us() {
        let g = Geometry::new(Rect::new(0, 1, 40, 10), (10, 20));
        let config = PlayerConfig::default();
        let args = mpv_args(&launch(&config, Mode::Embedded), Some(g), "/tmp/s");
        for expected in [
            "--vo=kitty",
            "--vo-kitty-alt-screen=no",
            "--vo-kitty-config-clear=no",
            "--hwdec=no",
            "--sws-scaler=fast-bilinear",
            "--framedrop=decoder+vo",
            "--vo-kitty-left=1",
            "--vo-kitty-top=2",
            "--vo-kitty-width=400",
            "--vo-kitty-height=200",
            "--input-ipc-server=/tmp/s",
        ] {
            assert!(args.contains(&expected.to_string()), "missing {expected}: {args:?}");
        }
    }

    #[test]
    fn hwdec_can_be_disabled() {
        let config = PlayerConfig { hwdec: false, ..Default::default() };
        assert!(mpv_args(&launch(&config, Mode::Window), None, "s").contains(&"--hwdec=no".into()));
    }

    #[test]
    fn commands_serialize_to_mpv_ipc() {
        let config = PlayerConfig::default();
        let one = |command| command_lines(&config, Mode::Embedded, false, None, command).join("\n");
        assert_eq!(one(PlayerCommand::TogglePause), r#"{"command":["cycle","pause"]}"#);
        assert_eq!(one(PlayerCommand::Seek(-5)), r#"{"command":["seek",-5,"relative"]}"#);
        assert_eq!(one(PlayerCommand::Volume(5)), r#"{"command":["add","volume",5]}"#);
        assert_eq!(one(PlayerCommand::ToggleMute), r#"{"command":["cycle","mute"]}"#);
        assert_eq!(one(PlayerCommand::CycleSubtitles), r#"{"command":["cycle","sub"]}"#);
        assert_eq!(one(PlayerCommand::NextTrack), r#"{"command":["playlist-next"]}"#);
    }

    #[test]
    fn language_change_reloads_at_the_same_second() {
        let config = PlayerConfig {
            prefer_codec: String::new(),
            language: Language::Dubbed,
            sub_lang: "en".into(),
            ..Default::default()
        };
        let cookies = std::path::Path::new("/c.txt");
        let lines = command_lines(&config, Mode::Window, false, Some(cookies), PlayerCommand::SubLang("en"));
        assert_eq!(
            lines,
            [
                r#"{"command":["set_property","ytdl-format","bv*[height<=720]+ba[language^=en]/bv*[height<=720]+ba/b[height<=720]/b"]}"#,
                r#"{"command":["set_property","ytdl-raw-options","sub-langs=[en],write-subs=,write-auto-subs=,cookies=/c.txt"]}"#,
                r#"{"command":["set_property","slang","en"]}"#,
                r#"{"command":["set_property","sid","no"]}"#,
            ]
        );
        assert_eq!(
            reload_line("https://youtu.be/x", 83.456),
            r#"{"command":["loadfile","https://youtu.be/x","replace",-1,"start=83.5"]}"#
        );
        let subs = PlayerConfig { language: Language::Subtitles, ..config };
        let lines = command_lines(&subs, Mode::Embedded, false, None, PlayerCommand::Language(Language::Subtitles));
        assert_eq!(lines[3], r#"{"command":["set_property","sid","auto"]}"#);
    }

    #[test]
    fn ipc_paths_are_unique() {
        assert_ne!(ipc_path(), ipc_path());
    }
}
