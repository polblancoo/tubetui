//! User configuration, read from `config.toml`.
//!
//! Every field has a default, so a missing file or a partial file is fine.
//! Unknown keys are rejected to surface typos instead of silently ignoring them.
//! Settings changed from the app are written back with [`set_value`], which
//! keeps the user's comments and formatting.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::{Lang, Text};
use crate::theme::{Border, Theme};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub session: SessionConfig,
    pub player: PlayerConfig,
    pub ui: UiConfig,
    pub search: SearchConfig,
    pub download: DownloadConfig,
    pub media: MediaConfig,
    pub effects: EffectsConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MediaConfig {
    /// Folders with videos to play; empty means the system videos folder.
    pub video_dirs: Vec<String>,
    /// Folders with music; empty means the system music folder.
    pub music_dirs: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SessionConfig {
    /// Where the YouTube login comes from, in yt-dlp `--cookies-from-browser`
    /// syntax: `BROWSER[+KEYRING][:PROFILE]`, e.g. `brave+GNOMEKEYRING:Default`.
    /// Empty means no login.
    pub browser: String,
    /// Keyring used when picking a browser from the "Sesión" list, e.g.
    /// `GNOMEKEYRING`. Empty detects it (KWallet, gnome-keyring or another
    /// Secret Service), which matters on window managers where yt-dlp guesses wrong.
    pub keyring: String,
    /// An imported `cookies.txt`, for machines without a browser (SSH,
    /// servers). Takes precedence over `browser`.
    pub cookies_file: String,
    /// The app already asked which browser to take the login from.
    pub asked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlayerConfig {
    pub mode: PlayerMode,
    /// Upper bound for the video height, e.g. 480, 720, 1080.
    pub max_height: u32,
    /// Preferred video codec prefix (`avc1` decodes in hardware almost
    /// everywhere). Empty means any codec.
    pub prefer_codec: String,
    /// Hardware decoding, in window mode (embedded video decodes in software
    /// to stay in sync).
    pub hwdec: bool,
    pub language: Language,
    /// Subtitle / dubbing language code.
    pub sub_lang: String,
    /// Keep listening when leaving the video tab (only the last video plays).
    pub background_audio: bool,
    /// Skip sponsors, self promotion and "subscribe!" reminders (SponsorBlock).
    pub sponsorblock: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayerMode {
    /// Embedded when the terminal speaks the kitty graphics protocol.
    Auto,
    /// Video drawn inside the app pane.
    Embedded,
    /// Video in its own mpv window: hardware decoding, and the tab shows
    /// the video's details and what to watch next. The default.
    #[default]
    Window,
    /// An mpv window without borders laid over the app pane (X11): looks
    /// embedded, plays like a window.
    Overlay,
}

impl PlayerMode {
    pub fn next(self) -> Self {
        match self {
            Self::Auto => Self::Embedded,
            Self::Embedded => Self::Window,
            Self::Window => Self::Overlay,
            Self::Overlay => Self::Auto,
        }
    }

    /// Value as written in `config.toml`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Embedded => "embedded",
            Self::Window => "window",
            Self::Overlay => "overlay",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Original audio with subtitles in `sub_lang` (auto-translated if needed).
    #[default]
    Subtitles,
    /// Original audio, no subtitles.
    Original,
    /// YouTube's automatic dubbing in `sub_lang` when available.
    Dubbed,
}

impl Language {
    pub fn next(self) -> Self {
        match self {
            Self::Subtitles => Self::Original,
            Self::Original => Self::Dubbed,
            Self::Dubbed => Self::Subtitles,
        }
    }

    /// Value as written in `config.toml`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Subtitles => "subtitles",
            Self::Original => "original",
            Self::Dubbed => "dubbed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    pub sidebar: bool,
    pub max_tabs: usize,
    pub icons: IconSet,
    pub theme: Theme,
    pub border: Border,
    /// Interface language; unset follows the system locale.
    pub lang: Option<Lang>,
    /// Video thumbnails in the preview panel (kitty only).
    pub thumbnails: bool,
    /// Clicks and the wheel (selecting text then needs Shift).
    pub mouse: bool,
    /// The tubetui logo for a moment when the app opens.
    pub splash: bool,
    /// Ask GitHub once a day whether there is a newer version.
    pub check_updates: bool,
    /// Inicio, Descubrir and Suscripciones leave out the videos I watched
    /// (`v` shows them for a while).
    pub hide_watched: bool,
    /// Feeds, searches and channels leave out YouTube Shorts.
    pub hide_shorts: bool,
    /// While the app is open, tell when my channels publish something new.
    pub notify_new: bool,
    /// The channels I watch most go first in Suscripciones and Mis canales.
    pub favorites_first: bool,
    /// "Mis recomendados" shows YouTube's whole home feed, not only my channels (`t`).
    pub home_all: bool,
}

/// Short animations, each one can be turned off.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct EffectsConfig {
    /// The menu and the welcome screen appear when the app starts.
    pub startup: bool,
    /// A new list, section or tab sweeps in.
    pub pages: bool,
    /// Help and questions open with a flash.
    pub dialogs: bool,
}

impl Default for EffectsConfig {
    fn default() -> Self {
        Self { startup: true, pages: true, dialogs: true }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IconSet {
    /// Colour emoji, like the original ytv script.
    #[default]
    Emoji,
    /// Needs a Nerd Font (kitty bundles the symbols).
    Nerd,
    /// Plain Unicode, works with any font.
    Unicode,
}

impl IconSet {
    pub fn next(self) -> Self {
        match self {
            Self::Emoji => Self::Nerd,
            Self::Nerd => Self::Unicode,
            Self::Unicode => Self::Emoji,
        }
    }

    /// Value as written in `config.toml`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Emoji => "emoji",
            Self::Nerd => "nerd",
            Self::Unicode => "unicode",
        }
    }
}

/// Subtitle / dubbing languages offered by the settings screen (any code works in the file).
pub const LANGUAGES: [(&str, Text); 9] = [
    ("es", Text::new("español", "Spanish")),
    ("en", Text::new("inglés", "English")),
    ("pt", Text::new("portugués", "Portuguese")),
    ("fr", Text::new("francés", "French")),
    ("it", Text::new("italiano", "Italian")),
    ("de", Text::new("alemán", "German")),
    ("ja", Text::new("japonés", "Japanese")),
    ("ko", Text::new("coreano", "Korean")),
    ("ru", Text::new("ruso", "Russian")),
];

/// Whether an audio track's language (mpv says `eng`, `spa`, or `en-US`
/// as YouTube spells it) is the 2-letter `code` of [`LANGUAGES`].
pub fn same_language(track: &str, code: &str) -> bool {
    // `en-US` is English too: only what comes before the dash counts.
    let track = track.split(['-', '_']).next().unwrap_or_default().to_ascii_lowercase();
    let three: &[&str] = match code {
        "es" => &["spa"],
        "en" => &["eng"],
        "pt" => &["por"],
        "fr" => &["fra", "fre"],
        "it" => &["ita"],
        "de" => &["deu", "ger"],
        "ja" => &["jpn"],
        "ko" => &["kor"],
        "ru" => &["rus"],
        _ => &[],
    };
    track == code || three.contains(&track.as_str())
}

pub fn next_language(code: &str) -> &'static str {
    let index = LANGUAGES.iter().position(|(c, _)| *c == code).map_or(0, |i| (i + 1) % LANGUAGES.len());
    LANGUAGES[index].0
}

pub fn language_name(code: &str, lang: Lang) -> String {
    LANGUAGES
        .iter()
        .find(|(c, _)| *c == code)
        .map_or_else(|| code.to_owned(), |(c, name)| format!("{} ({c})", name.get(lang)))
}

/// A donation address, shown on the About page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wallet {
    pub network: String,
    pub address: String,
}

/// The wallets in `donate.toml`, compiled into the app; empty ones are skipped.
pub fn donations() -> Vec<Wallet> {
    parse_donations(include_str!("../donate.toml"))
}

fn parse_donations(text: &str) -> Vec<Wallet> {
    #[derive(Deserialize)]
    struct File {
        #[serde(default)]
        wallet: Vec<Wallet>,
    }
    let wallets = toml::from_str::<File>(text).map(|f| f.wallet).unwrap_or_default();
    wallets.into_iter().filter(|w| !w.address.trim().is_empty()).collect()
}

/// Heights offered by the settings screen and the `v` key.
pub const QUALITIES: [u32; 5] = [360, 480, 720, 1080, 1440];

pub fn next_quality(current: u32) -> u32 {
    QUALITIES.iter().copied().find(|q| *q > current).unwrap_or(QUALITIES[0])
}

/// Video codecs offered by the `b` key: the `vcodec` prefix yt-dlp reports
/// (VP9 shows as `vp9` or `vp09`) and its name; empty means any.
pub const CODECS: [(&str, &str); 4] = [("avc1", "H.264"), ("vp", "VP9"), ("av01", "AV1"), ("", "")];

pub fn next_codec(current: &str) -> &'static str {
    let at = CODECS.iter().position(|(c, _)| *c == current);
    CODECS[at.map_or(0, |i| (i + 1) % CODECS.len())].0
}

pub fn codec_name(codec: &str, lang: Lang) -> &str {
    match CODECS.iter().find(|(c, _)| *c == codec) {
        Some((_, "")) => lang.pick("cualquiera", "any"),
        Some((_, name)) => name,
        None => codec,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SearchConfig {
    /// Videos fetched per list.
    pub results: u32,
    /// What "Descubrir" looks for, e.g. `["rust", "nba", "jazz"]`.
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DownloadConfig {
    /// Empty means the system videos folder (`~/Videos`).
    pub dir: String,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            mode: PlayerMode::Window,
            max_height: 720,
            prefer_codec: "avc1".into(),
            hwdec: true,
            language: Language::Subtitles,
            sub_lang: "es".into(),
            background_audio: true,
            sponsorblock: true,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            sidebar: true,
            max_tabs: 9,
            icons: IconSet::Emoji,
            theme: Theme::default(),
            border: Border::default(),
            lang: None,
            thumbnails: true,
            mouse: true,
            splash: true,
            check_updates: true,
            hide_watched: true,
            hide_shorts: true,
            notify_new: true,
            home_all: false,
            favorites_first: true,
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self { results: 60, topics: Vec::new() }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("invalid config {path}: {source}")]
    Parse { path: PathBuf, source: toml::de::Error },
    #[error("cannot update {path}: {reason}")]
    Write { path: PathBuf, reason: String },
}

impl Config {
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Loads `path`; a missing file yields the defaults.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => return Err(ConfigError::Read { path: path.into(), source }),
        };
        Self::from_toml(&text).map_err(|source| ConfigError::Parse { path: path.into(), source })
    }

    /// `~/.config/tubetui/config.toml` on Linux, the platform equivalent elsewhere.
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "tubetui").map(|dirs| dirs.config_dir().join("config.toml"))
    }
}

/// Sets `[table] key = value` in the file at `path`, creating file and
/// table when missing and leaving everything else untouched.
pub fn set_value(path: &Path, table: &str, key: &str, value: impl Into<toml_edit::Value>) -> Result<(), ConfigError> {
    let error = |reason: String| ConfigError::Write { path: path.into(), reason };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(error(e.to_string())),
    };
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| error(e.to_string()))?;
    let section = doc
        .entry(table)
        .or_insert_with(toml_edit::table)
        .as_table_mut()
        .ok_or_else(|| error(format!("`{table}` is not a table")))?;
    section[key] = toml_edit::Item::Value(value.into());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| error(e.to_string()))?;
    }
    std::fs::write(path, doc.to_string()).map_err(|e| error(e.to_string()))
}

#[cfg(test)]
pub(crate) fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tubetui-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let c = Config::default();
        assert_eq!(c.session.browser, "");
        assert_eq!(c.session.cookies_file, "");
        assert_eq!(c.player.mode, PlayerMode::Window);
        assert_eq!(c.player.max_height, 720);
        assert_eq!(c.player.prefer_codec, "avc1");
        assert!(c.player.hwdec);
        assert_eq!(c.player.language, Language::Subtitles);
        assert_eq!(c.player.sub_lang, "es");
        assert!(c.ui.sidebar);
        assert_eq!(c.ui.max_tabs, 9);
        assert_eq!(c.ui.icons, IconSet::Emoji);
        assert_eq!(c.ui.lang, None, "follows the system until chosen");
        assert_eq!(c.effects, EffectsConfig { startup: true, pages: true, dialogs: true });
        assert_eq!(c.search.results, 60);
        assert_eq!(c.download.dir, "");
    }

    #[test]
    fn donations_skip_empty_wallets() {
        let text = "[[wallet]]\nnetwork = \"USDT (TRC20)\"\naddress = \"TXyz\"\n[[wallet]]\nnetwork = \"BTC\"\naddress = \"\"\n";
        assert_eq!(parse_donations(text), vec![Wallet { network: "USDT (TRC20)".into(), address: "TXyz".into() }]);
        assert_eq!(parse_donations(""), vec![]);
        donations(); // the shipped file parses
        assert!(toml::from_str::<toml::Table>(include_str!("../donate.toml")).is_ok());
    }

    #[test]
    fn tracks_match_their_language_in_two_or_three_letters() {
        assert!(same_language("es", "es") && same_language("es-US", "es") && same_language("spa", "es"));
        assert!(same_language("eng", "en") && same_language("en-US", "en") && same_language("ENG", "en"));
        assert!(same_language("fre", "fr") && same_language("fra", "fr") && same_language("deu", "de"));
        assert!(!same_language("eng", "es") && !same_language("spa", "en") && !same_language("est", "es"));
        assert!(!same_language("und", "en"), "undetermined is not English");
    }

    #[test]
    fn empty_file_is_default() {
        assert_eq!(Config::from_toml("").unwrap(), Config::default());
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let c = Config::from_toml("[player]\nmode = \"window\"\nmax_height = 480\n").unwrap();
        assert_eq!(c.player.mode, PlayerMode::Window);
        assert_eq!(c.player.max_height, 480);
        assert_eq!(c.player.prefer_codec, "avc1");
        assert_eq!(c.ui, UiConfig::default());
    }

    #[test]
    fn enums_use_lowercase_names() {
        let c = Config::from_toml("[ui]\nicons = \"nerd\"\nlang = \"en\"\n[player]\nlanguage = \"dubbed\"\n").unwrap();
        assert_eq!(c.ui.icons, IconSet::Nerd);
        assert_eq!(c.ui.lang, Some(Lang::En));
        assert_eq!(c.player.language, Language::Dubbed);
        assert_eq!(Language::Dubbed.as_str(), "dubbed");
    }

    #[test]
    fn cycles_wrap_around() {
        assert_eq!(PlayerMode::Auto.next().next().next().next(), PlayerMode::Auto);
        assert_eq!(PlayerMode::Window.next().as_str(), "overlay");
        assert_eq!(PlayerMode::Embedded.as_str(), "embedded");
        assert_eq!(IconSet::Emoji.next(), IconSet::Nerd);
        assert_eq!(IconSet::Unicode.next(), IconSet::Emoji);
        assert_eq!(IconSet::Nerd.as_str(), "nerd");
        assert_eq!(next_quality(480), 720);
        assert_eq!(next_quality(1440), 360);
        assert_eq!(next_codec("avc1"), "vp");
        assert_eq!(next_codec(""), "avc1");
        assert_eq!(next_codec("hev1"), "avc1");
        assert_eq!(codec_name("", Lang::En), "any");
        assert_eq!(codec_name("av01", Lang::Es), "AV1");
        assert_eq!(next_quality(600), 720);
    }

    #[test]
    fn languages_cycle_and_have_names() {
        assert_eq!(next_language("es"), "en");
        assert_eq!(next_language("ru"), "es");
        assert_eq!(next_language("xx"), "es");
        assert_eq!(language_name("pt", Lang::Es), "portugués (pt)");
        assert_eq!(language_name("pt", Lang::En), "Portuguese (pt)");
        assert_eq!(language_name("eu", Lang::En), "eu");
    }

    #[test]
    fn media_folders_are_lists() {
        let c = Config::from_toml("[media]\nvideo_dirs = [\"~/Videos\", \"/mnt/pelis\"]\n").unwrap();
        assert_eq!(c.media.video_dirs, ["~/Videos", "/mnt/pelis"]);
        assert!(c.media.music_dirs.is_empty());
        let path = temp_dir("lists").join("config.toml");
        let list: toml_edit::Array = ["~/Música", "/mnt/discos"].into_iter().collect();
        set_value(&path, "media", "music_dirs", list).unwrap();
        assert_eq!(Config::load(&path).unwrap().media.music_dirs, ["~/Música", "/mnt/discos"]);
    }

    #[test]
    fn set_value_writes_numbers_as_numbers() {
        let path = temp_dir("number").join("config.toml");
        set_value(&path, "player", "max_height", 1080_i64).unwrap();
        assert_eq!(Config::load(&path).unwrap().player.max_height, 1080);
    }

    #[test]
    fn language_cycles() {
        assert_eq!(Language::Subtitles.next(), Language::Original);
        assert_eq!(Language::Original.next(), Language::Dubbed);
        assert_eq!(Language::Dubbed.next(), Language::Subtitles);
    }

    #[test]
    fn typos_are_rejected() {
        assert!(Config::from_toml("[player]\nmax_heigth = 480\n").is_err());
        assert!(Config::from_toml("[player]\nmode = \"fullscreen\"\n").is_err());
    }

    #[test]
    fn serializes_back_to_equal_config() {
        let c = Config::default();
        let text = toml::to_string(&c).unwrap();
        assert_eq!(Config::from_toml(&text).unwrap(), c);
    }

    #[test]
    fn missing_file_loads_defaults() {
        let c = Config::load(Path::new("/definitely/not/here/config.toml")).unwrap();
        assert_eq!(c, Config::default());
    }

    #[test]
    fn invalid_file_reports_path() {
        let path = temp_dir("invalid").join("config.toml");
        std::fs::write(&path, "[ui]\nsidebar = 3\n").unwrap();
        let err = Config::load(&path).unwrap_err().to_string();
        assert!(err.contains("config.toml"), "{err}");
    }

    #[test]
    fn default_path_ends_in_config_toml() {
        if let Some(p) = Config::default_path() {
            assert!(p.ends_with("config.toml") && p.to_string_lossy().contains("tubetui"), "{p:?}");
        }
    }

    #[test]
    fn set_value_creates_missing_file() {
        let path = temp_dir("create").join("sub/config.toml");
        set_value(&path, "session", "browser", "brave").unwrap();
        assert_eq!(Config::load(&path).unwrap().session.browser, "brave");
    }

    #[test]
    fn set_value_keeps_comments_and_other_keys() {
        let path = temp_dir("keep").join("config.toml");
        std::fs::write(&path, "# mi config\n[player]\nmax_height = 480 # bajo consumo\n").unwrap();
        set_value(&path, "player", "language", "dubbed").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# mi config") && text.contains("# bajo consumo"), "{text}");
        let c = Config::from_toml(&text).unwrap();
        assert_eq!(c.player.max_height, 480);
        assert_eq!(c.player.language, Language::Dubbed);
    }
}
