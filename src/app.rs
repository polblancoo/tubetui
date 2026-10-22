//! Application state and the pure `update` function.
//!
//! Tab 0 is always "Inicio": the section menu plus a stack of lists (opening
//! a channel pushes its videos, `Esc` pops back). Tabs 1.. are open videos,
//! each backed by its own player. The live configuration lives here too, so
//! the settings screen is just state changes plus `SaveSetting` effects.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::config::{
    Config, EffectsConfig, Language, PlayerMode, codec_name, language_name, next_codec, next_language, next_quality,
};
use crate::doctor::{Check, Manager};
use crate::domain::{Download, DownloadState, HistoryEntry, Size, TabId, Video, clock, resume_point};
use crate::i18n::Lang;
use crate::keymap::KeyContext;
use crate::media::{LocalFile, MediaKind, parse_dirs};
use crate::sponsorblock::Segment;
use crate::theme::Border;
use crate::update::Update;
use crate::youtube::{Comment, HistoryPage, Items, Rating, SearchPage, Source, Watch};

/// Lists that are filtered afterwards ask YouTube for more videos.
pub const FILTERED_LIMIT: u32 = 200;
/// The next page is asked for when the selection gets this close to the end.
const PREFETCH_ROWS: usize = 5;
/// Videos kept in "Historial".
const HISTORY_MAX: usize = 1000;
/// How many of the last watched videos "Según lo que vi" draws from.
const UP_NEXT_FROM: usize = 10;
/// A favorite channel: one I opened at least this many videos of in the last
/// days, among the ones I watch most.
const FAVORITE_DAYS: i64 = 30;
const FAVORITE_MIN: usize = 3;
const FAVORITES_MAX: usize = 10;
/// Videos of my favorite channels go first in Suscripciones while this recent.
const FAVORITE_FRESH_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Inicio,
    /// What YouTube suggests next to the videos I watched last.
    ParaVos,
    Descubrir,
    Suscripciones,
    VerDespues,
    /// What I opened in tubetui, newest first; resumes where I left.
    Historial,
    /// My YouTube playlists.
    MisListas,
    Canales,
    /// YouTube's picks from channels I don't follow.
    DescubrirCanales,
    BuscarVideos,
    BuscarCanales,
    MisVideos,
    MiMusica,
    /// Running and finished downloads.
    Descargas,
    Configuracion,
    AcercaDe,
    Salir,
}

/// Menu groups, drawn with a header each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    YouTube,
    Local,
    App,
}

impl Group {
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::YouTube => "YOUTUBE",
            Self::Local => lang.pick("EN TU EQUIPO", "ON THIS COMPUTER"),
            Self::App => "",
        }
    }
}

impl Section {
    pub const ALL: [Self; 17] = [
        Self::Inicio,
        Self::ParaVos,
        Self::Descubrir,
        Self::Suscripciones,
        Self::VerDespues,
        Self::Historial,
        Self::MisListas,
        Self::Canales,
        Self::DescubrirCanales,
        Self::BuscarVideos,
        Self::BuscarCanales,
        Self::MisVideos,
        Self::MiMusica,
        Self::Descargas,
        Self::Configuracion,
        Self::AcercaDe,
        Self::Salir,
    ];

    pub fn group(self) -> Group {
        match self {
            Self::MisVideos | Self::MiMusica | Self::Descargas => Group::Local,
            Self::Configuracion | Self::AcercaDe | Self::Salir => Group::App,
            _ => Group::YouTube,
        }
    }

    pub fn label(self, lang: Lang) -> &'static str {
        let (es, en) = match self {
            Self::Inicio => ("Mis recomendados", "My recommendations"),
            Self::ParaVos => ("Según lo que vi", "From what I watched"),
            Self::Descubrir => ("Descubrir", "Discover"),
            Self::Suscripciones => ("Suscripciones", "Subscriptions"),
            Self::VerDespues => ("Ver después", "Watch later"),
            Self::Historial => ("Historial", "History"),
            Self::MisListas => ("Mis listas", "My playlists"),
            Self::Canales => ("Mis canales", "My channels"),
            Self::DescubrirCanales => ("Descubrir canales", "Discover channels"),
            Self::BuscarVideos => ("Buscar videos", "Search videos"),
            Self::BuscarCanales => ("Buscar canales", "Search channels"),
            Self::MisVideos => ("Mis videos", "My videos"),
            Self::MiMusica => ("Mi música", "My music"),
            Self::Descargas => ("Descargas", "Downloads"),
            Self::Configuracion => ("Configuración", "Settings"),
            Self::AcercaDe => ("Acerca de", "About"),
            Self::Salir => ("Salir", "Quit"),
        };
        lang.pick(es, en)
    }
}

/// Rows of the settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Browser,
    ImportCookies,
    Refresh,
    Logout,
    Language,
    SubLang,
    Quality,
    VideoMode,
    BackgroundAudio,
    SponsorBlock,
    AppLang,
    Icons,
    Theme,
    Border,
    Thumbnails,
    Mouse,
    Splash,
    FxStartup,
    FxPages,
    FxDialogs,
    VideoDirs,
    MusicDirs,
    DownloadDir,
    DiscoverTopics,
    SystemCheck,
    HideWatched,
    HideShorts,
    NotifyNew,
    FavoritesFirst,
    CheckUpdates,
    EditFile,
}

impl Setting {
    pub const ALL: [Self; 31] = [
        Self::Browser,
        Self::ImportCookies,
        Self::Refresh,
        Self::Logout,
        Self::Language,
        Self::SubLang,
        Self::Quality,
        Self::VideoMode,
        Self::BackgroundAudio,
        Self::SponsorBlock,
        Self::AppLang,
        Self::Icons,
        Self::Theme,
        Self::Border,
        Self::Thumbnails,
        Self::Mouse,
        Self::Splash,
        Self::FxStartup,
        Self::FxPages,
        Self::FxDialogs,
        Self::VideoDirs,
        Self::MusicDirs,
        Self::DownloadDir,
        Self::DiscoverTopics,
        Self::SystemCheck,
        Self::HideWatched,
        Self::HideShorts,
        Self::NotifyNew,
        Self::FavoritesFirst,
        Self::CheckUpdates,
        Self::EditFile,
    ];

    pub fn label(self, lang: Lang) -> &'static str {
        let (es, en) = match self {
            Self::Browser => ("Sesión desde un navegador", "Log in with a browser"),
            Self::ImportCookies => {
                ("Importar cookies.txt (sin navegador / SSH)", "Import cookies.txt (no browser / SSH)")
            }
            Self::Refresh => ("Releer la sesión", "Read the login again"),
            Self::Logout => ("Cerrar sesión", "Log out"),
            Self::Language => ("Idioma del video", "Video language"),
            Self::SubLang => ("Idioma de subtítulos y doblaje", "Subtitles and dubbing language"),
            Self::Quality => ("Calidad máxima", "Maximum quality"),
            Self::VideoMode => ("Video", "Video"),
            Self::BackgroundAudio => ("Al salir de la pestaña del video", "When leaving the video tab"),
            Self::SponsorBlock => ("Saltar patrocinios (SponsorBlock)", "Skip sponsors (SponsorBlock)"),
            Self::AppLang => ("Idioma de la app (Ctrl+y + l)", "App language (Ctrl+y + l)"),
            Self::Icons => ("Íconos", "Icons"),
            Self::Theme => ("Tema de colores", "Colour theme"),
            Self::Border => ("Bordes de los paneles", "Panel borders"),
            Self::Thumbnails => ("Portadas de los videos (kitty)", "Video thumbnails (kitty)"),
            Self::Mouse => ("Mouse (para copiar texto: Shift)", "Mouse (to copy text: Shift)"),
            Self::Splash => ("Logo al abrir la app", "Logo when the app opens"),
            Self::FxStartup => ("Animación al abrir la app", "Animation when the app starts"),
            Self::FxPages => ("Animación al cambiar de página", "Animation when the page changes"),
            Self::FxDialogs => ("Animación de ayuda y preguntas", "Animation of help and questions"),
            Self::VideoDirs => ("Carpetas de videos", "Video folders"),
            Self::MusicDirs => ("Carpetas de música", "Music folders"),
            Self::DownloadDir => ("Carpeta de descargas", "Downloads folder"),
            Self::DiscoverTopics => ("Temas de Descubrir", "Discover topics"),
            Self::SystemCheck => ("Revisar el sistema (mpv, yt-dlp…)", "Check the system (mpv, yt-dlp…)"),
            Self::HideWatched => {
                ("Ocultar los vistos en Inicio (v los muestra)", "Hide watched in Home (v shows them)")
            }
            Self::HideShorts => ("Ocultar los Shorts", "Hide Shorts"),
            Self::NotifyNew => ("Avisar de videos nuevos de mis canales", "Tell me about new videos from my channels"),
            Self::FavoritesFirst => ("Primero mis canales más vistos", "My most watched channels first"),
            Self::CheckUpdates => ("Buscar versiones nuevas (una vez por día)", "Look for new versions (once a day)"),
            Self::EditFile => ("Editar config.toml", "Edit config.toml"),
        };
        lang.pick(es, en)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Menu,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelRef {
    pub name: String,
    pub url: String,
}

/// Where a video search looks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    All,
    Mine,
    Channel(ChannelRef),
}

impl Scope {
    pub fn label(&self, lang: Lang) -> String {
        match self {
            Self::All => lang.pick("todo YouTube", "all of YouTube").into(),
            Self::Mine => lang.pick("mis canales", "my channels").into(),
            Self::Channel(c) => format!("{} {}", lang.pick("canal", "channel"), c.name),
        }
    }
}

/// What the text box is asking for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    /// `channel` is the channel offered as a scope, when there is one.
    Videos {
        scope: Scope,
        channel: Option<ChannelRef>,
    },
    Channels,
    CookiesFile,
    DownloadDir,
    /// Comma-separated folders for local videos or music.
    MediaDirs(MediaKind),
    /// Comma-separated topics for "Descubrir".
    Topics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListKind {
    Plain,
    /// "Ver después", kept in sync with `State::later`.
    Later,
    /// "Historial", kept in sync with `State::history`.
    History,
    /// "Descargas", kept in sync with `State::downloads`.
    Downloads,
    /// A channel's videos; `/` searches inside it.
    Channel(ChannelRef),
    /// "Mis canales": unsubscribing takes the channel out.
    MyChannels,
    /// Inicio, Descubrir, Suscripciones, Descubrir canales: videos already watched can be hidden.
    Feed,
    Settings,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FileSort {
    #[default]
    Folder,
    Name,
    Size,
}

impl FileSort {
    fn next(self) -> Self {
        match self {
            Self::Folder => Self::Name,
            Self::Name => Self::Size,
            Self::Size => Self::Folder,
        }
    }

    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Folder => lang.pick("carpeta", "folder"),
            Self::Name => lang.pick("nombre", "name"),
            Self::Size => lang.pick("peso", "size"),
        }
    }

    fn sort(self, files: &mut [LocalFile]) {
        match self {
            Self::Folder => files.sort_by_cached_key(|f| (f.folder.to_lowercase(), f.name.to_lowercase())),
            Self::Name => files.sort_by_cached_key(|f| f.name.to_lowercase()),
            Self::Size => files.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub title: String,
    pub items: Items,
    pub selected: usize,
    pub kind: ListKind,
    /// Token for the next page of a search.
    pub next: Option<String>,
    /// The request loading that page.
    pub more: Option<u64>,
    /// Feeds: every video, watched ones included (`items` may leave them out).
    pub all: Option<Vec<Video>>,
    /// "Mis recomendados": `t` switches between my channels and all of YouTube's.
    pub home: bool,
}

impl Listing {
    pub fn new(title: String, items: Items, kind: ListKind) -> Self {
        Self { title, items, selected: 0, kind, next: None, more: None, all: None, home: false }
    }
}

/// A yes/no question before something hard to undo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    Unsubscribe {
        channel_id: String,
        name: String,
    },
    /// Local files to move to the trash.
    Trash(Vec<PathBuf>),
    /// Quitting stops this many downloads.
    Quit(usize),
    /// One of my YouTube playlists, deleted for good.
    DeletePlaylist {
        id: String,
        title: String,
    },
}

/// The right-click menu: what can be done with what is under the pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenu {
    /// The cell clicked: the menu opens there.
    pub at: (u16, u16),
    pub items: Vec<MenuItem>,
    pub selected: usize,
}

/// One line of the right-click menu: what it says, its key, what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub label: &'static str,
    pub key: &'static str,
    pub action: Action,
}

/// A value written to `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingValue {
    Text(String),
    Number(i64),
    Bool(bool),
    List(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Prefix,
    CancelPrefix,
    ToggleSidebar,
    ToggleHelp,
    Up,
    Down,
    Enter,
    Back,
    /// Search videos from anywhere (`/`, `C-y /`).
    FocusSearch,
    /// Next search scope (Tab in the search box).
    CycleScope,
    Input(char),
    Backspace,
    /// A list requested with `Effect::Load` / `Effect::ScanBrowsers` arrived.
    Loaded(u64, Result<Items, String>),
    /// A page requested with `Effect::Search`: a new list, or more of one.
    Page(u64, Result<SearchPage, String>),
    /// A page of my YouTube history requested with `Effect::YoutubeHistory`.
    YoutubeHistory(u64, Result<HistoryPage, String>),
    /// Cached channel ids, read at startup.
    SubsLoaded(HashSet<String>),
    /// When each browser profile last wrote its cookies, by spec (the browser picker).
    BrowsersUsed(HashMap<String, i64>),
    /// The watch page of a tab's video arrived.
    Watched(TabId, Result<Watch, String>),
    /// The login was (re)read. `cookies` is for the runtime; `subs` are my channels.
    SessionReady {
        cookies: Option<PathBuf>,
        subs: Result<HashSet<String>, String>,
    },
    /// A `cookies.txt` was validated and copied; carries its new path.
    CookiesImported(Result<String, String>),
    /// `config.toml` was edited outside the app.
    ConfigReloaded(Result<Config, String>),
    ToggleLater,
    ToggleSubscription,
    /// Answer to "¿Desuscribirte?".
    Confirm(bool),
    SubscriptionDone {
        channel_id: String,
        name: String,
        result: Result<bool, String>,
    },
    /// Like or dislike the selected video; the same again takes it back.
    Rate(Rating),
    Rated {
        video: String,
        title: String,
        rating: Rating,
        result: Result<(), String>,
    },
    /// A playlist deleted on YouTube, or why not.
    PlaylistDeleted {
        id: String,
        title: String,
        result: Result<(), String>,
    },
    Download,
    /// How a download is going.
    DownloadProgress {
        id: u64,
        permille: u16,
        speed: String,
        eta: String,
    },
    /// A download ended: the file, or why not.
    Downloaded {
        id: u64,
        result: Result<PathBuf, String>,
    },
    /// Play the selected video as sound only; in its tab, switch between sound and video.
    Listen,
    OpenChannel,
    Status(String),
    NextTab,
    PrevTab,
    /// 0 is Inicio, n is the n-th video tab.
    GoTab(usize),
    CloseTab,
    /// The player went away on its own (window closed, video ended, failed to start).
    PlayerClosed(TabId, Option<String>),
    /// The player of a YouTube video stopped here (seconds), to resume it later.
    Stopped {
        video: String,
        position: Option<u32>,
        duration: Option<u32>,
    },
    TogglePause,
    Seek(i32),
    /// Jump to this second (a click on the progress bar).
    SeekTo(u32),
    /// Previous / next chapter.
    Chapter(i32),
    /// The SponsorBlock segments of a tab's video arrived.
    SegmentsLoaded(TabId, Vec<Segment>),
    /// A tab's video reached this segment: jump over it.
    SkipSegment(TabId, usize),
    /// The newest videos of my channels, checked in the background.
    SubsPolled(Result<Vec<Video>, String>),
    /// What a video weighs (and lasts), asked to yt-dlp in the background.
    Sized(String, Result<Size, String>),
    /// The top comments of a tab's video arrived.
    CommentsLoaded(TabId, Result<Vec<Comment>, String>),
    /// Move through the comments of the active tab by this many.
    ScrollComments(i32),
    Volume(i32),
    ToggleMute,
    CycleSubtitles,
    NextTrack,
    PrevTrack,
    /// Next language mode (subtitles → original → dubbed) for this and later videos.
    CycleLanguage,
    /// Next subtitle / dubbing language (es → en → pt …) for this and later videos.
    CycleSubLang,
    /// Next maximum height (360p → 480p → …) for this and later videos.
    CycleQuality,
    /// Next preferred video codec (H.264 → VP9 → AV1 → any) for this and later videos.
    CycleCodec,
    /// Jump to the section menu from anywhere.
    FocusMenu,
    /// Switch the interface between Spanish and English.
    ToggleUiLang,
    /// Switch focus between the menu and the list.
    ToggleFocus,
    /// First start without a login: offer the browsers found.
    AskBrowser,
    /// What the system has (mpv, yt-dlp…), with its package manager.
    SystemChecked(Manager, Vec<Check>),
    /// Ids of watched videos: my YouTube history, or the local one at start.
    HistoryLoaded(HashSet<String>),
    /// Feeds: show / hide the videos already watched.
    ToggleWatched,
    /// "Mis recomendados": only my channels, or everything YouTube recommends.
    ToggleHomeAll,
    /// Local files: sort by folder, name or size (largest first).
    CycleSort,
    /// GitHub has a newer version (and how to get it here).
    UpdateFound(Update),
    /// Mark / unmark the selected local file and go to the next one.
    ToggleMark,
    /// Ask to move the marked files (or the selected one) to the trash.
    Trash,
    /// The files went to the trash (or the reason they did not).
    Trashed(Result<Vec<PathBuf>, String>),
    /// Open this section of the menu (a click on it).
    OpenSection(usize),
    /// Select this row of the list (a click on it).
    Select(usize),
    /// Move the list selection by this many rows (the mouse wheel).
    Scroll(isize),
    /// Right click at this cell: the options of what was there. `row`: a row
    /// of a list was clicked (else the video of the tab).
    ContextMenu {
        x: u16,
        y: u16,
        row: bool,
    },
    /// Do this line of the right-click menu.
    MenuPick(usize),
    /// The opening logo goes away (a key, a click or its time).
    EndSplash,
    /// The video fills the whole terminal (or screen, in window mode).
    ToggleFullscreen,
    Quit,
}

/// Side effects requested by `update`, performed by the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Look for mpv, yt-dlp and the optional helpers.
    CheckSystem,
    /// Fetch my YouTube watch history (to hide what I watched).
    LoadHistory,
    /// A page of my YouTube watch history, for "Historial": the first, or the next one.
    YoutubeHistory {
        id: u64,
        next: Option<String>,
    },
    /// Remember a video as watched.
    SaveWatched(String),
    /// Move these files to the system trash.
    Trash(Vec<PathBuf>),
    /// Fetch the top comments of a tab's video.
    Comments {
        tab: TabId,
        token: String,
    },
    /// Fetch the SponsorBlock segments of a tab's video.
    Segments {
        tab: TabId,
        video: String,
    },
    /// Fetch the details and related videos of a tab's video.
    Watch {
        tab: TabId,
        video: String,
    },
    Load {
        id: u64,
        source: Source,
        limit: u32,
    },
    ScanBrowsers {
        id: u64,
    },
    /// One page of a video search; `next` continues an earlier one.
    Search {
        id: u64,
        query: String,
        next: Option<String>,
    },
    /// List local media from the configured folders.
    ScanMedia {
        id: u64,
        kind: MediaKind,
    },
    /// (Re)read the login described by `config.session`.
    StartSession {
        force: bool,
    },
    SaveSetting {
        table: &'static str,
        key: &'static str,
        value: SettingValue,
    },
    ImportCookies(String),
    EditConfig,
    SaveLater(Vec<Video>),
    SaveHistory(Vec<HistoryEntry>),
    SaveSubs(HashSet<String>),
    /// Read my channels again with the current login (no browser).
    LoadSubs,
    Subscribe {
        channel_id: String,
        name: String,
        subscribe: bool,
    },
    DeletePlaylist {
        id: String,
        title: String,
    },
    Rate {
        video: String,
        title: String,
        rating: Rating,
    },
    Download {
        id: u64,
        video: Video,
    },
    /// Stop a running download.
    CancelDownload(u64),
    /// A desktop notification (best effort).
    Notify(String),
    SaveDownloads(Vec<Download>),
    Open(TabId, Playback),
    /// Restart a tab's player with a changed playback, at the second it was at.
    Reopen(TabId, Playback),
    Close(TabId),
    Command(TabId, PlayerCommand),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerCommand {
    TogglePause,
    Seek(i32),
    SeekTo(u32),
    Chapter(i32),
    Volume(i32),
    ToggleMute,
    CycleSubtitles,
    NextTrack,
    PrevTrack,
    /// Reload the video with other audio / subtitles, at the same second.
    Language(Language),
    /// Same, with another subtitle / dubbing language code.
    SubLang(&'static str),
    /// Same, with another maximum height.
    Quality(u32),
    /// Same, with another preferred codec.
    Codec(&'static str),
    /// Only meaningful for the mpv window.
    Fullscreen(bool),
}

/// What a tab plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playback {
    pub title: String,
    /// Channel or folder.
    pub subtitle: String,
    /// The YouTube video, which enables `l`/`s`/`d`/`c`; `None` for local files.
    pub video: Option<Video>,
    /// URLs or paths, played in order.
    pub queue: Vec<String>,
    /// Music: no video output, `.`/`,` move through the queue.
    pub audio_only: bool,
    /// Second to start at (resuming).
    pub start: Option<u32>,
}

impl Playback {
    pub fn youtube(video: Video) -> Self {
        Self {
            title: video.title.clone(),
            subtitle: video.channel.clone(),
            queue: vec![video.url()],
            video: Some(video),
            audio_only: false,
            start: None,
        }
    }

    /// `files[start]` and, for music, everything after it.
    pub fn local(files: &[LocalFile], start: usize, kind: MediaKind) -> Option<Self> {
        let first = files.get(start)?;
        let queue = match kind {
            MediaKind::Video => vec![first.path.display().to_string()],
            MediaKind::Music => files[start..].iter().map(|f| f.path.display().to_string()).collect(),
        };
        Some(Self {
            title: first.name.clone(),
            subtitle: first.folder.clone(),
            video: None,
            queue,
            audio_only: kind == MediaKind::Music,
            start: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub id: TabId,
    pub playback: Playback,
    /// Details and "up next" of its YouTube video, once fetched.
    pub watch: Option<Watch>,
    /// Selected row of `watch.related`.
    pub related: usize,
    /// The keys move in the related list instead of driving the player.
    pub on_related: bool,
    /// SponsorBlock segments of its video, and those already skipped.
    pub segments: Vec<Segment>,
    pub skipped: HashSet<usize>,
    /// Top comments of its video, once fetched; the first one shown.
    pub comments: Option<Vec<Comment>>,
    pub comments_from: usize,
    /// Its player is gone, but the details and "up next" stay; Enter plays again.
    pub ended: bool,
}

impl Tab {
    fn new(id: TabId, playback: Playback) -> Self {
        Self {
            id,
            playback,
            watch: None,
            related: 0,
            on_related: false,
            segments: Vec::new(),
            skipped: HashSet::new(),
            comments: None,
            comments_from: 0,
            ended: false,
        }
    }

    pub fn related_videos(&self) -> &[Video] {
        self.watch.as_ref().map_or(&[], |w| &w.related)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    None,
    /// Keep only videos from my channels.
    Mine,
    /// Drop videos from my channels ("Descubrir", "Descubrir canales").
    NotMine,
}

#[derive(Debug, Clone)]
struct Pending {
    id: u64,
    title: String,
    filter: Filter,
    push: bool,
    kind: ListKind,
    /// My favorite channels go first (Suscripciones, Mis canales).
    favorites_first: bool,
    /// "Mis recomendados".
    home: bool,
}

#[derive(Debug, Clone)]
pub struct State {
    pub config: Config,
    pub sidebar: bool,
    pub prefix: bool,
    pub help: bool,
    /// The active video fills the terminal.
    pub fullscreen: bool,
    /// The logo shown when the app opens (set by the runtime), until a key or a moment passes.
    pub splash: bool,
    pub focus: Focus,
    /// Selected entry of `Section::ALL`.
    pub menu: usize,
    pub prompt: Option<Prompt>,
    pub query: String,
    /// Lists on Inicio; the last one is shown.
    pub stack: Vec<Listing>,
    /// Title of the list being loaded.
    pub loading: Option<String>,
    /// "Ver después", newest first.
    pub later: Vec<Video>,
    /// "Historial", newest first.
    pub history: Vec<HistoryEntry>,
    /// What I watched elsewhere (YouTube's history), newest first: shown in
    /// "Historial" with `history`, never saved.
    pub youtube_history: Vec<HistoryEntry>,
    /// "Descargas", newest first.
    pub downloads: Vec<Download>,
    /// Videos of my channels seen by the background check (`None` until the first one).
    known_subs: Option<HashSet<String>>,
    /// New videos of my channels since "Suscripciones" was last opened…
    pub fresh: Vec<Video>,
    /// … and those, marked in the list while it is open.
    pub fresh_shown: HashSet<String>,
    /// Weight and length of the videos looked at, by id.
    pub sizes: HashMap<String, Size>,
    /// Videos I opened of each channel lately, and the channels I watch most.
    pub channel_views: HashMap<String, usize>,
    pub favorites: HashSet<String>,
    /// Unix time, kept current by the runtime (history dates).
    pub clock: i64,
    /// Ids of my channels.
    pub subs: HashSet<String>,
    /// How I rated the videos I opened or voted on.
    pub ratings: HashMap<String, Rating>,
    /// When each browser profile was last used, by spec.
    pub browsers_used: HashMap<String, i64>,
    pub confirm: Option<Confirm>,
    /// The right-click menu, while open.
    pub context: Option<ContextMenu>,
    /// Local files marked (space) for the next delete.
    pub marked: HashSet<PathBuf>,
    /// How "Mis videos" / "Mi música" are ordered.
    pub file_sort: FileSort,
    /// A newer version, shown in "Acerca de".
    pub update: Option<Update>,
    /// Videos I watched, here or on YouTube.
    pub watched: HashSet<String>,
    /// The last system check.
    pub system: Option<(Manager, Vec<Check>)>,
    /// Feeds show the watched videos too (`v`).
    pub show_watched: bool,
    pub tabs: Vec<Tab>,
    /// 0 = Inicio, n = `tabs[n - 1]`.
    pub active: usize,
    pub status: String,
    pub quit: bool,
    /// Videos play in their own window, so the video tab has room for its
    /// details and related videos (set by the runtime).
    pub video_window: bool,
    pending: Option<Pending>,
    /// A feed of my channels that came out empty while my channels may be an
    /// old copy: shown again once they are read again.
    recheck: Option<(Pending, Items, Option<String>)>,
    /// My channels were read again once already.
    subs_rechecked: bool,
    next_tab: u64,
    next_request: u64,
    next_download: u64,
}

impl State {
    pub fn new(config: Config, later: Vec<Video>) -> Self {
        Self {
            sidebar: config.ui.sidebar,
            show_watched: !config.ui.hide_watched,
            splash: false,
            focus: if config.ui.sidebar { Focus::Menu } else { Focus::List },
            config,
            prefix: false,
            help: false,
            fullscreen: false,
            menu: 0,
            prompt: None,
            query: String::new(),
            stack: Vec::new(),
            loading: None,
            later,
            history: Vec::new(),
            youtube_history: Vec::new(),
            downloads: Vec::new(),
            known_subs: None,
            fresh: Vec::new(),
            fresh_shown: HashSet::new(),
            sizes: HashMap::new(),
            channel_views: HashMap::new(),
            favorites: HashSet::new(),
            clock: 0,
            subs: HashSet::new(),
            ratings: HashMap::new(),
            browsers_used: HashMap::new(),
            confirm: None,
            context: None,
            marked: HashSet::new(),
            file_sort: FileSort::default(),
            update: None,
            watched: HashSet::new(),
            system: None,
            tabs: Vec::new(),
            active: 0,
            status: String::new(),
            quit: false,
            video_window: false,
            pending: None,
            recheck: None,
            subs_rechecked: false,
            next_tab: 0,
            next_request: 0,
            next_download: 0,
        }
    }

    /// The finished downloads of earlier sessions.
    pub fn set_downloads(&mut self, downloads: Vec<Download>) {
        self.next_download = downloads.iter().map(|d| d.id + 1).max().unwrap_or(0);
        self.downloads = downloads;
    }

    /// Downloads still running.
    pub fn downloading(&self) -> usize {
        self.downloads.iter().filter(|d| matches!(d.state, DownloadState::Running { .. })).count()
    }

    /// Interface language.
    pub fn lang(&self) -> Lang {
        self.config.ui.lang.unwrap_or_default()
    }

    /// `es` or `en`, in the interface language.
    fn tr(&self, es: &'static str, en: &'static str) -> &'static str {
        self.lang().pick(es, en)
    }

    /// The selected video of the shown list and the `n` after it.
    pub fn videos_from_selection(&self, n: usize) -> &[Video] {
        match self.stack.last() {
            Some(Listing { items: Items::Videos(videos), selected, .. }) if self.active == 0 => {
                let start = (*selected).min(videos.len());
                &videos[start..(start + 1 + n).min(videos.len())]
            }
            _ => &[],
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active.checked_sub(1).and_then(|i| self.tabs.get(i))
    }

    fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active.checked_sub(1).and_then(|i| self.tabs.get_mut(i))
    }

    /// A YouTube tab whose pane is free for the video's details and "Up next":
    /// the video plays in its own window, or there is no picture at all.
    pub fn details_shown(&self, tab: &Tab) -> bool {
        tab.playback.video.is_some() && (self.video_window || tab.playback.audio_only)
    }

    /// The active tab shows its related videos (details shown, list fetched).
    pub fn related_shown(&self) -> Option<&Tab> {
        self.active_tab().filter(|t| self.details_shown(t) && !t.related_videos().is_empty())
    }

    /// The related list of the active tab takes the keys.
    fn on_related(&self) -> bool {
        self.related_shown().is_some_and(|t| t.on_related)
    }

    pub fn listing(&self) -> Option<&Listing> {
        self.stack.last()
    }

    #[cfg(test)]
    fn listing_mut(&mut self) -> &mut Listing {
        self.stack.last_mut().expect("a list")
    }

    pub fn key_context(&self) -> KeyContext {
        KeyContext {
            prefix: self.prefix,
            help: self.help,
            confirm: self.confirm.is_some(),
            typing: self.prompt.is_some(),
            in_tab: self.active_tab().is_some(),
            related: self.on_related(),
            context_menu: self.context.is_some(),
            splash: self.splash,
        }
    }

    /// "brave+GNOMEKEYRING:Default", "cookies.txt importado" or "sin sesión".
    pub fn session_label(&self) -> String {
        let session = &self.config.session;
        if !session.cookies_file.is_empty() {
            self.tr("cookies.txt importado", "imported cookies.txt").into()
        } else if !session.browser.is_empty() {
            crate::session::label(&session.browser)
        } else {
            self.tr("sin sesión", "not logged in").into()
        }
    }

    fn logged_in(&self) -> bool {
        let session = &self.config.session;
        !session.cookies_file.is_empty() || !session.browser.is_empty()
    }

    /// Current value of a setting, for the settings screen.
    pub fn setting_value(&self, setting: Setting) -> String {
        let player = &self.config.player;
        let lang = self.lang();
        match setting {
            Setting::Browser | Setting::Logout => self.session_label(),
            Setting::ImportCookies | Setting::Refresh | Setting::EditFile => String::new(),
            Setting::Language => language_label(player.language, lang).into(),
            Setting::SubLang => language_name(&player.sub_lang, lang),
            Setting::Quality => format!("{}p", player.max_height),
            Setting::VideoMode => match player.mode {
                PlayerMode::Auto => self.tr("automático (integrado en kitty)", "automatic (embedded in kitty)").into(),
                PlayerMode::Embedded => self.tr("integrado", "embedded").into(),
                PlayerMode::Window => self.tr("ventana aparte", "separate window").into(),
                PlayerMode::Overlay => {
                    self.tr("superpuesto: ventana sobre la app (X11)", "overlay: a window over the app (X11)").into()
                }
            },
            Setting::BackgroundAudio => {
                if player.background_audio {
                    self.tr("sigue sonando", "keeps playing").into()
                } else {
                    self.tr("se pausa", "pauses").into()
                }
            }
            Setting::SponsorBlock => {
                let (es, en) = on_off(player.sponsorblock);
                self.tr(es, en).into()
            }
            Setting::AppLang => lang.name().into(),
            Setting::Icons => self.config.ui.icons.as_str().into(),
            Setting::Theme => self.config.ui.theme.name().into(),
            Setting::Border => match self.config.ui.border {
                Border::Rounded => self.tr("redondeados", "rounded"),
                Border::Plain => self.tr("rectos", "plain"),
                Border::Double => self.tr("dobles", "double"),
                Border::Thick => self.tr("gruesos", "thick"),
            }
            .into(),
            Setting::Thumbnails => {
                let (es, en) = on_off(self.config.ui.thumbnails);
                self.tr(es, en).into()
            }
            Setting::Mouse => {
                let (es, en) = on_off(self.config.ui.mouse);
                self.tr(es, en).into()
            }
            Setting::Splash => {
                let (es, en) = on_off(self.config.ui.splash);
                self.tr(es, en).into()
            }
            Setting::CheckUpdates => {
                let (es, en) = on_off(self.config.ui.check_updates);
                self.tr(es, en).into()
            }
            Setting::SystemCheck => match &self.system {
                None => String::new(),
                Some((_, checks)) if checks.iter().any(|c| c.tool.required() && !c.ok()) => {
                    self.tr("falta algo necesario", "something needed is missing").into()
                }
                Some((_, checks)) if checks.iter().all(Check::ok) => self.tr("todo instalado", "all installed").into(),
                Some(_) => self.tr("lo necesario está", "the essentials are there").into(),
            },
            Setting::HideWatched => {
                let (es, en) = on_off(self.config.ui.hide_watched);
                self.tr(es, en).into()
            }
            Setting::HideShorts => {
                let (es, en) = on_off(self.config.ui.hide_shorts);
                self.tr(es, en).into()
            }
            Setting::NotifyNew => {
                let (es, en) = on_off(self.config.ui.notify_new);
                self.tr(es, en).into()
            }
            Setting::FavoritesFirst => {
                let (es, en) = on_off(self.config.ui.favorites_first);
                self.tr(es, en).into()
            }
            Setting::FxStartup | Setting::FxPages | Setting::FxDialogs => {
                let mut effects = self.config.effects.clone();
                let on = effect_flag(&mut effects, setting).is_some_and(|(_, on)| *on);
                let (es, en) = on_off(on);
                self.tr(es, en).into()
            }
            Setting::VideoDirs => dirs_label(&self.config.media.video_dirs, "~/Videos", lang),
            Setting::MusicDirs => dirs_label(&self.config.media.music_dirs, "~/Música", lang),
            Setting::DownloadDir => match self.config.download.dir.as_str() {
                "" => "~/Videos".into(),
                dir => dir.into(),
            },
            Setting::DiscoverTopics => match self.config.search.topics.as_slice() {
                [] => self.tr("ninguno", "none").into(),
                topics => topics.join(", "),
            },
        }
    }

    pub fn update(&mut self, action: Action) -> Vec<Effect> {
        self.prefix = matches!(action, Action::Prefix);
        if let Some(effects) = self.in_context_menu(&action) {
            return effects;
        }
        let effects = self.apply(action);
        // Fullscreen belongs to a video tab; leaving it ends fullscreen.
        if self.active_tab().is_none() {
            self.fullscreen = false;
        }
        effects
    }

    fn apply(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::Prefix | Action::CancelPrefix => {}
            Action::ToggleSidebar => {
                self.sidebar = !self.sidebar;
                self.focus = if self.sidebar && self.stack.is_empty() { Focus::Menu } else { Focus::List };
            }
            Action::ToggleHelp => self.help = !self.help,
            Action::FocusMenu => {
                self.active = 0;
                self.prompt = None;
                self.sidebar = true;
                self.focus = Focus::Menu;
            }
            Action::AskBrowser => {
                let session = &self.config.session;
                if session.asked || !session.browser.is_empty() || !session.cookies_file.is_empty() {
                    return Vec::new();
                }
                let title =
                    self.tr("¿De qué navegador tomo tu sesión de YouTube?", "Which browser has your YouTube login?");
                let id = self.request(title.into(), Filter::None, false, ListKind::Plain);
                return vec![Effect::ScanBrowsers { id }];
            }
            Action::OpenSection(i) => {
                let Some(section) = Section::ALL.get(i) else { return Vec::new() };
                self.active = 0;
                self.prompt = None;
                self.menu = i;
                self.focus = Focus::Menu;
                return self.open_section(*section);
            }
            Action::Select(row) if self.related_shown().is_some() => {
                if let Some(tab) = self.active_tab_mut() {
                    tab.related = row.min(tab.related_videos().len().saturating_sub(1));
                    tab.on_related = true;
                }
            }
            Action::Select(row) => {
                let Some(listing) = self.stack.last_mut().filter(|_| self.active == 0) else { return Vec::new() };
                listing.selected = row.min(listing.items.len().saturating_sub(1));
                self.focus = Focus::List;
                return self.more_if_near_the_end();
            }
            Action::Scroll(rows) if self.related_shown().is_some() => {
                if let Some(tab) = self.active_tab_mut() {
                    tab.on_related = true;
                }
                return self.move_selection(rows);
            }
            Action::Scroll(rows) => {
                if self.active == 0 && !self.stack.is_empty() {
                    self.focus = Focus::List;
                    return self.move_selection(rows);
                }
            }
            Action::ToggleFocus if self.active_tab().is_some() => {
                if self.related_shown().is_some()
                    && let Some(tab) = self.active_tab_mut()
                {
                    tab.on_related = !tab.on_related;
                }
            }
            Action::ToggleFocus => {
                self.focus = match self.focus {
                    Focus::Menu if !self.stack.is_empty() => Focus::List,
                    Focus::List if self.sidebar => Focus::Menu,
                    focus => focus,
                };
            }
            Action::ContextMenu { x, y, row } => self.open_context_menu(x, y, row),
            Action::MenuPick(_) => {}
            Action::EndSplash => self.splash = false,
            Action::ToggleFullscreen => {
                self.fullscreen = !self.fullscreen && self.active_tab().is_some();
                return self.command(PlayerCommand::Fullscreen(self.fullscreen));
            }
            Action::Up => return self.move_selection(-1),
            Action::Down => return self.move_selection(1),
            Action::Enter => return self.enter(),
            Action::Back => self.back(),
            Action::FocusSearch => self.search_videos(),
            Action::CycleScope => self.cycle_scope(),
            Action::Input(c) => self.query.push(c),
            Action::Backspace => {
                self.query.pop();
            }
            Action::Loaded(id, result) => return self.loaded(id, result, None),
            Action::Page(id, result) => return self.page(id, result),
            Action::YoutubeHistory(id, result) => self.youtube_history_page(id, result),
            Action::SubsLoaded(subs) => self.subs = subs,
            Action::BrowsersUsed(used) => self.browsers_used = used,
            Action::Watched(id, result) => {
                // Without details the tab just stays as it is.
                if let (Some(tab), Ok(watch)) = (self.tabs.iter_mut().find(|t| t.id == id), result) {
                    if let (Some(rating), Some(video)) = (watch.rating, &tab.playback.video) {
                        self.ratings.insert(video.id.clone(), rating);
                    }
                    let token = watch.comments_token.clone();
                    tab.watch = Some(watch);
                    if let Some(token) = token.filter(|_| tab.comments.is_none()) {
                        return vec![Effect::Comments { tab: id, token }];
                    }
                }
            }
            Action::CommentsLoaded(id, result) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
                    // Failed: shown as none rather than loading forever.
                    tab.comments = Some(result.unwrap_or_default());
                }
            }
            Action::ScrollComments(delta) => {
                if let Some(tab) = self.active_tab_mut()
                    && let Some(comments) = &tab.comments
                {
                    let last = comments.len().saturating_sub(1);
                    tab.comments_from = tab.comments_from.saturating_add_signed(delta as isize).min(last);
                }
            }
            Action::SessionReady { subs, .. } => return self.session_ready(subs),
            Action::CookiesImported(Ok(path)) => {
                self.status = self
                    .tr("cookies.txt importado, leyendo tus canales…", "cookies.txt imported, reading your channels…")
                    .into();
                return self.set_session(String::new(), path);
            }
            Action::CookiesImported(Err(error)) | Action::ConfigReloaded(Err(error)) => self.status = error,
            Action::ConfigReloaded(Ok(config)) => {
                let session_changed = config.session != self.config.session;
                self.config = config;
                self.status = self.tr("Configuración recargada", "Settings reloaded").into();
                if session_changed {
                    return vec![Effect::StartSession { force: true }];
                }
            }
            Action::ToggleLater => return self.toggle_later(),
            Action::ToggleSubscription => return self.toggle_subscription(),
            Action::Confirm(yes) => match self.confirm.take().filter(|_| yes) {
                Some(Confirm::Unsubscribe { channel_id, name }) => {
                    self.status = format!("{} {name}…", self.tr("Desuscribiendo de", "Unsubscribing from"));
                    return vec![Effect::Subscribe { channel_id, name, subscribe: false }];
                }
                Some(Confirm::Trash(paths)) => {
                    self.status = format!("🗑 {} {}…", self.tr("A la papelera:", "To the trash:"), paths.len());
                    return vec![Effect::Trash(paths)];
                }
                Some(Confirm::Quit(_)) => self.quit = true,
                Some(Confirm::DeletePlaylist { id, title }) => {
                    self.status = format!("🗑 {} {title}…", self.tr("Borrando la lista", "Deleting the playlist"));
                    return vec![Effect::DeletePlaylist { id, title }];
                }
                None => {}
            },
            Action::SystemChecked(manager, checks) => self.system_checked(manager, checks),
            Action::HistoryLoaded(ids) => {
                self.watched.extend(ids);
                self.refilter_watched();
            }
            Action::ToggleHomeAll if self.listing().is_some_and(|l| l.home) => {
                self.config.ui.home_all = !self.config.ui.home_all;
                let mut effects = self.open_section(Section::Inicio);
                self.status = self.home_mode().into();
                effects.push(Effect::SaveSetting {
                    table: "ui",
                    key: "home_all",
                    value: SettingValue::Bool(self.config.ui.home_all),
                });
                return effects;
            }
            Action::ToggleHomeAll => {}
            Action::CycleSort => {
                if let Some(Listing { items: Items::Files(files, _), selected, .. }) = self.stack.last_mut()
                    && self.active == 0
                {
                    let current = files.get(*selected).map(|f| f.path.clone());
                    self.file_sort = self.file_sort.next();
                    self.file_sort.sort(files);
                    *selected = current.and_then(|p| files.iter().position(|f| f.path == p)).unwrap_or(0);
                    self.status = format!("{}: {}", self.tr("Orden", "Sorted by"), self.file_sort.label(self.lang()));
                }
            }
            Action::ToggleWatched => {
                self.show_watched = !self.show_watched;
                self.refilter_watched();
                self.status = if self.show_watched {
                    self.tr("Mostrando también los vistos (v los oculta)", "Showing watched videos too (v hides them)")
                } else {
                    self.tr("Vistos ocultos (v los muestra)", "Watched videos hidden (v shows them)")
                }
                .into();
                if !self.show_watched && self.logged_in() {
                    return vec![Effect::LoadHistory];
                }
            }
            Action::UpdateFound(update) => {
                let (available, see) = (self.tr("disponible", "available"), self.tr("mirá Acerca de", "see About"));
                self.status = format!("⬆ tubetui {} {available} · {see}", update.version);
                self.update = Some(update);
            }
            Action::ToggleMark => return self.toggle_mark(),
            Action::Trash if self.listing().is_some_and(|l| l.kind == ListKind::History) && self.active == 0 => {
                return self.forget_selected();
            }
            Action::Trash if self.listing().is_some_and(|l| l.kind == ListKind::Downloads) && self.active == 0 => {
                return self.cancel_or_forget_download();
            }
            Action::Trash if self.active == 0 && self.selected_playlist().is_some() => self.ask_delete_playlist(),
            Action::Trash => self.ask_trash(),
            Action::PlaylistDeleted { id, title, result } => self.playlist_deleted(id, title, result),
            Action::Trashed(result) => self.trashed(result),
            Action::SubscriptionDone { channel_id, name, result } => {
                return self.subscription_done(channel_id, name, result);
            }
            Action::Rate(rating) => return self.rate(rating),
            Action::Rated { video, title, rating, result } => self.rated(video, title, rating, result),
            Action::Download => return self.start_download(),
            Action::DownloadProgress { id, permille, speed, eta } => {
                if let Some(d) = self.downloads.iter_mut().find(|d| d.id == id)
                    && matches!(d.state, DownloadState::Running { .. })
                {
                    d.state = DownloadState::Running { permille, speed, eta };
                    self.sync_downloads();
                }
            }
            Action::Downloaded { id, result } => return self.downloaded(id, result),
            Action::SubsPolled(result) => return self.subs_polled(result),
            Action::Sized(id, result) => {
                // Failed: remembered as unknown, not asked again.
                self.sizes.insert(id, result.unwrap_or_default());
            }
            Action::OpenChannel => return self.open_channel(),
            Action::Listen => return self.listen(),
            Action::Status(text) => self.status = text,
            Action::NextTab => self.active = (self.active + 1) % (self.tabs.len() + 1),
            Action::PrevTab => self.active = self.active.checked_sub(1).unwrap_or(self.tabs.len()),
            Action::GoTab(n) if n <= self.tabs.len() => self.active = n,
            Action::GoTab(_) => {}
            Action::CloseTab => {
                if let Some(id) = self.active_tab().map(|t| t.id) {
                    self.remove_tab(id);
                    return vec![Effect::Close(id)];
                }
            }
            Action::PlayerClosed(id, error) => {
                let keep = error.is_none() && self.tabs.iter().any(|t| t.id == id && self.details_shown(t));
                if !keep {
                    self.remove_tab(id);
                    if let Some(error) = error {
                        self.status = error;
                    }
                    return Vec::new();
                }
                let tab = self.tabs.iter_mut().find(|t| t.id == id).expect("kept");
                tab.ended = true;
                tab.on_related = !tab.related_videos().is_empty();
                self.fullscreen = false;
                self.status =
                    self.tr("⏹ Video cerrado · Enter lo vuelve a ver", "⏹ Video closed · Enter plays it again").into();
            }
            Action::Stopped { video, position, duration } => {
                let Some(entry) = self.history.iter_mut().find(|e| e.video.id == video) else { return Vec::new() };
                let point = position.and_then(|p| resume_point(p.into(), duration.map(f64::from)));
                if entry.position == point {
                    return Vec::new();
                }
                entry.position = point;
                self.sync_history();
                return vec![Effect::SaveHistory(self.history.clone())];
            }
            Action::TogglePause if self.active_tab().is_some_and(|t| t.ended) => return self.replay(),
            Action::TogglePause => return self.command(PlayerCommand::TogglePause),
            Action::Seek(seconds) => return self.command(PlayerCommand::Seek(seconds)),
            Action::SeekTo(second) => return self.command(PlayerCommand::SeekTo(second)),
            Action::Chapter(delta) => return self.command(PlayerCommand::Chapter(delta)),
            Action::SegmentsLoaded(id, segments) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
                    tab.segments = segments;
                }
            }
            Action::SkipSegment(id, index) => {
                let lang = self.lang();
                let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) else { return Vec::new() };
                let Some(segment) = tab.segments.get(index) else { return Vec::new() };
                if !tab.skipped.insert(index) {
                    return Vec::new();
                }
                let end = segment.end.div_ceil(1000) as u32;
                self.status = format!("⏩ {}: {}", lang.pick("Salteado", "Skipped"), segment.label(lang));
                return vec![Effect::Command(id, PlayerCommand::SeekTo(end))];
            }
            Action::Volume(delta) => return self.command(PlayerCommand::Volume(delta)),
            Action::ToggleMute => return self.command(PlayerCommand::ToggleMute),
            Action::CycleSubtitles => return self.command(PlayerCommand::CycleSubtitles),
            Action::NextTrack => return self.command(PlayerCommand::NextTrack),
            Action::PrevTrack => return self.command(PlayerCommand::PrevTrack),
            Action::CycleLanguage => {
                let player = &mut self.config.player;
                player.language = player.language.next();
                let language = player.language;
                let lang = self.lang();
                self.status = format!(
                    "{}: {} ({})",
                    lang.pick("Idioma", "Language"),
                    language_label(language, lang),
                    lang.pick("recargando el video…", "reloading the video…")
                );
                let mut effects = vec![Effect::SaveSetting {
                    table: "player",
                    key: "language",
                    value: SettingValue::Text(language.as_str().into()),
                }];
                effects.extend(self.command(PlayerCommand::Language(language)));
                return effects;
            }
            Action::CycleSubLang => {
                let code = next_language(&self.config.player.sub_lang);
                self.config.player.sub_lang = code.into();
                let lang = self.lang();
                self.status = format!(
                    "{} {} ({})",
                    lang.pick("Subtítulos y doblaje en", "Subtitles and dubbing in"),
                    language_name(code, lang),
                    lang.pick("recargando el video…", "reloading the video…")
                );
                let mut effects = vec![Effect::SaveSetting {
                    table: "player",
                    key: "sub_lang",
                    value: SettingValue::Text(code.into()),
                }];
                effects.extend(self.command(PlayerCommand::SubLang(code)));
                return effects;
            }
            Action::CycleQuality => {
                let height = next_quality(self.config.player.max_height);
                self.config.player.max_height = height;
                let lang = self.lang();
                self.status = format!(
                    "{}: {height}p ({})",
                    lang.pick("Calidad máxima", "Maximum quality"),
                    lang.pick("recargando el video…", "reloading the video…")
                );
                let mut effects = vec![Effect::SaveSetting {
                    table: "player",
                    key: "max_height",
                    value: SettingValue::Number(height.into()),
                }];
                effects.extend(self.command(PlayerCommand::Quality(height)));
                return effects;
            }
            Action::CycleCodec => {
                let codec = next_codec(&self.config.player.prefer_codec);
                self.config.player.prefer_codec = codec.into();
                let lang = self.lang();
                self.status = format!(
                    "{}: {} ({})",
                    lang.pick("Códec preferido", "Preferred codec"),
                    codec_name(codec, lang),
                    lang.pick("recargando el video…", "reloading the video…")
                );
                let mut effects = vec![Effect::SaveSetting {
                    table: "player",
                    key: "prefer_codec",
                    value: SettingValue::Text(codec.into()),
                }];
                effects.extend(self.command(PlayerCommand::Codec(codec)));
                return effects;
            }
            Action::ToggleUiLang => return self.toggle_ui_lang(),
            Action::Quit if self.downloading() > 0 && !matches!(self.confirm, Some(Confirm::Quit(_))) => {
                self.confirm = Some(Confirm::Quit(self.downloading()));
            }
            Action::Quit => self.quit = true,
        }
        Vec::new()
    }

    fn move_selection(&mut self, delta: isize) -> Vec<Effect> {
        let step = |value: usize, len: usize| value.saturating_add_signed(delta).min(len.saturating_sub(1));
        if self.on_related() {
            if let Some(tab) = self.active_tab_mut() {
                tab.related = step(tab.related, tab.related_videos().len());
            }
            return Vec::new();
        }
        match self.focus {
            // One step past an end goes round to the other end; jumps stop there.
            Focus::Menu if delta.abs() == 1 => {
                let len = Section::ALL.len() as isize;
                self.menu = (self.menu as isize + delta).rem_euclid(len) as usize;
            }
            Focus::Menu => self.menu = step(self.menu, Section::ALL.len()),
            Focus::List => {
                if let Some(l) = self.stack.last_mut() {
                    l.selected = step(l.selected, l.items.len());
                }
                return self.more_if_near_the_end();
            }
        }
        Vec::new()
    }

    /// Asks for the next page of a search before the selection reaches its end.
    fn more_if_near_the_end(&mut self) -> Vec<Effect> {
        let id = self.next_request;
        let Some(listing) = self.stack.last_mut() else { return Vec::new() };
        if listing.more.is_some() || listing.selected + PREFETCH_ROWS < listing.items.len() {
            return Vec::new();
        }
        let Some(next) = listing.next.clone() else { return Vec::new() };
        listing.more = Some(id);
        self.next_request += 1;
        if listing.kind == ListKind::History {
            return vec![Effect::YoutubeHistory { id, next: Some(next) }];
        }
        vec![Effect::Search { id, query: String::new(), next: Some(next) }]
    }

    fn enter(&mut self) -> Vec<Effect> {
        if let Some(prompt) = self.prompt.clone() {
            return self.submit(prompt);
        }
        if self.on_related() {
            let video = self.active_tab().and_then(|t| t.related_videos().get(t.related)).cloned();
            return video.map(|v| self.play(Playback::youtube(v))).unwrap_or_default();
        }
        if self.active_tab().is_some() {
            return self.replay();
        }
        if self.focus == Focus::Menu {
            return self.open_section(Section::ALL[self.menu]);
        }
        let Some(listing) = self.stack.last() else { return Vec::new() };
        let selected = listing.selected;
        match &listing.items {
            Items::Videos(videos) => match videos.get(selected).cloned() {
                Some(video) => self.play(Playback::youtube(video)),
                None => Vec::new(),
            },
            Items::Files(files, kind) => match Playback::local(files, selected, *kind) {
                Some(playback) => self.play(playback),
                None => Vec::new(),
            },
            Items::Channels(channels) => match channels.get(selected).cloned() {
                Some(c) => self.open_channel_list(c.name, c.url),
                None => Vec::new(),
            },
            Items::Playlists(playlists) => match playlists.get(selected).cloned() {
                Some(p) => {
                    let title = p.name(self.lang()).to_owned();
                    self.load(Source::Playlist(p.id), title, Filter::None, true, ListKind::Plain)
                }
                None => Vec::new(),
            },
            Items::Profiles(profiles) => match profiles.get(selected).cloned() {
                Some(spec) => {
                    self.config.session.asked = true;
                    let mut effects =
                        vec![Effect::SaveSetting { table: "session", key: "asked", value: SettingValue::Bool(true) }];
                    self.status = if spec.is_empty() {
                        self.tr("Cerrando sesión…", "Logging out…").into()
                    } else {
                        let label = crate::session::label(&spec);
                        let reading = self.tr("Leyendo la sesión de", "Reading the login of");
                        if cfg!(target_os = "linux") && !spec.starts_with("firefox") {
                            // Chromium's cookie key is in the keyring, which may ask to be unlocked.
                            let note = self
                                .tr("puede pedir la contraseña del llavero", "the keyring may ask for its password");
                            format!("{reading} {label}… ({note})")
                        } else {
                            format!("{reading} {label}…")
                        }
                    };
                    effects.extend(self.set_session(spec, String::new()));
                    effects
                }
                None => Vec::new(),
            },
            Items::Downloads(downloads) => match downloads.get(selected).map(|d| &d.state) {
                Some(DownloadState::Done(path)) => {
                    let file = LocalFile {
                        name: path.file_stem().map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                        folder: path.parent().map_or_else(String::new, |p| p.display().to_string()),
                        size: std::fs::metadata(path).map_or(0, |m| m.len()),
                        path: path.clone(),
                    };
                    Playback::local(&[file], 0, MediaKind::Video).map(|p| self.play(p)).unwrap_or_default()
                }
                _ => Vec::new(),
            },
            Items::Settings => match Setting::ALL.get(selected) {
                Some(setting) => self.change_setting(*setting),
                None => Vec::new(),
            },
            Items::About => Vec::new(),
            // Check again, after installing something.
            Items::System => vec![Effect::CheckSystem],
        }
    }

    fn open_section(&mut self, section: Section) -> Vec<Effect> {
        let title = section.label(self.lang()).to_owned();
        match section {
            // YouTube's home feed, only my channels; "Descubrir canales" has the rest.
            Section::Inicio if !self.logged_in() => {
                let hint = self
                    .tr("Sin sesión: elegí un navegador en Configuración", "Not logged in: pick a browser in Settings");
                self.status = hint.into();
                Vec::new()
            }
            Section::Inicio => {
                let filter = if self.config.ui.home_all { Filter::None } else { Filter::Mine };
                let mut effects = self.load_feed(Source::Recommended, title, filter);
                if let Some(pending) = &mut self.pending {
                    pending.home = true;
                }
                // As many as for my channels only, so "all" is never shorter.
                for effect in &mut effects {
                    if let Effect::Load { limit, .. } = effect {
                        *limit = FILTERED_LIMIT;
                    }
                }
                effects
            }
            Section::Descubrir => self.discover(),
            Section::ParaVos => self.up_next(title),
            Section::DescubrirCanales => self.load_feed(Source::Recommended, title, Filter::NotMine),
            Section::Suscripciones => {
                self.fresh_shown = self.fresh.drain(..).map(|v| v.id).collect();
                self.load_feed(Source::Subscriptions, title, Filter::None)
            }
            Section::Canales => self.load(Source::MyChannels, title, Filter::None, false, ListKind::MyChannels),
            Section::MisListas if !self.logged_in() => {
                let hint = self
                    .tr("Sin sesión: elegí un navegador en Configuración", "Not logged in: pick a browser in Settings");
                self.status = hint.into();
                Vec::new()
            }
            Section::MisListas => self.load(Source::MyPlaylists, title, Filter::None, false, ListKind::Plain),
            Section::VerDespues => {
                let items = Items::Videos(self.later.clone());
                self.show(Listing::new(title, items, ListKind::Later));
                Vec::new()
            }
            Section::Historial => {
                let items = Items::Videos(self.history_entries().into_iter().map(|e| e.video).collect());
                let mut listing = Listing::new(title, items, ListKind::History);
                let effects = if self.logged_in() {
                    let id = self.next_request;
                    self.next_request += 1;
                    listing.more = Some(id);
                    self.status = self.tr("Trayendo tu historial de YouTube…", "Fetching your YouTube history…").into();
                    vec![Effect::YoutubeHistory { id, next: None }]
                } else {
                    if self.history.is_empty() {
                        self.status =
                            self.tr("Todavía no viste nada en tubetui", "Nothing watched in tubetui yet").into();
                    }
                    Vec::new()
                };
                self.show(listing);
                effects
            }
            Section::BuscarVideos => {
                self.search_videos();
                Vec::new()
            }
            Section::BuscarCanales => {
                self.start_prompt(Prompt::Channels, String::new());
                Vec::new()
            }
            Section::Descargas => {
                let items = Items::Downloads(self.downloads.clone());
                self.show(Listing::new(title, items, ListKind::Downloads));
                if self.downloads.is_empty() {
                    self.status = self
                        .tr(
                            "Sin descargas: d descarga el video elegido",
                            "No downloads: d downloads the selected video",
                        )
                        .into();
                }
                Vec::new()
            }
            Section::MisVideos => self.scan_media(title, MediaKind::Video),
            Section::MiMusica => self.scan_media(title, MediaKind::Music),
            Section::Configuracion => {
                self.show(Listing::new(title, Items::Settings, ListKind::Settings));
                Vec::new()
            }
            Section::AcercaDe => {
                self.show(Listing::new(title, Items::About, ListKind::Plain));
                Vec::new()
            }
            Section::Salir => self.apply(Action::Quit),
        }
    }

    fn scan_media(&mut self, title: String, kind: MediaKind) -> Vec<Effect> {
        let id = self.request(title, Filter::None, false, ListKind::Plain);
        vec![Effect::ScanMedia { id, kind }]
    }

    fn show(&mut self, listing: Listing) {
        self.stack = vec![listing];
        self.focus = Focus::List;
    }

    fn change_setting(&mut self, setting: Setting) -> Vec<Effect> {
        let save = |table, key, value| vec![Effect::SaveSetting { table, key, value }];
        let player = &mut self.config.player;
        match setting {
            Setting::Browser => {
                let id = self.request(self.tr("Navegadores", "Browsers").into(), Filter::None, true, ListKind::Plain);
                vec![Effect::ScanBrowsers { id }]
            }
            Setting::ImportCookies => {
                self.start_prompt(Prompt::CookiesFile, String::new());
                Vec::new()
            }
            Setting::Refresh => {
                self.status = format!(
                    "{} ({})…",
                    self.tr("Releyendo la sesión", "Reading the login again"),
                    self.session_label()
                );
                vec![Effect::StartSession { force: true }]
            }
            Setting::Logout => {
                self.status = self.tr("Sesión cerrada", "Logged out").into();
                self.set_session(String::new(), String::new())
            }
            Setting::Language => {
                player.language = player.language.next();
                save("player", "language", SettingValue::Text(player.language.as_str().into()))
            }
            Setting::SubLang => {
                player.sub_lang = next_language(&player.sub_lang).into();
                self.status = self.tr("Se aplica a los próximos videos", "Applies to the next videos").into();
                save("player", "sub_lang", SettingValue::Text(self.config.player.sub_lang.clone()))
            }
            Setting::Quality => {
                player.max_height = next_quality(player.max_height);
                save("player", "max_height", SettingValue::Number(player.max_height.into()))
            }
            Setting::VideoMode => {
                player.mode = player.mode.next();
                self.status = self.tr("Se aplica a los próximos videos", "Applies to the next videos").into();
                save("player", "mode", SettingValue::Text(self.config.player.mode.as_str().into()))
            }
            Setting::BackgroundAudio => {
                player.background_audio = !player.background_audio;
                save("player", "background_audio", SettingValue::Bool(player.background_audio))
            }
            Setting::SponsorBlock => {
                player.sponsorblock = !player.sponsorblock;
                self.status = self.tr("Se aplica a los próximos videos", "Applies to the next videos").into();
                save("player", "sponsorblock", SettingValue::Bool(self.config.player.sponsorblock))
            }
            Setting::AppLang => self.toggle_ui_lang(),
            Setting::Icons => {
                self.config.ui.icons = self.config.ui.icons.next();
                save("ui", "icons", SettingValue::Text(self.config.ui.icons.as_str().into()))
            }
            Setting::Theme => {
                self.config.ui.theme = self.config.ui.theme.next();
                save("ui", "theme", SettingValue::Text(self.config.ui.theme.as_str().into()))
            }
            Setting::Border => {
                self.config.ui.border = self.config.ui.border.next();
                save("ui", "border", SettingValue::Text(self.config.ui.border.as_str().into()))
            }
            Setting::Thumbnails => {
                self.config.ui.thumbnails = !self.config.ui.thumbnails;
                save("ui", "thumbnails", SettingValue::Bool(self.config.ui.thumbnails))
            }
            Setting::Mouse => {
                self.config.ui.mouse = !self.config.ui.mouse;
                save("ui", "mouse", SettingValue::Bool(self.config.ui.mouse))
            }
            Setting::SystemCheck => {
                self.show_system();
                vec![Effect::CheckSystem]
            }
            Setting::HideWatched => {
                self.config.ui.hide_watched = !self.config.ui.hide_watched;
                self.show_watched = !self.config.ui.hide_watched;
                self.refilter_watched();
                save("ui", "hide_watched", SettingValue::Bool(self.config.ui.hide_watched))
            }
            Setting::Splash => {
                self.config.ui.splash = !self.config.ui.splash;
                save("ui", "splash", SettingValue::Bool(self.config.ui.splash))
            }
            Setting::NotifyNew => {
                self.config.ui.notify_new = !self.config.ui.notify_new;
                save("ui", "notify_new", SettingValue::Bool(self.config.ui.notify_new))
            }
            Setting::FavoritesFirst => {
                self.config.ui.favorites_first = !self.config.ui.favorites_first;
                self.refresh_favorites();
                save("ui", "favorites_first", SettingValue::Bool(self.config.ui.favorites_first))
            }
            Setting::HideShorts => {
                self.config.ui.hide_shorts = !self.config.ui.hide_shorts;
                self.status = self.tr("Se aplica a las próximas listas", "Applies to the next lists").into();
                save("ui", "hide_shorts", SettingValue::Bool(self.config.ui.hide_shorts))
            }
            Setting::CheckUpdates => {
                self.config.ui.check_updates = !self.config.ui.check_updates;
                save("ui", "check_updates", SettingValue::Bool(self.config.ui.check_updates))
            }
            Setting::FxStartup | Setting::FxPages | Setting::FxDialogs => {
                match effect_flag(&mut self.config.effects, setting) {
                    Some((key, on)) => {
                        *on = !*on;
                        save("effects", key, SettingValue::Bool(*on))
                    }
                    None => Vec::new(),
                }
            }
            Setting::VideoDirs => {
                let current = self.config.media.video_dirs.join(", ");
                self.start_prompt(Prompt::MediaDirs(MediaKind::Video), current);
                Vec::new()
            }
            Setting::MusicDirs => {
                let current = self.config.media.music_dirs.join(", ");
                self.start_prompt(Prompt::MediaDirs(MediaKind::Music), current);
                Vec::new()
            }
            Setting::DownloadDir => {
                let current = self.config.download.dir.clone();
                self.start_prompt(Prompt::DownloadDir, current);
                Vec::new()
            }
            Setting::DiscoverTopics => {
                let current = self.config.search.topics.join(", ");
                self.start_prompt(Prompt::Topics, current);
                Vec::new()
            }
            Setting::EditFile => vec![Effect::EditConfig],
        }
    }

    /// This week's videos on my topics, out of my channels; without topics,
    /// asks for them first.
    fn discover(&mut self) -> Vec<Effect> {
        let topics = self.config.search.topics.clone();
        if topics.is_empty() {
            self.status = self
                .tr(
                    "Escribí qué te interesa descubrir, separado por coma: rust, nba, jazz",
                    "Type what you'd like to discover, comma separated: rust, nba, jazz",
                )
                .into();
            self.start_prompt(Prompt::Topics, String::new());
            return Vec::new();
        }
        let title = Section::Descubrir.label(self.lang()).to_owned();
        self.load_feed(Source::Discover(topics), title, Filter::NotMine)
    }

    /// "Up next" of the last videos I watched, all together.
    fn up_next(&mut self, title: String) -> Vec<Effect> {
        let ids: Vec<String> = self.history.iter().take(UP_NEXT_FROM).map(|e| e.video.id.clone()).collect();
        if ids.is_empty() {
            self.status = self
                .tr("Mirá algunos videos y acá aparece qué sigue", "Watch a few videos and what's next shows up here")
                .into();
            return Vec::new();
        }
        self.load_feed(Source::UpNext(ids), title, Filter::None)
    }

    /// Switches the login to a browser profile or an imported file (both empty = none).
    fn set_session(&mut self, browser: String, cookies_file: String) -> Vec<Effect> {
        self.config.session.browser = browser.clone();
        self.config.session.cookies_file = cookies_file.clone();
        vec![
            Effect::SaveSetting { table: "session", key: "browser", value: SettingValue::Text(browser) },
            Effect::SaveSetting { table: "session", key: "cookies_file", value: SettingValue::Text(cookies_file) },
            Effect::StartSession { force: true },
        ]
    }

    /// Opens the search box; inside a channel the search starts scoped to it.
    fn search_videos(&mut self) {
        let channel = match self.stack.last() {
            Some(Listing { kind: ListKind::Channel(c), .. }) if self.active == 0 => Some(c.clone()),
            _ => None,
        };
        let scope = channel.clone().map_or(Scope::All, Scope::Channel);
        self.start_prompt(Prompt::Videos { scope, channel }, String::new());
    }

    fn cycle_scope(&mut self) {
        if let Some(Prompt::Videos { scope, channel }) = &mut self.prompt {
            *scope = match (&*scope, channel) {
                (Scope::All, _) => Scope::Mine,
                (Scope::Mine, Some(c)) => Scope::Channel(c.clone()),
                (Scope::Mine, None) | (Scope::Channel(_), _) => Scope::All,
            };
        }
    }

    fn start_prompt(&mut self, prompt: Prompt, query: String) {
        self.active = 0;
        self.prompt = Some(prompt);
        self.query = query;
    }

    fn submit(&mut self, prompt: Prompt) -> Vec<Effect> {
        let query = self.query.trim().to_owned();
        if query.is_empty() && !matches!(prompt, Prompt::DownloadDir | Prompt::MediaDirs(_) | Prompt::Topics) {
            return Vec::new();
        }
        self.prompt = None;
        match prompt {
            Prompt::Videos { scope: Scope::All, .. } => {
                let id = self.request(query.clone(), Filter::None, false, ListKind::Plain);
                vec![Effect::Search { id, query, next: None }]
            }
            Prompt::Videos { scope: Scope::Mine, .. } => {
                let title = format!("{query} · {}", self.tr("mis canales", "my channels"));
                self.load(Source::Search(query), title, Filter::Mine, false, ListKind::Plain)
            }
            Prompt::Videos { scope: Scope::Channel(c), .. } => {
                let title = format!("«{query}»");
                let source = Source::SearchInChannel { url: c.url, query };
                self.load(source, title, Filter::None, true, ListKind::Plain)
            }
            Prompt::Channels => {
                self.load(Source::ChannelSearch(query.clone()), query, Filter::None, false, ListKind::Plain)
            }
            Prompt::CookiesFile => {
                self.status = self.tr("Importando cookies.txt…", "Importing cookies.txt…").into();
                vec![Effect::ImportCookies(query)]
            }
            Prompt::DownloadDir => {
                self.config.download.dir = query.clone();
                vec![Effect::SaveSetting { table: "download", key: "dir", value: SettingValue::Text(query) }]
            }
            Prompt::MediaDirs(kind) => {
                let dirs = parse_dirs(&query);
                let key = match kind {
                    MediaKind::Video => {
                        self.config.media.video_dirs = dirs.clone();
                        "video_dirs"
                    }
                    MediaKind::Music => {
                        self.config.media.music_dirs = dirs.clone();
                        "music_dirs"
                    }
                };
                vec![Effect::SaveSetting { table: "media", key, value: SettingValue::List(dirs) }]
            }
            Prompt::Topics => {
                let topics = parse_dirs(&query);
                self.config.search.topics = topics.clone();
                let mut effects =
                    vec![Effect::SaveSetting { table: "search", key: "topics", value: SettingValue::List(topics) }];
                if !self.config.search.topics.is_empty() {
                    effects.extend(self.discover());
                }
                effects
            }
        }
    }

    fn load(&mut self, source: Source, title: String, filter: Filter, push: bool, kind: ListKind) -> Vec<Effect> {
        let limit = if filter == Filter::None { self.config.search.results } else { FILTERED_LIMIT };
        let id = self.request(title, filter, push, kind);
        if let Some(pending) = &mut self.pending {
            pending.favorites_first = matches!(source, Source::Subscriptions | Source::MyChannels);
        }
        vec![Effect::Load { id, source, limit }]
    }

    /// Registers a pending list; only the latest request is shown when it lands.
    fn request(&mut self, title: String, filter: Filter, push: bool, kind: ListKind) -> u64 {
        let id = self.next_request;
        self.next_request += 1;
        self.status = format!("{} {title}…", self.tr("Cargando", "Loading"));
        self.loading = Some(title.clone());
        self.pending = Some(Pending { id, title, filter, push, kind, favorites_first: false, home: false });
        id
    }

    /// A search page: the first one of a new list, or the next one of a shown list.
    fn page(&mut self, id: u64, result: Result<SearchPage, String>) -> Vec<Effect> {
        if self.pending.as_ref().is_some_and(|p| p.id == id) {
            let next = result.as_ref().ok().and_then(|p| p.next.clone());
            return self.loaded(id, result.map(|p| Items::Videos(p.videos)), next);
        }
        let Some(listing) = self.stack.iter_mut().find(|l| l.more == Some(id)) else { return Vec::new() };
        listing.more = None;
        match result {
            Ok(page) => {
                let hide_shorts = self.config.ui.hide_shorts;
                if let Items::Videos(videos) = &mut listing.items {
                    let seen: HashSet<String> = videos.iter().map(|v| v.id.clone()).collect();
                    let unwanted = |v: &Video| seen.contains(&v.id) || (hide_shorts && v.short);
                    videos.extend(page.videos.into_iter().filter(|v| !unwanted(v)));
                }
                listing.next = page.next;
                self.status = format!("{} · {}", listing.title, listing.items.len());
            }
            Err(error) => self.status = error,
        }
        Vec::new()
    }

    /// A page of my YouTube history: the first replaces what came before, the
    /// next ones add to it.
    fn youtube_history_page(&mut self, id: u64, result: Result<HistoryPage, String>) {
        let Some(listing) = self.stack.iter_mut().find(|l| l.more == Some(id)) else { return };
        let first = listing.next.is_none();
        listing.more = None;
        match result {
            Ok(page) => {
                listing.next = page.next;
                if first {
                    self.youtube_history.clear();
                }
                self.watched.extend(page.videos.iter().map(|(v, _)| v.id.clone()));
                let entries = page.videos.into_iter().map(|(video, at)| HistoryEntry { video, at, position: None });
                self.youtube_history.extend(entries);
                self.sync_history();
                self.refilter_watched();
                let count = self.listing().map_or(0, |l| l.items.len());
                self.status = format!("{} · {count}", self.tr("Historial", "History"));
            }
            Err(error) => self.status = error,
        }
    }

    fn loaded(&mut self, id: u64, result: Result<Items, String>, next: Option<String>) -> Vec<Effect> {
        let Some(pending) = self.pending.take_if(|p| p.id == id) else {
            return Vec::new(); // superseded by a newer request
        };
        self.loading = None;
        let mut items = match result {
            Ok(items) => items,
            Err(error) => {
                self.status = error;
                return Vec::new();
            }
        };
        // None of my channels in a whole feed: my channels are likely an old
        // copy of a stale login. Read them again once and filter again.
        if pending.filter == Filter::Mine
            && pending.kind == ListKind::Feed
            && !items.is_empty()
            && !self.subs_rechecked
            && self.logged_in()
            && let Items::Videos(videos) = &items
            && !videos.iter().any(|v| v.channel_id.as_ref().is_some_and(|c| self.subs.contains(c)))
        {
            self.subs_rechecked = true;
            self.loading = Some(pending.title.clone());
            self.status = self.tr("Releyendo tus canales…", "Reading your channels again…").into();
            self.recheck = Some((pending, items, next));
            return vec![Effect::LoadSubs];
        }
        if let Items::Videos(videos) = &mut items {
            if self.config.ui.hide_shorts {
                videos.retain(|v| !v.short);
            }
            let mine = |v: &Video| v.channel_id.as_ref().is_some_and(|c| self.subs.contains(c));
            match pending.filter {
                Filter::None => {}
                Filter::Mine => videos.retain(mine),
                Filter::NotMine => videos.retain(|v| v.channel_id.is_some() && !mine(v)),
            }
            if pending.favorites_first {
                self.favorites_first(videos);
            }
        }
        if let Items::Channels(channels) = &mut items
            && pending.favorites_first
            && self.config.ui.favorites_first
        {
            // Most watched first; the rest keep their order.
            channels.sort_by_key(|c| std::cmp::Reverse(self.channel_views.get(&c.id).copied().unwrap_or(0)));
        }
        if let Items::Files(files, _) = &mut items {
            self.file_sort.sort(files);
        }
        self.status = if items.is_empty() {
            self.tr("Sin resultados", "No results").into()
        } else if let Items::Profiles(profiles) = &items {
            if profiles.iter().all(String::is_empty) {
                self.tr(
                    "No encontré navegadores: iniciá sesión en youtube.com con tu navegador, navegá un poco y \
                     volvé a Configuración → Sesión desde un navegador; o importá un cookies.txt",
                    "No browsers found: log in to youtube.com in your browser, browse a little and come back \
                     to Settings → Log in with a browser; or import a cookies.txt",
                )
                .into()
            } else {
                self.tr(
                    "Elegí con Enter el navegador donde entrás a YouTube",
                    "Press Enter on the browser you use YouTube in",
                )
                .into()
            }
        } else {
            format!("{} · {}", pending.title, items.len())
        };
        let mut listing = Listing { next, home: pending.home, ..Listing::new(pending.title, items, pending.kind) };
        if listing.kind == ListKind::Feed
            && let Items::Videos(videos) = &listing.items
        {
            listing.all = Some(videos.clone());
            let hidden = filter_watched(&mut listing, &self.watched, self.show_watched);
            if hidden > 0 {
                let note = self.tr("vistos ocultos (v los muestra)", "watched hidden (v shows them)");
                self.status = format!("{} · {} · {hidden} {note}", listing.title, listing.items.len());
            }
        }
        if listing.home {
            self.status = format!("{} · {}", self.status, self.home_mode());
        }
        if pending.push {
            self.stack.push(listing);
            self.focus = Focus::List;
        } else {
            self.show(listing);
        }
        Vec::new()
    }

    fn show_system(&mut self) {
        let title = self.tr("Revisión del sistema", "System check").to_owned();
        if self.stack.last().is_none_or(|l| l.items != Items::System) {
            self.active = 0;
            self.stack.push(Listing::new(title, Items::System, ListKind::Plain));
            self.focus = Focus::List;
        }
    }

    /// Something needed missing (or yt-dlp too old): the check opens by
    /// itself with what to install; missing extras only get a line.
    fn system_checked(&mut self, manager: Manager, checks: Vec<Check>) {
        let lacking: Vec<&str> =
            checks.iter().filter(|c| c.tool.required() && !c.ok()).map(|c| c.tool.program()).collect();
        let extras = checks.iter().filter(|c| !c.tool.required() && !c.ok()).count();
        let on_screen = self.stack.last().is_some_and(|l| l.items == Items::System);
        if !lacking.is_empty() {
            self.status =
                format!("⚠ {}: {}", self.tr("Falta o hay que actualizar", "Missing or outdated"), lacking.join(", "));
            self.show_system();
        } else if !on_screen && extras > 0 {
            let note = self.tr(
                "opcionales sin instalar · Configuración → Revisar el sistema",
                "optional tools not installed · Settings → Check the system",
            );
            self.status = format!("{extras} {note}");
        } else if on_screen {
            self.status = self.tr("Sistema revisado", "System checked").into();
        }
        self.system = Some((manager, checks));
    }

    /// A feed, and my YouTube history to hide what I already watched.
    fn load_feed(&mut self, source: Source, title: String, filter: Filter) -> Vec<Effect> {
        let mut effects = self.load(source, title, filter, false, ListKind::Feed);
        if self.logged_in() && !self.show_watched {
            effects.push(Effect::LoadHistory);
        }
        effects
    }

    /// Hides or shows the watched videos of every feed.
    fn refilter_watched(&mut self) {
        for listing in &mut self.stack {
            filter_watched(listing, &self.watched, self.show_watched);
        }
    }

    fn back(&mut self) {
        if self.prompt.take().is_some() {
            return;
        }
        if self.fullscreen {
            self.fullscreen = false;
            return;
        }
        if self.focus == Focus::List {
            if self.stack.len() > 1 {
                self.stack.pop();
            } else if self.sidebar {
                self.focus = Focus::Menu;
            }
        }
    }

    fn session_ready(&mut self, subs: Result<HashSet<String>, String>) -> Vec<Effect> {
        match subs {
            Ok(subs) => {
                self.status = if self.logged_in() {
                    let (session, channels) = (self.tr("Sesión", "Login"), self.tr("canales", "channels"));
                    format!("{session}: {} · {} {channels}", self.session_label(), subs.len())
                } else {
                    self.tr("Sin sesión", "Not logged in").into()
                };
                self.subs = subs;
                let mut effects = vec![Effect::SaveSubs(self.subs.clone())];
                // The feed waiting for them, unless something else is loading now.
                if let Some((pending, items, next)) = self.recheck.take()
                    && self.pending.is_none()
                    && self.loading.as_ref() == Some(&pending.title)
                {
                    let id = pending.id;
                    self.pending = Some(pending);
                    effects.extend(self.loaded(id, Ok(items), next));
                }
                effects
            }
            Err(error) => {
                self.status = error;
                if let Some((pending, ..)) = self.recheck.take()
                    && self.loading.as_ref() == Some(&pending.title)
                {
                    self.loading = None;
                }
                Vec::new()
            }
        }
    }

    /// The video the user is pointing at: the open tab, or the selected list row.
    fn selected_video(&self) -> Option<&Video> {
        if let Some(tab) = self.active_tab() {
            if self.on_related() {
                return tab.related_videos().get(tab.related);
            }
            return tab.playback.video.as_ref();
        }
        match self.stack.last() {
            Some(Listing { items: Items::Videos(videos), selected, .. }) if self.focus == Focus::List => {
                videos.get(*selected)
            }
            _ => None,
        }
    }

    /// The video whose weight is worth asking for: the one looked at, then
    /// the ones downloading. Not live ones, nor those already known.
    pub fn size_wanted(&self) -> Option<&Video> {
        let downloading = self.downloads.iter().filter(|d| matches!(d.state, DownloadState::Running { .. }));
        self.selected_video()
            .into_iter()
            .chain(downloading.map(|d| &d.video))
            .find(|v| v.live == crate::domain::Live::No && !self.sizes.contains_key(&v.id))
    }

    /// `(id, name, url)` of the channel the user is pointing at.
    pub(crate) fn selected_channel(&self) -> Option<(String, String, Option<String>)> {
        if let Some(v) = self.selected_video() {
            return v.channel_id.clone().map(|id| (id, v.channel.clone(), v.channel_url.clone()));
        }
        match self.stack.last() {
            Some(Listing { items: Items::Channels(channels), selected, .. }) if self.focus == Focus::List => {
                channels.get(*selected).map(|c| (c.id.clone(), c.name.clone(), Some(c.url.clone())))
            }
            _ => None,
        }
    }

    /// While the right-click menu is open it takes the moves, Enter and Esc;
    /// the rest (what arrives from the background) goes on as usual.
    fn in_context_menu(&mut self, action: &Action) -> Option<Vec<Effect>> {
        let menu = self.context.as_mut()?;
        let last = menu.items.len().saturating_sub(1);
        match *action {
            Action::Up => menu.selected = menu.selected.checked_sub(1).unwrap_or(last),
            Action::Down => menu.selected = if menu.selected >= last { 0 } else { menu.selected + 1 },
            Action::Select(i) => menu.selected = i.min(last),
            Action::Back => self.context = None,
            Action::Enter | Action::MenuPick(_) => {
                let i = if let Action::MenuPick(i) = *action { i } else { menu.selected };
                let item = self.context.take().and_then(|m| m.items.into_iter().nth(i));
                return Some(item.map(|item| self.update(item.action)).unwrap_or_default());
            }
            _ => return None,
        }
        Some(Vec::new())
    }

    fn open_context_menu(&mut self, x: u16, y: u16, row: bool) {
        // The video of the tab, not its related list.
        if !row && let Some(tab) = self.active_tab_mut() {
            tab.on_related = false;
        }
        let items = self.context_items();
        self.context = (!items.is_empty()).then_some(ContextMenu { at: (x, y), items, selected: 0 });
    }

    /// What the right-click menu offers for the selection (or the tab's video).
    fn context_items(&self) -> Vec<MenuItem> {
        let item = |es, en, key, action| MenuItem { label: self.tr(es, en), key, action };
        let mut items = Vec::new();
        if let Some(tab) = self.active_tab() {
            if self.on_related() {
                items.push(item("Ver", "Play", "Enter", Action::Enter));
                items.push(item("Solo audio", "Sound only", "a", Action::Listen));
                items.extend(self.video_items());
                return items;
            }
            if tab.ended {
                items.push(item("Volver a verlo", "Play it again", "Enter", Action::Enter));
            } else {
                items.push(item("Pausa / seguir", "Pause / resume", "espacio", Action::TogglePause));
                if !tab.playback.audio_only {
                    items.push(item("Pantalla completa", "Fullscreen", "f", Action::ToggleFullscreen));
                }
                items.push(item("Silenciar", "Mute", "m", Action::ToggleMute));
                items.push(item("Subtítulos", "Subtitles", "j", Action::CycleSubtitles));
            }
            if tab.playback.video.is_some() {
                items.push(if tab.playback.audio_only {
                    item("Volver al video", "Back to the video", "a", Action::Listen)
                } else {
                    item("Solo audio", "Sound only", "a", Action::Listen)
                });
                items.extend(self.video_items());
            }
            items.push(item("Cerrar la pestaña", "Close the tab", "Ctrl+y x", Action::CloseTab));
            return items;
        }
        let Some(listing) = self.listing().filter(|_| self.focus == Focus::List) else { return items };
        let selected = listing.selected;
        match &listing.items {
            Items::Videos(videos) if videos.get(selected).is_some() => {
                if listing.kind == ListKind::History {
                    items.push(item("Ver (sigue donde quedó)", "Play (resumes)", "Enter", Action::Enter));
                } else {
                    items.push(item("Ver", "Play", "Enter", Action::Enter));
                }
                items.push(item("Solo audio", "Sound only", "a", Action::Listen));
                items.extend(self.video_items());
                if listing.home && self.config.ui.home_all {
                    items.push(item("Solo de mis canales", "Only my channels", "t", Action::ToggleHomeAll));
                } else if listing.home {
                    items.push(item("Todo, como YouTube", "All, as on YouTube", "t", Action::ToggleHomeAll));
                }
                if listing.kind == ListKind::History {
                    items.push(item("Quitar del historial", "Remove from history", "x", Action::Trash));
                }
            }
            Items::Channels(channels) if channels.get(selected).is_some() => {
                items.push(item("Ver sus videos", "Its videos", "Enter", Action::Enter));
                items.extend(self.subscribe_item());
            }
            Items::Playlists(playlists) if playlists.get(selected).is_some() => {
                items.push(item("Ver sus videos", "Its videos", "Enter", Action::Enter));
                items.push(item("Borrar la lista", "Delete the playlist", "x", Action::Trash));
            }
            Items::Files(files, kind) if files.get(selected).is_some() => {
                items.push(match kind {
                    MediaKind::Music => item("Escuchar desde acá", "Play from here", "Enter", Action::Enter),
                    MediaKind::Video => item("Ver", "Play", "Enter", Action::Enter),
                });
                items.push(item("Marcar / desmarcar", "Mark / unmark", "espacio", Action::ToggleMark));
                items.push(item("Cambiar el orden", "Change the sort order", "o", Action::CycleSort));
                items.push(item("A la papelera", "To the trash", "x", Action::Trash));
            }
            Items::Downloads(downloads) => match downloads.get(selected).map(|d| &d.state) {
                Some(DownloadState::Done(_)) => {
                    items.push(item("Ver", "Play", "Enter", Action::Enter));
                    items.push(item("Quitar de la lista", "Remove from the list", "x", Action::Trash));
                }
                Some(_) => items.push(item("Cancelar / quitar", "Cancel / remove", "x", Action::Trash)),
                None => {}
            },
            Items::Profiles(profiles) if profiles.get(selected).is_some() => {
                items.push(item("Usar esta sesión", "Use this login", "Enter", Action::Enter));
            }
            Items::Settings => items.push(item("Cambiar", "Change", "Enter", Action::Enter)),
            _ => {}
        }
        if self.stack.len() > 1 {
            items.push(item("Volver", "Back", "Esc", Action::Back));
        }
        items
    }

    /// What can be done with the selected YouTube video.
    fn video_items(&self) -> Vec<MenuItem> {
        let item = |es, en, key, action| MenuItem { label: self.tr(es, en), key, action };
        let Some(video) = self.selected_video() else { return Vec::new() };
        let mut items = vec![
            if self.later.iter().any(|v| v.id == video.id) {
                item("Quitar de Ver después", "Remove from Watch later", "l", Action::ToggleLater)
            } else {
                item("Ver después", "Watch later", "l", Action::ToggleLater)
            },
            item("Descargar", "Download", "d", Action::Download),
            if self.ratings.get(&video.id) == Some(&Rating::Like) {
                item("Quitar el me gusta", "Remove the like", "g", Action::Rate(Rating::Like))
            } else {
                item("Me gusta", "Like", "g", Action::Rate(Rating::Like))
            },
        ];
        if self.selected_channel().is_some_and(|(_, _, url)| url.is_some()) {
            items.push(item("Ir al canal", "Go to the channel", "c", Action::OpenChannel));
        }
        items.extend(self.subscribe_item());
        items
    }

    /// Subscribe to (or leave) the selected channel, if there is one.
    fn subscribe_item(&self) -> Option<MenuItem> {
        let (id, ..) = self.selected_channel()?;
        let (es, en) = if self.subs.contains(&id) {
            ("Desuscribirse (pide confirmar)", "Unsubscribe (asks first)")
        } else {
            ("Suscribirse", "Subscribe")
        };
        Some(MenuItem { label: self.tr(es, en), key: "s", action: Action::ToggleSubscription })
    }

    fn toggle_later(&mut self) -> Vec<Effect> {
        let Some(video) = self.selected_video().cloned() else { return Vec::new() };
        if let Some(i) = self.later.iter().position(|v| v.id == video.id) {
            self.later.remove(i);
            self.status =
                format!("🗑 {}: {}", self.tr("Quitado de Ver después", "Removed from Watch later"), video.title);
        } else {
            self.status = format!("🕒 {}: {}", self.tr("Guardado para después", "Saved for later"), video.title);
            self.later.insert(0, video);
        }
        for listing in self.stack.iter_mut().filter(|l| l.kind == ListKind::Later) {
            listing.items = Items::Videos(self.later.clone());
            listing.selected = listing.selected.min(self.later.len().saturating_sub(1));
        }
        vec![Effect::SaveLater(self.later.clone())]
    }

    fn toggle_subscription(&mut self) -> Vec<Effect> {
        let Some((channel_id, name, _)) = self.selected_channel() else {
            if self.selected_video().is_some() {
                self.status = self.tr("Este video no tiene canal", "This video has no channel").into();
            }
            return Vec::new();
        };
        if self.subs.contains(&channel_id) {
            self.confirm = Some(Confirm::Unsubscribe { channel_id, name });
            Vec::new()
        } else {
            self.status = format!("{} {name}…", self.tr("Suscribiendo a", "Subscribing to"));
            vec![Effect::Subscribe { channel_id, name, subscribe: true }]
        }
    }

    /// The selected file of a local files list.
    fn selected_file(&self) -> Option<&LocalFile> {
        match self.stack.last() {
            Some(Listing { items: Items::Files(files, _), selected, .. }) if self.active == 0 => files.get(*selected),
            _ => None,
        }
    }

    fn toggle_mark(&mut self) -> Vec<Effect> {
        let Some(path) = self.selected_file().map(|f| f.path.clone()) else { return Vec::new() };
        if !self.marked.remove(&path) {
            self.marked.insert(path);
        }
        let marked = self.marked_files().len();
        self.status =
            format!("{marked} {}", self.tr("marcados · x o Supr: a la papelera", "marked · x or Del: to the trash"));
        self.move_selection(1)
    }

    /// The marked files of the shown list, in its order.
    pub(crate) fn marked_files(&self) -> Vec<PathBuf> {
        match self.stack.last() {
            Some(Listing { items: Items::Files(files, _), .. }) if self.active == 0 => {
                files.iter().filter(|f| self.marked.contains(&f.path)).map(|f| f.path.clone()).collect()
            }
            _ => Vec::new(),
        }
    }

    fn ask_trash(&mut self) {
        let mut paths = self.marked_files();
        if paths.is_empty() {
            paths.extend(self.selected_file().map(|f| f.path.clone()));
        }
        if !paths.is_empty() {
            self.confirm = Some(Confirm::Trash(paths));
        }
    }

    fn selected_playlist(&self) -> Option<&crate::domain::Playlist> {
        match self.listing() {
            Some(Listing { items: Items::Playlists(playlists), selected, .. }) if self.focus == Focus::List => {
                playlists.get(*selected)
            }
            _ => None,
        }
    }

    fn ask_delete_playlist(&mut self) {
        let Some(playlist) = self.selected_playlist() else { return };
        // Liked videos and Watch later are YouTube's own: they can't go.
        if matches!(playlist.id.as_str(), "LL" | "WL") {
            self.status = self
                .tr("Esa lista es de YouTube: no se puede borrar", "That list is YouTube's: it can't be deleted")
                .into();
            return;
        }
        let (id, title) = (playlist.id.clone(), playlist.title.clone());
        self.confirm = Some(Confirm::DeletePlaylist { id, title });
    }

    fn playlist_deleted(&mut self, id: String, title: String, result: Result<(), String>) {
        if let Err(error) = result {
            self.status = format!("❌ {} {title}: {error}", self.tr("No se pudo borrar", "Could not delete"));
            return;
        }
        for listing in &mut self.stack {
            if let Items::Playlists(playlists) = &mut listing.items {
                playlists.retain(|p| p.id != id);
                listing.selected = listing.selected.min(playlists.len().saturating_sub(1));
            }
        }
        self.status = format!("🗑 {} {title}", self.tr("Lista borrada:", "Playlist deleted:"));
    }

    fn trashed(&mut self, result: Result<Vec<PathBuf>, String>) {
        let paths = match result {
            Ok(paths) => paths,
            Err(error) => {
                self.status = format!(
                    "❌ {} {error}",
                    self.tr("No se pudo mandar a la papelera:", "Could not move to the trash:")
                );
                return;
            }
        };
        for listing in &mut self.stack {
            if let Items::Files(files, _) = &mut listing.items {
                files.retain(|f| !paths.contains(&f.path));
                listing.selected = listing.selected.min(files.len().saturating_sub(1));
            }
        }
        self.marked.retain(|p| !paths.contains(p));
        let (sent, recover) = (
            self.tr("a la papelera", "moved to the trash"),
            self.tr("(se recuperan desde la papelera del sistema)", "(recover them from the system trash)"),
        );
        self.status = format!("🗑 {} {sent} {recover}", paths.len());
    }

    fn rate(&mut self, rating: Rating) -> Vec<Effect> {
        let Some(video) = self.selected_video() else { return Vec::new() };
        if !self.logged_in() {
            let hint =
                self.tr("Sin sesión: elegí un navegador en Configuración", "Not logged in: pick a browser in Settings");
            self.status = hint.into();
            return Vec::new();
        }
        let (id, title) = (video.id.clone(), video.title.clone());
        let current = self.ratings.get(&id).copied().unwrap_or_default();
        let rating = if current == rating { Rating::None } else { rating };
        vec![Effect::Rate { video: id, title, rating }]
    }

    fn rated(&mut self, video: String, title: String, rating: Rating, result: Result<(), String>) {
        if let Err(error) = result {
            self.status = format!("❌ {} {title}: {error}", self.tr("No se pudo votar", "Could not vote on"));
            return;
        }
        let done = match rating {
            Rating::Like => self.tr("👍 Te gusta", "👍 Liked"),
            Rating::Dislike => self.tr("👎 No te gusta", "👎 Disliked"),
            Rating::None => self.tr("Voto quitado", "Vote taken back"),
        };
        self.status = format!("{done}: {title}");
        self.ratings.insert(video, rating);
    }

    fn subscription_done(&mut self, channel_id: String, name: String, result: Result<bool, String>) -> Vec<Effect> {
        match result {
            Ok(true) => {
                self.subs.insert(channel_id);
                self.status = format!("🔔 {}: {name}", self.tr("Suscripto", "Subscribed"));
            }
            Ok(false) => {
                self.subs.remove(&channel_id);
                self.status = format!("🔕 {}: {name}", self.tr("Desuscripto", "Unsubscribed"));
                for listing in self.stack.iter_mut().filter(|l| l.kind == ListKind::MyChannels) {
                    if let Items::Channels(channels) = &mut listing.items {
                        channels.retain(|c| c.id != channel_id);
                        listing.selected = listing.selected.min(channels.len().saturating_sub(1));
                    }
                }
            }
            Err(error) => {
                let failed = self.tr("No se pudo cambiar la suscripción a", "Could not change the subscription to");
                self.status = format!("❌ {failed} {name}: {error}");
                return Vec::new();
            }
        }
        vec![Effect::SaveSubs(self.subs.clone())]
    }

    fn listen(&mut self) -> Vec<Effect> {
        if !self.on_related()
            && let Some(tab) = self.active_tab_mut().filter(|t| t.playback.video.is_some())
        {
            tab.playback.audio_only = !tab.playback.audio_only;
            if tab.ended {
                return self.replay();
            }
            let (id, playback) = (tab.id, tab.playback.clone());
            self.fullscreen = false;
            self.status = if playback.audio_only {
                self.tr("🎧 Solo audio · a vuelve al video", "🎧 Sound only · a brings the video back")
            } else {
                self.tr("Volviendo al video…", "Bringing the video back…")
            }
            .into();
            return vec![Effect::Reopen(id, playback)];
        }
        let Some(video) = self.selected_video().cloned() else { return Vec::new() };
        self.play(Playback { audio_only: true, ..Playback::youtube(video) })
    }

    fn open_channel(&mut self) -> Vec<Effect> {
        match self.selected_channel() {
            Some((_, name, Some(url))) => {
                self.active = 0;
                self.open_channel_list(name, url)
            }
            _ => Vec::new(),
        }
    }

    fn open_channel_list(&mut self, name: String, url: String) -> Vec<Effect> {
        let kind = ListKind::Channel(ChannelRef { name: name.clone(), url: url.clone() });
        self.load(Source::Channel(url), name, Filter::None, true, kind)
    }

    fn play(&mut self, mut playback: Playback) -> Vec<Effect> {
        if let Some(i) = self.tabs.iter().position(|t| t.playback.queue.first() == playback.queue.first()) {
            self.active = i + 1;
            return Vec::new();
        }
        if self.tabs.len() >= self.config.ui.max_tabs {
            let (max, close) = (
                self.tr("Máximo de", "At most"),
                self.tr("pestañas: cerrá una con Ctrl+y + x", "tabs: close one with Ctrl+y + x"),
            );
            self.status = format!("{max} {} {close}", self.config.ui.max_tabs);
            return Vec::new();
        }
        let id = TabId(self.next_tab);
        self.next_tab += 1;
        let mut history = Vec::new();
        if let Some(video) = &playback.video {
            playback.start = playback.start.or_else(|| self.resume_at(&video.id));
            if let Some(start) = playback.start {
                self.status = format!("⏯ {} {}", self.tr("Sigue desde", "Resuming at"), clock(start));
            }
            history = self.remember(video.clone(), playback.start);
        }
        self.tabs.push(Tab::new(id, playback.clone()));
        self.active = self.tabs.len();
        let mut effects = vec![Effect::Open(id, playback.clone())];
        if let Some(video) = &playback.video {
            self.watched.insert(video.id.clone());
            effects.push(Effect::Watch { tab: id, video: video.id.clone() });
            effects.push(Effect::SaveWatched(video.id.clone()));
        }
        effects.extend(history);
        if let Some(video) = playback.video.as_ref().filter(|_| self.config.player.sponsorblock) {
            effects.push(Effect::Segments { tab: id, video: video.id.clone() });
        }
        effects
    }

    /// Plays the closed video of the active tab again, from where it was left.
    fn replay(&mut self) -> Vec<Effect> {
        let resume = self.active_tab().and_then(|t| t.playback.video.as_ref()).and_then(|v| self.resume_at(&v.id));
        let Some(tab) = self.active_tab_mut().filter(|t| t.ended) else { return Vec::new() };
        tab.ended = false;
        tab.on_related = false;
        tab.skipped.clear();
        tab.playback.start = resume;
        let (id, playback) = (tab.id, tab.playback.clone());
        if let Some(start) = resume {
            self.status = format!("⏯ {} {}", self.tr("Sigue desde", "Resuming at"), clock(start));
        }
        vec![Effect::Open(id, playback)]
    }

    /// Where I left `id`, if it is worth resuming.
    pub fn resume_at(&self, id: &str) -> Option<u32> {
        self.history.iter().find(|e| e.video.id == id).and_then(|e| e.position)
    }

    /// Puts `video` first in the history.
    fn remember(&mut self, video: Video, position: Option<u32>) -> Vec<Effect> {
        self.history.retain(|e| e.video.id != video.id);
        self.history.insert(0, HistoryEntry { video, at: self.clock, position });
        self.history.truncate(HISTORY_MAX);
        self.sync_history();
        vec![Effect::SaveHistory(self.history.clone())]
    }

    /// Takes the selected video out of the history.
    fn forget_selected(&mut self) -> Vec<Effect> {
        let Some(video) = self.selected_video().cloned() else { return Vec::new() };
        let on_youtube = self.youtube_history.iter().any(|e| e.video.id == video.id);
        self.history.retain(|e| e.video.id != video.id);
        self.youtube_history.retain(|e| e.video.id != video.id);
        self.sync_history();
        self.status = format!("🗑 {}: {}", self.tr("Quitado del historial", "Removed from history"), video.title);
        if on_youtube {
            let note = self.tr("(en YouTube sigue en tu historial)", "(still in your YouTube history)");
            self.status = format!("{} {note}", self.status);
        }
        vec![Effect::SaveHistory(self.history.clone())]
    }

    /// What "Mis recomendados" shows, and how to switch.
    fn home_mode(&self) -> &'static str {
        if self.config.ui.home_all {
            self.tr(
                "todo lo que recomienda YouTube (t: solo mis canales)",
                "all YouTube recommends (t: only my channels)",
            )
        } else {
            self.tr("solo de mis canales (t: todo, como YouTube)", "only my channels (t: all, as on YouTube)")
        }
    }

    /// "Historial": what I watched in tubetui and on YouTube, newest first.
    pub fn history_entries(&self) -> Vec<HistoryEntry> {
        let mut entries = self.history.clone();
        let mine: HashSet<&str> = self.history.iter().map(|e| e.video.id.as_str()).collect();
        entries.extend(self.youtube_history.iter().filter(|e| !mine.contains(e.video.id.as_str())).cloned());
        entries.sort_by_key(|e| std::cmp::Reverse(e.at));
        entries
    }

    /// When I last watched `id`, in tubetui or on YouTube.
    pub fn watched_at(&self, id: &str) -> Option<i64> {
        let local = self.history.iter().find(|e| e.video.id == id).map(|e| e.at);
        local.or_else(|| self.youtube_history.iter().find(|e| e.video.id == id).map(|e| e.at))
    }

    /// Shown history lists follow `self.history`, the selection staying on its video.
    /// Live videos, then the recent ones of my favorite channels, then the rest.
    fn favorites_first(&self, videos: &mut [Video]) {
        let recent = self.clock - FAVORITE_FRESH_DAYS * 86_400;
        videos.sort_by_key(|v| match v.live {
            crate::domain::Live::Now => 0,
            _ if self.is_favorite(v) && v.published.is_some_and(|t| t >= recent) => 1,
            _ => 2,
        });
    }

    /// A video from one of the channels I watch most.
    pub fn is_favorite(&self, video: &Video) -> bool {
        video.channel_id.as_ref().is_some_and(|c| self.favorites.contains(c))
    }

    /// Counts again what I watched of each channel (after the history changes).
    pub fn refresh_favorites(&mut self) {
        self.channel_views = crate::domain::channel_views(&self.history, self.clock - FAVORITE_DAYS * 86_400);
        self.favorites = if self.config.ui.favorites_first {
            crate::domain::favorite_channels(&self.channel_views, FAVORITE_MIN, FAVORITES_MAX)
        } else {
            HashSet::new()
        };
    }

    fn sync_history(&mut self) {
        self.refresh_favorites();
        let videos: Vec<Video> = self.history_entries().into_iter().map(|e| e.video).collect();
        for listing in self.stack.iter_mut().filter(|l| l.kind == ListKind::History) {
            let current = match &listing.items {
                Items::Videos(shown) => shown.get(listing.selected).map(|v| v.id.clone()),
                _ => None,
            };
            listing.selected = current
                .and_then(|id| videos.iter().position(|v| v.id == id))
                .unwrap_or(listing.selected.min(videos.len().saturating_sub(1)));
            listing.items = Items::Videos(videos.clone());
        }
    }

    /// The first check only learns what is there; later ones tell what is new.
    fn subs_polled(&mut self, result: Result<Vec<Video>, String>) -> Vec<Effect> {
        let Ok(videos) = result else { return Vec::new() };
        let videos: Vec<Video> = videos
            .into_iter()
            .filter(|v| v.live != crate::domain::Live::Upcoming && !(self.config.ui.hide_shorts && v.short))
            .collect();
        let Some(known) = &mut self.known_subs else {
            self.known_subs = Some(videos.into_iter().map(|v| v.id).collect());
            return Vec::new();
        };
        let mut new: Vec<Video> =
            videos.into_iter().filter(|v| known.insert(v.id.clone()) && !self.watched.contains(&v.id)).collect();
        if new.is_empty() {
            return Vec::new();
        }
        // My favorite channels are named first.
        new.sort_by_key(|v| !self.is_favorite(v));
        let star = |v: &Video| if self.is_favorite(v) { "⭐ " } else { "" };
        let message = match new.as_slice() {
            [one] => format!("🔔 {}{}: {}", star(one), one.channel, one.title),
            [first, ..] if self.is_favorite(first) => format!(
                "🔔 ⭐ {}: {} (+{} {})",
                first.channel,
                first.title,
                new.len() - 1,
                self.tr("más de tus canales", "more from your channels")
            ),
            many => format!(
                "🔔 {} {}",
                many.len(),
                self.tr("videos nuevos de tus canales", "new videos from your channels")
            ),
        };
        self.status = format!("{message} · {}", self.tr("mirá Suscripciones", "see Subscriptions"));
        self.fresh.extend(new);
        vec![Effect::Notify(message)]
    }

    fn start_download(&mut self) -> Vec<Effect> {
        let Some(video) = self.selected_video().cloned() else { return Vec::new() };
        let running = |d: &Download| d.video.id == video.id && matches!(d.state, DownloadState::Running { .. });
        if self.downloads.iter().any(running) {
            self.status = format!("⬇ {}: {}", self.tr("Ya se está descargando", "Already downloading"), video.title);
            return Vec::new();
        }
        let id = self.next_download;
        self.next_download += 1;
        let state = DownloadState::Running { permille: 0, speed: String::new(), eta: String::new() };
        self.downloads.insert(0, Download { id, video: video.clone(), state });
        self.sync_downloads();
        let see = self.tr("mirá Descargas", "see Downloads");
        self.status = format!("⬇ {}: {} · {see}", self.tr("Descargando", "Downloading"), video.title);
        vec![Effect::Download { id, video }]
    }

    fn downloaded(&mut self, id: u64, result: Result<PathBuf, String>) -> Vec<Effect> {
        let lang = self.lang();
        let Some(d) = self.downloads.iter_mut().find(|d| d.id == id) else { return Vec::new() };
        if d.state == DownloadState::Cancelled {
            return Vec::new();
        }
        self.status = match &result {
            Ok(path) => format!(
                "✅ {} {}: {}",
                lang.pick("Descarga lista en", "Downloaded to"),
                path.parent().map_or_else(String::new, |p| p.display().to_string()),
                d.video.title
            ),
            Err(e) => format!("❌ {} {}: {e}", lang.pick("Falló la descarga de", "Download failed:"), d.video.title),
        };
        d.state = match result {
            Ok(path) => DownloadState::Done(path),
            Err(e) => DownloadState::Failed(e),
        };
        self.sync_downloads();
        vec![Effect::SaveDownloads(self.downloads.clone())]
    }

    /// `x` in "Descargas": stops the selected download, or takes a finished
    /// one off the list (the file stays).
    fn cancel_or_forget_download(&mut self) -> Vec<Effect> {
        let Some(Listing { items: Items::Downloads(shown), selected, .. }) = self.stack.last() else {
            return Vec::new();
        };
        let Some(id) = shown.get(*selected).map(|d| d.id) else { return Vec::new() };
        let Some(i) = self.downloads.iter().position(|d| d.id == id) else { return Vec::new() };
        let title = self.downloads[i].video.title.clone();
        let effects = if matches!(self.downloads[i].state, DownloadState::Running { .. }) {
            self.downloads[i].state = DownloadState::Cancelled;
            self.status = format!("⏹ {}: {title}", self.tr("Descarga cancelada", "Download cancelled"));
            vec![Effect::CancelDownload(id)]
        } else {
            self.downloads.remove(i);
            let note = self.tr("(el archivo queda)", "(the file stays)");
            self.status = format!("🗑 {}: {title} {note}", self.tr("Quitado de la lista", "Taken off the list"));
            vec![Effect::SaveDownloads(self.downloads.clone())]
        };
        self.sync_downloads();
        effects
    }

    /// Shown download lists follow `self.downloads`.
    fn sync_downloads(&mut self) {
        for listing in self.stack.iter_mut().filter(|l| l.kind == ListKind::Downloads) {
            listing.items = Items::Downloads(self.downloads.clone());
            listing.selected = listing.selected.min(self.downloads.len().saturating_sub(1));
        }
    }

    /// Removes a tab; the active one moves to its right neighbour, like browsers do.
    fn remove_tab(&mut self, id: TabId) {
        let Some(i) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        self.tabs.remove(i);
        let index = i + 1;
        if self.active > index {
            self.active -= 1;
        } else if self.active == index {
            self.active = index.min(self.tabs.len());
        }
    }

    fn toggle_ui_lang(&mut self) -> Vec<Effect> {
        let lang = self.lang().next();
        self.config.ui.lang = Some(lang);
        self.status = lang.pick("Idioma: español", "Language: English").into();
        vec![Effect::SaveSetting { table: "ui", key: "lang", value: SettingValue::Text(lang.as_str().into()) }]
    }

    fn command(&self, command: PlayerCommand) -> Vec<Effect> {
        self.active_tab().map(|t| Effect::Command(t.id, command)).into_iter().collect()
    }
}

/// A feed's videos without the watched ones (unless `show`), keeping the
/// selection on the same video; how many it hides.
fn filter_watched(listing: &mut Listing, watched: &HashSet<String>, show: bool) -> usize {
    let Some(all) = &listing.all else { return 0 };
    let current = match &listing.items {
        Items::Videos(videos) => videos.get(listing.selected).map(|v| v.id.clone()),
        _ => None,
    };
    let videos: Vec<Video> = all.iter().filter(|v| show || !watched.contains(&v.id)).cloned().collect();
    let hidden = all.len() - videos.len();
    listing.selected = current
        .and_then(|id| videos.iter().position(|v| v.id == id))
        .unwrap_or(listing.selected.min(videos.len().saturating_sub(1)));
    listing.items = Items::Videos(videos);
    hidden
}

fn dirs_label(dirs: &[String], default: &str, lang: Lang) -> String {
    if dirs.is_empty() { format!("{default} ({})", lang.pick("del sistema", "system")) } else { dirs.join(", ") }
}

/// The `[effects]` key and flag behind an animation setting.
fn effect_flag(effects: &mut EffectsConfig, setting: Setting) -> Option<(&'static str, &mut bool)> {
    match setting {
        Setting::FxStartup => Some(("startup", &mut effects.startup)),
        Setting::FxPages => Some(("pages", &mut effects.pages)),
        Setting::FxDialogs => Some(("dialogs", &mut effects.dialogs)),
        _ => None,
    }
}

/// "sí"/"no" in both languages.
fn on_off(on: bool) -> (&'static str, &'static str) {
    if on { ("sí", "on") } else { ("no", "off") }
}

pub fn language_label(language: Language, lang: Lang) -> &'static str {
    let (es, en) = match language {
        Language::Subtitles => ("original + subtítulos", "original + subtitles"),
        Language::Original => ("original, sin subtítulos", "original, no subtitles"),
        Language::Dubbed => ("audio doblado", "dubbed audio"),
    };
    lang.pick(es, en)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{IconSet, UiConfig};
    use crate::domain::{channel, video};

    fn state() -> State {
        State::new(Config::default(), Vec::new())
    }

    fn section_index(section: Section) -> usize {
        Section::ALL.iter().position(|s| *s == section).unwrap()
    }

    /// Opens `section` from the menu and returns its effects.
    fn open(s: &mut State, section: Section) -> Vec<Effect> {
        s.update(Action::GoTab(0));
        s.prompt = None;
        s.focus = Focus::Menu;
        s.menu = section_index(section);
        s.update(Action::Enter)
    }

    fn load_id(effects: &[Effect]) -> u64 {
        match effects {
            [Effect::Load { id, .. }, ..] | [Effect::ScanBrowsers { id }] | [Effect::ScanMedia { id, .. }] => *id,
            other => panic!("expected a load, got {other:?}"),
        }
    }

    fn with_videos(ids: &[&str]) -> State {
        let mut s = state();
        let id = load_id(&open(&mut s, Section::Suscripciones));
        s.update(Action::Loaded(id, Ok(Items::Videos(ids.iter().map(|i| video(i)).collect()))));
        s
    }

    /// Plays the video at `index` of the current list and returns the new tab id.
    fn play(s: &mut State, index: usize) -> TabId {
        s.update(Action::GoTab(0));
        s.stack.last_mut().unwrap().selected = index;
        match s.update(Action::Enter).as_slice() {
            [Effect::Open(id, _), Effect::Watch { tab, .. }, Effect::SaveWatched(_), Effect::SaveHistory(_), ..]
                if tab == id =>
            {
                *id
            }
            other => panic!("expected Open and Watch, got {other:?}"),
        }
    }

    fn type_text(s: &mut State, text: &str) {
        for c in text.chars() {
            s.update(Action::Input(c));
        }
    }

    /// Opens Configuración and activates `setting`.
    fn change(s: &mut State, setting: Setting) -> Vec<Effect> {
        open(s, Section::Configuracion);
        s.stack[0].selected = Setting::ALL.iter().position(|x| *x == setting).unwrap();
        s.update(Action::Enter)
    }

    fn saved(table: &'static str, key: &'static str, value: SettingValue) -> Effect {
        Effect::SaveSetting { table, key, value }
    }

    fn text(value: &str) -> SettingValue {
        SettingValue::Text(value.into())
    }

    /// Inside the channel "name UCx", with one video loaded.
    fn inside_channel() -> State {
        let mut s = state();
        let id = load_id(&open(&mut s, Section::Canales));
        s.update(Action::Loaded(id, Ok(Items::Channels(vec![channel("UCx")]))));
        let id = load_id(&s.update(Action::Enter));
        s.update(Action::Loaded(id, Ok(Items::Videos(vec![video("v")]))));
        s
    }

    #[test]
    fn playing_remembers_the_video_and_resumes_where_it_stopped() {
        let mut s = with_videos(&["a", "b"]);
        s.clock = 100;
        let tab = play(&mut s, 0);
        assert_eq!(s.history.len(), 1);
        assert_eq!((s.history[0].video.id.as_str(), s.history[0].at, s.history[0].position), ("a", 100, None));
        let effects = s.update(Action::Stopped { video: "a".into(), position: Some(125), duration: Some(600) });
        assert!(matches!(effects.as_slice(), [Effect::SaveHistory(h)] if h[0].position == Some(125)));
        assert_eq!(s.resume_at("a"), Some(125));
        s.update(Action::CloseTab);
        assert!(s.tabs.iter().all(|t| t.id != tab));
        s.stack.last_mut().unwrap().selected = 0;
        s.update(Action::GoTab(0));
        match s.update(Action::Enter).as_slice() {
            [Effect::Open(_, playback), ..] => assert_eq!(playback.start, Some(125)),
            other => panic!("{other:?}"),
        }
        assert!(s.status.contains("2:05"), "{}", s.status);
        // Watched to the end: next time it starts over.
        s.update(Action::Stopped { video: "a".into(), position: Some(590), duration: Some(600) });
        assert_eq!(s.resume_at("a"), None);
        assert!(s.update(Action::Stopped { video: "zz".into(), position: Some(90), duration: None }).is_empty());
    }

    #[test]
    fn history_section_lists_newest_first_and_x_forgets() {
        let mut s = with_videos(&["a", "b"]);
        play(&mut s, 0);
        play(&mut s, 1);
        open(&mut s, Section::Historial);
        let listing = s.listing().unwrap();
        assert_eq!(listing.kind, ListKind::History);
        assert_eq!(listing.items, Items::Videos(vec![video("b"), video("a")]));
        let effects = s.update(Action::Trash);
        assert!(matches!(effects.as_slice(), [Effect::SaveHistory(h)] if h.len() == 1 && h[0].video.id == "a"));
        assert_eq!(s.listing().unwrap().items, Items::Videos(vec![video("a")]));
        assert!(s.confirm.is_none(), "no question for the history");
    }

    #[test]
    fn the_history_brings_what_i_watched_on_youtube_too() {
        let mut s = with_videos(&["a"]);
        s.clock = 1000;
        play(&mut s, 0);
        s.config.session.browser = "brave".into();
        let id = match open(&mut s, Section::Historial).as_slice() {
            [Effect::YoutubeHistory { id, next: None }] => *id,
            other => panic!("{other:?}"),
        };
        assert_eq!(s.listing().unwrap().items, Items::Videos(vec![video("a")]), "mine while it comes");
        let page = |videos: &[(&str, i64)], next: Option<&str>| HistoryPage {
            videos: videos.iter().map(|(id, at)| (video(id), *at)).collect(),
            next: next.map(str::to_owned),
        };
        s.update(Action::YoutubeHistory(id, Ok(page(&[("y1", 1500), ("a", 900), ("y2", 500)], Some("T")))));
        let ids = |s: &State| match &s.listing().unwrap().items {
            Items::Videos(v) => v.iter().map(|v| v.id.clone()).collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        assert_eq!(ids(&s), ["y1", "a", "y2"], "by date, mine not repeated");
        assert_eq!(s.listing().unwrap().selected, 1, "still on a");
        assert_eq!((s.watched_at("a"), s.watched_at("y1")), (Some(1000), Some(1500)));
        assert!(s.watched.contains("y2"), "hidden from the feeds too");
        let more = match s.update(Action::Down).as_slice() {
            [Effect::YoutubeHistory { id, next: Some(token) }] if token == "T" => *id,
            other => panic!("{other:?}"),
        };
        s.update(Action::YoutubeHistory(more, Ok(page(&[("y3", 100)], None))));
        assert_eq!(ids(&s), ["y1", "a", "y2", "y3"]);
        assert_eq!(s.listing().unwrap().selected, 2, "the selection stays on y2");
        s.listing_mut().selected = 1;
        s.update(Action::Trash);
        assert_eq!(ids(&s), ["y1", "y2", "y3"]);
        s.listing_mut().selected = 0;
        s.update(Action::Trash);
        assert!(s.status.contains("YouTube"), "{}", s.status);
        assert_eq!(ids(&s), ["y2", "y3"]);
    }

    #[test]
    fn the_menu_goes_round_at_its_ends() {
        let mut s = state();
        s.focus = Focus::Menu;
        s.menu = 0;
        s.update(Action::Up);
        assert_eq!(s.menu, Section::ALL.len() - 1, "from the top to the bottom");
        s.update(Action::Down);
        assert_eq!(s.menu, 0, "from the bottom to the top");
    }

    #[test]
    fn shorts_are_left_out_unless_wanted() {
        let short = Video { short: true, ..video("s") };
        let mut s = state();
        let id = load_id(&open(&mut s, Section::Suscripciones));
        s.update(Action::Loaded(id, Ok(Items::Videos(vec![video("a"), short.clone()]))));
        assert_eq!(s.listing().unwrap().items, Items::Videos(vec![video("a")]));
        assert_eq!(change(&mut s, Setting::HideShorts), vec![saved("ui", "hide_shorts", SettingValue::Bool(false))]);
        let id = load_id(&open(&mut s, Section::Suscripciones));
        s.update(Action::Loaded(id, Ok(Items::Videos(vec![video("a"), short.clone()]))));
        assert_eq!(s.listing().unwrap().items, Items::Videos(vec![video("a"), short]));
    }

    #[test]
    fn a_plays_sound_only_and_switches_back_in_the_tab() {
        let mut s = with_videos(&["a"]);
        match s.update(Action::Listen).as_slice() {
            [Effect::Open(_, p), ..] => assert!(p.audio_only && p.video.is_some()),
            other => panic!("{other:?}"),
        }
        let tab = s.active_tab().unwrap().clone();
        assert!(s.details_shown(&tab), "no picture: the pane shows the details");
        match s.update(Action::Listen).as_slice() {
            [Effect::Reopen(id, p)] => assert!(*id == tab.id && !p.audio_only),
            other => panic!("{other:?}"),
        }
        assert!(!s.active_tab().unwrap().playback.audio_only);
        assert!(!s.details_shown(s.active_tab().unwrap()), "embedded video: the pane is the video");
    }

    #[test]
    fn sponsor_segments_are_skipped_once() {
        let mut s = with_videos(&["a"]);
        let effects = s.update(Action::Enter);
        let tab = s.active_tab().unwrap().id;
        assert!(effects.contains(&Effect::Segments { tab, video: "a".into() }));
        let sponsor = Segment { start: 10_000, end: 20_400, category: "sponsor".into() };
        s.update(Action::SegmentsLoaded(tab, vec![sponsor]));
        assert_eq!(s.update(Action::SkipSegment(tab, 0)), [Effect::Command(tab, PlayerCommand::SeekTo(21))]);
        assert!(s.status.contains("patrocinio"), "{}", s.status);
        assert!(s.update(Action::SkipSegment(tab, 0)).is_empty(), "once");
        s.config.player.sponsorblock = false;
        s.update(Action::GoTab(0));
        s.stack[0].items = Items::Videos(vec![video("b")]);
        assert!(!s.update(Action::Enter).iter().any(|e| matches!(e, Effect::Segments { .. })));
    }

    #[test]
    fn comments_are_asked_for_once_details_arrive_and_scroll() {
        let mut s = with_videos(&["a"]);
        let tab = play(&mut s, 0);
        let watch = Watch { comments_token: Some("T".into()), ..Watch::default() };
        assert_eq!(s.update(Action::Watched(tab, Ok(watch))), [Effect::Comments { tab, token: "T".into() }]);
        let comment = |n: &str| Comment { author: n.into(), text: "hi".into(), likes: None, published: None };
        s.update(Action::CommentsLoaded(tab, Ok(vec![comment("a"), comment("b")])));
        s.update(Action::ScrollComments(3));
        assert_eq!(s.active_tab().unwrap().comments_from, 1, "stops at the last one");
        s.update(Action::ScrollComments(-3));
        assert_eq!(s.active_tab().unwrap().comments_from, 0);
        s.update(Action::CommentsLoaded(tab, Err("offline".into())));
        assert_eq!(s.active_tab().unwrap().comments, Some(Vec::new()), "failed: none, not loading forever");
    }

    #[test]
    fn my_playlists_open_their_videos() {
        let mut s = state();
        assert!(open(&mut s, Section::MisListas).is_empty(), "needs a login");
        s.config.session.browser = "brave".into();
        let id = match open(&mut s, Section::MisListas).as_slice() {
            [Effect::Load { id, source: Source::MyPlaylists, .. }] => *id,
            other => panic!("{other:?}"),
        };
        let liked = crate::domain::Playlist { id: "LL".into(), title: "LL".into(), count: None };
        s.update(Action::Loaded(id, Ok(Items::Playlists(vec![liked]))));
        match s.update(Action::Enter).as_slice() {
            [Effect::Load { source: Source::Playlist(list), .. }] => assert_eq!(list, "LL"),
            other => panic!("{other:?}"),
        }
        assert_eq!(s.loading.as_deref(), Some("👍 Videos que me gustan"));
    }

    #[test]
    fn my_playlists_can_be_deleted_after_asking() {
        let mut s = state();
        s.config.session.browser = "brave".into();
        let id = load_id(&open(&mut s, Section::MisListas));
        let playlist = |id: &str| crate::domain::Playlist { id: id.into(), title: format!("t {id}"), count: None };
        s.update(Action::Loaded(id, Ok(Items::Playlists(vec![playlist("LL"), playlist("PLa"), playlist("PLb")]))));
        s.update(Action::Trash);
        assert!(s.confirm.is_none(), "Liked videos is YouTube's");
        s.update(Action::Down);
        s.update(Action::Trash);
        assert!(s.update(Action::Confirm(false)).is_empty());
        s.update(Action::Trash);
        assert_eq!(
            s.update(Action::Confirm(true)),
            [Effect::DeletePlaylist { id: "PLa".into(), title: "t PLa".into() }]
        );
        s.update(Action::PlaylistDeleted { id: "PLa".into(), title: "t PLa".into(), result: Err("401".into()) });
        assert!(matches!(&s.listing().unwrap().items, Items::Playlists(p) if p.len() == 3));
        s.update(Action::PlaylistDeleted { id: "PLa".into(), title: "t PLa".into(), result: Ok(()) });
        assert!(matches!(&s.listing().unwrap().items, Items::Playlists(p) if p.len() == 2));
        assert!(s.status.contains("borrada"));
    }

    #[test]
    fn downloads_show_progress_finish_and_can_be_cancelled() {
        let mut s = with_videos(&["a", "b"]);
        assert_eq!(s.update(Action::Download), [Effect::Download { id: 0, video: video("a") }]);
        assert!(s.update(Action::Download).is_empty(), "already downloading");
        s.stack.last_mut().unwrap().selected = 1;
        assert_eq!(s.update(Action::Download), [Effect::Download { id: 1, video: video("b") }]);
        assert_eq!(s.downloading(), 2);
        open(&mut s, Section::Descargas);
        assert_eq!(s.listing().unwrap().kind, ListKind::Downloads);
        s.update(Action::DownloadProgress { id: 0, permille: 453, speed: "1MiB/s".into(), eta: "00:09".into() });
        let shown = |s: &State| match &s.listing().unwrap().items {
            Items::Downloads(d) => d.clone(),
            other => panic!("{other:?}"),
        };
        assert!(matches!(shown(&s)[1].state, DownloadState::Running { permille: 453, .. }), "newest first");
        // Cancel the selected one (b, the newest).
        assert_eq!(s.update(Action::Trash), [Effect::CancelDownload(1)]);
        assert!(s.update(Action::Downloaded { id: 1, result: Err("killed".into()) }).is_empty(), "stays cancelled");
        assert_eq!(shown(&s)[0].state, DownloadState::Cancelled);
        let file = PathBuf::from("/v/title a.mp4");
        let effects = s.update(Action::Downloaded { id: 0, result: Ok(file.clone()) });
        assert!(matches!(effects.as_slice(), [Effect::SaveDownloads(_)]));
        assert!(s.status.contains("/v"), "{}", s.status);
        s.listing_mut().selected = 1;
        match s.update(Action::Enter).as_slice() {
            [Effect::Open(_, p)] => assert_eq!(p.queue, ["/v/title a.mp4"]),
            other => panic!("{other:?}"),
        }
        s.update(Action::GoTab(0));
        assert!(matches!(s.update(Action::Trash).as_slice(), [Effect::SaveDownloads(d)] if d.len() == 1));
        assert_eq!(s.downloading(), 0);
    }

    #[test]
    fn quitting_with_downloads_running_asks_first() {
        let mut s = with_videos(&["a"]);
        s.update(Action::Download);
        s.update(Action::Quit);
        assert_eq!((s.quit, s.confirm.clone()), (false, Some(Confirm::Quit(1))));
        s.update(Action::Confirm(false));
        assert!(!s.quit);
        open(&mut s, Section::Salir);
        assert_eq!(s.confirm, Some(Confirm::Quit(1)), "the menu entry asks too");
        s.update(Action::Confirm(true));
        assert!(s.quit);
    }

    #[test]
    fn new_videos_of_my_channels_are_announced_after_the_first_check() {
        let mut s = state();
        assert!(s.update(Action::SubsPolled(Ok(vec![video("a"), video("b")]))).is_empty(), "first: learns");
        assert!(s.fresh.is_empty());
        s.watched.insert("w".into());
        let short = Video { short: true, ..video("s") };
        let effects = s.update(Action::SubsPolled(Ok(vec![video("c"), video("a"), video("w"), short])));
        assert_eq!(effects, [Effect::Notify("🔔 chan c: title c".into())]);
        assert_eq!(s.fresh, [video("c")], "not the known, watched or Shorts");
        assert!(s.update(Action::SubsPolled(Ok(vec![video("c")]))).is_empty(), "told once");
        assert!(s.update(Action::SubsPolled(Err("offline".into()))).is_empty());
        open(&mut s, Section::Suscripciones);
        assert!(s.fresh.is_empty());
        assert!(s.fresh_shown.contains("c"), "marked in the list");
        assert_eq!(change(&mut s, Setting::NotifyNew), vec![saved("ui", "notify_new", SettingValue::Bool(false))]);
    }

    #[test]
    fn starts_on_the_menu() {
        let s = state();
        assert_eq!((s.active, s.focus, s.menu), (0, Focus::Menu, 0));
        let config = Config { ui: UiConfig { sidebar: false, ..Default::default() }, ..Default::default() };
        assert_eq!(State::new(config, vec![]).focus, Focus::List);
    }

    #[test]
    fn prefix_lasts_one_action() {
        let mut s = state();
        s.update(Action::Prefix);
        assert!(s.prefix);
        s.update(Action::ToggleSidebar);
        assert!(!s.prefix);
        assert!(!s.sidebar);
    }

    #[test]
    fn hiding_the_sidebar_moves_focus_to_the_list() {
        let mut s = state();
        s.update(Action::ToggleSidebar);
        assert_eq!(s.focus, Focus::List);
        s.update(Action::ToggleSidebar);
        assert_eq!(s.focus, Focus::Menu);
    }

    #[test]
    fn ui_language_toggles_and_is_saved() {
        let mut s = state();
        assert_eq!(s.lang(), Lang::Es, "Spanish until the runtime picks the system's");
        assert_eq!(s.update(Action::ToggleUiLang), vec![saved("ui", "lang", text("en"))]);
        assert_eq!((s.lang(), s.status.as_str()), (Lang::En, "Language: English"));
        assert_eq!(Section::Inicio.label(s.lang()), "My recommendations");
        assert_eq!(s.update(Action::ToggleUiLang), vec![saved("ui", "lang", text("es"))]);
        assert_eq!(change(&mut s, Setting::AppLang), vec![saved("ui", "lang", text("en"))]);
        assert_eq!(change(&mut s, Setting::FxPages), vec![saved("effects", "pages", SettingValue::Bool(false))]);
        assert!(!s.config.effects.pages && s.config.effects.startup && s.config.effects.dialogs);
        assert_eq!(s.setting_value(Setting::FxPages), "off");
        assert_eq!(s.setting_value(Setting::FxDialogs), "on");
    }

    #[test]
    fn lists_are_titled_in_the_interface_language() {
        let mut s = state();
        s.update(Action::ToggleUiLang);
        open(&mut s, Section::Suscripciones);
        assert_eq!(s.loading.as_deref(), Some("Subscriptions"));
        assert_eq!(s.status, "Loading Subscriptions…");
    }

    #[test]
    fn about_is_a_page_on_inicio() {
        let mut s = state();
        assert!(open(&mut s, Section::AcercaDe).is_empty());
        assert_eq!(s.listing().map(|l| &l.items), Some(&Items::About));
        assert!(s.update(Action::Enter).is_empty());
    }

    #[test]
    fn help_toggles() {
        let mut s = state();
        s.update(Action::ToggleHelp);
        assert!(s.help && s.key_context().help);
        s.update(Action::ToggleHelp);
        assert!(!s.help);
    }

    #[test]
    fn feed_sections_load_their_source() {
        let cases = [
            (Section::DescubrirCanales, Source::Recommended, FILTERED_LIMIT),
            (Section::Suscripciones, Source::Subscriptions, 60),
            (Section::Canales, Source::MyChannels, 60),
        ];
        for (section, expected, expected_limit) in cases {
            let mut s = state();
            match open(&mut s, section).as_slice() {
                [Effect::Load { source, limit, .. }] => {
                    assert_eq!((source, *limit), (&expected, expected_limit), "{section:?}");
                }
                other => panic!("{section:?}: {other:?}"),
            }
            assert!(s.loading.is_some(), "{section:?}");
        }
    }

    #[test]
    fn loaded_list_replaces_stack_and_focuses_it() {
        let s = with_videos(&["a", "b"]);
        assert_eq!(s.stack.len(), 1);
        assert_eq!(s.focus, Focus::List);
        assert_eq!(s.loading, None);
        assert_eq!(s.listing().unwrap().items, Items::Videos(vec![video("a"), video("b")]));
    }

    #[test]
    fn stale_results_are_ignored() {
        let mut s = state();
        let first = load_id(&open(&mut s, Section::DescubrirCanales));
        let second = load_id(&open(&mut s, Section::Suscripciones));
        s.update(Action::Loaded(first, Ok(Items::Videos(vec![video("old")]))));
        assert!(s.stack.is_empty());
        s.update(Action::Loaded(second, Ok(Items::Videos(vec![video("new")]))));
        assert_eq!(s.listing().unwrap().title, "Suscripciones");
    }

    #[test]
    fn load_error_is_reported_and_keeps_the_list() {
        let mut s = with_videos(&["a"]);
        let id = load_id(&open(&mut s, Section::Suscripciones));
        s.update(Action::Loaded(id, Err("sin sesión".into())));
        assert_eq!(s.status, "sin sesión");
        assert_eq!(s.listing().unwrap().title, "Suscripciones");
    }

    #[test]
    fn discover_hides_my_channels_and_channelless_entries() {
        let mut s = state();
        s.update(Action::SubsLoaded(HashSet::from(["UCa".to_string()])));
        let id = load_id(&open(&mut s, Section::DescubrirCanales));
        let mix = Video { channel_id: None, ..video("RDmix") };
        s.update(Action::Loaded(id, Ok(Items::Videos(vec![video("a"), video("b"), mix]))));
        assert_eq!(s.listing().unwrap().items, Items::Videos(vec![video("b")]));
    }

    #[test]
    fn my_recommendations_keep_only_my_channels() {
        let mut s = state();
        assert!(open(&mut s, Section::Inicio).is_empty());
        assert!(s.status.contains("Sin sesión"), "{}", s.status);
        s.config.session.browser = "brave".into();
        s.update(Action::SubsLoaded(HashSet::from(["UCa".to_string()])));
        let id = match open(&mut s, Section::Inicio).as_slice() {
            [Effect::Load { id, source: Source::Recommended, .. }, ..] => *id,
            other => panic!("{other:?}"),
        };
        s.update(Action::Loaded(id, Ok(Items::Videos(vec![video("a"), video("b")]))));
        assert_eq!(s.listing().unwrap().items, Items::Videos(vec![video("a")]));
        assert_eq!(s.listing().unwrap().title, "Mis recomendados");
    }

    #[test]
    fn t_switches_my_recommendations_to_all_of_youtube_and_back() {
        let mut s = state();
        s.config.session.browser = "brave".into();
        s.update(Action::SubsLoaded(HashSet::from(["UCa".to_string()])));
        let feed = vec![video("a"), Video { channel_id: Some("UCz".into()), ..video("z") }];
        let load = |s: &mut State, effects: Vec<Effect>| {
            let id = match effects.as_slice() {
                [Effect::Load { id, source: Source::Recommended, .. }, ..] => *id,
                other => panic!("{other:?}"),
            };
            s.update(Action::Loaded(id, Ok(Items::Videos(feed.clone()))));
            s.listing().unwrap().items.len()
        };
        let effects = open(&mut s, Section::Inicio);
        assert_eq!(load(&mut s, effects), 1, "only mine");
        assert!(s.status.contains("t: todo"), "{}", s.status);
        let effects = s.update(Action::ToggleHomeAll);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::SaveSetting { key: "home_all", value: SettingValue::Bool(true), .. }))
        );
        assert_eq!(load(&mut s, effects), 2, "all of YouTube's");
        assert!(s.status.contains("t: solo mis canales"), "{}", s.status);
        let effects = s.update(Action::ToggleHomeAll);
        assert_eq!(load(&mut s, effects), 1);
        let id = match open(&mut s, Section::Suscripciones).as_slice() {
            [Effect::Load { id, .. }, ..] => *id,
            other => panic!("{other:?}"),
        };
        s.update(Action::Loaded(id, Ok(Items::Videos(feed.clone()))));
        assert!(s.update(Action::ToggleHomeAll).is_empty(), "only in Mis recomendados");
    }

    #[test]
    fn my_recommendations_read_my_channels_again_when_none_match() {
        let mut s = state();
        s.config.session.browser = "brave".into();
        s.update(Action::SubsLoaded(HashSet::from(["UCold".to_string()])));
        let id = match open(&mut s, Section::Inicio).as_slice() {
            [Effect::Load { id, .. }, ..] => *id,
            other => panic!("{other:?}"),
        };
        let feed = Items::Videos(vec![video("a"), video("b")]);
        assert_eq!(s.update(Action::Loaded(id, Ok(feed))), [Effect::LoadSubs], "an old copy of my channels");
        assert!(s.status.contains("Releyendo"), "{}", s.status);
        let subs = HashSet::from(["UCb".to_string()]);
        s.update(Action::SessionReady { cookies: None, subs: Ok(subs) });
        assert_eq!(s.listing().unwrap().items, Items::Videos(vec![video("b")]));
        // Only once: still none of mine shows an empty list.
        let id = match open(&mut s, Section::Inicio).as_slice() {
            [Effect::Load { id, .. }, ..] => *id,
            other => panic!("{other:?}"),
        };
        assert!(s.update(Action::Loaded(id, Ok(Items::Videos(vec![video("a")])))).is_empty());
        assert!(s.listing().unwrap().items.is_empty());
    }

    #[test]
    fn discover_asks_for_topics_then_searches_them() {
        let mut s = state();
        assert!(open(&mut s, Section::Descubrir).is_empty());
        assert_eq!(s.prompt, Some(Prompt::Topics));
        type_text(&mut s, "rust, nba");
        let topics = vec!["rust".to_string(), "nba".into()];
        match s.update(Action::Enter).as_slice() {
            [Effect::SaveSetting { table: "search", key: "topics", value }, Effect::Load { source, .. }] => {
                assert_eq!(value, &SettingValue::List(topics.clone()));
                assert_eq!(source, &Source::Discover(topics.clone()));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(s.setting_value(Setting::DiscoverTopics), "rust, nba");
        match open(&mut s, Section::Descubrir).as_slice() {
            [Effect::Load { source, .. }, ..] => assert_eq!(source, &Source::Discover(topics)),
            other => panic!("{other:?}"),
        }
        assert_eq!(s.pending.as_ref().map(|p| p.filter), Some(Filter::NotMine), "out of my channels");
    }

    #[test]
    fn search_all_youtube_by_default() {
        let mut s = state();
        assert!(open(&mut s, Section::BuscarVideos).is_empty());
        assert_eq!(s.prompt, Some(Prompt::Videos { scope: Scope::All, channel: None }));
        type_text(&mut s, "rust x");
        s.update(Action::Backspace);
        s.update(Action::Backspace);
        match s.update(Action::Enter).as_slice() {
            [Effect::Search { query, next: None, .. }] => assert_eq!(query, "rust"),
            other => panic!("{other:?}"),
        }
        assert_eq!(s.prompt, None);
    }

    #[test]
    fn videos_from_the_selection_feed_the_thumbnails() {
        let mut s = with_videos(&["a", "b", "c", "d"]);
        s.update(Action::Down);
        let ids = |s: &State, n| s.videos_from_selection(n).iter().map(|v| v.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&s, 2), ["b", "c", "d"]);
        assert_eq!(ids(&s, 10), ["b", "c", "d"]);
        play(&mut s, 0);
        assert!(s.videos_from_selection(3).is_empty(), "not while watching");
    }

    #[test]
    fn search_results_come_in_pages_loaded_while_scrolling() {
        let mut s = state();
        s.update(Action::FocusSearch);
        type_text(&mut s, "rust");
        let id = match s.update(Action::Enter).as_slice() {
            [Effect::Search { id, .. }] => *id,
            other => panic!("{other:?}"),
        };
        let page = |ids: &[&str], next: Option<&str>| SearchPage {
            videos: ids.iter().map(|i| video(i)).collect(),
            next: next.map(str::to_owned),
        };
        let first: Vec<String> = (0..8).map(|i| format!("v{i}")).collect();
        let first: Vec<&str> = first.iter().map(String::as_str).collect();
        s.update(Action::Page(id, Ok(page(&first, Some("T1")))));
        assert_eq!(s.listing().unwrap().items.len(), 8);
        assert!(s.update(Action::Down).is_empty(), "far from the end");
        assert!(s.update(Action::Down).is_empty(), "far from the end");
        let more = match s.update(Action::Down).as_slice() {
            [Effect::Search { id, next: Some(token), .. }] => {
                assert_eq!(token, "T1");
                *id
            }
            other => panic!("{other:?}"),
        };
        assert!(s.update(Action::Down).is_empty(), "one request at a time");
        s.update(Action::Page(more, Ok(page(&["v7", "v8", "v9"], None))));
        let listing = s.listing().unwrap();
        assert_eq!(listing.items.len(), 10, "appended without duplicates");
        assert_eq!((listing.selected, listing.next.as_deref(), listing.more), (4, None, None));
        for _ in 0..10 {
            assert!(s.update(Action::Down).is_empty(), "no more pages");
        }
    }

    #[test]
    fn search_in_my_channels_filters_results() {
        let mut s = state();
        s.update(Action::SubsLoaded(HashSet::from(["UCa".to_string()])));
        s.update(Action::FocusSearch);
        s.update(Action::CycleScope);
        type_text(&mut s, "rust");
        let id = match s.update(Action::Enter).as_slice() {
            [Effect::Load { id, source: Source::Search(_), limit: FILTERED_LIMIT }] => *id,
            other => panic!("{other:?}"),
        };
        s.update(Action::Loaded(id, Ok(Items::Videos(vec![video("a"), video("b")]))));
        assert_eq!(s.listing().unwrap().items, Items::Videos(vec![video("a")]));
        assert_eq!(s.listing().unwrap().title, "rust · mis canales");
    }

    #[test]
    fn search_inside_a_channel_starts_scoped_and_cycles() {
        let mut s = inside_channel();
        s.update(Action::FocusSearch);
        let here = ChannelRef { name: "name UCx".into(), url: "https://www.youtube.com/channel/UCx".into() };
        assert_eq!(s.prompt, Some(Prompt::Videos { scope: Scope::Channel(here.clone()), channel: Some(here.clone()) }));
        type_text(&mut s, "tui");
        match s.update(Action::Enter).as_slice() {
            [Effect::Load { source: Source::SearchInChannel { url, query }, .. }] => {
                assert_eq!((url.as_str(), query.as_str()), (here.url.as_str(), "tui"));
            }
            other => panic!("{other:?}"),
        }
        // Tab cycles: canal → todo → mis canales → canal.
        s.update(Action::FocusSearch);
        let scopes: Vec<String> = (0..3)
            .map(|_| {
                s.update(Action::CycleScope);
                match &s.prompt {
                    Some(Prompt::Videos { scope, .. }) => scope.label(Lang::Es),
                    other => panic!("{other:?}"),
                }
            })
            .collect();
        assert_eq!(scopes, ["todo YouTube", "mis canales", "canal name UCx"]);
    }

    #[test]
    fn without_a_channel_scope_cycles_between_two() {
        let mut s = state();
        s.update(Action::FocusSearch);
        s.update(Action::CycleScope);
        s.update(Action::CycleScope);
        assert_eq!(s.prompt, Some(Prompt::Videos { scope: Scope::All, channel: None }));
    }

    #[test]
    fn channel_search_prompt() {
        let mut s = state();
        open(&mut s, Section::BuscarCanales);
        type_text(&mut s, "tango");
        assert!(matches!(s.update(Action::Enter).as_slice(), [Effect::Load { source: Source::ChannelSearch(_), .. }]));
    }

    #[test]
    fn blank_search_does_nothing_and_esc_cancels() {
        let mut s = state();
        s.update(Action::FocusSearch);
        s.update(Action::Input(' '));
        assert!(s.update(Action::Enter).is_empty());
        s.update(Action::Back);
        assert_eq!(s.prompt, None);
        assert_eq!(s.focus, Focus::Menu);
    }

    #[test]
    fn search_from_a_tab_goes_home() {
        let mut s = with_videos(&["a"]);
        play(&mut s, 0);
        s.update(Action::FocusSearch);
        assert_eq!(s.active, 0);
        assert!(s.prompt.is_some());
    }

    #[test]
    fn list_selection_is_clamped() {
        let mut s = with_videos(&["a", "b"]);
        s.update(Action::Up);
        assert_eq!(s.listing().unwrap().selected, 0);
        s.update(Action::Down);
        s.update(Action::Down);
        assert_eq!(s.listing().unwrap().selected, 1);
    }

    #[test]
    fn channel_list_pushes_and_esc_pops() {
        let mut s = inside_channel();
        assert_eq!(s.stack.len(), 2);
        assert_eq!(s.listing().unwrap().title, "name UCx");
        assert!(matches!(s.listing().unwrap().kind, ListKind::Channel(_)));
        s.update(Action::Back);
        assert_eq!((s.stack.len(), s.focus), (1, Focus::List));
        s.update(Action::Back);
        assert_eq!(s.focus, Focus::Menu);
    }

    #[test]
    fn open_channel_from_a_video() {
        let mut s = with_videos(&["a"]);
        match s.update(Action::OpenChannel).as_slice() {
            [Effect::Load { source: Source::Channel(url), .. }] => assert!(url.ends_with("UCa")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn play_opens_a_tab_and_switches_to_it() {
        let mut s = with_videos(&["a", "b"]);
        let id = play(&mut s, 1);
        assert_eq!(s.active, 1);
        assert_eq!(s.active_tab().unwrap(), &Tab::new(id, Playback::youtube(video("b"))));
    }

    #[test]
    fn playing_an_open_video_reuses_its_tab() {
        let mut s = with_videos(&["a", "b"]);
        play(&mut s, 0);
        play(&mut s, 1);
        s.update(Action::GoTab(0));
        s.stack[0].selected = 0;
        assert!(s.update(Action::Enter).is_empty());
        assert_eq!((s.tabs.len(), s.active), (2, 1));
    }

    #[test]
    fn max_tabs_is_enforced() {
        let config = Config { ui: UiConfig { max_tabs: 1, ..Default::default() }, ..Default::default() };
        let mut s = State::new(config, vec![]);
        let id = load_id(&open(&mut s, Section::Suscripciones));
        s.update(Action::Loaded(id, Ok(Items::Videos(vec![video("a"), video("b")]))));
        play(&mut s, 0);
        s.update(Action::GoTab(0));
        s.stack[0].selected = 1;
        assert!(s.update(Action::Enter).is_empty());
        assert!(s.status.contains("Máximo"));
    }

    #[test]
    fn tabs_navigate_and_close_like_a_browser() {
        let mut s = with_videos(&["a", "b", "c"]);
        play(&mut s, 0);
        let b = play(&mut s, 1);
        play(&mut s, 2);
        s.update(Action::NextTab);
        assert_eq!(s.active, 0);
        s.update(Action::PrevTab);
        assert_eq!(s.active, 3);
        s.update(Action::GoTab(9));
        assert_eq!(s.active, 3);
        s.update(Action::GoTab(2));
        assert_eq!(s.update(Action::CloseTab), vec![Effect::Close(b)]);
        assert_eq!(s.active_tab().unwrap().playback.video, Some(video("c")));
        s.update(Action::CloseTab);
        s.update(Action::CloseTab);
        assert_eq!(s.active, 0);
        assert!(s.update(Action::CloseTab).is_empty());
    }

    #[test]
    fn player_closed_drops_tab_without_closing_again() {
        let mut s = with_videos(&["a", "b"]);
        let a = play(&mut s, 0);
        play(&mut s, 1);
        assert!(s.update(Action::PlayerClosed(a, Some("mpv no está".into()))).is_empty());
        assert_eq!(s.active_tab().unwrap().playback.video, Some(video("b")));
        assert_eq!(s.status, "mpv no está");
        s.update(Action::PlayerClosed(TabId(99), None));
        assert_eq!(s.tabs.len(), 1);
    }

    #[test]
    fn player_commands_target_the_active_tab() {
        let mut s = with_videos(&["a"]);
        let a = play(&mut s, 0);
        for (action, command) in [
            (Action::TogglePause, PlayerCommand::TogglePause),
            (Action::Seek(-5), PlayerCommand::Seek(-5)),
            (Action::SeekTo(90), PlayerCommand::SeekTo(90)),
            (Action::Volume(5), PlayerCommand::Volume(5)),
            (Action::ToggleMute, PlayerCommand::ToggleMute),
            (Action::CycleSubtitles, PlayerCommand::CycleSubtitles),
        ] {
            assert_eq!(s.update(action), vec![Effect::Command(a, command)]);
        }
        s.update(Action::GoTab(0));
        assert!(s.update(Action::Volume(5)).is_empty());
    }

    #[test]
    fn later_toggles_from_list_and_tab_and_persists() {
        let mut s = with_videos(&["a", "b"]);
        assert_eq!(s.update(Action::ToggleLater), vec![Effect::SaveLater(vec![video("a")])]);
        play(&mut s, 1);
        assert_eq!(s.update(Action::ToggleLater), vec![Effect::SaveLater(vec![video("b"), video("a")])]);
        s.update(Action::ToggleLater);
        assert_eq!(s.later, vec![video("a")]);
        assert!(s.status.contains("Quitado"));
    }

    #[test]
    fn later_section_shows_and_tracks_the_list() {
        let mut s = State::new(Config::default(), vec![video("a"), video("b")]);
        assert!(open(&mut s, Section::VerDespues).is_empty());
        assert_eq!(s.listing().unwrap().kind, ListKind::Later);
        s.update(Action::Down);
        s.update(Action::ToggleLater);
        assert_eq!(s.listing().unwrap().items, Items::Videos(vec![video("a")]));
        assert_eq!(s.listing().unwrap().selected, 0);
    }

    #[test]
    fn later_needs_a_video() {
        let mut s = state();
        assert!(s.update(Action::ToggleLater).is_empty());
    }

    #[test]
    fn subscribe_directly_but_confirm_unsubscribe() {
        let mut s = with_videos(&["a"]);
        let subscribe = Effect::Subscribe { channel_id: "UCa".into(), name: "chan a".into(), subscribe: true };
        assert_eq!(s.update(Action::ToggleSubscription), vec![subscribe]);
        s.update(Action::SubscriptionDone { channel_id: "UCa".into(), name: "chan a".into(), result: Ok(true) });
        assert!(s.subs.contains("UCa"));

        assert!(s.update(Action::ToggleSubscription).is_empty());
        assert!(s.key_context().confirm);
        assert!(s.update(Action::Confirm(false)).is_empty());
        assert_eq!(s.confirm, None);

        s.update(Action::ToggleSubscription);
        let unsubscribe = Effect::Subscribe { channel_id: "UCa".into(), name: "chan a".into(), subscribe: false };
        assert_eq!(s.update(Action::Confirm(true)), vec![unsubscribe]);
        let effects =
            s.update(Action::SubscriptionDone { channel_id: "UCa".into(), name: "chan a".into(), result: Ok(false) });
        assert_eq!(effects, vec![Effect::SaveSubs(HashSet::new())]);
    }

    #[test]
    fn subscription_failure_keeps_state() {
        let mut s = with_videos(&["a"]);
        let done = Action::SubscriptionDone { channel_id: "UCa".into(), name: "n".into(), result: Err("403".into()) };
        assert!(s.update(done).is_empty());
        assert!(s.subs.is_empty());
        assert!(s.status.contains("403"));
    }

    fn check(tool: crate::doctor::Tool, found: bool) -> Check {
        let found = found.then(|| (format!("/usr/bin/{}", tool.program()), Some("1.0".to_string())));
        Check { tool, found, outdated: false, fix: None }
    }

    #[test]
    fn the_system_check_opens_by_itself_only_when_something_needed_is_missing() {
        use crate::doctor::Tool;
        let mut s = state();
        s.update(Action::SystemChecked(
            Manager::Apt,
            vec![check(Tool::Mpv, true), check(Tool::YtDlp, true), check(Tool::Ffmpeg, false)],
        ));
        assert!(s.stack.is_empty(), "all needed is there: no page");
        assert!(s.status.starts_with("1 opcionales"), "{}", s.status);
        s.update(Action::SystemChecked(Manager::Apt, vec![check(Tool::Mpv, false), check(Tool::YtDlp, true)]));
        assert_eq!(s.listing().unwrap().items, Items::System);
        assert!(s.status.contains("mpv"), "{}", s.status);
        assert_eq!(s.update(Action::Enter), [Effect::CheckSystem], "Enter checks again");
        assert_eq!(s.stack.len(), 1, "not opened twice");
        assert_eq!(s.setting_value(Setting::SystemCheck), "falta algo necesario");
    }

    #[test]
    fn feeds_hide_watched_videos_and_v_shows_them() {
        let mut s = state();
        s.watched.insert("b".into());
        let id = load_id(&open(&mut s, Section::Suscripciones));
        s.update(Action::Loaded(id, Ok(Items::Videos(vec![video("a"), video("b"), video("c")]))));
        let ids = |s: &State| match &s.listing().unwrap().items {
            Items::Videos(v) => v.iter().map(|v| v.id.clone()).collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(ids(&s), ["a", "c"]);
        assert!(s.status.contains("1 vistos ocultos"), "{}", s.status);
        // My YouTube history arrives later: c goes too, the selection stays on a.
        s.update(Action::HistoryLoaded(HashSet::from(["c".to_string()])));
        assert_eq!(ids(&s), ["a"]);
        s.update(Action::ToggleWatched);
        assert_eq!(ids(&s), ["a", "b", "c"]);
        assert_eq!(s.listing().unwrap().selected, 0, "still on a");
        s.stack[0].selected = 2;
        s.update(Action::ToggleWatched);
        assert_eq!(ids(&s), ["a"]);
        assert_eq!(s.listing().unwrap().selected, 0, "c is hidden: clamped");
    }

    #[test]
    fn playing_marks_the_video_watched() {
        let mut s = with_videos(&["a"]);
        let effects = s.update(Action::Enter);
        assert!(effects.contains(&Effect::SaveWatched("a".into())));
        assert!(s.watched.contains("a"));
        let mut logged = state();
        logged.config.session.browser = "brave".into();
        assert!(open(&mut logged, Section::Inicio).contains(&Effect::LoadHistory), "my history, to hide it");
        assert!(!open(&mut logged, Section::Canales).contains(&Effect::LoadHistory));
    }

    #[test]
    fn marked_files_go_to_the_trash_after_asking() {
        let mut s = state();
        let id = load_id(&open(&mut s, Section::MiMusica));
        s.update(Action::Loaded(id, Ok(Items::Files(files(&["a", "b", "c"]), MediaKind::Music))));
        s.update(Action::ToggleMark); // a, and moves to b
        s.update(Action::Down); // c
        s.update(Action::ToggleMark);
        assert_eq!(s.marked.len(), 2);
        assert!(s.status.starts_with("2 marcados"), "{}", s.status);
        s.update(Action::Trash);
        let marked = vec![PathBuf::from("/m/a.mp3"), PathBuf::from("/m/c.mp3")];
        assert_eq!(s.confirm, Some(Confirm::Trash(marked.clone())));
        assert!(s.key_context().confirm);
        assert_eq!(s.update(Action::Confirm(true)), [Effect::Trash(marked.clone())]);
        s.update(Action::Trashed(Ok(marked)));
        assert_eq!(s.listing().unwrap().items, Items::Files(files(&["b"]), MediaKind::Music));
        assert!(s.marked.is_empty());
        assert!(s.status.contains("2 a la papelera"), "{}", s.status);
        // Nothing marked: the selected file; saying no keeps it.
        s.update(Action::Trash);
        assert_eq!(s.confirm, Some(Confirm::Trash(vec![PathBuf::from("/m/b.mp3")])));
        assert!(s.update(Action::Confirm(false)).is_empty());
        s.update(Action::Trashed(Err("permiso denegado".into())));
        assert!(s.status.contains("permiso denegado"));
        assert_eq!(s.listing().unwrap().items.len(), 1);
    }

    #[test]
    fn marking_does_nothing_outside_file_lists() {
        let mut s = with_videos(&["a"]);
        assert!(s.update(Action::ToggleMark).is_empty());
        s.update(Action::Trash);
        assert_eq!(s.confirm, None);
    }

    #[test]
    fn unsubscribing_takes_the_channel_out_of_my_channels() {
        let mut s = state();
        let id = load_id(&open(&mut s, Section::Canales));
        s.update(Action::Loaded(id, Ok(Items::Channels(vec![channel("UCa"), channel("UCb")]))));
        s.subs.extend(["UCa".to_string(), "UCb".to_string()]);
        s.stack[0].selected = 1;
        assert!(s.update(Action::ToggleSubscription).is_empty(), "asks first");
        assert!(matches!(s.update(Action::Confirm(true)).as_slice(), [Effect::Subscribe { subscribe: false, .. }]));
        s.update(Action::SubscriptionDone { channel_id: "UCb".into(), name: "b".into(), result: Ok(false) });
        assert_eq!(s.listing().unwrap().items, Items::Channels(vec![channel("UCa")]));
        assert_eq!(s.listing().unwrap().selected, 0);
    }

    #[test]
    fn subscribe_from_channel_list() {
        let mut s = state();
        let id = load_id(&open(&mut s, Section::Canales));
        s.update(Action::Loaded(id, Ok(Items::Channels(vec![channel("UCx")]))));
        assert!(matches!(s.update(Action::ToggleSubscription).as_slice(), [Effect::Subscribe { subscribe: true, .. }]));
    }

    #[test]
    fn video_without_channel_cannot_subscribe() {
        let mut s = state();
        let id = load_id(&open(&mut s, Section::Suscripciones));
        s.update(Action::Loaded(id, Ok(Items::Videos(vec![Video { channel_id: None, ..video("a") }]))));
        assert!(s.update(Action::ToggleSubscription).is_empty());
        assert_eq!(s.status, "Este video no tiene canal");
    }

    #[test]
    fn download_selected_video() {
        let mut s = with_videos(&["a"]);
        assert_eq!(s.update(Action::Download), vec![Effect::Download { id: 0, video: video("a") }]);
        s.focus = Focus::Menu;
        assert!(s.update(Action::Download).is_empty());
    }

    #[test]
    fn settings_screen_lists_every_setting() {
        let mut s = state();
        assert!(open(&mut s, Section::Configuracion).is_empty());
        assert_eq!(s.listing().unwrap().kind, ListKind::Settings);
        assert_eq!(s.listing().unwrap().items.len(), Setting::ALL.len());
        assert_eq!(s.setting_value(Setting::Quality), "720p");
        assert_eq!(s.setting_value(Setting::DownloadDir), "~/Videos");
        assert_eq!(s.setting_value(Setting::Browser), "sin sesión");
    }

    #[test]
    fn cycling_settings_saves_them() {
        let mut s = state();
        assert_eq!(change(&mut s, Setting::Language), [saved("player", "language", text("original"))]);
        assert_eq!(s.config.player.language, Language::Original);
        assert_eq!(change(&mut s, Setting::Quality), [saved("player", "max_height", SettingValue::Number(1080))]);
        assert_eq!(
            change(&mut s, Setting::VideoMode),
            [saved("player", "mode", text("overlay"))],
            "from the default, window"
        );
        assert_eq!(s.config.player.mode, PlayerMode::Overlay);
        assert_eq!(change(&mut s, Setting::Icons), [saved("ui", "icons", text("nerd"))]);
        assert_eq!(s.config.ui.icons, IconSet::Nerd);
        assert_eq!(change(&mut s, Setting::Theme), [saved("ui", "theme", text("catppuccin"))]);
        assert_eq!(change(&mut s, Setting::Border), [saved("ui", "border", text("plain"))]);
        assert_eq!(change(&mut s, Setting::EditFile), [Effect::EditConfig]);
    }

    #[test]
    fn subtitle_language_cycles_and_saves() {
        let mut s = state();
        assert_eq!(s.setting_value(Setting::SubLang), "español (es)");
        assert_eq!(change(&mut s, Setting::SubLang), [saved("player", "sub_lang", text("en"))]);
        assert_eq!(s.setting_value(Setting::SubLang), "inglés (en)");
    }

    #[test]
    fn background_audio_toggles_and_saves() {
        let mut s = state();
        assert_eq!(s.setting_value(Setting::BackgroundAudio), "sigue sonando");
        assert_eq!(
            change(&mut s, Setting::BackgroundAudio),
            [saved("player", "background_audio", SettingValue::Bool(false))]
        );
        assert_eq!(s.setting_value(Setting::BackgroundAudio), "se pausa");
    }

    #[test]
    fn language_changes_the_current_video_and_is_saved() {
        let mut s = with_videos(&["a"]);
        let a = play(&mut s, 0);
        assert_eq!(
            s.update(Action::CycleLanguage),
            [
                saved("player", "language", text("original")),
                Effect::Command(a, PlayerCommand::Language(Language::Original)),
            ]
        );
        assert!(s.status.contains("recargando"));
        s.update(Action::GoTab(0));
        assert_eq!(s.update(Action::CycleLanguage), [saved("player", "language", text("dubbed"))]);
    }

    #[test]
    fn related_videos_take_the_keys_with_tab() {
        let mut s = with_videos(&["a"]);
        s.video_window = true;
        let a = play(&mut s, 0);
        s.update(Action::Watched(TabId(99), Ok(Watch::default())));
        assert_eq!(s.update(Action::ToggleFocus), [], "nothing related yet");
        assert!(!s.key_context().related);
        let watch = Watch { related: vec![video("r1"), video("r2"), video("r3")], ..Watch::default() };
        s.update(Action::Watched(a, Ok(watch)));
        s.update(Action::ToggleFocus);
        assert!(s.key_context().related);
        s.update(Action::Down);
        s.update(Action::Down);
        s.update(Action::Down);
        assert_eq!(s.active_tab().unwrap().related, 2, "stops at the end");
        s.update(Action::Up);
        assert_eq!(s.selected_video().unwrap().id, "r2", "l / s / d / c act on it");
        let effects = s.update(Action::Enter);
        assert!(
            matches!(effects.as_slice(), [Effect::Open(_, p), Effect::Watch { video, .. }, Effect::SaveWatched(_), Effect::SaveHistory(_), ..]
            if p.video.as_ref().unwrap().id == "r2" && video == "r2"),
            "{effects:?}"
        );
        assert_eq!(s.tabs.len(), 2);
        s.update(Action::GoTab(1));
        s.update(Action::Select(0));
        assert_eq!(s.active_tab().unwrap().related, 0);
        s.update(Action::ToggleFocus);
        assert!(!s.key_context().related, "Tab goes back to the player keys");
        s.video_window = false;
        s.update(Action::ToggleFocus);
        assert!(!s.key_context().related, "embedded: the list is under the video");
    }

    #[test]
    fn from_what_i_watched_asks_for_the_up_next_of_the_last_ones() {
        let mut s = state();
        assert!(open(&mut s, Section::ParaVos).is_empty(), "nothing watched yet");
        assert!(s.status.contains("Mirá algunos"), "{}", s.status);
        for id in ["a", "b", "c"] {
            s.remember(video(id), None);
        }
        match open(&mut s, Section::ParaVos).as_slice() {
            [Effect::Load { source: Source::UpNext(ids), .. }, ..] => assert_eq!(ids, &["c", "b", "a"]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn my_most_watched_channels_go_first() {
        const DAY: i64 = 86_400;
        let mut s = state();
        s.clock = 100 * DAY;
        let of = |id: &str, channel: &str, published: Option<i64>| Video {
            channel_id: Some(channel.into()),
            published,
            ..video(id)
        };
        for id in ["f1", "f2", "f3"] {
            s.remember(of(id, "UCfav", None), None);
        }
        s.remember(of("x", "UCone", None), None);
        assert!(s.favorites.contains("UCfav") && !s.favorites.contains("UCone"), "3 videos or more");

        let id = load_id(&open(&mut s, Section::Suscripciones));
        let live = Video { live: crate::domain::Live::Now, ..of("live", "UCother", None) };
        let videos = vec![
            live,
            of("other", "UCother", Some(99 * DAY)),
            of("fav", "UCfav", Some(98 * DAY)),
            of("fav-old", "UCfav", Some(80 * DAY)),
        ];
        s.update(Action::Loaded(id, Ok(Items::Videos(videos))));
        let ids = |s: &State| match &s.listing().unwrap().items {
            Items::Videos(v) => v.iter().map(|v| v.id.clone()).collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        assert_eq!(ids(&s), ["live", "fav", "other", "fav-old"], "an old one waits its turn");

        let id = load_id(&open(&mut s, Section::Canales));
        s.update(Action::Loaded(id, Ok(Items::Channels(vec![channel("UCone"), channel("UCx"), channel("UCfav")]))));
        match &s.listing().unwrap().items {
            Items::Channels(c) => {
                assert_eq!(c.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["UCfav", "UCone", "UCx"])
            }
            other => panic!("{other:?}"),
        }

        s.update(Action::SubsPolled(Ok(Vec::new())));
        let fresh = vec![of("n1", "UCother", Some(DAY)), of("n2", "UCfav", Some(DAY))];
        match s.update(Action::SubsPolled(Ok(fresh))).as_slice() {
            [Effect::Notify(message)] => assert!(message.starts_with("🔔 ⭐ chan n2"), "{message}"),
            other => panic!("{other:?}"),
        }

        change(&mut s, Setting::FavoritesFirst);
        assert!(s.favorites.is_empty(), "turned off");
    }

    #[test]
    fn the_selected_video_is_weighed_once() {
        let mut s = with_videos(&["a", "b"]);
        s.update(Action::GoTab(0));
        assert_eq!(s.size_wanted().unwrap().id, "a");
        s.update(Action::Sized("a".into(), Ok(Size { bytes: Some(1), ..Size::default() })));
        assert!(s.size_wanted().is_none(), "known");
        s.update(Action::Down);
        assert_eq!(s.size_wanted().unwrap().id, "b");
        s.update(Action::Sized("b".into(), Err("falló".into())));
        assert!(s.size_wanted().is_none(), "failed: not asked again");
    }

    #[test]
    fn a_closed_video_keeps_its_details_and_up_next() {
        let mut s = with_videos(&["a"]);
        s.video_window = true;
        let a = play(&mut s, 0);
        s.update(Action::Watched(a, Ok(Watch { related: vec![video("r1"), video("r2")], ..Watch::default() })));
        s.update(Action::Stopped { video: "a".into(), position: Some(125), duration: Some(600) });
        assert!(s.update(Action::PlayerClosed(a, None)).is_empty());
        let tab = s.active_tab().unwrap();
        assert!(tab.ended && tab.on_related, "the tab stays, on its up next list");
        s.update(Action::Down);
        let effects = s.update(Action::Enter);
        assert!(matches!(effects.as_slice(), [Effect::Open(_, p), ..] if p.video.as_ref().unwrap().id == "r2"));
        s.update(Action::GoTab(1));
        s.update(Action::ToggleFocus);
        match s.update(Action::Enter).as_slice() {
            [Effect::Open(id, p)] => assert!(*id == a && p.start == Some(125), "again, where it was left"),
            other => panic!("expected Open, got {other:?}"),
        }
        assert!(!s.active_tab().unwrap().ended);
        s.update(Action::PlayerClosed(a, Some("falló".into())));
        assert_eq!(s.tabs.len(), 1, "errors still close the tab");
        s.video_window = false;
        let r1 = s.tabs[0].id;
        s.update(Action::PlayerClosed(r1, None));
        assert!(s.tabs.is_empty(), "embedded: nothing left to show");
    }

    #[test]
    fn a_right_click_offers_what_can_be_done_there() {
        let mut s = with_videos(&["a", "b"]);
        s.update(Action::GoTab(0));
        s.update(Action::Select(1));
        s.update(Action::ContextMenu { x: 3, y: 4, row: true });
        let menu = s.context.as_ref().expect("a menu");
        assert_eq!(menu.at, (3, 4));
        assert_eq!(menu.items[0].action, Action::Enter);
        assert!(menu.items.iter().any(|i| i.action == Action::Download));
        // Moves stay in the menu; the background goes on.
        s.update(Action::Down);
        s.update(Action::Sized("a".into(), Err("falló".into())));
        assert_eq!(s.context.as_ref().unwrap().selected, 1);
        assert_eq!(s.listing().unwrap().selected, 1, "the list did not move");
        let later = s.context.as_ref().unwrap().items.iter().position(|i| i.action == Action::ToggleLater).unwrap();
        s.update(Action::MenuPick(later));
        assert!(s.context.is_none());
        assert_eq!(s.later[0].id, "b", "done to the clicked video");
        s.update(Action::ContextMenu { x: 0, y: 0, row: true });
        assert_eq!(s.context.as_ref().unwrap().items[1].label, "Solo audio");
        assert!(s.context.as_ref().unwrap().items.iter().any(|i| i.label == "Quitar de Ver después"));
        s.update(Action::Back);
        assert!(s.context.is_none() && s.listing().is_some(), "Esc closes only the menu");

        let a = play(&mut s, 0);
        s.update(Action::ContextMenu { x: 0, y: 0, row: false });
        let actions: Vec<_> = s.context.as_ref().unwrap().items.iter().map(|i| i.action.clone()).collect();
        assert_eq!(actions[0], Action::TogglePause);
        assert_eq!(actions.last(), Some(&Action::CloseTab));
        assert_eq!(s.update(Action::MenuPick(actions.len() - 1)), [Effect::Close(a)]);
    }

    #[test]
    fn mouse_actions_open_select_and_scroll() {
        let mut s = with_videos(&["a", "b", "c"]);
        s.focus = Focus::Menu;
        assert!(s.update(Action::Select(2)).is_empty());
        assert_eq!((s.listing().unwrap().selected, s.focus), (2, Focus::List));
        s.update(Action::Select(99));
        assert_eq!(s.listing().unwrap().selected, 2, "clamped to the list");
        s.update(Action::Scroll(-3));
        assert_eq!(s.listing().unwrap().selected, 0);
        let settings = Section::ALL.iter().position(|x| *x == Section::Configuracion).unwrap();
        s.update(Action::OpenSection(settings));
        assert_eq!((s.menu, s.listing().unwrap().items.clone()), (settings, Items::Settings));
        assert!(s.update(Action::OpenSection(99)).is_empty());
    }

    #[test]
    fn sub_language_changes_the_current_video_and_is_saved() {
        let mut s = with_videos(&["a"]);
        let a = play(&mut s, 0);
        assert_eq!(
            s.update(Action::CycleSubLang),
            [saved("player", "sub_lang", text("en")), Effect::Command(a, PlayerCommand::SubLang("en"))]
        );
        assert_eq!(s.config.player.sub_lang, "en");
    }

    #[test]
    fn quality_and_codec_change_the_current_video_and_are_saved() {
        let mut s = with_videos(&["a"]);
        let a = play(&mut s, 0);
        assert_eq!(
            s.update(Action::CycleQuality),
            [
                saved("player", "max_height", SettingValue::Number(1080)),
                Effect::Command(a, PlayerCommand::Quality(1080))
            ]
        );
        assert!(s.status.contains("1080p"), "{}", s.status);
        assert_eq!(
            s.update(Action::CycleCodec),
            [saved("player", "prefer_codec", text("vp")), Effect::Command(a, PlayerCommand::Codec("vp"))]
        );
        assert!(s.status.contains("VP9"), "{}", s.status);
        assert_eq!(s.config.player.prefer_codec, "vp");
    }

    fn files(names: &[&str]) -> Vec<LocalFile> {
        names
            .iter()
            .map(|n| LocalFile {
                path: format!("/m/{n}.mp3").into(),
                name: n.to_string(),
                folder: "Disco".into(),
                size: 0,
            })
            .collect()
    }

    #[test]
    fn menu_is_grouped() {
        let groups: Vec<Group> = Section::ALL.iter().map(|s| s.group()).collect();
        assert_eq!(groups[0], Group::YouTube);
        assert_eq!(Section::MisVideos.group(), Group::Local);
        assert_eq!(Section::MiMusica.group(), Group::Local);
        assert_eq!(groups.last(), Some(&Group::App));
        // Groups are contiguous, so each header appears once.
        assert!(groups.windows(2).filter(|w| w[0] != w[1]).count() == 2);
    }

    #[test]
    fn local_sections_scan_their_kind() {
        let mut s = state();
        assert!(matches!(
            open(&mut s, Section::MisVideos).as_slice(),
            [Effect::ScanMedia { kind: MediaKind::Video, .. }]
        ));
        assert!(matches!(
            open(&mut s, Section::MiMusica).as_slice(),
            [Effect::ScanMedia { kind: MediaKind::Music, .. }]
        ));
    }

    #[test]
    fn music_plays_from_the_chosen_song_to_the_end() {
        let mut s = state();
        let id = match open(&mut s, Section::MiMusica).as_slice() {
            [Effect::ScanMedia { id, .. }] => *id,
            other => panic!("{other:?}"),
        };
        s.update(Action::Loaded(id, Ok(Items::Files(files(&["a", "b", "c"]), MediaKind::Music))));
        s.update(Action::Down);
        let playback = match s.update(Action::Enter).as_slice() {
            [Effect::Open(_, playback)] => playback.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(playback.queue, ["/m/b.mp3", "/m/c.mp3"]);
        assert!(playback.audio_only);
        assert_eq!((playback.title.as_str(), playback.video), ("b", None));
        let tab = s.active_tab().unwrap().id;
        assert_eq!(s.update(Action::NextTrack), [Effect::Command(tab, PlayerCommand::NextTrack)]);
        // No YouTube video: watch later / subscribe do nothing.
        assert!(s.update(Action::ToggleLater).is_empty());
    }

    #[test]
    fn a_local_video_plays_alone() {
        let playback = Playback::local(&files(&["a", "b"]), 0, MediaKind::Video).unwrap();
        assert_eq!(playback.queue, ["/m/a.mp3"]);
        assert!(!playback.audio_only);
        assert!(Playback::local(&files(&["a"]), 5, MediaKind::Video).is_none());
    }

    #[test]
    fn media_folders_are_edited_as_a_list() {
        let mut s = state();
        assert_eq!(s.setting_value(Setting::MusicDirs), "~/Música (del sistema)");
        assert!(change(&mut s, Setting::MusicDirs).is_empty());
        assert_eq!(s.prompt, Some(Prompt::MediaDirs(MediaKind::Music)));
        type_text(&mut s, "~/Música, /mnt/discos");
        let list = SettingValue::List(vec!["~/Música".into(), "/mnt/discos".into()]);
        assert_eq!(s.update(Action::Enter), [saved("media", "music_dirs", list)]);
        assert_eq!(s.setting_value(Setting::MusicDirs), "~/Música, /mnt/discos");
    }

    #[test]
    fn menu_is_reachable_from_anywhere() {
        let mut s = with_videos(&["a"]);
        play(&mut s, 0);
        s.update(Action::ToggleSidebar);
        s.update(Action::FocusMenu);
        assert_eq!((s.active, s.focus, s.sidebar), (0, Focus::Menu, true));
    }

    #[test]
    fn tab_switches_between_menu_and_list() {
        let mut s = state();
        s.update(Action::ToggleFocus);
        assert_eq!(s.focus, Focus::Menu, "nothing to focus yet");
        let mut s = with_videos(&["a"]);
        s.update(Action::ToggleFocus);
        assert_eq!(s.focus, Focus::Menu);
        s.update(Action::ToggleFocus);
        assert_eq!(s.focus, Focus::List);
    }

    #[test]
    fn fullscreen_only_for_a_video_and_esc_leaves_it() {
        let mut s = with_videos(&["a"]);
        s.update(Action::ToggleFullscreen);
        assert!(!s.fullscreen, "no video on Inicio");
        let a = play(&mut s, 0);
        assert_eq!(s.update(Action::ToggleFullscreen), [Effect::Command(a, PlayerCommand::Fullscreen(true))]);
        assert!(s.fullscreen);
        s.update(Action::Back);
        assert!(!s.fullscreen);
        s.update(Action::ToggleFullscreen);
        s.update(Action::GoTab(0));
        assert!(!s.fullscreen, "leaving the tab ends fullscreen");
    }

    #[test]
    fn download_dir_is_edited_in_place() {
        let mut s = state();
        assert!(change(&mut s, Setting::DownloadDir).is_empty());
        assert_eq!(s.prompt, Some(Prompt::DownloadDir));
        type_text(&mut s, "~/Descargas");
        assert_eq!(s.update(Action::Enter), [saved("download", "dir", text("~/Descargas"))]);
        assert_eq!(s.config.download.dir, "~/Descargas");
    }

    #[test]
    fn browser_session_is_picked_from_the_scan() {
        let mut s = state();
        let id = load_id(&change(&mut s, Setting::Browser));
        s.update(Action::Loaded(id, Ok(Items::Profiles(vec!["brave:Default".into(), String::new()]))));
        assert_eq!(s.stack.len(), 2);
        assert_eq!(
            s.update(Action::Enter),
            [
                saved("session", "asked", SettingValue::Bool(true)),
                saved("session", "browser", text("brave:Default")),
                saved("session", "cookies_file", text("")),
                Effect::StartSession { force: true },
            ]
        );
        assert_eq!(s.session_label(), "brave · Default");
        if cfg!(target_os = "linux") {
            assert!(s.status.contains("llavero"), "{}", s.status);
        }
    }

    #[test]
    fn cookies_file_import_replaces_the_browser() {
        let mut s = state();
        s.config.session.browser = "brave".into();
        assert!(change(&mut s, Setting::ImportCookies).is_empty());
        assert_eq!(s.prompt, Some(Prompt::CookiesFile));
        type_text(&mut s, "~/cookies.txt");
        assert_eq!(s.update(Action::Enter), [Effect::ImportCookies("~/cookies.txt".into())]);
        let effects = s.update(Action::CookiesImported(Ok("/cache/cookies-imported.txt".into())));
        assert_eq!(effects[0], saved("session", "browser", text("")));
        assert_eq!(effects[1], saved("session", "cookies_file", text("/cache/cookies-imported.txt")));
        assert_eq!(s.session_label(), "cookies.txt importado");
        s.update(Action::CookiesImported(Err("no tiene sesión".into())));
        assert_eq!(s.status, "no tiene sesión");
    }

    #[test]
    fn like_toggles_and_needs_a_login() {
        let mut s = with_videos(&["a"]);
        assert!(s.update(Action::Rate(Rating::Like)).is_empty());
        assert!(s.status.contains("Sin sesión"), "{}", s.status);
        s.config.session.browser = "brave".into();
        let rate = |rating| Effect::Rate { video: "a".into(), title: "title a".into(), rating };
        assert_eq!(s.update(Action::Rate(Rating::Like)), [rate(Rating::Like)]);
        let done = |rating, result| Action::Rated { video: "a".into(), title: "title a".into(), rating, result };
        s.update(done(Rating::Like, Ok(())));
        assert_eq!(s.ratings["a"], Rating::Like);
        assert!(s.status.contains("Te gusta: title a"), "{}", s.status);
        assert_eq!(s.update(Action::Rate(Rating::Like)), [rate(Rating::None)], "again takes it back");
        assert_eq!(s.update(Action::Rate(Rating::Dislike)), [rate(Rating::Dislike)]);
        s.update(done(Rating::None, Err("HTTP 401".into())));
        assert_eq!(s.ratings["a"], Rating::Like, "a failed vote changes nothing");
        assert!(s.status.contains("HTTP 401"), "{}", s.status);
    }

    #[test]
    fn logout_and_refresh() {
        let mut s = state();
        s.config.session.browser = "brave".into();
        let effects = change(&mut s, Setting::Logout);
        assert_eq!(effects.last(), Some(&Effect::StartSession { force: true }));
        assert_eq!(s.session_label(), "sin sesión");
        assert_eq!(change(&mut s, Setting::Refresh), [Effect::StartSession { force: true }]);
    }

    #[test]
    fn first_start_asks_for_a_browser_once() {
        let mut s = state();
        let id = match s.update(Action::AskBrowser).as_slice() {
            [Effect::ScanBrowsers { id }] => *id,
            other => panic!("{other:?}"),
        };
        s.update(Action::Loaded(id, Ok(Items::Profiles(vec!["firefox:x".into(), String::new()]))));
        assert!(s.status.contains("Elegí"), "{}", s.status);
        let effects = s.update(Action::Enter);
        assert_eq!(effects[0], saved("session", "asked", SettingValue::Bool(true)));
        assert!(effects.contains(&Effect::StartSession { force: true }));
        assert!(s.status.contains("firefox · x"), "{}", s.status);
        assert!(s.update(Action::AskBrowser).is_empty(), "asked already");
        let mut logged = state();
        logged.config.session.browser = "brave".into();
        assert!(logged.update(Action::AskBrowser).is_empty());
        let mut bare = state();
        let id = match bare.update(Action::AskBrowser).as_slice() {
            [Effect::ScanBrowsers { id }] => *id,
            other => panic!("{other:?}"),
        };
        bare.update(Action::Loaded(id, Ok(Items::Profiles(vec![String::new()]))));
        assert!(bare.status.contains("cookies.txt"), "{}", bare.status);
        assert!(bare.status.contains("iniciá sesión en youtube.com"), "{}", bare.status);
    }

    #[test]
    fn session_ready_updates_channels() {
        let mut s = state();
        s.config.session.browser = "brave:Default".into();
        let subs = HashSet::from(["UCa".to_string()]);
        let effects = s.update(Action::SessionReady { cookies: None, subs: Ok(subs.clone()) });
        assert_eq!(effects, vec![Effect::SaveSubs(subs)]);
        assert!(s.status.contains("1 canales"));
        assert!(s.update(Action::SessionReady { cookies: None, subs: Err("keyring".into()) }).is_empty());
        assert_eq!(s.status, "keyring");
    }

    #[test]
    fn reloaded_config_restarts_session_only_when_it_changed() {
        let mut s = state();
        let mut edited = Config::default();
        edited.player.max_height = 480;
        assert!(s.update(Action::ConfigReloaded(Ok(edited.clone()))).is_empty());
        assert_eq!(s.config.player.max_height, 480);
        edited.session.browser = "firefox".into();
        assert_eq!(s.update(Action::ConfigReloaded(Ok(edited))), [Effect::StartSession { force: true }]);
        s.update(Action::ConfigReloaded(Err("línea 3".into())));
        assert_eq!(s.status, "línea 3");
    }

    #[test]
    fn salir_quits() {
        let mut s = state();
        open(&mut s, Section::Salir);
        assert!(s.quit);
    }
}
