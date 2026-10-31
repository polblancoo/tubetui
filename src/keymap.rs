//! Keys → actions. Global commands live behind the `Ctrl+y` prefix
//! (tmux style: the prefix waits for exactly one more key); plain keys go to
//! whatever has focus, so the video tab can use space and arrows freely.
//! The help screen is generated from the same tables.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::Action;
use crate::i18n::{Lang, Text};
use crate::youtube::Rating;

/// The slice of state that decides how a key is read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyContext {
    /// `Ctrl+y` was pressed and the next key is a command.
    pub prefix: bool,
    pub help: bool,
    /// A yes/no question is open.
    pub confirm: bool,
    /// The search box has focus.
    pub typing: bool,
    /// A video tab is active.
    pub in_tab: bool,
    /// ... and its related list has the keys.
    pub related: bool,
    /// The right-click menu is open.
    pub context_menu: bool,
    /// The opening logo is shown: any key skips it.
    pub splash: bool,
}

pub const SEEK_SECONDS: i32 = 5;
pub const VOLUME_STEP: i32 = 5;

/// `Ctrl+y` commands. Digits (`0`–`9`, go to tab) are handled apart.
pub const PREFIX: &[(char, Action, Text)] = &[
    ('b', Action::ToggleSidebar, Text::new("mostrar / ocultar el menú", "show / hide the menu")),
    ('m', Action::FocusMenu, Text::new("ir al menú", "go to the menu")),
    ('f', Action::ToggleFullscreen, Text::new("pantalla completa", "fullscreen")),
    ('n', Action::NextTab, Text::new("pestaña siguiente", "next tab")),
    ('p', Action::PrevTab, Text::new("pestaña anterior", "previous tab")),
    ('x', Action::CloseTab, Text::new("cerrar la pestaña", "close the tab")),
    ('/', Action::FocusSearch, Text::new("buscar videos", "search videos")),
    ('l', Action::ToggleUiLang, Text::new("idioma: pasar a English", "language: switch to español")),
    ('?', Action::ToggleHelp, Text::new("esta ayuda", "this help")),
    ('q', Action::Quit, Text::new("salir", "quit")),
];

/// Keys that act on the selected video, in lists and in video tabs.
pub const ON_VIDEO: &[(&str, Text)] = &[
    ("l", Text::new("agregar / quitar de Ver después", "add to / remove from Watch later")),
    ("s", Text::new("suscribirse / desuscribirse (pide confirmar)", "subscribe / unsubscribe (asks first)")),
    ("g", Text::new("me gusta (otra vez: lo quita)", "like (again: take it back)")),
    ("G", Text::new("no me gusta (otra vez: lo quita)", "dislike (again: take it back)")),
    ("d", Text::new("descargar (con subtítulos)", "download (with subtitles)")),
    ("a", Text::new("escuchar solo el audio / volver al video", "sound only / back to the video")),
    ("c", Text::new("ver los videos del canal", "the channel's videos")),
];

pub const IN_MENU: &[(&str, Text)] = &[
    ("↑ ↓", Text::new("elegir sección", "choose a section")),
    ("Enter", Text::new("abrir", "open")),
    ("Tab", Text::new("ir a la lista", "go to the list")),
    ("/", Text::new("buscar videos", "search videos")),
];

pub const IN_LISTS: &[(&str, Text)] = &[
    ("↑ ↓  j k", Text::new("moverse", "move")),
    ("Enter", Text::new("ver video · abrir canal · cambiar opción", "play · open channel · change option")),
    ("Esc  ←  h", Text::new("volver", "back")),
    ("Tab", Text::new("ir al menú", "go to the menu")),
    ("/", Text::new("buscar · en archivos: filtrar", "search · in files: filter")),
    ("v  t  o", Text::new("vistos · t: todo o lo mío · o: ordenar", "watched · t: all or mine · o: sort")),
    ("espacio m r", Text::new("marcar · mover · renombrar archivos", "mark · move · rename files")),
    ("x  Supr", Text::new("papelera (pide s) · Historial: quitarlo", "trash (asks y) · History: remove it")),
];

pub const WITH_MOUSE: &[(&str, Text)] = &[
    ("clic", Text::new("elegir · en el menú y las pestañas: abrir", "select · menu and tabs: open")),
    ("doble clic", Text::new("ver video · abrir canal · cambiar opción", "play · open channel · change option")),
    ("rueda", Text::new("moverse en la lista · en el video: volumen", "move in the list · on the video: volume")),
    ("clic video", Text::new("pausa (doble clic: pantalla completa)", "pause (double click: fullscreen)")),
    ("clic der.", Text::new("opciones de lo que está debajo", "options for what is under it")),
    ("clic barra", Text::new("saltar a ese momento del video", "jump to that point of the video")),
    ("Shift", Text::new("mantenela para seleccionar y copiar texto", "hold it to select and copy text")),
];

pub const IN_SEARCH: &[(&str, Text)] = &[
    ("Enter", Text::new("buscar / aceptar", "search / accept")),
    ("Tab", Text::new("dónde buscar: todo / mis canales / canal", "where: everywhere / my channels / channel")),
    ("Esc", Text::new("cancelar", "cancel")),
];

pub const IN_TAB: &[(&str, Text)] = &[
    ("espacio", Text::new("pausa", "pause")),
    ("v", Text::new("calidad: 360p → 480p → 720p → 1080p …", "quality: 360p → 480p → 720p → 1080p …")),
    ("b", Text::new("compresión: H.264 → VP9 → AV1 → cualquiera", "codec: H.264 → VP9 → AV1 → any")),
    ("f  Esc", Text::new("pantalla completa / salir", "fullscreen / leave")),
    ("← →", Text::new("retroceder / avanzar 5 s", "back / forward 5 s")),
    ("[ ]", Text::new("capítulo anterior / siguiente", "previous / next chapter")),
    ("↑ ↓  m", Text::new("subir / bajar volumen · silenciar", "volume up / down · mute")),
    ("j", Text::new("subtítulos sí / no", "subtitles on / off")),
    (
        "i",
        Text::new(
            "doblaje: subtítulos → original → doblado",
            "dubbing: subtitles → original → dubbed",
        ),
    ),
    (
        "o",
        Text::new(
            "idioma de doblaje y subtítulos (es → en …)",
            "dub and subtitles language (es → en …)",
        ),
    ),
    (
        "T",
        Text::new(
            "voz traducida: no → inglés → ruso",
            "voice-over: off → English → Russian",
        ),
    ),
    (". ,", Text::new("música: siguiente / anterior", "music: next / previous")),
    ("Tab", Text::new("videos relacionados (en ventana)", "related videos (in a window)")),
    ("PgUp PgDn", Text::new("recorrer los comentarios (en ventana)", "scroll the comments (in a window)")),
];

/// `(keys, description)` rows of a table, in `lang`.
pub fn rows(table: &[(&str, Text)], lang: Lang) -> Vec<(String, &'static str)> {
    table.iter().map(|(k, d)| (space_key(k, lang), d.get(lang))).collect()
}

/// The space bar is named in the interface language.
fn space_key(keys: &str, lang: Lang) -> String {
    if keys == "espacio" { lang.pick("espacio", "space").into() } else { keys.into() }
}

/// `Ctrl+y` commands plus the digits, in `lang`.
pub fn prefix_rows(lang: Lang) -> Vec<(String, &'static str)> {
    let mut commands: Vec<_> = PREFIX.iter().map(|(k, _, d)| (k.to_string(), d.get(lang))).collect();
    commands.insert(6, ("0-9".into(), lang.pick("ir a la pestaña (0 = Inicio)", "go to tab (0 = Home)")));
    commands
}

pub fn map(key: KeyEvent, ctx: KeyContext) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && key.code == KeyCode::Char('c') {
        return Some(Action::Quit);
    }
    if ctx.splash {
        return Some(Action::EndSplash);
    }
    if ctx.help {
        return Some(Action::ToggleHelp);
    }
    if ctx.confirm {
        return Some(Action::Confirm(matches!(key.code, KeyCode::Char('s' | 'S' | 'y' | 'Y'))));
    }
    if ctx.context_menu {
        // Like the yes/no question: any other key closes it.
        return Some(match key.code {
            KeyCode::Up | KeyCode::Char('k') => Action::Up,
            KeyCode::Down | KeyCode::Char('j') => Action::Down,
            KeyCode::Enter | KeyCode::Char(' ') => Action::Enter,
            _ => Action::Back,
        });
    }
    if ctrl {
        return (key.code == KeyCode::Char('y')).then_some(Action::Prefix);
    }
    if ctx.prefix {
        return Some(prefixed(key.code));
    }
    if ctx.typing {
        return typing(key.code);
    }
    match key.code {
        KeyCode::Char('l') => return Some(Action::ToggleLater),
        KeyCode::Char('s') => return Some(Action::ToggleSubscription),
        KeyCode::Char('g') => return Some(Action::Rate(Rating::Like)),
        KeyCode::Char('G') => return Some(Action::Rate(Rating::Dislike)),
        KeyCode::Char('d') => return Some(Action::Download),
        KeyCode::Char('a') => return Some(Action::Listen),
        KeyCode::Char('c') => return Some(Action::OpenChannel),
        _ => {}
    }
    match (ctx.in_tab, ctx.related) {
        (true, true) => related(key.code).or_else(|| player(key.code)),
        (true, false) => player(key.code),
        _ => lists(key.code),
    }
}

fn prefixed(code: KeyCode) -> Action {
    match code {
        KeyCode::Char(c @ '0'..='9') => Action::GoTab(c as usize - '0' as usize),
        KeyCode::Char(c) => {
            PREFIX.iter().find(|(key, ..)| *key == c).map_or(Action::CancelPrefix, |(_, action, _)| action.clone())
        }
        _ => Action::CancelPrefix,
    }
}

fn typing(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Char(c) => Some(Action::Input(c)),
        KeyCode::Backspace => Some(Action::Backspace),
        KeyCode::Enter => Some(Action::Enter),
        KeyCode::Esc => Some(Action::Back),
        KeyCode::Tab => Some(Action::CycleScope),
        _ => None,
    }
}

fn player(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Char(' ') => Some(Action::TogglePause),
        KeyCode::Left => Some(Action::Seek(-SEEK_SECONDS)),
        KeyCode::Right => Some(Action::Seek(SEEK_SECONDS)),
        KeyCode::Up => Some(Action::Volume(VOLUME_STEP)),
        KeyCode::Down => Some(Action::Volume(-VOLUME_STEP)),
        KeyCode::Char('m') => Some(Action::ToggleMute),
        KeyCode::Char('j') => Some(Action::CycleSubtitles),
        KeyCode::Char('i') => Some(Action::CycleLanguage),
        KeyCode::Char('o') => Some(Action::CycleSubLang),
        KeyCode::Char('T') => Some(Action::CycleVoice),
        KeyCode::Char('v') => Some(Action::CycleQuality),
        KeyCode::Char('b') => Some(Action::CycleCodec),
        KeyCode::Char('.') => Some(Action::NextTrack),
        KeyCode::Char('f') => Some(Action::ToggleFullscreen),
        KeyCode::PageDown => Some(Action::ScrollComments(3)),
        KeyCode::PageUp => Some(Action::ScrollComments(-3)),
        KeyCode::Enter => Some(Action::Enter),
        KeyCode::Char('[') => Some(Action::Chapter(-1)),
        KeyCode::Char(']') => Some(Action::Chapter(1)),
        KeyCode::Tab => Some(Action::ToggleFocus),
        KeyCode::Esc => Some(Action::Back),
        KeyCode::Char(',') => Some(Action::PrevTrack),
        _ => None,
    }
}

/// The related videos of a tab: move and open; the rest still drives the player.
fn related(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::Down),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::Up),
        KeyCode::Enter => Some(Action::Enter),
        KeyCode::Tab | KeyCode::Esc => Some(Action::ToggleFocus),
        _ => None,
    }
}

fn lists(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Char(' ') => Some(Action::ToggleMark),
        KeyCode::Char('x') | KeyCode::Delete => Some(Action::Trash),
        KeyCode::Char('v') => Some(Action::ToggleWatched),
        KeyCode::Char('t') => Some(Action::ToggleHomeAll),
        KeyCode::Char('o') => Some(Action::CycleSort),
        KeyCode::Char('m') => Some(Action::MoveFiles),
        KeyCode::Char('r') => Some(Action::Rename),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::Down),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::Up),
        KeyCode::Enter => Some(Action::Enter),
        KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => Some(Action::Back),
        KeyCode::Char('/') => Some(Action::FocusSearch),
        KeyCode::Tab => Some(Action::ToggleFocus),
        _ => None,
    }
}

/// Sections of the help screen: title and `(keys, description)` rows.
pub fn help(lang: Lang) -> Vec<(&'static str, Vec<(String, &'static str)>)> {
    vec![
        (lang.pick("Comandos: Ctrl+y y después una tecla", "Commands: Ctrl+y, then a key"), prefix_rows(lang)),
        (lang.pick("Menú", "Menu"), rows(IN_MENU, lang)),
        (lang.pick("Listas", "Lists"), rows(IN_LISTS, lang)),
        (lang.pick("Sobre un video (lista o pestaña)", "On a video (list or tab)"), rows(ON_VIDEO, lang)),
        (lang.pick("Pestaña de video", "Video tab"), rows(IN_TAB, lang)),
        (lang.pick("Cuadro de búsqueda", "Search box"), rows(IN_SEARCH, lang)),
        ("Mouse", rows(WITH_MOUSE, lang)),
        (lang.pick("Siempre", "Always"), vec![("Ctrl+c".into(), lang.pick("salir", "quit"))]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventState;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn char(c: char) -> KeyEvent {
        key(KeyCode::Char(c))
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    const LISTS: KeyContext = KeyContext {
        prefix: false,
        help: false,
        confirm: false,
        typing: false,
        in_tab: false,
        related: false,
        context_menu: false,
        splash: false,
    };
    const TYPING: KeyContext = KeyContext { typing: true, ..LISTS };
    const TAB: KeyContext = KeyContext { in_tab: true, ..LISTS };
    const PREFIXED: KeyContext = KeyContext { prefix: true, ..LISTS };
    const HELP: KeyContext = KeyContext { help: true, ..LISTS };
    const CONFIRM: KeyContext = KeyContext { confirm: true, ..LISTS };

    #[test]
    fn ctrl_y_starts_prefix_everywhere_but_dialogs() {
        for ctx in [LISTS, TYPING, TAB] {
            assert_eq!(map(ctrl('y'), ctx), Some(Action::Prefix), "{ctx:?}");
        }
        assert_eq!(map(ctrl('y'), HELP), Some(Action::ToggleHelp));
    }

    #[test]
    fn prefix_commands_come_from_the_table() {
        for (c, action, _) in PREFIX {
            assert_eq!(map(char(*c), PREFIXED).as_ref(), Some(action), "{c}");
        }
        assert_eq!(map(char('0'), PREFIXED), Some(Action::GoTab(0)));
        assert_eq!(map(char('3'), PREFIXED), Some(Action::GoTab(3)));
        assert_eq!(map(char('?'), PREFIXED), Some(Action::ToggleHelp));
    }

    #[test]
    fn unknown_key_after_prefix_cancels_it() {
        assert_eq!(map(char('z'), PREFIXED), Some(Action::CancelPrefix));
        assert_eq!(map(key(KeyCode::Esc), PREFIXED), Some(Action::CancelPrefix));
    }

    #[test]
    fn prefix_wins_over_typing() {
        assert_eq!(map(char('b'), KeyContext { typing: true, ..PREFIXED }), Some(Action::ToggleSidebar));
    }

    #[test]
    fn any_key_closes_help() {
        assert_eq!(map(char('x'), HELP), Some(Action::ToggleHelp));
        assert_eq!(map(key(KeyCode::Esc), HELP), Some(Action::ToggleHelp));
    }

    #[test]
    fn confirm_only_accepts_yes() {
        assert_eq!(map(char('s'), CONFIRM), Some(Action::Confirm(true)));
        assert_eq!(map(char('y'), CONFIRM), Some(Action::Confirm(true)));
        assert_eq!(map(char('n'), CONFIRM), Some(Action::Confirm(false)));
        assert_eq!(map(key(KeyCode::Enter), CONFIRM), Some(Action::Confirm(false)));
    }

    #[test]
    fn typing_edits_the_query() {
        assert_eq!(map(char('l'), TYPING), Some(Action::Input('l')));
        assert_eq!(map(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT), TYPING), Some(Action::Input('A')));
        assert_eq!(map(key(KeyCode::Backspace), TYPING), Some(Action::Backspace));
        assert_eq!(map(key(KeyCode::Enter), TYPING), Some(Action::Enter));
        assert_eq!(map(key(KeyCode::Esc), TYPING), Some(Action::Back));
        assert_eq!(map(key(KeyCode::Tab), TYPING), Some(Action::CycleScope));
    }

    #[test]
    fn list_navigation() {
        assert_eq!(map(char('j'), LISTS), Some(Action::Down));
        assert_eq!(map(key(KeyCode::Down), LISTS), Some(Action::Down));
        assert_eq!(map(char('k'), LISTS), Some(Action::Up));
        assert_eq!(map(key(KeyCode::Enter), LISTS), Some(Action::Enter));
        assert_eq!(map(key(KeyCode::Esc), LISTS), Some(Action::Back));
        assert_eq!(map(char('h'), LISTS), Some(Action::Back));
        assert_eq!(map(char('/'), LISTS), Some(Action::FocusSearch));
        assert_eq!(map(key(KeyCode::Tab), LISTS), Some(Action::ToggleFocus));
    }

    #[test]
    fn video_keys_work_in_lists_and_tabs() {
        for ctx in [LISTS, TAB] {
            assert_eq!(map(char('l'), ctx), Some(Action::ToggleLater));
            assert_eq!(map(char('s'), ctx), Some(Action::ToggleSubscription));
            assert_eq!(map(char('g'), ctx), Some(Action::Rate(Rating::Like)));
            assert_eq!(map(char('G'), ctx), Some(Action::Rate(Rating::Dislike)));
            assert_eq!(map(char('d'), ctx), Some(Action::Download));
            assert_eq!(map(char('a'), ctx), Some(Action::Listen));
            assert_eq!(map(char('c'), ctx), Some(Action::OpenChannel));
        }
    }

    #[test]
    fn player_controls_in_tab() {
        assert_eq!(map(char(' '), TAB), Some(Action::TogglePause));
        assert_eq!(map(key(KeyCode::Left), TAB), Some(Action::Seek(-SEEK_SECONDS)));
        assert_eq!(map(key(KeyCode::Right), TAB), Some(Action::Seek(SEEK_SECONDS)));
        assert_eq!(map(key(KeyCode::Up), TAB), Some(Action::Volume(VOLUME_STEP)));
        assert_eq!(map(key(KeyCode::Down), TAB), Some(Action::Volume(-VOLUME_STEP)));
        assert_eq!(map(char('m'), TAB), Some(Action::ToggleMute));
        assert_eq!(map(char('j'), TAB), Some(Action::CycleSubtitles));
        assert_eq!(map(char('i'), TAB), Some(Action::CycleLanguage));
        assert_eq!(map(char('o'), TAB), Some(Action::CycleSubLang));
        assert_eq!(map(char('T'), TAB), Some(Action::CycleVoice));
        assert_eq!(map(char('v'), TAB), Some(Action::CycleQuality));
        assert_eq!(map(char(' '), LISTS), Some(Action::ToggleMark));
        assert_eq!(map(char('x'), LISTS), Some(Action::Trash));
        assert_eq!(map(key(KeyCode::Delete), LISTS), Some(Action::Trash));
        assert_eq!(map(char('b'), TAB), Some(Action::CycleCodec));
        assert_eq!(map(char('.'), TAB), Some(Action::NextTrack));
        assert_eq!(map(char('f'), TAB), Some(Action::ToggleFullscreen));
        assert_eq!(map(key(KeyCode::Esc), TAB), Some(Action::Back));
        assert_eq!(map(char(','), TAB), Some(Action::PrevTrack));
        assert_eq!(map(char('['), TAB), Some(Action::Chapter(-1)));
        assert_eq!(map(char(']'), TAB), Some(Action::Chapter(1)));
        assert_eq!(map(key(KeyCode::Enter), TAB), Some(Action::Enter), "plays a closed video again");
    }

    #[test]
    fn plain_q_does_not_quit_but_ctrl_c_always_does() {
        assert_eq!(map(char('q'), LISTS), None);
        for ctx in [LISTS, TYPING, TAB, PREFIXED, HELP, CONFIRM] {
            assert_eq!(map(ctrl('c'), ctx), Some(Action::Quit), "{ctx:?}");
        }
    }

    #[test]
    fn key_releases_are_ignored() {
        // Windows reports both press and release.
        let release = KeyEvent { kind: KeyEventKind::Release, state: KeyEventState::NONE, ..ctrl('y') };
        assert_eq!(map(release, LISTS), None);
    }

    #[test]
    fn help_lists_every_prefix_command() {
        for lang in [Lang::Es, Lang::En] {
            let help = help(lang);
            let (_, commands) = &help[0];
            for (c, ..) in PREFIX {
                assert!(commands.iter().any(|(k, _)| *k == c.to_string()), "{c} missing from help");
            }
            assert!(commands.iter().any(|(k, _)| k == "0-9"));
        }
    }

    #[test]
    fn help_is_translated() {
        assert_eq!(help(Lang::En)[0].1.iter().find(|(k, _)| k == "q").unwrap().1, "quit");
        assert_eq!(help(Lang::Es)[0].1.iter().find(|(k, _)| k == "q").unwrap().1, "salir");
        assert_eq!(rows(IN_TAB, Lang::En)[0].0, "space");
        assert_eq!(map(char('l'), PREFIXED), Some(Action::ToggleUiLang));
    }
}
