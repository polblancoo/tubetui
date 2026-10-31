//! Rendering. `areas` is the single source of truth for the layout, so the
//! runtime places the embedded video exactly where `draw` leaves room for it.

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Tabs,
};

use crate::app::{
    Confirm, ContextMenu, Focus, Group, ListKind, Listing, Prompt, Section, Setting, State, Tab, language_label,
};
use crate::config::{IconSet, donations};
use crate::doctor::{Manager, Tool};
use crate::domain::{Channel, Download, DownloadState, Live, Playlist, Size, Video, bytes_label, clock, date_labels};
use crate::i18n::Lang;
use crate::media::{LocalFile, MediaKind};
use crate::player::Info;
use crate::sponsorblock::Segment;
use crate::theme::{self, border, palette};
use crate::update::Install;
#[cfg(test)]
use crate::youtube::Watch;
use crate::youtube::{Items, Rating, compact};
use crate::{keymap, session};

pub const SIDEBAR_WIDTH: u16 = 26;
/// Width of the preview panel next to video lists.
const PREVIEW_WIDTH: u16 = 46;
/// Video lists narrower than this show no preview panel.
const PREVIEW_MIN_WIDTH: u16 = 96;
const CHANNEL_WIDTH: usize = 20;
/// Columns of the channel lists; the @handle goes last.
const CHANNEL_NAME_WIDTH: usize = 26;
const FOLLOWERS_WIDTH: usize = 24;
/// Width of the date column: `"10/10/26 hoy   "`.
const DATE_WIDTH: usize = 14;

// Colours by role, from the selected theme.
fn fg() -> Color {
    palette().fg
}
fn muted() -> Color {
    palette().muted
}
fn highlight() -> Color {
    palette().highlight
}
fn rule() -> Color {
    palette().rule
}
fn blue() -> Color {
    palette().blue
}
fn cyan() -> Color {
    palette().cyan
}
fn magenta() -> Color {
    palette().magenta
}
fn green() -> Color {
    palette().green
}
fn orange() -> Color {
    palette().orange
}
fn yellow() -> Color {
    palette().yellow
}
fn red() -> Color {
    palette().red
}
fn teal() -> Color {
    palette().teal
}
fn purple() -> Color {
    palette().purple
}

pub struct Icons {
    /// In `Section::ALL` order.
    pub sections: [&'static str; 17],
    /// In `Setting::ALL` order.
    pub settings: [&'static str; 31],
    pub logo: &'static str,
    pub tab: &'static str,
    pub live: &'static str,
    pub upcoming: &'static str,
    pub later: &'static str,
    /// A channel I watch a lot.
    pub favorite: &'static str,
    pub subscribed: &'static str,
    pub liked: &'static str,
    pub disliked: &'static str,
    pub user: &'static str,
    pub language: &'static str,
    pub volume: &'static str,
    pub muted: &'static str,
    /// A video already watched.
    pub watched: &'static str,
}

impl Icons {
    fn section(&self, section: Section) -> &'static str {
        self.sections[Section::ALL.iter().position(|s| *s == section).unwrap_or(0)]
    }

    fn setting(&self, setting: Setting) -> &'static str {
        self.settings[Setting::ALL.iter().position(|s| *s == setting).unwrap_or(0)]
    }
}

const EMOJI: Icons = Icons {
    sections: ["🏠", "🎯", "🧭", "📺", "🕒", "📜", "📚", "⭐", "💡", "🔎", "📡", "📼", "🎵", "📥", "🔧", "💜", "🚪"],
    settings: [
        "👤", "📄", "🔄", "🚪", "🌐", "💬", "📐", "🎬", "🎧", "⏩", "🗣", "😀", "🎨", "🔲", "🖼", "🖱", "🎞", "✨", "🌊",
        "💥", "📼", "🎵", "📁", "🏷", "🩺", "👁", "🩳", "🔔", "⭐", "⬆", "📝",
    ],
    logo: "❯_",
    tab: "🎬",
    live: "🔴",
    upcoming: "⏳",
    later: "🕒",
    favorite: "⭐",
    subscribed: "✓",
    liked: "👍",
    disliked: "👎",
    user: "👤",
    language: "🌐",
    volume: "🔊",
    muted: "🔇",
    watched: "👁",
};

const NERD: Icons = Icons {
    sections: [
        "\u{f015}",
        "\u{f140}",
        "\u{f14e}",
        "\u{f05c6}",
        "\u{f017}",
        "\u{f1da}",
        "\u{f0cb}",
        "\u{f005}",
        "\u{f0eb}",
        "\u{f002}",
        "\u{f2ce}",
        "\u{f03d}",
        "\u{f001}",
        "\u{f019}",
        "\u{f013}",
        "\u{f004}",
        "\u{f08b}",
    ],
    settings: [
        "\u{f007}", "\u{f15b}", "\u{f021}", "\u{f08b}", "\u{f1ab}", "\u{f075}", "\u{f1de}", "\u{f008}", "\u{f025}",
        "\u{f051}", "\u{f0ac}", "\u{f118}", "\u{f1fc}", "\u{f096}", "\u{f03e}", "\u{f245}", "\u{f04b}", "\u{f0d0}",
        "\u{f0e7}", "\u{f0a1}", "\u{f03d}", "\u{f001}", "\u{f07b}", "\u{f02c}", "\u{f0f1}", "\u{f06e}", "\u{f10b}",
        "\u{f0f3}", "\u{f005}", "\u{f01b}", "\u{f044}",
    ],
    logo: "\u{f120}",
    tab: "\u{f144}",
    live: "\u{f111}",
    upcoming: "\u{f252}",
    later: "\u{f017}",
    favorite: "\u{f005}",
    subscribed: "\u{f00c}",
    liked: "\u{f164}",
    disliked: "\u{f165}",
    user: "\u{f007}",
    language: "\u{f1ab}",
    volume: "\u{f028}",
    muted: "\u{f026}",
    watched: "\u{f06e}",
};

const UNICODE: Icons = Icons {
    sections: ["⌂", "◈", "◎", "▣", "◷", "↺", "≣", "★", "✧", "⌕", "⌁", "▷", "♫", "⇩", "⚙", "♥", "⏻"],
    settings: [
        "☺", "▤", "↻", "⏻", "⇄", "≡", "▦", "▶", "♪", "»", "⚑", "☻", "◐", "□", "▨", "➚", "◆", "✦", "≈", "✸", "▷", "♫",
        "▭", "#", "✚", "◉", "▯", "♪", "★", "↑", "✎",
    ],
    logo: "❯_",
    tab: "▶",
    live: "●",
    upcoming: "…",
    later: "◷",
    favorite: "★",
    subscribed: "✓",
    liked: "▲",
    disliked: "▼",
    user: "☺",
    language: "⇄",
    volume: "♪",
    muted: "✕",
    watched: "◉",
};

pub fn icons(set: IconSet) -> &'static Icons {
    match set {
        IconSet::Emoji => &EMOJI,
        IconSet::Nerd => &NERD,
        IconSet::Unicode => &UNICODE,
    }
}

/// Rendering options that do not belong to the app state.
pub struct View {
    pub embedded: bool,
    /// Unix time, for "hace 3 días".
    pub now: i64,
    /// How long the current list has been loading.
    pub loading: Option<Duration>,
    /// How long the video of the active tab has been loading, until it plays.
    pub opening: Option<Duration>,
    /// The preview panel leaves room for a thumbnail (the runtime draws it).
    pub thumbnails: bool,
    /// What mpv reports about the video of the active tab.
    pub playing: Option<Info>,
}

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// `"⠹ Cargando Suscripciones… 3 s"`.
fn loading_text(title: &str, elapsed: Duration, lang: Lang) -> String {
    let frame = SPINNER[(elapsed.as_millis() / 100) as usize % SPINNER.len()];
    format!("{frame} {} {title}… {} s", lang.pick("Cargando", "Loading"), elapsed.as_secs())
}

/// A block bouncing inside `width` cells: progress without a known total.
fn bouncing_bar(width: usize, elapsed: Duration) -> String {
    let block = 6.min(width);
    let span = width - block;
    let step = (elapsed.as_millis() / 60) as usize % (2 * span.max(1));
    let left = if step <= span { step } else { 2 * span - step };
    format!("{}{}{}", "░".repeat(left), "█".repeat(block), "░".repeat(span - left))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Areas {
    pub sidebar: Option<Rect>,
    pub tabs: Rect,
    pub content: Rect,
    pub status: Rect,
    /// Keys that work right now, with what they do.
    pub hints: Rect,
}

pub fn areas(area: Rect, sidebar: bool) -> Areas {
    let [body, status, hints] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1), Constraint::Length(1)]).areas(area);
    let width = if sidebar { SIDEBAR_WIDTH } else { 0 };
    let [side, main] = Layout::horizontal([Constraint::Length(width), Constraint::Fill(1)]).areas(body);
    let [tabs, content] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(main);
    Areas { sidebar: sidebar.then_some(side), tabs, content, status, hints }
}

/// What is under the mouse pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Spot {
    /// A section of the menu, in `Section::ALL` order.
    Section(usize),
    /// A tab, in the menu or the tab bar; 0 is Inicio.
    Tab(usize),
    /// A row of the current list.
    Row(usize),
    /// The picture of the active video tab.
    Video,
    /// A second on the progress bar.
    Progress(u32),
    /// A line of the right-click menu.
    MenuItem(usize),
}

/// What is at cell (`x`, `y`), laid out as [`draw`] does it.
pub fn spot(state: &State, playing: Option<&Info>, screen: Rect, x: u16, y: u16) -> Option<Spot> {
    let at = Position::new(x, y);
    if state.splash || state.help || state.confirm.is_some() {
        return None;
    }
    if let Some(menu) = &state.context {
        let inner = Block::bordered().inner(context_area(menu, screen));
        return inner.contains(at).then(|| Spot::MenuItem(usize::from(y - inner.y)));
    }
    if state.fullscreen {
        return Some(Spot::Video);
    }
    let areas = areas(screen, state.sidebar);
    if let Some(side) = areas.sidebar.filter(|a| a.contains(at)) {
        let inner = Block::bordered().inner(side);
        let line = usize::from(y.checked_sub(inner.y)?);
        return sidebar_items(state, icons(state.config.ui.icons)).get(line)?.1;
    }
    if areas.tabs.contains(at) {
        // Tabs draws ` title ` and a one-cell divider after each.
        let mut start = areas.tabs.x;
        for (i, title) in tab_titles(state, icons(state.config.ui.icons)).iter().enumerate() {
            let end = start + Span::raw(title.as_str()).width() as u16 + 2;
            if x < end {
                return (x >= start).then_some(Spot::Tab(i));
            }
            start = end + 1;
        }
        return None;
    }
    if !areas.content.contains(at) {
        return None;
    }
    if let Some(tab) = state.active_tab() {
        if let Some((bar, duration)) = playing.and_then(|info| progress_bar(info, areas.content))
            && bar.contains(at)
        {
            let fraction = (f64::from(x - bar.x) + 0.5) / f64::from(bar.width);
            return Some(Spot::Progress((fraction * duration) as u32));
        }
        if state.details_shown(tab) {
            // The video is elsewhere (or not shown): here are its details and the related list.
            state.related_shown()?;
            let list = Block::bordered().inner(watch_layout(Block::bordered().inner(areas.content)).related);
            return list_row(list, tab.related, tab.related_videos().len(), at);
        }
        let inner = Block::bordered().inner(areas.content);
        return (!tab.playback.audio_only && inner.contains(at)).then_some(Spot::Video);
    }
    if state.prompt == Some(Prompt::CookiesFile) {
        return None;
    }
    let listing = state.listing()?;
    let (_, rest) = split_prompt(state, areas.content);
    let list = match listing.items {
        Items::Videos(_) | Items::Channels(_) => split_preview(rest).0,
        Items::About | Items::System => return None,
        _ => rest,
    };
    list_row(Block::bordered().inner(list), listing.selected, listing.items.len(), at)
}

/// The row of a list (drawn in `inner`, `selected` of `len`) at `at`.
fn list_row(inner: Rect, selected: usize, len: usize, at: Position) -> Option<Spot> {
    if !inner.contains(at) || inner.height == 0 {
        return None;
    }
    // A list drawn from offset 0 scrolls just enough to show the selection last.
    let offset = selected.saturating_sub(usize::from(inner.height) - 1);
    let row = offset + usize::from(at.y - inner.y);
    (row < len).then_some(Spot::Row(row))
}

/// The cells the embedded video may cover: the active video tab, unless a
/// dialog is open (kitty draws images above text, it would hide the dialog).
pub fn video_area(state: &State, area: Rect) -> Option<Rect> {
    if state.splash || state.help || state.confirm.is_some() {
        return None;
    }
    state.active_tab().filter(|t| !t.playback.audio_only)?;
    let cells = if state.fullscreen { area } else { Block::bordered().inner(areas(area, state.sidebar).content) };
    under_context_menu(state, area, cells)
}

/// `cells`, unless the right-click menu falls on them (an image would hide it).
fn under_context_menu(state: &State, screen: Rect, cells: Rect) -> Option<Rect> {
    let covered = state.context.as_ref().is_some_and(|menu| context_area(menu, screen).intersects(cells));
    (!covered).then_some(cells)
}

/// Where the right-click menu is drawn: at the pointer, inside the screen.
pub fn context_area(menu: &ContextMenu, screen: Rect) -> Rect {
    let widest = menu.items.iter().map(|i| Span::raw(i.label).width() + Span::raw(i.key).width()).max();
    let width = (widest.unwrap_or(0) as u16 + 7).min(screen.width);
    let height = (menu.items.len() as u16 + 2).min(screen.height);
    // One row below the pointer, so what was clicked stays in sight.
    let (x, y) = menu.at;
    Rect {
        x: x.min(screen.right().saturating_sub(width)),
        y: (y + 1).min(screen.bottom().saturating_sub(height)),
        width,
        height,
    }
}

pub fn draw(frame: &mut Frame, state: &State, view: &View) {
    theme::select(state.config.ui.theme, state.config.ui.border);
    if let Some(background) = palette().background {
        frame.render_widget(Block::new().style(Style::new().bg(background).fg(fg())), frame.area());
    }
    let icons = icons(state.config.ui.icons);
    let lang = state.lang();
    if state.splash {
        draw_splash(frame, lang);
        return;
    }
    if state.fullscreen {
        // Nothing but the video (embedded) or a hint (window mode).
        if !view.embedded {
            let note = lang.pick(
                "Pantalla completa en la ventana de mpv · f o Esc para salir",
                "Fullscreen in the mpv window · f or Esc to leave",
            );
            frame.render_widget(Paragraph::new(note).fg(muted()).alignment(Alignment::Center), frame.area());
        }
        if state.help {
            draw_help(frame, lang);
        }
        return;
    }
    let areas = areas(frame.area(), state.sidebar);
    if let Some(side) = areas.sidebar {
        draw_sidebar(frame, state, icons, side);
    }
    draw_tabs(frame, state, icons, areas.tabs);
    match state.active_tab() {
        None => draw_inicio(frame, state, icons, view, areas.content),
        Some(tab) => {
            let playback = &tab.playback;
            let title = Line::from(vec![
                Span::raw(format!(" {} ", tab_icon(icons, tab))),
                Span::styled(playback.title.clone(), Style::new().fg(fg()).bold()),
                Span::styled(format!(" · {} ", playback.subtitle), magenta()),
            ]);
            let body = if state.details_shown(tab) {
                Paragraph::default() // the details, drawn below
            } else if playback.audio_only {
                let songs = playback.queue.len();
                Paragraph::new(vec![
                    Line::from(""),
                    Line::from(format!("{}  {}", icons.section(Section::MiMusica), playback.title)).fg(fg()).bold(),
                    Line::from(playback.subtitle.clone()).fg(magenta()),
                    Line::from(""),
                    Line::from(format!(
                        "{songs} {}",
                        lang.pick(
                            "canciones en la cola · . siguiente · , anterior",
                            "songs queued · . next · , previous"
                        )
                    ))
                    .fg(muted()),
                ])
                .alignment(Alignment::Center)
            } else if let Some(elapsed) = view.opening {
                // In embedded mode the first video frame covers it.
                Paragraph::new(vec![
                    Line::from(""),
                    Line::from(loading_text(lang.pick("el video", "the video"), elapsed, lang)).fg(yellow()),
                    Line::from(bouncing_bar(30, elapsed)).fg(blue()),
                ])
                .alignment(Alignment::Center)
            } else if view.embedded {
                Paragraph::default() // the video
            } else {
                Paragraph::new(lang.pick("Se reproduce en una ventana de mpv", "Playing in an mpv window")).fg(muted())
            };
            let mut block = panel(title, blue());
            if let Some(info) = &view.playing {
                block = block.title_top(Line::from(playing_label(info, icons)).fg(cyan()).right_aligned());
                if let Some(progress) = progress_line(info, &tab.segments, areas.content.width) {
                    block = block.title_bottom(progress);
                }
            }
            frame.render_widget(body.block(block), areas.content);
            if state.details_shown(tab) {
                draw_watch(frame, state, icons, view, tab, Block::bordered().inner(areas.content));
            }
        }
    }
    draw_status(frame, state, icons, view, areas.status);
    draw_hints(frame, &hints(state), areas.hints);
    if state.help {
        draw_help(frame, lang);
    }
    if let Some(confirm) = &state.confirm {
        draw_confirm(frame, confirm, lang);
    }
    if let Some(menu) = &state.context {
        draw_context_menu(frame, menu, lang);
    }
}

/// Cells of a video tab whose video plays in its own window.
struct WatchLayout {
    thumbnail: Rect,
    details: Rect,
    related: Rect,
    /// Beside "Up next", when there is room.
    comments: Option<Rect>,
}

/// Columns of the thumbnail on a video tab.
const WATCH_THUMBNAIL_WIDTH: u16 = 36;
/// Narrower than this, a video tab shows no comments.
const COMMENTS_MIN_WIDTH: u16 = 100;

fn watch_layout(inner: Rect) -> WatchLayout {
    let width = WATCH_THUMBNAIL_WIDTH.min(inner.width / 3);
    let rows = (u32::from(width) * 9 / 32) as u16;
    let [card, _, below] =
        Layout::vertical([Constraint::Length(rows.max(7)), Constraint::Length(1), Constraint::Fill(1)]).areas(inner);
    let [thumbnail, _, details] =
        Layout::horizontal([Constraint::Length(width), Constraint::Length(2), Constraint::Fill(1)]).areas(card);
    let (related, comments) = if below.width >= COMMENTS_MIN_WIDTH {
        // "Up next" a little wider: its rows carry date, channel, title and length.
        let [related, comments] = Layout::horizontal([Constraint::Fill(11), Constraint::Fill(9)]).areas(below);
        (related, Some(comments))
    } else {
        (below, None)
    };
    WatchLayout { thumbnail: Rect { height: rows.min(card.height), ..thumbnail }, details, related, comments }
}

/// The video plays in its own window: the tab shows its details and what to watch next.
fn draw_watch(frame: &mut Frame, state: &State, icons: &Icons, view: &View, tab: &Tab, inner: Rect) {
    let lang = state.lang();
    let Some(video) = &tab.playback.video else { return };
    let layout = watch_layout(inner);
    let details = if view.thumbnails {
        let middle = Rect { y: layout.thumbnail.y + layout.thumbnail.height / 2, height: 1, ..layout.thumbnail };
        frame.render_widget(Paragraph::new("⏳").fg(muted()).alignment(Alignment::Center), middle);
        layout.details
    } else {
        Rect { x: inner.x, width: inner.width, ..layout.details }
    };
    let watch = tab.watch.as_ref();
    let mut channel = vec![Span::styled(video.channel.as_str(), magenta())];
    if let Some(subscribers) = watch.and_then(|w| w.subscribers.as_deref()) {
        channel.push(Span::styled(format!(" · {subscribers} {}", lang.pick("suscriptores", "subscribers")), muted()));
    }
    if video.channel_id.as_ref().is_some_and(|c| state.subs.contains(c)) {
        channel.push(Span::styled(format!("  {} {}", icons.subscribed, lang.pick("Suscripto", "Subscribed")), green()));
    }
    let mut numbers = Vec::new();
    if let Some(views) = watch.and_then(|w| w.views) {
        numbers.push(format!("{} {}", compact(views), lang.pick("vistas", "views")));
    }
    if let Some(likes) = watch.and_then(|w| w.likes) {
        numbers.push(format!("👍 {}", compact(likes)));
    }
    let mut when = vec![length_label(state, video)];
    if let Some(ts) = watch.and_then(|w| w.published).or(video.published) {
        let (date, age) = date_labels(ts, view.now, lang);
        when.push(format!("{age} ({date})"));
    }
    let mut lines = vec![Line::from(video.title.as_str()).fg(fg()).bold(), Line::from(""), Line::from(channel)];
    for row in [numbers, when] {
        let row: Vec<String> = row.into_iter().filter(|p| !p.is_empty()).collect();
        if !row.is_empty() {
            lines.push(Line::from(row.join(" · ")).fg(muted()));
        }
    }
    if state.later.iter().any(|l| l.id == video.id) {
        lines.push(Line::from(format!("{} {}", icons.later, lang.pick("En Ver después", "In Watch later"))).fg(cyan()));
    }
    lines.extend(rating_line(state, icons, &video.id));
    lines.extend(size_line(state, video));
    if tab.playback.audio_only {
        let note = lang.pick("🎧 Solo audio · a vuelve al video", "🎧 Sound only · a brings the video back");
        lines.push(Line::from(note).fg(teal()));
    }
    if tab.ended {
        let note = lang.pick("⏹ Video cerrado · Enter lo vuelve a ver", "⏹ Video closed · Enter plays it again");
        lines.push(Line::from(note).fg(yellow()));
    }
    if let Some(elapsed) = view.opening {
        lines.push(Line::from(loading_text(lang.pick("el video", "the video"), elapsed, lang)).fg(yellow()));
    }
    frame.render_widget(Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true }), details);

    let related = tab.related_videos();
    let focused = tab.on_related && !related.is_empty();
    let title = match watch {
        None => format!(" {} ", lang.pick("Buscando videos relacionados…", "Looking for related videos…")),
        Some(_) if related.is_empty() => format!(" {} ", lang.pick("Sin videos relacionados", "No related videos")),
        Some(_) if focused => format!(" {} · {} ", lang.pick("A continuación", "Up next"), related.len()),
        Some(_) => format!(" {} · {} · Tab ", lang.pick("A continuación", "Up next"), related.len()),
    };
    let highlight = if focused { Style::new().bg(highlight()).add_modifier(Modifier::BOLD) } else { Style::new() };
    let list = List::new(related.iter().map(|v| video_row(state, icons, view, v)))
        .block(panel(title, if focused { blue() } else { muted() }))
        .highlight_symbol(if focused { "▌" } else { " " })
        .highlight_style(highlight);
    let selected = (!related.is_empty()).then_some(tab.related);
    frame.render_stateful_widget(list, layout.related, &mut ListState::default().with_selected(selected));
    if let Some(area) = layout.comments {
        draw_comments(frame, tab, lang, area);
    }
}

/// The video's top comments, from the one scrolled to (PgUp / PgDn).
fn draw_comments(frame: &mut Frame, tab: &Tab, lang: Lang, area: Rect) {
    let title = match &tab.comments {
        None if tab.watch.is_none() || tab.watch.as_ref().is_some_and(|w| w.comments_token.is_some()) => {
            format!(" {} ", lang.pick("Buscando comentarios…", "Looking for comments…"))
        }
        None => format!(" {} ", lang.pick("Comentarios desactivados", "Comments are off")),
        Some(c) if c.is_empty() => format!(" {} ", lang.pick("Sin comentarios", "No comments")),
        Some(c) => format!(
            " {} {}/{} · PgUp PgDn ",
            lang.pick("Comentarios", "Comments"),
            (tab.comments_from + 1).min(c.len()),
            c.len()
        ),
    };
    let mut lines = Vec::new();
    for comment in tab.comments.iter().flatten().skip(tab.comments_from) {
        let mut head = vec![Span::styled(comment.author.clone(), Style::new().fg(magenta()).bold())];
        if let Some(likes) = &comment.likes {
            head.push(Span::styled(format!("  👍 {likes}"), muted()));
        }
        if let Some(published) = &comment.published {
            head.push(Span::styled(format!("  {published}"), muted()));
        }
        lines.push(Line::from(head));
        lines.extend(comment.text.lines().map(|l| Line::from(l.to_owned()).fg(fg())));
        lines.push(Line::from(""));
    }
    let comments = Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true }).block(panel(title, muted()));
    frame.render_widget(comments, area);
}

fn tab_icon<'a>(icons: &'a Icons, tab: &Tab) -> &'a str {
    if tab.playback.audio_only { icons.section(Section::MiMusica) } else { icons.tab }
}

/// `" 🔊 85 %  720p  H.264 "`: what the video keys change, as it is now.
fn playing_label(info: &Info, icons: &Icons) -> String {
    let mut parts = Vec::new();
    if let Some(title) = info.chapter_title() {
        parts.push(format!("§ {}", truncate(title, 32)));
    }
    if let Some(volume) = info.volume {
        let icon = if info.muted { icons.muted } else { icons.volume };
        parts.push(format!("{icon} {volume:.0} %"));
    }
    if let Some(height) = info.height {
        parts.push(format!("{height}p"));
    }
    if let Some(codec) = &info.codec {
        parts.push(match codec.as_str() {
            "h264" => "H.264".into(),
            "hevc" => "H.265".into(),
            other => other.to_uppercase(),
        });
    }
    if parts.is_empty() { String::new() } else { format!(" {} ", parts.join("  ")) }
}

/// Where the progress bar sits in the bottom border of the video tab:
/// the cells of the bar itself and the seconds it spans.
pub fn progress_bar(info: &Info, content: Rect) -> Option<(Rect, f64)> {
    info.position?;
    let duration = info.duration.filter(|d| *d > 0.0)?;
    let clock_width = clock(duration as u32).len() as u16;
    // ` ▶ 1:23 ` before the bar and ` 4:56 ` after it, inside the corners.
    let before = 2 + 2 + clock_width + 1;
    let after = 1 + clock_width + 1;
    let width = content.width.checked_sub(2 + before + after).filter(|w| *w >= 10)?;
    Some((Rect::new(content.x + 1 + before, content.bottom().saturating_sub(1), width, 1), duration))
}

/// ` ▶ 1:23 ━━━━━━━━━━━━──┃───── 4:56 `, the width of the tab's bottom border:
/// played in the gradient, downloaded in grey, the rest dim; the parts SponsorBlock
/// skips in green and where chapters start as `┃`.
fn progress_line(info: &Info, segments: &[Segment], width: u16) -> Option<Line<'static>> {
    let (bar, duration) = progress_bar(info, Rect::new(0, 0, width, 1))?;
    let position = info.position.unwrap_or_default().clamp(0.0, duration);
    let cells = |seconds: f64| ((seconds.clamp(0.0, duration) / duration) * f64::from(bar.width)).round() as usize;
    let done = cells(position);
    let loaded = cells(info.buffered.unwrap_or_default()).max(done);
    let width = usize::from(bar.width);
    // The seconds each cell spans.
    let span = |i: usize| (i as f64 * duration / width as f64, (i + 1) as f64 * duration / width as f64);
    let mut track: Vec<Span<'static>> = Vec::new();
    for i in 0..width {
        let (from, to) = span(i);
        let mut symbol = if i < loaded { "━" } else { "─" };
        let mut color = if i < done {
            gradient(i, usize::from(bar.width))
        } else if i < loaded {
            muted()
        } else {
            rule()
        };
        if segments.iter().any(|s| s.start_secs() < to && s.end_secs() > from) {
            (symbol, color) = ("━", green());
        }
        if info.chapters.iter().any(|(start, _)| *start > 0.0 && *start >= from && *start < to) {
            symbol = "┃";
        }
        match track.last_mut() {
            Some(last) if last.style.fg == Some(color) && last.content.starts_with(symbol) => {
                last.content.to_mut().push_str(symbol);
            }
            _ => track.push(Span::styled(symbol.to_owned(), Style::new().fg(color))),
        }
    }
    let clock_width = clock(duration as u32).len();
    // One cell each: the emoji pause sign takes two in most terminals.
    let icon = if info.paused { "‖" } else { "▶" };
    let mut spans =
        vec![Span::styled(format!(" {icon} {:>clock_width$} ", clock(position as u32)), Style::new().fg(fg()))];
    spans.extend(track);
    spans.push(Span::styled(format!(" {} ", clock(duration as u32)), Style::new().fg(fg())));
    Some(Line::from(spans))
}

fn panel<'a>(title: impl Into<Line<'a>>, color: Color) -> Block<'a> {
    Block::bordered().border_type(border().border_type()).border_style(Style::new().fg(color)).title(title)
}

/// The menu's lines, each with what a click on it does.
fn sidebar_items(state: &State, icons: &Icons) -> Vec<(ListItem<'static>, Option<Spot>)> {
    let lang = state.lang();
    let on_inicio = state.active == 0;
    let mut items: Vec<(ListItem, Option<Spot>)> = Vec::new();
    let mut group = None;
    for (i, section) in Section::ALL.iter().enumerate() {
        if group != Some(section.group()) {
            group = Some(section.group());
            if i > 0 {
                items.push((ListItem::new(""), None));
            }
            if section.group() != Group::App {
                items.push((
                    ListItem::new(Line::from(format!(" {}", section.group().label(lang))).fg(muted()).bold()),
                    None,
                ));
            }
        }
        let mut label = section.label(lang).to_owned();
        if *section == Section::VerDespues && !state.later.is_empty() {
            label.push_str(&format!(" ({})", state.later.len()));
        }
        if *section == Section::Suscripciones && !state.fresh.is_empty() {
            label.push_str(&format!(" ({} {})", state.fresh.len(), lang.pick("nuevos", "new")));
        }
        if *section == Section::Descargas && state.downloading() > 0 {
            label.push_str(&format!(" ({} ⬇)", state.downloading()));
        }
        if *section == Section::AcercaDe && state.update.is_some() {
            label.push_str(" ⬆");
        }
        let selected = i == state.menu;
        let focused = selected && on_inicio && state.focus == Focus::Menu;
        let color = magenta();
        let mut style = Style::new().fg(if selected { color } else { fg() });
        if focused {
            style = style.bg(highlight()).add_modifier(Modifier::BOLD);
        } else if selected && on_inicio {
            style = style.bg(highlight());
        }
        let line = Line::from(vec![
            Span::styled(if focused { "▌" } else { " " }, color),
            Span::raw(format!("{} ", icons.sections[i])),
            Span::raw(label),
        ]);
        items.push((ListItem::new(line).style(style), Some(Spot::Section(i))));
    }
    if !state.tabs.is_empty() {
        items.push((ListItem::new(""), None));
        items.push((ListItem::new(Line::from(lang.pick(" PESTAÑAS", " TABS")).fg(muted()).bold()), None));
        for (i, tab) in state.tabs.iter().enumerate() {
            let active = state.active == i + 1;
            let style = if active { Style::new().fg(magenta()).bg(highlight()).bold() } else { Style::new().fg(fg()) };
            let line =
                format!("{}{} {} {}", if active { "▌" } else { " " }, i + 1, tab_icon(icons, tab), tab.playback.title);
            items.push((ListItem::new(line).style(style), Some(Spot::Tab(i + 1))));
        }
    }
    items
}

fn draw_sidebar(frame: &mut Frame, state: &State, icons: &Icons, area: Rect) {
    let items = sidebar_items(state, icons).into_iter().map(|(item, _)| item);
    let title = Line::from(vec![
        Span::raw(format!(" {} ", icons.logo)),
        Span::styled("tube", Style::new().fg(fg()).bold()),
        Span::styled("tui ", Style::new().fg(magenta()).bold()),
    ]);
    frame.render_widget(List::new(items).block(panel(title, muted())), area);
}

fn tab_titles(state: &State, icons: &Icons) -> Vec<String> {
    let home = state.lang().pick("Inicio", "Home");
    std::iter::once(format!("{} {home}", icons.section(Section::Inicio)))
        .chain(
            state
                .tabs
                .iter()
                .enumerate()
                .map(|(i, t)| format!("{} {} {}", i + 1, tab_icon(icons, t), truncate(&t.playback.title, 20))),
        )
        .collect()
}

fn draw_tabs(frame: &mut Frame, state: &State, icons: &Icons, area: Rect) {
    let tabs = Tabs::new(tab_titles(state, icons))
        .select(state.active)
        .style(Style::new().fg(muted()))
        .highlight_style(Style::new().fg(blue()).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, area);
}

/// The search box and the rest of Inicio below it.
fn split_prompt(state: &State, area: Rect) -> (Option<Rect>, Rect) {
    match state.prompt {
        Some(_) => {
            let [input, rest] = Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);
            (Some(input), rest)
        }
        None => (None, area),
    }
}

/// A video list and, when there is room, the preview panel beside it.
fn split_preview(area: Rect) -> (Rect, Option<Rect>) {
    if area.width < PREVIEW_MIN_WIDTH {
        return (area, None);
    }
    let [list, preview] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(PREVIEW_WIDTH)]).areas(area);
    (list, Some(preview))
}

/// The cells of the thumbnail inside a preview panel: 16:9, taking terminal
/// cells as about twice as tall as wide.
fn thumbnail_cells(preview: Rect) -> Rect {
    let inner = Block::bordered().inner(preview);
    let rows = (inner.width as u32 * 9 / 32) as u16;
    Rect { height: rows.min(inner.height), ..inner }
}

/// Where the thumbnail of the selected video goes, with that video's id.
pub fn thumbnail_area(state: &State, screen: Rect) -> Option<(String, Rect)> {
    if state.splash || state.help || state.confirm.is_some() || state.prompt == Some(Prompt::CookiesFile) {
        return None;
    }
    if let Some(tab) = state.active_tab() {
        let video = tab.playback.video.as_ref().filter(|_| state.details_shown(tab) && !state.fullscreen)?;
        let inner = Block::bordered().inner(areas(screen, state.sidebar).content);
        let cells = watch_layout(inner).thumbnail;
        return under_context_menu(state, screen, cells).filter(|c| c.height > 0).map(|c| (video.id.clone(), c));
    }
    let video = state.videos_from_selection(0).first()?;
    let (_, list) = split_prompt(state, areas(screen, state.sidebar).content);
    let (_, preview) = split_preview(list);
    Some((video.id.clone(), under_context_menu(state, screen, thumbnail_cells(preview?))?))
}

fn draw_inicio(frame: &mut Frame, state: &State, icons: &Icons, view: &View, area: Rect) {
    let lang = state.lang();
    let (input, rest) = split_prompt(state, area);
    let area = match (&state.prompt, input) {
        (Some(prompt), Some(input)) => {
            let title = match prompt {
                Prompt::Videos { scope, .. } => Line::from(vec![
                    Span::raw(format!(
                        " {} {} ",
                        icons.section(Section::BuscarVideos),
                        lang.pick("Buscar videos en", "Search videos in")
                    )),
                    Span::styled(scope.label(lang), Style::new().fg(yellow()).bold()),
                    Span::raw(lang.pick(" · Tab cambia ", " · Tab changes ")).fg(muted()),
                ]),
                Prompt::Channels => Line::from(format!(
                    " {} {} ",
                    icons.section(Section::BuscarCanales),
                    lang.pick("Buscar canales", "Search channels")
                )),
                Prompt::CookiesFile => Line::from(format!(
                    " {} {} ",
                    icons.setting(Setting::ImportCookies),
                    lang.pick("Ruta del cookies.txt", "cookies.txt path")
                )),
                Prompt::DownloadDir => Line::from(format!(
                    " {} {} ",
                    icons.setting(Setting::DownloadDir),
                    lang.pick("Carpeta de descargas", "Downloads folder")
                )),
                Prompt::MediaDirs(kind) => {
                    let (icon, what) = match kind {
                        MediaKind::Video => {
                            (icons.setting(Setting::VideoDirs), lang.pick("Carpetas de videos", "Video folders"))
                        }
                        MediaKind::Music => {
                            (icons.setting(Setting::MusicDirs), lang.pick("Carpetas de música", "Music folders"))
                        }
                    };
                    let how = lang.pick(
                        "separadas por coma (vacío = la del sistema)",
                        "comma separated (empty = the system one)",
                    );
                    Line::from(format!(" {icon} {what}, {how} "))
                }
                Prompt::Filter => Line::from(format!(
                    " {} {} ",
                    icons.section(Section::BuscarVideos),
                    lang.pick("Filtrar esta lista", "Filter this list")
                )),
                Prompt::Move(files) => Line::from(format!(
                    " {} {} ",
                    lang.pick("Mover a la carpeta", "Move to the folder"),
                    match files.as_slice() {
                        [one] => one.file_name().map_or_else(String::new, |n| truncate(&n.to_string_lossy(), 40)),
                        many => format!("({} {})", many.len(), lang.pick("archivos", "files")),
                    }
                )),
                Prompt::Rename(from) => Line::from(format!(
                    " {} {} ",
                    lang.pick("Nuevo nombre de", "New name for"),
                    from.file_name().map_or_else(String::new, |n| truncate(&n.to_string_lossy(), 40))
                )),
                Prompt::Topics => Line::from(format!(
                    " {} {} ",
                    icons.setting(Setting::DiscoverTopics),
                    lang.pick("Temas de Descubrir, separados por coma", "Discover topics, comma separated")
                )),
            };
            frame.render_widget(Paragraph::new(state.query.as_str()).block(panel(title, cyan())), input);
            let x = input.x + 1 + state.query.chars().count() as u16;
            frame.set_cursor_position(Position::new(x.min(input.right().saturating_sub(2)), input.y + 1));
            if *prompt == Prompt::CookiesFile {
                draw_cookies_help(frame, rest, lang);
                return;
            }
            rest
        }
        _ => area,
    };
    match state.listing() {
        Some(listing) => draw_listing(frame, state, icons, view, listing, area),
        None => draw_welcome(frame, state, icons, view, area),
    }
}

fn draw_cookies_help(frame: &mut Frame, area: Rect, lang: Lang) {
    let line = |es, en, color| Line::from(lang.pick(es, en)).fg(color);
    let lines = vec![
        Line::from(""),
        line(
            "Para usar tu cuenta en una máquina sin navegador (servidor, SSH):",
            "To use your account on a machine without a browser (server, SSH):",
            fg(),
        ),
        Line::from(""),
        line(
            "  1. En cualquier PC con navegador, entrá a youtube.com con tu cuenta.",
            "  1. On any computer with a browser, log in to youtube.com.",
            fg(),
        ),
        line(
            "  2. Exportá las cookies en formato Netscape (cookies.txt), por ejemplo con",
            "  2. Export the cookies in Netscape format (cookies.txt), for example with",
            fg(),
        ),
        line(
            "     la extensión «Get cookies.txt LOCALLY», o con:",
            "     the “Get cookies.txt LOCALLY” extension, or with:",
            fg(),
        ),
        Line::from("     yt-dlp --cookies-from-browser firefox --cookies cookies.txt --skip-download :ytsubs")
            .fg(cyan()),
        line(
            "  3. Copialo a esta máquina (scp cookies.txt servidor:~/) y escribí la ruta arriba.",
            "  3. Copy it to this machine (scp cookies.txt server:~/) and type its path above.",
            fg(),
        ),
        Line::from(""),
        line(
            "tubetui comprueba que tenga una sesión de YouTube y guarda una copia privada (permisos 600).",
            "tubetui checks it holds a YouTube login and keeps a private copy (mode 600).",
            muted(),
        ),
        line(
            "Las cookies dan acceso a tu cuenta: no las compartas.",
            "Cookies give access to your account: never share them.",
            yellow(),
        ),
    ];
    let title = lang.pick(" Importar sesión ", " Import login ");
    frame.render_widget(Paragraph::new(lines).block(panel(title, muted())), area);
}

fn draw_welcome(frame: &mut Frame, state: &State, icons: &Icons, view: &View, area: Rect) {
    let lang = state.lang();
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw(format!("{}  ", icons.logo)),
            Span::styled("tube", Style::new().fg(fg()).bold()),
            Span::styled("tui", Style::new().fg(magenta()).bold()),
        ]),
        Line::from(lang.pick("YouTube en tu terminal", "YouTube in your terminal")).fg(muted()),
        Line::from(""),
        Line::from(
            lang.pick("Elegí una sección del menú con ↑ ↓ y Enter", "Pick a section from the menu with ↑ ↓ and Enter"),
        )
        .fg(fg()),
        Line::from(lang.pick(
            "Abajo siempre ves las teclas que sirven en cada momento",
            "The bottom line always shows the keys that work right now",
        ))
        .fg(muted()),
        Line::from(lang.pick(
            "Ctrl+y ?  muestra todos los atajos explicados · Ctrl+y l  English",
            "Ctrl+y ?  shows every shortcut, explained · Ctrl+y l  español",
        ))
        .fg(muted()),
    ];
    if let Some(title) = &state.loading {
        lines.push(Line::from(""));
        lines.push(Line::from(loading_text(title, view.loading.unwrap_or_default(), lang)).fg(yellow()));
        lines.push(Line::from(bouncing_bar(30, view.loading.unwrap_or_default())).fg(blue()));
    }
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center).block(panel("", muted())), area);
}

fn draw_listing(frame: &mut Frame, state: &State, icons: &Icons, view: &View, listing: &Listing, area: Rect) {
    if listing.items == Items::About {
        draw_about(frame, state, icons, area);
        return;
    }
    if listing.items == Items::System {
        draw_system(frame, state, area);
        return;
    }
    let breadcrumb: Vec<&str> = state.stack.iter().map(|l| l.title.as_str()).collect();
    // "3/50": where the selection is; and what the list is narrowed to.
    let mut count = match listing.items.len() {
        0 => "0".to_owned(),
        len => format!("{}/{len}", listing.selected + 1),
    };
    if !listing.filter.is_empty() {
        count.push_str(&format!(" · /{}", listing.filter));
    }
    let title = match &state.loading {
        Some(loading) => format!(" {} ", loading_text(loading, view.loading.unwrap_or_default(), state.lang())),
        None if listing.items == Items::Settings => format!(" {} ", breadcrumb.join(" › ")),
        None => match &listing.items {
            Items::Videos(videos) if listing.all.is_some() => {
                let hidden = listing.all.as_ref().map_or(0, Vec::len) - videos.len();
                let note = match (state.show_watched, hidden) {
                    (true, _) => {
                        format!(" · {} {}", icons.watched, state.lang().pick("con los vistos", "watched included"))
                    }
                    (false, 0) => String::new(),
                    (false, n) => {
                        format!(" · {} {n} {}", icons.watched, state.lang().pick("vistos ocultos", "watched hidden"))
                    }
                };
                format!(" {} · {count}{note} ", breadcrumb.join(" › "))
            }
            Items::Files(files, _) if files.iter().any(|f| state.marked.contains(&f.path)) => {
                let marked = files.iter().filter(|f| state.marked.contains(&f.path)).count();
                let label = state.lang().pick("marcados", "marked");
                format!(" {} · {count} · ✗ {marked} {label} ", breadcrumb.join(" › "))
            }
            Items::Files(..) => {
                let (sort, by) = (state.file_sort.label(state.lang()), state.lang().pick("orden", "sort"));
                format!(" {} · {count} · {by}: {sort} ", breadcrumb.join(" › "))
            }
            _ => format!(" {} · {count} ", breadcrumb.join(" › ")),
        },
    };
    let items: Vec<ListItem> = match &listing.items {
        Items::Videos(videos) => {
            let history = listing.kind == ListKind::History;
            videos.iter().map(|v| video_row_dated(state, icons, view, v, history)).collect()
        }
        Items::Channels(channels) => {
            // The description goes in the panel when there is one, other details in the row.
            let panel = split_preview(area).1.is_some();
            channels.iter().map(|c| channel_row(state, icons, c, panel)).collect()
        }
        Items::Profiles(profiles) => profiles.iter().map(|p| profile_row(state, icons, p)).collect(),
        Items::Playlists(playlists) => playlists.iter().map(|p| playlist_row(icons, p, state.lang())).collect(),
        Items::Settings => {
            Setting::ALL.iter().enumerate().map(|(i, s)| setting_row(state, icons.settings[i], *s)).collect()
        }
        Items::Files(files, kind) => {
            let folder_width = files.iter().map(|f| Span::raw(f.folder.as_str()).width()).max().unwrap_or(0).min(28);
            // What is left of the row (border, highlight, mark, icon, folder, size) is for the name.
            let name_width = usize::from(area.width).saturating_sub(2 + 1 + 1 + 3 + folder_width + 1 + 10);
            files
                .iter()
                .map(|f| file_row(icons, f, *kind, state.marked.contains(&f.path), folder_width, name_width))
                .collect()
        }
        Items::Downloads(downloads) => downloads.iter().map(|d| download_row(state, d, state.lang())).collect(),
        Items::About | Items::System => Vec::new(),
    };
    let focused = state.focus == Focus::List;
    let highlight = if focused { Style::new().bg(highlight()).add_modifier(Modifier::BOLD) } else { Style::new() };
    let list = List::new(items)
        .block(panel(title, if focused { blue() } else { muted() }))
        .highlight_symbol(if focused { "▌" } else { " " })
        .highlight_style(highlight);
    let selected = (!listing.items.is_empty()).then_some(listing.selected);
    let area = match (&listing.items, split_preview(area)) {
        (Items::Videos(videos), (left, Some(preview))) => {
            draw_preview(frame, state, icons, view, videos.get(listing.selected), preview);
            left
        }
        (Items::Channels(channels), (left, Some(preview))) => {
            draw_channel_preview(frame, state, icons, channels.get(listing.selected), preview);
            left
        }
        _ => area,
    };
    frame.render_stateful_widget(list, area, &mut ListState::default().with_selected(selected));
    draw_scrollbar(frame, area, listing.items.len(), listing.selected);
}

/// A thin bar on the right border when the list does not fit.
fn draw_scrollbar(frame: &mut Frame, area: Rect, len: usize, selected: usize) {
    let rows = usize::from(area.height.saturating_sub(2));
    if len <= rows || rows == 0 {
        return;
    }
    let mut state = ScrollbarState::new(len - rows).position(selected.saturating_sub(rows - 1));
    let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(None)
        .thumb_symbol("┃")
        .thumb_style(Style::new().fg(blue()));
    frame.render_stateful_widget(bar, area.inner(Margin::new(0, 1)), &mut state);
}

/// The selected video: thumbnail (drawn by the runtime over the empty cells
/// left here) and its details.
fn draw_preview(frame: &mut Frame, state: &State, icons: &Icons, view: &View, video: Option<&Video>, area: Rect) {
    let lang = state.lang();
    let block = panel(lang.pick(" Vista previa ", " Preview "), muted());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(v) = video else { return };
    let text_area = if view.thumbnails {
        let image = thumbnail_cells(area);
        frame.render_widget(
            Paragraph::new("⏳").fg(muted()).alignment(Alignment::Center),
            Rect { y: image.y + image.height / 2, height: 1, ..image },
        );
        Rect { y: image.bottom() + 1, height: inner.bottom().saturating_sub(image.bottom() + 1), ..inner }
    } else {
        inner
    };
    let mut details = vec![length_label(state, v)];
    if let Some(ts) = v.published {
        let (date, age) = date_labels(ts, view.now, lang);
        details.push(format!("{age} ({date})"));
    }
    let mut lines = vec![
        Line::from(v.title.as_str()).fg(fg()).bold(),
        Line::from(""),
        Line::from(v.channel.as_str()).fg(magenta()),
        Line::from(details.into_iter().filter(|d| !d.is_empty()).collect::<Vec<_>>().join(" · ")).fg(muted()),
    ];
    lines.extend(size_line(state, v));
    match v.live {
        Live::Now => lines.push(Line::from(format!("{} {}", icons.live, lang.pick("en vivo", "live"))).fg(red())),
        Live::Upcoming => {
            lines.push(Line::from(format!("{} {}", icons.upcoming, lang.pick("próximo", "upcoming"))).fg(yellow()))
        }
        Live::No => {}
    }
    if let Some(at) = state.resume_at(&v.id) {
        lines.push(Line::from(format!("⏯ {} {}", lang.pick("Sigue desde", "Resumes at"), clock(at))).fg(yellow()));
    }
    if state.later.iter().any(|l| l.id == v.id) {
        lines.push(Line::from(format!("{} {}", icons.later, lang.pick("En Ver después", "In Watch later"))).fg(cyan()));
    }
    if v.channel_id.as_ref().is_some_and(|c| state.subs.contains(c)) {
        lines.push(Line::from(format!("{} {}", icons.subscribed, lang.pick("Suscripto", "Subscribed"))).fg(green()));
    }
    lines.extend(rating_line(state, icons, &v.id));
    frame.render_widget(Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true }), text_area);
}

/// "Te gusta" / "No te gusta" when I voted on the video.
fn rating_line(state: &State, icons: &Icons, id: &str) -> Option<Line<'static>> {
    let lang = state.lang();
    match state.ratings.get(id)? {
        Rating::Like => Some(Line::from(format!("{} {}", icons.liked, lang.pick("Te gusta", "Liked"))).fg(green())),
        Rating::Dislike => {
            Some(Line::from(format!("{} {}", icons.disliked, lang.pick("No te gusta", "Disliked"))).fg(red()))
        }
        Rating::None => None,
    }
}

/// The selected channel: name, subscribers and its whole description.
fn draw_channel_preview(frame: &mut Frame, state: &State, icons: &Icons, channel: Option<&Channel>, area: Rect) {
    let lang = state.lang();
    let block = panel(lang.pick(" Canal ", " Channel "), muted());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(c) = channel else { return };
    let mut lines = vec![Line::from(c.name.as_str()).fg(magenta()).bold()];
    if let Some(handle) = &c.handle {
        lines.push(Line::from(handle.as_str()).fg(cyan()));
    }
    if let Some(followers) = &c.followers {
        lines.push(Line::from(followers.as_str()).fg(muted()));
    }
    if state.subs.contains(&c.id) {
        lines.push(Line::from(format!("{} {}", icons.subscribed, lang.pick("Suscripto", "Subscribed"))).fg(green()));
    }
    lines.push(Line::from(""));
    match &c.description {
        Some(description) => lines.extend(description.lines().map(|l| Line::from(l).fg(fg()))),
        None => lines.push(Line::from(lang.pick("Sin descripción", "No description")).fg(muted())),
    }
    frame.render_widget(Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true }), inner);
}

fn video_row<'a>(state: &State, icons: &Icons, view: &View, v: &'a Video) -> ListItem<'a> {
    video_row_dated(state, icons, view, v, false)
}

/// `watched_date`: the date column says when I watched it (Historial).
fn video_row_dated<'a>(state: &State, icons: &Icons, view: &View, v: &'a Video, watched_date: bool) -> ListItem<'a> {
    let lang = state.lang();
    let watched_at = watched_date.then(|| state.watched_at(&v.id)).flatten();
    let live = if watched_at.is_some() { Live::No } else { v.live };
    let mut spans = match (live, watched_at.or(v.published)) {
        (Live::Now, _) => {
            let live = format!("{} {}", icons.live, lang.pick("en vivo", "live"));
            vec![Span::styled(pad_cells(&live, DATE_WIDTH), red())]
        }
        (Live::Upcoming, _) => {
            let upcoming = format!("{} {}", icons.upcoming, lang.pick("próximo", "upcoming"));
            vec![Span::styled(pad_cells(&upcoming, DATE_WIDTH), yellow())]
        }
        (Live::No, Some(ts)) => {
            let (date, age) = date_labels(ts, view.now, lang);
            vec![Span::styled(date, green()), Span::styled(format!(" {age:<5}"), muted())]
        }
        (Live::No, None) => vec![Span::raw(" ".repeat(DATE_WIDTH))],
    };
    spans.push(Span::styled(format!(" {} ", pad(&v.channel, CHANNEL_WIDTH)), magenta()));
    if state.fresh_shown.contains(&v.id) {
        spans.push(Span::styled(format!("{} ", lang.pick("NUEVO", "NEW")), Style::new().fg(green()).bold()));
    }
    if state.is_favorite(v) {
        spans.push(Span::raw(format!("{} ", icons.favorite)));
    }
    if state.later.iter().any(|l| l.id == v.id) {
        spans.push(Span::raw(format!("{} ", icons.later)));
    }
    if state.watched.contains(&v.id) {
        // Already seen: marked and dimmed.
        spans.push(Span::styled(format!("{} ", icons.watched), muted()));
        spans.push(Span::styled(v.title.as_str(), muted()));
    } else {
        spans.push(Span::styled(v.title.as_str(), fg()));
    }
    spans.push(Span::styled(format!("  {}", v.duration_label()), muted()));
    if let Some(at) = state.resume_at(&v.id) {
        spans.extend(resume_bar(at, v.duration));
    }
    ListItem::new(Line::from(spans))
}

/// `from` to `to`, `t` of the way (colors that are not RGB do not blend).
fn blend(from: Color, to: Color, t: f64) -> Color {
    match (from, to) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let mix = |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as u8;
            Color::Rgb(mix(r1, r2), mix(g1, g2), mix(b1, b2))
        }
        _ => from,
    }
}

/// Cell `i` of `cells` on the blue-to-magenta gradient of the progress bars.
fn gradient(i: usize, cells: usize) -> Color {
    blend(blue(), magenta(), i as f64 / cells.saturating_sub(1).max(1) as f64)
}

/// A bar of `cells`, `done` of them filled with the gradient and the rest a quiet track.
fn bar(done: usize, cells: usize) -> Vec<Span<'static>> {
    let done = done.min(cells);
    let mut spans: Vec<Span<'static>> = (0..done).map(|i| Span::styled("━", gradient(i, cells))).collect();
    spans.push(Span::styled("─".repeat(cells - done), rule()));
    spans
}

/// How far I got: `  ━━━━─────` (or `  ⏯ 12:34` when the length is unknown).
fn resume_bar(at: u32, duration: Option<u32>) -> Vec<Span<'static>> {
    const CELLS: usize = 10;
    match duration.filter(|d| *d > 0) {
        Some(d) => {
            let done = ((u64::from(at) * CELLS as u64 / u64::from(d)) as usize).clamp(1, CELLS);
            let mut spans = vec![Span::raw("  ")];
            spans.extend(bar(done, CELLS));
            spans
        }
        None => vec![Span::styled(format!("  ⏯ {}", clock(at)), yellow())],
    }
}

/// Name, subscribers and @handle in columns; the description too unless
/// the side panel shows it.
fn channel_row<'a>(state: &State, icons: &Icons, c: &'a Channel, panel: bool) -> ListItem<'a> {
    let mark = if state.subs.contains(&c.id) { icons.subscribed } else { " " };
    let column = |text: &str, width: usize| pad_cells(&truncate(text, width - 2), width);
    let mut spans = vec![
        Span::styled(format!(" {mark} "), green()),
        Span::styled(column(&c.name, CHANNEL_NAME_WIDTH), magenta()),
        Span::styled(column(c.followers.as_deref().unwrap_or(""), FOLLOWERS_WIDTH), muted()),
    ];
    if let Some(n) = state.channel_views.get(&c.id).filter(|_| state.config.ui.favorites_first) {
        let star = if state.favorites.contains(&c.id) { icons.favorite } else { " " };
        let word = if *n == 1 { state.lang().pick("visto", "watched") } else { state.lang().pick("vistos", "watched") };
        let seen = format!("{star} {n} {word}");
        spans.push(Span::styled(pad_cells(&seen, 14), yellow()));
    }
    if let Some(handle) = &c.handle {
        spans.push(Span::styled(handle.as_str(), cyan()));
    }
    if let Some(description) = c.description.as_ref().filter(|_| !panel) {
        spans.push(Span::styled(
            format!("  {}", description.replace('\n', " ")),
            Style::new().fg(fg()).add_modifier(Modifier::DIM),
        ));
    }
    ListItem::new(Line::from(spans))
}

fn playlist_row<'a>(icons: &Icons, playlist: &'a Playlist, lang: Lang) -> ListItem<'a> {
    let mut spans = vec![
        Span::raw(format!(" {} ", icons.section(Section::MisListas))),
        Span::styled(pad_cells(&truncate(playlist.name(lang), 48), 50), fg()),
    ];
    if let Some(count) = &playlist.count {
        spans.push(Span::styled(count.as_str(), muted()));
    }
    ListItem::new(Line::from(spans))
}

/// `⬇  45.3% ━━━━━─────  2.1MiB/s  00:12   title · channel`.
/// How long `video` is, from the list or, when it did not say, from yt-dlp.
fn length_label(state: &State, video: &Video) -> String {
    video.duration.or_else(|| state.sizes.get(&video.id).and_then(|s| s.duration)).map(clock).unwrap_or_default()
}

/// `"💾 Descarga ≈ 30 MB (720p)"`, or that it is being worked out.
fn size_line(state: &State, video: &Video) -> Option<Line<'static>> {
    let lang = state.lang();
    let text = match state.sizes.get(&video.id) {
        Some(size) if size.bytes.is_some() => format!("💾 {} {}", lang.pick("Descarga", "Download"), size.label()),
        Some(_) => return None,
        None if video.live == Live::No => format!("💾 {}", lang.pick("Calculando el peso…", "Working out the size…")),
        None => return None,
    };
    Some(Line::from(text).fg(muted()))
}

fn download_row(state: &State, download: &Download, lang: Lang) -> ListItem<'static> {
    const BAR: usize = 12;
    let mut spans = match &download.state {
        DownloadState::Running { permille, speed, eta } => {
            let done = usize::from(*permille) * BAR / 1000;
            vec![Span::styled(format!(" ⬇ {:>5.1}% ", f64::from(*permille) / 10.0), Style::new().fg(yellow()).bold())]
                .into_iter()
                .chain(bar(done, BAR))
                .chain([Span::styled(format!(" {speed:>10} {eta:>8}  "), muted())])
                .collect()
        }
        DownloadState::Done(_) => {
            vec![Span::styled(pad_cells(&format!(" ✓ {}", lang.pick("lista", "done")), 41), green())]
        }
        DownloadState::Failed(_) => {
            vec![Span::styled(pad_cells(&format!(" ✗ {}", lang.pick("falló", "failed")), 41), red())]
        }
        DownloadState::Cancelled => {
            vec![Span::styled(pad_cells(&format!(" ⏹ {}", lang.pick("cancelada", "cancelled")), 41), muted())]
        }
    };
    spans.push(Span::styled(download.video.title.clone(), fg()));
    spans.push(Span::styled(format!(" · {}", download.video.channel), magenta()));
    let size = match &download.state {
        DownloadState::Done(path) => std::fs::metadata(path).ok().map(|m| crate::domain::bytes_label(m.len())),
        _ => state.sizes.get(&download.video.id).map(Size::label).filter(|l| !l.is_empty()),
    };
    if let Some(size) = size {
        spans.push(Span::styled(format!("  {size}"), yellow()));
    }
    match &download.state {
        DownloadState::Done(path) => spans.push(Span::styled(format!("  {}", path.display()), muted())),
        DownloadState::Failed(error) => spans.push(Span::styled(format!("  {error}"), red())),
        _ => {}
    }
    ListItem::new(Line::from(spans))
}

fn profile_row<'a>(state: &State, icons: &Icons, spec: &'a str) -> ListItem<'a> {
    let current = if spec == state.config.session.browser && state.config.session.cookies_file.is_empty() {
        "● "
    } else {
        "  "
    };
    let label = if spec.is_empty() {
        state.lang().pick("Sin sesión", "Not logged in").to_owned()
    } else {
        session::label(spec)
    };
    let mut spans = vec![
        Span::styled(format!(" {current}"), green()),
        Span::raw(format!("{} ", icons.user)),
        Span::styled(label, fg()),
    ];
    if let Some(&used) = state.browsers_used.get(spec) {
        let (date, age) = date_labels(used, state.clock, state.lang());
        let last = state.lang().pick("último uso", "last used");
        spans.push(Span::styled(format!("  · {last} {date} ({age})"), muted()));
    }
    ListItem::new(Line::from(spans))
}

fn file_row<'a>(
    icons: &Icons,
    file: &'a LocalFile,
    kind: MediaKind,
    marked: bool,
    folder_width: usize,
    name_width: usize,
) -> ListItem<'a> {
    let icon = match kind {
        MediaKind::Video => icons.section(Section::MisVideos),
        MediaKind::Music => icons.section(Section::MiMusica),
    };
    ListItem::new(Line::from(vec![
        Span::styled(if marked { "✗" } else { " " }, Style::new().fg(red()).bold()),
        Span::raw(format!("{icon} ")),
        Span::styled(format!("{} ", pad(&file.folder, folder_width)), muted()),
        Span::styled(
            format!("{:>8}  ", if file.size > 0 { bytes_label(file.size) } else { String::new() }),
            size_color(file.size),
        ),
        Span::styled(readable_name(&file.name, name_width), fg()),
    ]))
}

/// Light files green, then yellow, orange and red for the heavy ones.
fn size_color(bytes: u64) -> Color {
    const MB: u64 = 1 << 20;
    match bytes {
        0 => muted(),
        b if b < 500 * MB => green(),
        b if b < 2048 * MB => yellow(),
        b if b < 4096 * MB => orange(),
        _ => red(),
    }
}

/// A release name (`Show.1x01.Title.WEB-DL`) with spaces instead of dots, cut
/// in the middle with `…` to fit `width` cells, so the end still shows.
fn readable_name(name: &str, width: usize) -> String {
    let name =
        if !name.contains(' ') && name.matches('.').count() >= 3 { name.replace('.', " ") } else { name.to_owned() };
    let chars: Vec<char> = name.chars().collect();
    if width < 8 || chars.len() <= width {
        return name;
    }
    let keep = width - 1;
    let head = keep * 3 / 5;
    let tail = keep - head;
    let (start, end): (String, String) = (chars[..head].iter().collect(), chars[chars.len() - tail..].iter().collect());
    format!("{}…{}", start.trim_end(), end.trim_start())
}

fn setting_row(state: &State, icon: &str, setting: Setting) -> ListItem<'static> {
    let value = state.setting_value(setting);
    let mut spans =
        vec![Span::raw(format!(" {icon} ")), Span::styled(pad_cells(setting.label(state.lang()), 44), fg())];
    if !value.is_empty() {
        spans.push(Span::styled(value, Style::new().fg(yellow()).bold()));
    }
    ListItem::new(Line::from(spans))
}

fn draw_status(frame: &mut Frame, state: &State, icons: &Icons, view: &View, area: Rect) {
    let right = format!(
        "{} {}  {} {} ",
        icons.user,
        state.session_label(),
        icons.language,
        language_label(state.config.player.language, state.lang())
    );
    let right_width = Span::raw(right.as_str()).width() as u16 + 1;
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_width)]).areas(area);
    let left = match &state.loading {
        Some(title) => {
            Paragraph::new(format!(" {}", loading_text(title, view.loading.unwrap_or_default(), state.lang())))
                .fg(yellow())
        }
        None => Paragraph::new(format!(" {}", state.status)).fg(fg()),
    };
    // A shaded bar, so it reads apart from the keys below it.
    frame.render_widget(Block::new().bg(highlight()), area);
    frame.render_widget(left, left_area);
    frame.render_widget(Paragraph::new(right).fg(blue()).alignment(Alignment::Right), right_area);
}

/// Keys that work right now, each with what it does.
pub fn hints(state: &State) -> Vec<(String, &'static str)> {
    let lang = state.lang();
    let rows = |table| keymap::rows(table, lang);
    let key = |k: &str, es, en| (k.to_string(), lang.pick(es, en));
    // Global commands are two keys: Ctrl+y, let go, then the letter.
    let all_keys = key("Ctrl+y + ?", "todos los atajos", "every shortcut");
    // `s` asks before unsubscribing; it says so when it will.
    let subscribe = if state.selected_channel().is_some_and(|(id, ..)| state.subs.contains(&id)) {
        key("s", "desuscribirse (pide confirmar)", "unsubscribe (asks first)")
    } else {
        key("s", "suscribirse", "subscribe")
    };
    let mut hints = if let Some(confirm) = &state.confirm {
        let yes = match confirm {
            Confirm::Unsubscribe { .. } => key(lang.pick("s", "y"), "sí, desuscribirme", "yes, unsubscribe"),
            Confirm::Trash(_) => key(lang.pick("s", "y"), "sí, a la papelera", "yes, to the trash"),
            Confirm::Quit(_) => key(lang.pick("s", "y"), "sí, salir y cortarlas", "yes, quit and stop them"),
            Confirm::DeletePlaylist { .. } => key(lang.pick("s", "y"), "sí, borrarla", "yes, delete it"),
        };
        vec![yes, key(lang.pick("otra tecla", "other key"), "no", "no")]
    } else if state.help {
        vec![key(lang.pick("cualquier tecla", "any key"), "cerrar la ayuda", "close the help")]
    } else if state.context.is_some() {
        vec![
            key("↑ ↓", "elegir", "choose"),
            key(lang.pick("Enter / clic", "Enter / click"), "hacerlo", "do it"),
            key("Esc", "cerrar", "close"),
        ]
    } else if state.prefix {
        let mut keys = vec![key("Ctrl+y +", "ahora la tecla:", "now the key:")];
        keys.extend(keymap::prefix_rows(lang));
        return keys;
    } else if let Some(prompt) = &state.prompt {
        match prompt {
            Prompt::Videos { .. } => rows(keymap::IN_SEARCH),
            Prompt::Filter => vec![
                key("Enter", "dejar el filtro", "keep the filter"),
                key("Esc", "quitar el filtro", "clear the filter"),
            ],
            _ => vec![key("Enter", "aceptar", "accept"), key("Esc", "cancelar", "cancel")],
        }
    } else if state.key_context().related {
        vec![
            key("Enter", "ver", "play"),
            key("↑ ↓", "elegir", "choose"),
            key("Tab", "volver al video", "back to the video"),
            key("l", "ver después", "watch later"),
            key("c", "canal", "channel"),
            if state.active_tab().is_some_and(|t| t.ended) {
                key("espacio", "volver a verlo", "play it again")
            } else {
                key("espacio", "pausa", "pause")
            },
        ]
    } else if state.active_tab().is_some() {
        let mut keys = rows(keymap::IN_TAB);
        keys.insert(1, key("Ctrl+y + x", "cerrar la pestaña", "close the tab"));
        if state.related_shown().is_some() {
            keys.insert(0, key("Tab", "A continuación", "Up next"));
        }
        if state.active_tab().is_some_and(|t| t.ended) {
            keys.insert(0, key("Enter", "volver a verlo", "play it again"));
        }
        keys.extend(rows(keymap::ON_VIDEO));
        keys
    } else if state.focus == Focus::Menu || state.listing().is_none() {
        rows(keymap::IN_MENU)
    } else {
        let back = key("Esc", "volver", "back");
        match state.listing().map(|l| &l.items) {
            Some(Items::Videos(_)) if state.listing().is_some_and(|l| l.all.is_some()) => {
                let home = state.listing().filter(|l| l.home).map(|_| {
                    if state.config.ui.home_all {
                        key("t", "solo mis canales", "only my channels")
                    } else {
                        key("t", "todo, como YouTube", "all, as on YouTube")
                    }
                });
                let mut keys = vec![
                    key("Enter", "ver", "play"),
                    if state.show_watched {
                        key("v", "ocultar vistos", "hide watched")
                    } else {
                        key("v", "mostrar vistos", "show watched")
                    },
                    key("l", "ver después", "watch later"),
                    key("a", "solo audio", "sound only"),
                    subscribe.clone(),
                    key("g", "me gusta", "like"),
                    key("d", "descargar", "download"),
                    key("c", "canal", "channel"),
                    key("/", "buscar", "search"),
                    back,
                ];
                if let Some(home) = home {
                    keys.insert(1, home);
                }
                keys
            }
            Some(Items::Videos(_)) if state.listing().is_some_and(|l| l.kind == ListKind::History) => vec![
                key("Enter", "ver (sigue donde lo dejaste)", "play (resumes where you left)"),
                key("x", "quitar del historial", "remove from history"),
                key("l", "ver después", "watch later"),
                key("c", "canal", "channel"),
                back,
            ],
            Some(Items::Videos(_)) => vec![
                key("Enter", "ver", "play"),
                key("l", "ver después", "watch later"),
                key("a", "solo audio", "sound only"),
                subscribe.clone(),
                key("g", "me gusta", "like"),
                key("d", "descargar", "download"),
                key("c", "canal", "channel"),
                key("/", "buscar", "search"),
                back,
            ],
            Some(Items::Channels(_)) => vec![key("Enter", "ver sus videos", "its videos"), subscribe, back],
            Some(Items::Profiles(_)) => vec![key("Enter", "usar esta sesión", "use this login"), back],
            Some(Items::Playlists(_)) => vec![
                key("Enter", "ver sus videos", "its videos"),
                key("x", "borrar la lista (pide confirmar)", "delete the playlist (asks first)"),
                back,
            ],
            Some(Items::Files(_, kind)) => vec![
                match kind {
                    MediaKind::Music => key("Enter", "escuchar desde esta canción", "play from this song"),
                    MediaKind::Video => key("Enter", "ver", "play"),
                },
                key("m", "mover a otra carpeta", "move to another folder"),
                key("r", "renombrar", "rename"),
                key("/", "filtrar la lista", "filter the list"),
                key("o", "ordenar: carpeta / nombre / peso", "sort: folder / name / size"),
                // Several files: mark them first, then `x` sends them all.
                if state.marked_files().is_empty() {
                    key(lang.pick("espacio", "space"), "marcar varios", "mark several")
                } else {
                    key(lang.pick("espacio", "space"), "marcar / desmarcar", "mark / unmark")
                },
                if state.marked_files().is_empty() {
                    key("x", "este a la papelera (pide confirmar)", "this one to the trash (asks first)")
                } else {
                    key("x", "los marcados a la papelera (pide confirmar)", "the marked ones to the trash (asks first)")
                },
                back,
            ],
            Some(Items::Downloads(downloads)) => {
                let selected = state.listing().and_then(|l| downloads.get(l.selected)).map(|d| &d.state);
                let mut keys = Vec::new();
                match selected {
                    Some(DownloadState::Running { .. }) => {
                        keys.push(key("x", "cancelar la descarga", "cancel the download"))
                    }
                    Some(DownloadState::Done(_)) => {
                        keys.push(key("Enter", "ver", "play"));
                        keys.push(key("m", "mover a otra carpeta", "move to another folder"));
                        keys.push(key("r", "renombrar", "rename"));
                        keys.push(key(
                            "x",
                            "quitar de la lista (el archivo queda)",
                            "take off the list (the file stays)",
                        ));
                    }
                    Some(_) => keys.push(key("x", "quitar de la lista", "take off the list")),
                    None => {}
                }
                keys.push(key("/", "filtrar la lista", "filter the list"));
                keys.push(back);
                keys
            }
            Some(Items::About) => vec![back],
            Some(Items::System) => vec![key("Enter", "revisar de nuevo", "check again"), back],
            _ => vec![key("Enter", "cambiar / abrir", "change / open"), back],
        }
    };
    hints.push(all_keys);
    hints
}

/// As many hints as fit in the line, whole; the last one (`Ctrl+y + ?`, every
/// shortcut) is always kept.
fn draw_hints(frame: &mut Frame, hints: &[(String, &str)], area: Rect) {
    let hint = |(key, description): &(String, &str)| {
        [
            Span::styled(format!(" {key} "), Style::new().fg(cyan()).bg(highlight()).bold()),
            Span::styled(format!(" {description}  "), muted()),
        ]
    };
    let width = |spans: &[Span]| spans.iter().map(Span::width).sum::<usize>();
    let Some((last, rest)) = hints.split_last() else { return };
    let last = hint(last);
    let mut spans = vec![Span::raw(" ")];
    let room = (area.width as usize).saturating_sub(width(&last) + 1);
    for h in rest {
        let next = hint(h);
        if width(&spans) + width(&next) > room {
            break;
        }
        spans.extend(next);
    }
    spans.extend(last);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Where the help or the confirmation dialog is drawn, if one is open.
pub fn dialog_area(state: &State, screen: Rect) -> Option<Rect> {
    if state.help {
        Some(help_area(screen, state.lang()))
    } else {
        state.confirm.as_ref().map(|_| centered(screen, 56, 5))
    }
}

/// The help screen's two balanced columns, so everything fits on a laptop screen.
type HelpSection = (&'static str, Vec<(String, &'static str)>);

/// The help's sections in two columns of about the same height.
fn help_columns(lang: Lang) -> (Vec<HelpSection>, Vec<HelpSection>) {
    let mut sections = keymap::help(lang);
    let sizes: Vec<usize> = sections.iter().map(|(_, rows)| help_box_height(rows.len()) as usize).collect();
    let split =
        (1..sections.len()).min_by_key(|&k| sizes[..k].iter().sum::<usize>().max(sizes[k..].iter().sum())).unwrap_or(1);
    let right = sections.split_off(split);
    (sections, right)
}

/// A section's box: its rows between two border lines.
fn help_box_height(rows: usize) -> u16 {
    rows as u16 + 2
}

fn help_column_height(sections: &[HelpSection]) -> u16 {
    sections.iter().map(|(_, rows)| help_box_height(rows.len())).sum()
}

/// The help's size: the taller column inside the outer border.
fn help_area(screen: Rect, lang: Lang) -> Rect {
    let (left, right) = help_columns(lang);
    centered(screen, 118, help_column_height(&left).max(help_column_height(&right)) + 2)
}

fn draw_help(frame: &mut Frame, lang: Lang) {
    let (left, right) = help_columns(lang);
    let area = help_area(frame.area(), lang);
    frame.render_widget(Clear, area);
    let title =
        lang.pick(" ⌨  Atajos de teclado · cualquier tecla cierra ", " ⌨  Keyboard shortcuts · any key closes ");
    let block = panel(title, magenta());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let columns: [Rect; 2] = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).spacing(1).areas(inner);
    for (sections, column) in [left, right].iter().zip(columns) {
        let heights = sections.iter().map(|(_, rows)| Constraint::Length(help_box_height(rows.len())));
        for ((title, rows), cell) in sections.iter().zip(Layout::vertical(heights).split(column).iter()) {
            draw_help_section(frame, title, rows, *cell);
        }
    }
}

/// One section of the help in its own box: keys aligned, then what they do.
fn draw_help_section(frame: &mut Frame, title: &str, rows: &[(String, &'static str)], area: Rect) {
    let width = rows.iter().map(|(keys, _)| Span::raw(keys.as_str()).width()).max().unwrap_or(0) + 2;
    let lines: Vec<Line> = rows
        .iter()
        .map(|(keys, description)| {
            Line::from(vec![
                Span::styled(format!(" {}", pad_cells(keys, width)), Style::new().fg(cyan()).bold()),
                Span::styled(*description, fg()),
            ])
        })
        .collect();
    let block = Block::bordered()
        .border_type(border().border_type())
        .border_style(Style::new().fg(rule()))
        .title(Line::from(format!(" {title} ")).fg(blue()).bold());
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_confirm(frame: &mut Frame, confirm: &Confirm, lang: Lang) {
    let area = centered(frame.area(), 56, 5);
    let question = match confirm {
        Confirm::Unsubscribe { name, .. } => Line::from(vec![
            Span::raw(lang.pick("¿Desuscribirte de ", "Unsubscribe from ")),
            Span::styled(name.as_str(), Style::new().fg(magenta()).bold()),
            Span::raw("?"),
        ]),
        Confirm::Quit(n) => Line::from(vec![
            Span::raw(lang.pick("Hay ", "There are ")),
            Span::styled(n.to_string(), Style::new().fg(magenta()).bold()),
            Span::raw(lang.pick(" descargas en curso. ¿Salir igual?", " downloads running. Quit anyway?")),
        ]),
        Confirm::DeletePlaylist { title, .. } => Line::from(vec![
            Span::raw(lang.pick("¿Borrar de YouTube la lista ", "Delete the playlist ")),
            Span::styled(truncate(title, 24), Style::new().fg(magenta()).bold()),
            Span::raw(lang.pick("?", " from YouTube?")),
        ]),
        Confirm::Trash(paths) => {
            let what = match paths.as_slice() {
                [one] => one.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                many => format!("{} {}", many.len(), lang.pick("archivos", "files")),
            };
            Line::from(vec![
                Span::raw(lang.pick("¿Mandar a la papelera ", "Move to the trash ")),
                Span::styled(truncate(&what, 30), Style::new().fg(magenta()).bold()),
                Span::raw("?"),
            ])
        }
    };
    let lines = vec![
        Line::from(""),
        question,
        Line::from(lang.pick("s = sí · cualquier otra tecla = no", "y = yes · any other key = no")).fg(muted()),
    ];
    let title = lang.pick(" Confirmar ", " Confirm ");
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center).block(panel(title, red())), area);
}

/// "tubetui" in big letters (toilet's smmono12 font, 5 cells a letter).
const LOGO: [&str; 7] = [
    "          ▗▖                    █  ",
    " ▐▌       ▐▌         ▐▌         ▀  ",
    "▐███ ▐▌ ▐▌▐▙█▙  ▟█▙ ▐███ ▐▌ ▐▌ ██  ",
    " ▐▌  ▐▌ ▐▌▐▛ ▜▌▐▙▄▟▌ ▐▌  ▐▌ ▐▌  █  ",
    " ▐▌  ▐▌ ▐▌▐▌ ▐▌▐▛▀▀▘ ▐▌  ▐▌ ▐▌  █  ",
    " ▐▙▄ ▐▙▄█▌▐█▄█▘▝█▄▄▌ ▐▙▄ ▐▙▄█▌▗▄█▄▖",
    "  ▀▀  ▀▀▝▘▝▘▀▘  ▝▀▀   ▀▀  ▀▀▝▘▝▀▀▀▘",
];
/// tubetui's mark: a terminal window with a play triangle and a prompt.
fn mark() -> Vec<Line<'static>> {
    const INNER: usize = 18;
    let edge =
        |left: &'static str, right: &'static str| Line::from(format!("{left}{}{right}", "─".repeat(INNER))).fg(blue());
    let side = |mut spans: Vec<Span<'static>>| {
        let used: usize = spans.iter().map(Span::width).sum();
        spans.push(Span::raw(" ".repeat(INNER.saturating_sub(used))));
        spans.insert(0, Span::styled("│", blue()));
        spans.push(Span::styled("│", blue()));
        Line::from(spans)
    };
    let dots = side(vec![
        Span::raw(" "),
        Span::styled("●", red()),
        Span::raw(" "),
        Span::styled("●", yellow()),
        Span::raw(" "),
        Span::styled("●", green()),
    ]);
    // The triangle in half blocks, three rows: top / bottom half widths 2-4, 6-6, 4-2.
    let triangle = [("██▄▄  ", cyan()), ("██████", blue()), ("██▀▀  ", magenta())];
    let mut lines = vec![edge("╭", "╮"), dots, edge("├", "┤")];
    for (row, (shape, color)) in triangle.into_iter().enumerate() {
        let mut spans = vec![Span::raw("   "), Span::styled(shape, color)];
        if row == 1 {
            spans.extend([
                Span::raw("  "),
                Span::styled("❯", Style::new().fg(magenta()).bold()),
                Span::styled("_", fg()),
            ]);
        }
        lines.push(side(spans));
    }
    lines.push(edge("╰", "╯"));
    lines
}

/// The logo for a moment when the app opens.
fn draw_splash(frame: &mut Frame, lang: Lang) {
    let area = frame.area();
    let tagline = lang.pick("YouTube en tu terminal", "YouTube in your terminal");
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let skip = lang.pick("cualquier tecla para seguir", "any key to go on");
    let text_width = LOGO.iter().map(|l| Span::raw(*l).width()).max().unwrap_or(0);
    let width = 20 + 2 + text_width as u16;
    if area.width < width || area.height < 12 {
        let line = Line::from(vec![
            Span::styled(" ❯_ ", Style::new().fg(Color::White).bg(magenta())),
            Span::raw(" tubetui").bold(),
        ]);
        frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), centered(area, area.width, 1));
        return;
    }
    let mut lines: Vec<Line> = mark()
        .into_iter()
        .zip(LOGO)
        .map(|(button, text)| {
            // "tube" in the theme's text color, "tui" in the accent.
            let cut = text.char_indices().nth(20).map_or(text.len(), |(i, _)| i);
            let mut spans = button.spans.into_iter().map(|s| s.patch_style(button.style)).collect::<Vec<_>>();
            spans.push(Span::raw("  "));
            spans.push(Span::styled(&text[..cut], Style::new().fg(fg()).bold()));
            spans.push(Span::styled(&text[cut..], Style::new().fg(magenta()).bold()));
            Line::from(spans)
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(
        Line::from(vec![
            Span::styled(tagline, Style::new().fg(blue())),
            Span::styled(format!("  {version}"), Style::new().fg(muted())),
        ])
        .centered(),
    );
    lines.push(Line::from(""));
    lines.push(Line::from(skip).fg(muted()).centered());
    let height = lines.len() as u16;
    frame.render_widget(Paragraph::new(lines), centered(area, width, height));
}

/// The right-click menu: each option with its key, the chosen one lit.
fn draw_context_menu(frame: &mut Frame, menu: &ContextMenu, lang: Lang) {
    let area = context_area(menu, frame.area());
    let block = panel(lang.pick(" Opciones ", " Options "), magenta());
    let width = usize::from(block.inner(area).width);
    let lines: Vec<Line> = menu
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let gap = width.saturating_sub(Span::raw(item.label).width() + Span::raw(item.key).width() + 2);
            let line = Line::from(vec![
                Span::raw(format!(" {}{}", item.label, " ".repeat(gap))),
                Span::styled(format!("{} ", item.key), Style::new().fg(muted())),
            ]);
            if i == menu.selected { line.style(Style::new().bg(highlight()).fg(blue()).bold()) } else { line }
        })
        .collect();
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// What the system has and lacks, with the command that installs it.
fn draw_system(frame: &mut Frame, state: &State, area: Rect) {
    let lang = state.lang();
    let title = format!(" {} ", lang.pick("Revisión del sistema", "System check"));
    let block = panel(title, blue());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some((manager, checks)) = &state.system else {
        let wait = lang.pick("Revisando…", "Checking…");
        frame.render_widget(Paragraph::new(wait).fg(yellow()).alignment(Alignment::Center), inner);
        return;
    };
    let mut lines = vec![Line::from("")];
    let width = checks.iter().map(|c| c.tool.program().len()).max().unwrap_or(0) + 2;
    for check in checks {
        let (mark, color, note) = match (&check.found, check.outdated) {
            (Some(_), false) => ("✓", green(), String::new()),
            (Some((keyring, _)), true) if check.tool == Tool::Keyring => {
                let missing = if keyring.starts_with("KWALLET") { "kwallet-query" } else { "secretstorage" };
                (
                    "⚠",
                    yellow(),
                    format!(
                        "{}: {missing}",
                        lang.pick("yt-dlp no lo puede abrir, falta", "yt-dlp can't open it, missing")
                    ),
                )
            }
            (Some(_), true) => {
                ("⚠", yellow(), lang.pick("viejo: YouTube cambia seguido", "too old: YouTube changes often").into())
            }
            (None, _) if check.tool.required() => {
                ("✗", red(), lang.pick("falta · necesario", "missing · needed").into())
            }
            (None, _) => ("·", muted(), lang.pick("no está · opcional", "not installed · optional").into()),
        };
        let version = check.found.as_ref().and_then(|(_, v)| v.clone()).unwrap_or_default();
        let mut row = vec![
            Span::styled(format!("  {mark} "), Style::new().fg(color).bold()),
            Span::styled(pad(check.tool.program(), width), Style::new().fg(fg()).bold()),
            Span::styled(pad(&version, 16), muted()),
            Span::styled(check.tool.purpose().get(lang), fg()),
        ];
        if !note.is_empty() {
            row.push(Span::styled(format!("  ({note})"), color));
        }
        lines.push(Line::from(row));
        if let Some(fix) = &check.fix {
            lines.push(Line::from(vec![Span::raw(" ".repeat(width + 5)), Span::styled(fix.as_str(), cyan())]));
        } else if check.found.is_none() {
            let how =
                lang.pick("instalalo con el gestor de paquetes de tu sistema", "install it with your package manager");
            lines.push(Line::from(vec![Span::raw(" ".repeat(width + 5)), Span::styled(how, muted())]));
        }
    }
    lines.push(Line::from(""));
    let footer = match manager {
        Manager::Unknown => lang.pick("Gestor de paquetes no reconocido", "Package manager not recognized").to_owned(),
        manager => format!("{}: {}", lang.pick("Comandos para", "Commands for"), manager.name()),
    };
    lines.push(
        Line::from(format!(
            "  {footer} · {}",
            lang.pick("Shift + arrastrar copia · Enter revisa de nuevo", "Shift + drag copies · Enter checks again",)
        ))
        .fg(muted()),
    );
    frame.render_widget(Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }), inner);
}

/// Version, links and the donation addresses from `donate.toml`.
fn draw_about(frame: &mut Frame, state: &State, icons: &Icons, area: Rect) {
    let lang = state.lang();
    let label = |text: &'static str| Span::styled(format!("{text:>12}  "), muted());
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw(format!("{}  ", icons.logo)),
            Span::styled("tube", Style::new().fg(red()).bold()),
            Span::styled("tui", Style::new().fg(fg()).bold()),
            Span::styled(format!("  v{}", env!("CARGO_PKG_VERSION")), muted()),
        ]),
        Line::from(lang.pick(
            "YouTube y tus videos y música, en la terminal",
            "YouTube and your own videos and music, in the terminal",
        ))
        .fg(fg()),
        Line::from(""),
        Line::from(vec![label("GitHub"), Span::styled(env!("CARGO_PKG_REPOSITORY"), cyan())]),
        Line::from(vec![label("crates.io"), Span::styled("https://crates.io/crates/tubetui", cyan())]),
        Line::from(vec![label(lang.pick("Licencia", "License")), Span::styled("AGPL-3.0", fg())]),
        Line::from(vec![
            label(lang.pick("Errores", "Bugs")),
            Span::styled(concat!(env!("CARGO_PKG_REPOSITORY"), "/issues"), cyan()),
        ]),
    ];
    if let Some(update) = &state.update {
        lines.push(Line::from(""));
        lines.push(
            Line::from(format!(
                "⬆ {} {}",
                lang.pick("Hay una versión nueva:", "A new version is out:"),
                update.version
            ))
            .fg(green())
            .bold(),
        );
        let (how, value) = match &update.how {
            Install::Command(command) => (lang.pick("Actualizá con:", "Update with:"), command),
            Install::Link(url) => (lang.pick("Bajá el instalador:", "Get the installer:"), url),
        };
        lines.push(Line::from(how).fg(fg()));
        lines.push(Line::from(value.as_str()).fg(cyan()));
        lines.push(
            Line::from(lang.pick("(Shift + arrastrar para seleccionar y copiar)", "(Shift + drag to select and copy)"))
                .fg(muted()),
        );
    }
    let wallets = donations();
    if !wallets.is_empty() {
        lines.push(Line::from(""));
        lines.push(
            Line::from(lang.pick(
                "☕ ¿Te sirve? Podés apoyar el proyecto con una donación:",
                "☕ Find it useful? You can support the project with a donation:",
            ))
            .fg(yellow())
            .bold(),
        );
        for wallet in wallets {
            lines.push(Line::from(vec![
                Span::styled(format!("{:>12}  ", wallet.network), magenta()),
                Span::styled(wallet.address, Style::new().fg(green()).bold()),
            ]));
        }
    }
    let title = format!(" {} {} ", icons.section(Section::AcercaDe), Section::AcercaDe.label(lang));
    // Wrapped: the update command can be longer than the panel.
    let about = Paragraph::new(lines).alignment(Alignment::Center).wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(about.block(panel(title, purple())), area);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [row] = Layout::vertical([Constraint::Length(height.min(area.height))]).flex(Flex::Center).areas(area);
    let [cell] = Layout::horizontal([Constraint::Length(width.min(area.width))]).flex(Flex::Center).areas(row);
    cell
}

fn truncate(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_owned(),
    }
}

/// Truncates or pads to exactly `width` characters, for aligned columns.
fn pad(text: &str, width: usize) -> String {
    let text: String = text.chars().take(width).collect();
    format!("{text:<width$}")
}

/// Pads to `width` terminal cells; emoji take two cells, so `{:<w}` is not enough.
fn pad_cells(text: &str, width: usize) -> String {
    let used = Span::raw(text).width();
    format!("{text}{}", " ".repeat(width.saturating_sub(used)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Action, Effect};
    use crate::config::Config;
    use crate::domain::{channel, video};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    const VIEW: View =
        View { embedded: true, now: 1_791_590_400, loading: None, opening: None, thumbnails: true, playing: None };

    fn render_with(state: &State, view: &View) -> String {
        render_sized(state, view, 130)
    }

    fn render_sized(state: &State, view: &View, width: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, 40)).unwrap();
        terminal.draw(|f| draw(f, state, view)).unwrap();
        let buffer = terminal.backend().buffer();
        buffer
            .content
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render(state: &State) -> String {
        render_with(state, &VIEW)
    }

    /// Plain-Unicode icons keep every symbol one cell wide in the test buffer.
    fn state() -> State {
        let mut config = Config::default();
        config.ui.icons = IconSet::Unicode;
        State::new(config, Vec::new())
    }

    /// Opens "Suscripciones" from the menu and fills it with `items`.
    fn with_items(items: Items) -> State {
        let mut s = state();
        s.menu = Section::ALL.iter().position(|x| *x == Section::Suscripciones).unwrap();
        let id = match s.update(Action::Enter).as_slice() {
            [Effect::Load { id, .. }] => *id,
            other => panic!("{other:?}"),
        };
        s.update(Action::Loaded(id, Ok(items)));
        s
    }

    fn with_tab() -> State {
        let mut s = with_items(Items::Videos(vec![video("a"), video("b")]));
        s.update(Action::Enter);
        s
    }

    fn open_settings() -> State {
        let mut s = state();
        s.menu = Section::ALL.iter().position(|x| *x == Section::Configuracion).unwrap();
        s.update(Action::Enter);
        s
    }

    fn hint_keys(state: &State) -> Vec<String> {
        hints(state).into_iter().map(|(k, _)| k).collect()
    }

    #[test]
    fn layout_reserves_status_and_hint_lines() {
        let area = Rect::new(0, 0, 100, 30);
        let with = areas(area, true);
        assert_eq!(with.sidebar.unwrap().width, SIDEBAR_WIDTH);
        assert_eq!(with.content.x, SIDEBAR_WIDTH);
        let without = areas(area, false);
        assert_eq!(without.sidebar, None);
        assert_eq!(without.content, Rect::new(0, 1, 100, 27));
        assert_eq!(without.status, Rect::new(0, 28, 100, 1));
        assert_eq!(without.hints, Rect::new(0, 29, 100, 1));
    }

    #[test]
    fn video_area_only_for_video_tabs_without_dialogs() {
        let area = Rect::new(0, 0, 100, 30);
        let mut s = with_tab();
        assert_eq!(video_area(&s, area), Some(Block::bordered().inner(areas(area, true).content)));
        s.update(Action::ToggleHelp);
        assert_eq!(video_area(&s, area), None);
        s.update(Action::ToggleHelp);
        s.update(Action::GoTab(0));
        assert_eq!(video_area(&s, area), None);
    }

    #[test]
    fn menu_shows_every_section_with_icons_and_later_count() {
        let mut s = state();
        s.later = vec![video("x"), video("y")];
        let screen = render(&s);
        for (section, icon) in Section::ALL.iter().zip(UNICODE.sections) {
            assert!(screen.contains(&format!("{icon} {}", section.label(Lang::Es))), "{section:?}\n{screen}");
        }
        assert!(screen.contains("Ver después (2)"));
        assert!(screen.contains("Elegí una sección"));
    }

    #[test]
    fn icon_set_follows_the_config() {
        let mut s = state();
        s.config.ui.icons = IconSet::Nerd;
        assert!(render(&s).contains(&format!("{} Inicio", NERD.sections[0])));
    }

    #[test]
    fn video_rows_show_date_channel_title_duration_and_later_mark() {
        let dated = Video { published: Some(VIEW.now - 3 * 86_400), ..video("a") };
        let live = Video { live: Live::Now, ..video("b") };
        let mut s = with_items(Items::Videos(vec![dated, live]));
        s.later = vec![video("a")];
        let screen = render(&s);
        assert!(screen.contains("07/10/26 3d"), "{screen}");
        assert!(screen.contains("● en vivo"));
        assert!(screen.contains("chan a"));
        assert!(screen.contains("◷ title a"));
        assert!(screen.contains("1:00"));
        assert!(screen.contains("Suscripciones · 2"));
    }

    #[test]
    fn channel_rows_mark_my_channels() {
        let followed = Channel {
            followers: Some("358 mil suscriptores".into()),
            handle: Some("@x".into()),
            description: Some("Música\ny charlas".into()),
            ..channel("UCx")
        };
        let mut s = with_items(Items::Channels(vec![followed, channel("UCy")]));
        s.subs.insert("UCx".into());
        let name = pad_cells("name UCx", CHANNEL_NAME_WIDTH);
        let row = format!("✓ {name}{}@x", pad_cells("358 mil suscriptores", FOLLOWERS_WIDTH));
        let wide = render(&s);
        let line = wide.lines().find(|l| l.contains("✓ name UCx")).unwrap();
        assert!(line.contains(&row), "{line}");
        assert!(!line.contains("Música"), "the panel shows it: {line}");
        let narrow = render_sized(&s, &VIEW, 110);
        assert!(narrow.contains(&format!("{row}  Música y charlas")), "{narrow}");
        assert!(wide.contains("   name UCy"));
    }

    #[test]
    fn profile_rows_mark_current_session() {
        let mut s = with_items(Items::Profiles(vec!["brave:Default".into(), String::new()]));
        s.config.session.browser = "brave:Default".into();
        let screen = render(&s);
        assert!(screen.contains("● ☺ brave · Default"), "{screen}");
        assert!(!screen.contains("último uso"), "unknown: nothing");
        s.clock = 1_791_590_400 + 3_600;
        s.browsers_used.insert("brave:Default".into(), 1_791_590_400 - 40 * 86_400);
        assert!(render(&s).contains("brave · Default  · último uso 31/08/26 (1m)"), "{}", render(&s));
        assert!(screen.contains("☺ Sin sesión"));
    }

    #[test]
    fn settings_screen_shows_labels_and_values() {
        let screen = render(&open_settings());
        for setting in Setting::ALL {
            assert!(screen.contains(setting.label(Lang::Es)), "{setting:?}\n{screen}");
        }
        assert!(screen.contains("720p"));
        assert!(screen.contains("~/Videos"));
        assert!(screen.contains("original + subtítulos"));
    }

    #[test]
    fn search_box_shows_scope_and_query() {
        let mut s = state();
        s.update(Action::FocusSearch);
        for c in "lofi".chars() {
            s.update(Action::Input(c));
        }
        let screen = render(&s);
        assert!(screen.contains("Buscar videos en todo YouTube"), "{screen}");
        assert!(screen.contains("lofi"));
        s.update(Action::CycleScope);
        assert!(render(&s).contains("Buscar videos en mis canales"));
    }

    #[test]
    fn cookies_prompt_explains_how_to_export() {
        let mut s = open_settings();
        s.stack[0].selected = Setting::ALL.iter().position(|x| *x == Setting::ImportCookies).unwrap();
        s.update(Action::Enter);
        let screen = render(&s);
        assert!(screen.contains("Ruta del cookies.txt"));
        assert!(screen.contains("scp cookies.txt"));
    }

    #[test]
    fn tabs_appear_in_bar_and_sidebar() {
        let screen = render(&with_tab());
        assert!(screen.contains("PESTAÑAS"));
        assert_eq!(screen.matches("1 ▶ title a").count(), 2, "{screen}");
    }

    #[test]
    fn window_mode_tab_explains_where_video_is() {
        assert!(render_with(&with_tab(), &View { embedded: false, ..VIEW }).contains("ventana de mpv"));
        assert!(!render(&with_tab()).contains("ventana de mpv"));
    }

    #[test]
    fn the_logo_shows_alone_when_the_app_opens() {
        let mut s = state();
        s.splash = true;
        let screen = render(&s);
        assert!(screen.contains("YouTube en tu terminal") && screen.contains("▐███"), "{screen}");
        assert!(!screen.contains("Inicio"), "only the logo");
        assert_eq!(spot(&s, None, Rect::new(0, 0, 120, 40), 5, 5), None);
        s.update(Action::EndSplash);
        assert!(render(&s).contains("Inicio"));
    }

    #[test]
    fn help_shows_every_section_with_explanations() {
        let mut s = state();
        s.update(Action::ToggleHelp);
        let screen = render(&s);
        for (title, rows) in keymap::help(Lang::Es) {
            assert!(screen.contains(title), "{title}\n{screen}");
            for (_, description) in rows {
                assert!(screen.contains(description), "{description}\n{screen}");
            }
        }
    }

    #[test]
    fn hints_follow_the_context() {
        let mut s = state();
        assert_eq!(hint_keys(&s), ["↑ ↓", "Enter", "Tab", "/", "Ctrl+y + ?"]);
        s.update(Action::Prefix);
        assert!(hint_keys(&s).contains(&"b".to_string()) && hint_keys(&s).contains(&"0-9".to_string()));
        let tab = with_tab();
        assert!(hint_keys(&tab).contains(&"espacio".to_string()) && hint_keys(&tab).contains(&"[ ]".to_string()));
        let list = with_items(Items::Videos(vec![video("a")]));
        assert_eq!(
            hint_keys(&list),
            ["Enter", "v", "l", "a", "s", "g", "d", "c", "/", "Esc", "Ctrl+y + ?"],
            "Inicio: a feed"
        );
        let mut search = state();
        search.update(Action::FocusSearch);
        assert!(hint_keys(&search).contains(&"Tab".to_string()));
    }

    #[test]
    fn hints_explain_two_step_keys() {
        let hint = |s: &State, k: &str| hints(s).into_iter().find(|(key, _)| key == k).map(|(_, d)| d);
        let mut s = state();
        s.update(Action::Prefix);
        assert_eq!(hints(&s)[0].0, "Ctrl+y +", "says the key comes next");
        let mut list = with_items(Items::Videos(vec![video("a")]));
        assert_eq!(hint(&list, "s"), Some("suscribirse"));
        list.subs.insert("UCa".into());
        assert_eq!(hint(&list, "s"), Some("desuscribirse (pide confirmar)"));
        let file =
            |n: &str| LocalFile { path: format!("/m/{n}.mp3").into(), name: n.into(), folder: "D".into(), size: 0 };
        let mut files = with_items(Items::Files(vec![file("a"), file("b")], MediaKind::Music));
        assert_eq!(hint(&files, "espacio"), Some("marcar varios"));
        assert_eq!(hint(&files, "x"), Some("este a la papelera (pide confirmar)"));
        files.update(Action::ToggleMark);
        assert_eq!(hint(&files, "x"), Some("los marcados a la papelera (pide confirmar)"));
        files.update(Action::Trash);
        assert_eq!(hint(&files, "s"), Some("sí, a la papelera"));
    }

    #[test]
    fn release_names_read_as_words_and_keep_their_end() {
        assert_eq!(readable_name("Ahsoka.1x01.Maestro.y.aprendiz", 80), "Ahsoka 1x01 Maestro y aprendiz");
        assert_eq!(readable_name("A video with spaces.v2", 80), "A video with spaces.v2", "only release names");
        let cut = readable_name("Ahsoka.1x01.Maestro.y.aprendiz.WEB-DL.1080p.x264", 30);
        assert_eq!(cut.chars().count(), 30, "{cut}");
        assert!(cut.starts_with("Ahsoka 1x01") && cut.ends_with("x264") && cut.contains('…'), "{cut}");
    }

    #[test]
    fn sizes_go_from_green_to_red() {
        let colors: Vec<Color> = [0, 100 << 20, 1 << 30, 3 << 30, 8 << 30].map(size_color).into();
        assert_eq!(colors, [muted(), green(), yellow(), orange(), red()]);
    }

    #[test]
    fn hint_bar_shows_keys_with_descriptions() {
        let screen = render(&with_items(Items::Videos(vec![video("a")])));
        assert!(screen.contains("l  ver después"), "{screen}");
        assert!(screen.contains("Ctrl+y + ?  todos los atajos"));
    }

    #[test]
    fn confirm_dialog_names_the_channel() {
        let mut s = with_items(Items::Videos(vec![video("a")]));
        s.subs.insert("UCa".into());
        s.update(Action::ToggleSubscription);
        let screen = render(&s);
        assert!(screen.contains("¿Desuscribirte de chan a?"));
        assert!(screen.contains("sí, desuscribirme"));
    }

    #[test]
    fn status_bar_shows_session_and_language() {
        let mut s = state();
        s.config.session.browser = "brave:Default".into();
        let screen = render(&s);
        assert!(screen.contains("brave · Default"), "{screen}");
        assert!(screen.contains("original + subtítulos"));
    }

    #[test]
    fn menu_shows_group_headers() {
        let screen = render(&state());
        assert!(screen.contains("YOUTUBE"));
        assert!(screen.contains("EN TU EQUIPO"));
        assert!(screen.contains("▷ Mis videos") && screen.contains("♫ Mi música"));
    }

    #[test]
    fn music_tab_shows_the_song_and_takes_no_video_area() {
        let file =
            |n: &str| LocalFile { path: format!("/m/{n}.mp3").into(), name: n.into(), folder: "Disco".into(), size: 0 };
        let mut s = with_items(Items::Files(vec![file("Tema uno"), file("Tema dos")], MediaKind::Music));
        let screen = render(&s);
        assert!(screen.contains("♫ Disco"), "{screen}");
        assert!(screen.contains("escuchar desde esta canción"));
        s.update(Action::Enter);
        let screen = render(&s);
        assert!(screen.contains("2 canciones en la cola"), "{screen}");
        assert_eq!(video_area(&s, Rect::new(0, 0, 100, 30)), None);
    }

    #[test]
    fn fullscreen_gives_the_whole_terminal_to_the_video() {
        let mut s = with_tab();
        s.update(Action::ToggleFullscreen);
        let area = Rect::new(0, 0, 100, 30);
        assert_eq!(video_area(&s, area), Some(area));
        assert!(!render(&s).contains("tubetui"), "no sidebar or bars");
    }

    #[test]
    fn loading_shows_spinner_seconds_and_a_moving_bar() {
        let mut s = state();
        s.menu = Section::ALL.iter().position(|x| *x == Section::Suscripciones).unwrap();
        s.update(Action::Enter); // Suscripciones starts loading
        let view = View { loading: Some(Duration::from_millis(3_250)), ..VIEW };
        let screen = render_with(&s, &view);
        assert!(screen.contains("Cargando Suscripciones… 3 s"), "{screen}");
        assert!(screen.contains("█"));
        let later = View { loading: Some(Duration::from_millis(3_550)), ..VIEW };
        let (now, then) = (view.loading.unwrap(), later.loading.unwrap());
        assert_ne!(bouncing_bar(30, now), bouncing_bar(30, then), "the bar moves");
        assert_eq!(bouncing_bar(30, now).chars().count(), 30);
    }

    #[test]
    fn video_tab_shows_a_loading_bar_until_the_video_plays() {
        let opening = View { opening: Some(Duration::from_millis(2_400)), ..VIEW };
        let screen = render_with(&with_tab(), &opening);
        assert!(screen.contains("Cargando el video… 2 s"), "{screen}");
        assert!(screen.contains("█"));
        assert!(!render(&with_tab()).contains("Cargando"), "playing: nothing under the video");
        let window = View { embedded: false, ..VIEW };
        assert!(render_with(&with_tab(), &window).contains("ventana de mpv"));
    }

    #[test]
    fn english_interface() {
        let mut s = with_tab();
        s.update(Action::ToggleUiLang);
        s.update(Action::GoTab(0));
        let screen = render(&s);
        for text in ["Home", "Watch later", "ON THIS COMPUTER", "Settings", "About", "TABS", "every shortcut"] {
            assert!(screen.contains(text), "{text} missing\n{screen}");
        }
        assert!(!screen.contains("Ver después"), "{screen}");
        s.update(Action::ToggleHelp);
        assert!(render(&s).contains("Keyboard shortcuts"));
    }

    #[test]
    fn about_shows_version_and_links() {
        let mut s = state();
        s.menu = Section::ALL.iter().position(|x| *x == Section::AcercaDe).unwrap();
        s.update(Action::Enter);
        let screen = render(&s);
        assert!(screen.contains(env!("CARGO_PKG_VERSION")), "{screen}");
        assert!(screen.contains("github.com/polblancoo/tubetui"), "{screen}");
    }

    #[test]
    fn hints_fit_whole_and_keep_the_help_key() {
        let mut terminal = Terminal::new(TestBackend::new(60, 1)).unwrap();
        let hints = hints(&with_tab());
        terminal.draw(|f| draw_hints(f, &hints, f.area())).unwrap();
        let line: String = terminal.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(line.contains("Ctrl+y + ?  todos los atajos"), "{line}");
        assert!(line.contains("espacio  pausa"), "{line}");
        assert!(!line.contains("silenciar"), "does not fit, left out whole: {line}");
    }

    #[test]
    fn wide_video_lists_show_a_preview_of_the_selection() {
        let mut s = with_items(Items::Videos(vec![video("a"), video("b")]));
        s.update(Action::Down);
        let screen = render(&s); // 130 columns: room for the panel
        assert!(screen.contains("Vista previa"), "{screen}");
        let screen_area = Rect::new(0, 0, 130, 40);
        let (id, cells) = thumbnail_area(&s, screen_area).unwrap();
        assert_eq!(id, "b", "the selected video");
        assert_eq!(cells.width, PREVIEW_WIDTH - 2);
        assert_eq!(cells.height, (PREVIEW_WIDTH - 2) * 9 / 32);
        s.update(Action::ToggleHelp);
        assert_eq!(thumbnail_area(&s, screen_area), None, "kitty would draw it over the help");
        s.update(Action::ToggleHelp);
        assert_eq!(thumbnail_area(&s, Rect::new(0, 0, 100, 40)), None, "too narrow");
        assert!(!render_with(&s, &View { thumbnails: false, ..VIEW }).contains("⏳"));
    }

    #[test]
    fn wide_channel_lists_show_the_whole_description() {
        let described = Channel { description: Some("Primera línea\nSegunda línea".into()), ..channel("UCx") };
        let s = with_items(Items::Channels(vec![described, channel("UCy")]));
        let screen = render(&s);
        assert!(screen.contains(" Canal "), "{screen}");
        assert!(screen.contains("│Primera línea"), "{screen}");
        assert!(screen.contains("│Segunda línea"), "kept on its own line: {screen}");
        assert_eq!(thumbnail_area(&s, Rect::new(0, 0, 130, 40)), None, "channels have no thumbnail");
    }

    #[test]
    fn video_tab_shows_volume_quality_and_codec() {
        let mut s = with_items(Items::Videos(vec![video("a")]));
        s.update(Action::Enter);
        let info =
            Info { volume: Some(85.0), muted: false, height: Some(480), codec: Some("h264".into()), ..Info::default() };
        let screen = render_with(&s, &View { playing: Some(info.clone()), ..VIEW });
        assert!(screen.contains("♪ 85 %  480p  H.264"), "{screen}");
        let muted = Info { muted: true, codec: Some("vp9".into()), ..info };
        assert_eq!(playing_label(&muted, icons(IconSet::Unicode)), " ✕ 85 %  480p  VP9 ");
        assert_eq!(playing_label(&Info::default(), icons(IconSet::Unicode)), "");
    }

    #[test]
    fn video_tab_shows_a_progress_bar() {
        let mut s = with_items(Items::Videos(vec![video("a")]));
        s.update(Action::Enter);
        let info = Info { position: Some(30.0), duration: Some(120.0), buffered: Some(60.0), ..Info::default() };
        let screen = render_with(&s, &View { playing: Some(info.clone()), ..VIEW });
        let bottom = screen.lines().find(|l| l.contains("2:00")).unwrap_or_default();
        assert!(bottom.contains("▶ 0:30 ━"), "{screen}");
        let content = areas(Rect::new(0, 0, 130, 40), s.sidebar).content;
        let (bar, duration) = progress_bar(&info, content).unwrap();
        assert_eq!(duration, 120.0);
        let track = bottom.split("0:30 ").nth(1).and_then(|t| t.split(" 2:00").next()).unwrap();
        assert_eq!(track.chars().count(), bar.width as usize, "{bottom}");
        let thick = track.chars().filter(|c| *c == '━').count();
        assert_eq!(thick, (f64::from(bar.width) / 2.0).round() as usize, "a quarter played, another downloaded");
        let line = progress_line(&info, &[], content.width).unwrap();
        // Neighbouring cells of the same colour share a span.
        let played: Vec<_> = line.spans[1..].iter().take_while(|s| s.style.fg != Some(muted())).collect();
        let cells: usize = played.iter().map(|s| s.content.chars().count()).sum();
        assert_eq!(cells, (f64::from(bar.width) / 4.0).round() as usize);
        assert_eq!(line.spans[1].style.fg, Some(gradient(0, usize::from(bar.width))));
        assert_eq!(line.spans[1 + played.len()].style.fg, Some(muted()));
        assert_eq!(bar.y, content.bottom() - 1);
        let paused = progress_line(&Info { paused: true, ..info.clone() }, &[], 60).unwrap();
        assert!(paused.to_string().starts_with(" ‖ 0:30 "));
        assert!(progress_line(&Info { duration: None, ..info }, &[], 60).is_none(), "live: no total");
    }

    #[test]
    fn progress_bar_marks_sponsors_and_chapters() {
        let info = Info {
            position: Some(0.0),
            duration: Some(100.0),
            chapters: vec![(0.0, "Intro".into()), (50.0, "Second half".into())],
            chapter: Some(1),
            ..Info::default()
        };
        let sponsor = Segment { start: 10_000, end: 20_000, category: "sponsor".into() };
        let line = progress_line(&info, std::slice::from_ref(&sponsor), 40).unwrap();
        let (bar, _) = progress_bar(&info, Rect::new(0, 0, 40, 1)).unwrap();
        let track: String = line.spans[1..line.spans.len() - 1].iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(track.chars().count(), bar.width as usize);
        let green: usize =
            line.spans.iter().filter(|s| s.style.fg == Some(green())).map(|s| s.content.chars().count()).sum();
        assert!(green >= 2, "{line:?}");
        assert_eq!(track.matches('┃').count(), 1, "only the chapter that starts after 0: {track}");
        assert!(playing_label(&info, icons(IconSet::Unicode)).contains("§ Second half"));
    }

    /// The first cell whose row contains `text`, at the start of `text`.
    fn find(screen: &str, text: &str) -> (u16, u16) {
        let (y, line) = screen.lines().enumerate().find(|(_, l)| l.contains(text)).unwrap();
        let x = line[..line.find(text).unwrap()].chars().count();
        (x as u16, y as u16)
    }

    #[test]
    fn spots_under_the_pointer() {
        let screen_area = Rect::new(0, 0, 130, 40);
        let videos: Vec<Video> = (0..60).map(|i| video(&i.to_string())).collect();
        let mut s = with_items(Items::Videos(videos));
        let screen = render(&s);
        let spot_at = |s: &State, (x, y): (u16, u16)| spot(s, None, screen_area, x, y);
        let musica = Section::ALL.iter().position(|x| *x == Section::MiMusica).unwrap();
        assert_eq!(spot_at(&s, find(&screen, Section::MiMusica.label(Lang::Es))), Some(Spot::Section(musica)));
        assert_eq!(spot_at(&s, find(&screen, "title 2  1:00")), Some(Spot::Row(2)));
        assert_eq!(spot_at(&s, (0, 39)), None, "the key hints");
        // Scrolled: the selection is the last visible row.
        for _ in 0..50 {
            s.update(Action::Down);
        }
        let screen = render(&s);
        assert_eq!(spot_at(&s, find(&screen, "title 50  1:00")), Some(Spot::Row(50)));
        assert_eq!(spot_at(&s, find(&screen, "title 30  1:00")), Some(Spot::Row(30)));
        s.update(Action::Enter); // plays video 50 in tab 1
        let screen = render(&s);
        assert_eq!(spot_at(&s, find(&screen, "1 ▶ title 50")), Some(Spot::Tab(1)));
        assert_eq!(spot_at(&s, find(&screen, "Inicio ")), Some(Spot::Tab(0)));
        assert_eq!(spot_at(&s, (80, 20)), Some(Spot::Video));
        let info = Info { position: Some(0.0), duration: Some(100.0), ..Info::default() };
        let content = areas(screen_area, s.sidebar).content;
        let (bar, _) = progress_bar(&info, content).unwrap();
        assert_eq!(spot(&s, Some(&info), screen_area, bar.x, bar.y), Some(Spot::Progress(0)));
        let middle = spot(&s, Some(&info), screen_area, bar.x + bar.width / 2, bar.y);
        assert!(matches!(middle, Some(Spot::Progress(49..=51))), "{middle:?}");
        s.update(Action::ToggleHelp);
        assert_eq!(spot_at(&s, (80, 20)), None, "the help covers everything");
    }

    #[test]
    fn video_tab_in_window_mode_shows_details_and_related() {
        let mut s = with_items(Items::Videos(vec![video("a")]));
        s.video_window = true;
        s.update(Action::Enter);
        let tab = s.active_tab().unwrap().id;
        let screen = render_with(&s, &View { embedded: false, ..VIEW });
        assert!(screen.contains("Buscando videos relacionados"), "{screen}");
        let watch = Watch {
            views: Some(5771),
            likes: Some(739),
            published: None,
            subscribers: Some("559K".into()),
            related: vec![video("r1"), video("r2")],
            rating: Some(Rating::Like),
            comments_token: Some("T".into()),
        };
        s.update(Action::Watched(tab, Ok(watch)));
        assert!(render_with(&s, &View { embedded: false, ..VIEW }).contains("Buscando comentarios"));
        let comment = crate::youtube::Comment {
            author: "@ana".into(),
            text: "Qué temazo".into(),
            likes: Some("12".into()),
            published: Some("hace 1 año".into()),
        };
        s.update(Action::CommentsLoaded(tab, Ok(vec![comment])));
        let view = View { embedded: false, ..VIEW };
        let screen = render_with(&s, &view);
        for text in
            ["title a", "chan a · 559K suscriptores", "5 k vistas", "Te gusta", "A continuación · 2 · Tab", "title r2"]
                .into_iter()
                .chain(["Comentarios 1/1", "@ana  👍", "12  hace 1 año", "Qué temazo"])
        {
            assert!(screen.contains(text), "{text}\n{screen}");
        }
        assert!(!screen.contains("ventana de mpv"));
        let screen_area = Rect::new(0, 0, 130, 40);
        let (id, cells) = thumbnail_area(&s, screen_area).unwrap();
        assert_eq!(id, "a");
        assert_eq!((cells.width, cells.height), (34, 34 * 9 / 32), "a third of the tab, at most 36 columns");
        let (x, y) = find(&screen, "title r2  1:00");
        assert_eq!(spot(&s, None, screen_area, x, y), Some(Spot::Row(1)));
        assert_eq!(hints(&s)[0].0, "Tab");
        s.update(Action::ToggleFocus);
        let screen = render_with(&s, &view);
        assert!(screen.contains("A continuación · 2 "), "{screen}");
        assert_eq!(hints(&s)[0].1, "ver");
        s.video_window = false;
        assert_eq!(thumbnail_area(&s, screen_area), None, "embedded: the video covers the tab");
    }

    #[test]
    fn a_new_version_is_announced_with_how_to_get_it() {
        let mut s = state();
        let update =
            crate::update::Update { version: "9.9.9".into(), how: Install::Command("cargo install tubetui".into()) };
        s.update(Action::UpdateFound(update));
        assert!(s.status.contains("⬆ tubetui 9.9.9 disponible"), "{}", s.status);
        let screen = render(&s);
        assert!(screen.contains("Acerca de ⬆"), "{screen}");
        let about = Section::ALL.iter().position(|x| *x == Section::AcercaDe).unwrap();
        s.update(Action::OpenSection(about));
        let screen = render(&s);
        assert!(screen.contains("Hay una versión nueva: 9.9.9"), "{screen}");
        assert!(screen.contains("cargo install tubetui"));
    }

    #[test]
    fn feeds_say_how_many_watched_videos_they_hide() {
        let mut s = state();
        s.watched.insert("b".into());
        s.menu = Section::ALL.iter().position(|x| *x == Section::Suscripciones).unwrap();
        let id = match s.update(Action::Enter).as_slice() {
            [Effect::Load { id, .. }, ..] => *id,
            other => panic!("{other:?}"),
        };
        s.update(Action::Loaded(id, Ok(Items::Videos(vec![video("a"), video("b")]))));
        let screen = render(&s);
        assert!(screen.contains("Suscripciones · 1/1 · ◉ 1 vistos ocultos"), "{screen}");
        assert!(hints(&s).iter().any(|(k, d)| k == "v" && *d == "mostrar vistos"));
        s.update(Action::ToggleWatched);
        let screen = render(&s);
        assert!(screen.contains("◉ con los vistos"), "{screen}");
        assert!(screen.contains("◉ title b"), "marked: {screen}");
    }

    #[test]
    fn system_check_page_says_what_to_install() {
        use crate::doctor::Check;
        let mut s = state();
        let missing = Check { tool: Tool::Mpv, found: None, outdated: false, fix: Some("sudo apt install mpv".into()) };
        let old = Check {
            tool: Tool::YtDlp,
            found: Some(("/usr/bin/yt-dlp".into(), Some("2024.04.09".into()))),
            outdated: true,
            fix: Some("sudo apt install pipx && pipx install yt-dlp".into()),
        };
        let fine = Check {
            tool: Tool::Kitty,
            found: Some(("/usr/bin/kitty".into(), Some("0.39.1".into()))),
            outdated: false,
            fix: None,
        };
        let keyring = Check {
            tool: Tool::Keyring,
            found: Some(("GNOMEKEYRING".into(), Some("GNOMEKEYRING".into()))),
            outdated: true,
            fix: Some("pipx inject yt-dlp secretstorage".into()),
        };
        s.update(Action::SystemChecked(Manager::Apt, vec![missing, old, fine, keyring]));
        let screen = render(&s);
        for text in [
            "✗ mpv",
            "falta · necesario",
            "sudo apt install mpv",
            "⚠ yt-dlp",
            "2024.04.09",
            "pipx install yt-dlp",
            "✓ kitty",
            "⚠ keyring",
            "falta: secretstorage",
            "pipx inject yt-dlp secretstorage",
            "Comandos para: apt",
        ] {
            assert!(screen.contains(text), "{text}\n{screen}");
        }
    }

    #[test]
    fn helpers() {
        assert_eq!(truncate("ñandú", 3), "ñan…");
        assert_eq!(truncate("abc", 3), "abc");
        assert_eq!(pad("ab", 4), "ab  ");
        assert_eq!(pad("abcdef", 4), "abcd");
        assert_eq!(pad_cells("🔴 en vivo", 14), "🔴 en vivo    ");
        assert_eq!(pad_cells("● en vivo", 14), "● en vivo     ");
    }
}
