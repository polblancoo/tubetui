//! The tubetui icon in the system tray (XEmbed, the protocol Fluxbox, trayer
//! and most X11 trays speak), with a menu on click. It lives while the
//! process does; with no tray it does nothing.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::sync::mpsc::Sender;

use crate::app::{Action, Section};
use crate::i18n::Lang;

/// What a menu entry does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Cmd {
    /// Bring the terminal to the front.
    Show,
    Pause,
    Section(Section),
    Quit,
}

enum Entry {
    Item(&'static str, &'static str, Cmd),
    Separator,
}

fn entries() -> Vec<Entry> {
    use Entry::*;
    vec![
        Item("Mostrar terminal", "Show terminal", Cmd::Show),
        Separator,
        Item("Pausa / seguir", "Pause / resume", Cmd::Pause),
        Separator,
        Item("Mis recomendados", "My recommendations", Cmd::Section(Section::Inicio)),
        Item("Buscar videos", "Search videos", Cmd::Section(Section::BuscarVideos)),
        Item("Mis Videos", "My Videos", Cmd::Section(Section::MisVideos)),
        Item("Mi Música", "My Music", Cmd::Section(Section::MiMusica)),
        Item("Descargas", "Downloads", Cmd::Section(Section::Descargas)),
        Item("Configuración", "Settings", Cmd::Section(Section::Configuracion)),
        Separator,
        Item("Salir", "Quit", Cmd::Quit),
    ]
}

/// The action a command sends to the app, if any.
fn action(cmd: Cmd) -> Option<Action> {
    match cmd {
        Cmd::Show => None,
        Cmd::Pause => Some(Action::TogglePause),
        Cmd::Section(section) => Section::ALL.iter().position(|s| *s == section).map(Action::OpenSection),
        Cmd::Quit => Some(Action::Quit),
    }
}

/// Docks the icon in the tray and serves its menu on a thread of its own.
#[cfg(target_os = "linux")]
pub fn start(tx: Sender<Action>, lang: Lang) {
    std::thread::spawn(move || {
        let _ = run(&tx, lang);
    });
}

#[cfg(not(target_os = "linux"))]
pub fn start(_tx: Sender<Action>, _lang: Lang) {}

#[cfg(target_os = "linux")]
mod x11 {
    use super::*;
    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt, CreateGCAux, CreateWindowAux, EventMask, GrabMode, ImageFormat,
        Point, PropMode, Rectangle, Window, WindowClass,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    const BACKGROUND: u32 = 0x1a1b26;
    const TEXT: u32 = 0xc0caf5;
    const HOVER: u32 = 0x7aa2f7;
    const BORDER: u32 = 0x7aa2f7;
    const RULE: u32 = 0x414868;
    const ESCAPE: u8 = 9;

    struct Menu {
        window: Window,
        width: u16,
        height: u16,
        /// Top of each entry, and its height.
        rows: Vec<(i16, u16)>,
        hover: Option<usize>,
    }

    pub fn run(tx: &Sender<Action>, lang: Lang) -> Option<()> {
        let (conn, screen_num) = x11rb::connect(None).ok()?;
        let screen = conn.setup().roots[screen_num].clone();
        let atom = |name: &str| conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok().map(|r| r.atom);
        let manager = conn.get_selection_owner(atom(&format!("_NET_SYSTEM_TRAY_S{screen_num}"))?).ok()?.reply().ok()?.owner;
        if manager == x11rb::NONE {
            return None;
        }

        let icon = conn.generate_id().ok()?;
        let aux = CreateWindowAux::new().background_pixel(screen.black_pixel).event_mask(
            EventMask::EXPOSURE | EventMask::STRUCTURE_NOTIFY | EventMask::BUTTON_PRESS,
        );
        conn.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            icon,
            screen.root,
            0,
            0,
            24,
            24,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &aux,
        )
        .ok()?;
        conn.change_property8(PropMode::REPLACE, icon, AtomEnum::WM_NAME, AtomEnum::STRING, b"tubetui").ok()?;
        // _XEMBED_INFO: version 0, flags XEMBED_MAPPED.
        let xembed_info = atom("_XEMBED_INFO")?;
        conn.change_property32(PropMode::REPLACE, icon, xembed_info, xembed_info, &[0, 1]).ok()?;
        let dock = ClientMessageEvent::new(32, manager, atom("_NET_SYSTEM_TRAY_OPCODE")?, [x11rb::CURRENT_TIME, 0, icon, 0, 0]);
        // An empty mask: the message goes to whoever owns the tray.
        conn.send_event(false, manager, EventMask::NO_EVENT, dock).ok()?;

        let picture = image::load_from_memory_with_format(crate::windowicon::ICON, image::ImageFormat::Png).ok()?.to_rgba8();
        let gc = conn.generate_id().ok()?;
        conn.create_gc(gc, icon, &CreateGCAux::new()).ok()?;
        let font = open_font(&conn)?;
        conn.change_gc(gc, &x11rb::protocol::xproto::ChangeGCAux::new().font(font)).ok()?;
        let metrics = conn.query_font(font).ok()?.reply().ok()?;
        let (ascent, descent, cell) =
            (metrics.font_ascent.max(1), metrics.font_descent.max(0), metrics.max_bounds.character_width.max(1));
        conn.flush().ok()?;

        let entries = entries();
        let label = |e: &Entry| match e {
            Entry::Item(es, en, _) => Some(lang.pick(*es, *en)),
            Entry::Separator => None,
        };
        let (mut width, mut height) = (24u16, 24u16);
        let mut menu: Option<Menu> = None;
        loop {
            match conn.wait_for_event().ok()? {
                Event::ConfigureNotify(e) if e.window == icon => (width, height) = (e.width.max(1), e.height.max(1)),
                Event::Expose(e) if e.window == icon && e.count == 0 => {
                    let scaled = image::imageops::resize(
                        &picture,
                        u32::from(width),
                        u32::from(height),
                        image::imageops::FilterType::Triangle,
                    );
                    let data = crate::windowicon::bgrx(&scaled);
                    conn.put_image(ImageFormat::Z_PIXMAP, icon, gc, width, height, 0, 0, 0, screen.root_depth, &data).ok()?;
                    conn.flush().ok()?;
                }
                Event::DestroyNotify(e) if e.window == icon => return None,
                Event::ButtonPress(e) if e.event == icon => {
                    if let Some(open) = menu.take() {
                        close(&conn, &open);
                        continue;
                    }
                    let pointer = conn.query_pointer(screen.root).ok()?.reply().ok()?;
                    let line = ascent + descent + 10;
                    let longest = entries.iter().filter_map(label).map(|l| l.chars().count()).max().unwrap_or(0);
                    let w = (longest as i16 * cell + 36).max(120) as u16;
                    let mut rows = Vec::new();
                    let mut top = 4i16;
                    for entry in &entries {
                        let h = if matches!(entry, Entry::Separator) { 7 } else { line as u16 };
                        rows.push((top, h));
                        top += h as i16;
                    }
                    let h = (top + 4) as u16;
                    let x = (pointer.root_x - w as i16 / 2).clamp(0, screen.width_in_pixels as i16 - w as i16);
                    let above = pointer.root_y - h as i16 - 8;
                    let y = if above >= 0 { above } else { (pointer.root_y + 8).min(screen.height_in_pixels as i16 - h as i16) };
                    let window = conn.generate_id().ok()?;
                    let aux = CreateWindowAux::new()
                        .override_redirect(1)
                        .background_pixel(BACKGROUND)
                        .border_pixel(BORDER)
                        .event_mask(
                            EventMask::EXPOSURE
                                | EventMask::BUTTON_PRESS
                                | EventMask::POINTER_MOTION
                                | EventMask::KEY_PRESS,
                        );
                    conn.create_window(
                        x11rb::COPY_DEPTH_FROM_PARENT,
                        window,
                        screen.root,
                        x,
                        y,
                        w,
                        h,
                        1,
                        WindowClass::INPUT_OUTPUT,
                        screen.root_visual,
                        &aux,
                    )
                    .ok()?;
                    conn.map_window(window).ok()?;
                    let mask = EventMask::BUTTON_PRESS | EventMask::POINTER_MOTION;
                    conn.grab_pointer(true, window, mask, GrabMode::ASYNC, GrabMode::ASYNC, x11rb::NONE, x11rb::NONE, x11rb::CURRENT_TIME).ok()?;
                    conn.grab_keyboard(true, window, x11rb::CURRENT_TIME, GrabMode::ASYNC, GrabMode::ASYNC).ok()?;
                    conn.flush().ok()?;
                    menu = Some(Menu { window, width: w, height: h, rows, hover: None });
                }
                Event::Expose(e) if menu.as_ref().is_some_and(|m| m.window == e.window) && e.count == 0 => {
                    draw(&conn, gc, menu.as_ref()?, &entries, ascent, &label)?;
                }
                Event::MotionNotify(e) if menu.is_some() => {
                    let open = menu.as_mut()?;
                    let hover = pick(open, &entries, e.event_x, e.event_y);
                    if hover != open.hover {
                        open.hover = hover;
                        draw(&conn, gc, open, &entries, ascent, &label)?;
                    }
                }
                Event::ButtonPress(e) if menu.is_some() => {
                    let open = menu.take()?;
                    close(&conn, &open);
                    if let Some(Entry::Item(_, _, cmd)) = pick(&open, &entries, e.event_x, e.event_y).map(|i| &entries[i]) {
                        run_command(&conn, screen.root, tx, *cmd);
                    }
                }
                Event::KeyPress(e) if menu.is_some() && e.detail == ESCAPE => {
                    close(&conn, &menu.take()?);
                }
                _ => {}
            }
        }
    }

    fn open_font(conn: &RustConnection) -> Option<u32> {
        let font = conn.generate_id().ok()?;
        for name in [&b"9x15"[..], b"fixed"] {
            if conn.open_font(font, name).ok()?.check().is_ok() {
                return Some(font);
            }
        }
        None
    }

    /// The entry under a point of the menu, if it is one that can be picked.
    fn pick(menu: &Menu, entries: &[Entry], x: i16, y: i16) -> Option<usize> {
        if x < 0 || x >= menu.width as i16 {
            return None;
        }
        let i = menu.rows.iter().position(|&(top, h)| y >= top && y < top + h as i16)?;
        matches!(entries[i], Entry::Item(..)).then_some(i)
    }

    fn close(conn: &RustConnection, menu: &Menu) {
        let _ = conn.ungrab_pointer(x11rb::CURRENT_TIME);
        let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
        let _ = conn.destroy_window(menu.window);
        let _ = conn.flush();
    }

    fn draw(
        conn: &RustConnection,
        gc: u32,
        menu: &Menu,
        entries: &[Entry],
        ascent: i16,
        label: &dyn Fn(&Entry) -> Option<&'static str>,
    ) -> Option<()> {
        use x11rb::protocol::xproto::ChangeGCAux;
        let fill = |color: u32, rect: Rectangle| -> Option<()> {
            conn.change_gc(gc, &ChangeGCAux::new().foreground(color)).ok()?;
            conn.poly_fill_rectangle(menu.window, gc, &[rect]).ok()?;
            Some(())
        };
        fill(BACKGROUND, Rectangle { x: 0, y: 0, width: menu.width, height: menu.height })?;
        for (i, entry) in entries.iter().enumerate() {
            let (top, h) = menu.rows[i];
            match label(entry) {
                None => {
                    conn.change_gc(gc, &ChangeGCAux::new().foreground(RULE)).ok()?;
                    let y = top + h as i16 / 2;
                    conn.poly_line(
                        x11rb::protocol::xproto::CoordMode::ORIGIN,
                        menu.window,
                        gc,
                        &[Point { x: 8, y }, Point { x: menu.width as i16 - 8, y }],
                    )
                    .ok()?;
                }
                Some(text) => {
                    let hovered = menu.hover == Some(i);
                    if hovered {
                        fill(HOVER, Rectangle { x: 2, y: top, width: menu.width - 4, height: h })?;
                    }
                    let (fg, bg) = if hovered { (BACKGROUND, HOVER) } else { (TEXT, BACKGROUND) };
                    conn.change_gc(gc, &ChangeGCAux::new().foreground(fg).background(bg)).ok()?;
                    // The core font is Latin-1: Spanish accents fit in a byte.
                    let bytes: Vec<u8> = text.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect();
                    conn.image_text8(menu.window, gc, 14, top + 5 + ascent, &bytes).ok()?;
                }
            }
        }
        conn.flush().ok()
    }

    fn run_command(conn: &RustConnection, root: Window, tx: &Sender<Action>, cmd: Cmd) {
        if let Some(action) = action(cmd) {
            let _ = tx.send(action);
        }
        if !matches!(cmd, Cmd::Pause | Cmd::Quit) {
            show_terminal(conn, root);
        }
    }

    /// Asks the window manager to bring the terminal (`$WINDOWID`) to the front.
    fn show_terminal(conn: &RustConnection, root: Window) {
        let Some(window) = std::env::var("WINDOWID").ok().and_then(|w| w.parse::<u32>().ok()) else { return };
        let Some(active) = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW").ok().and_then(|c| c.reply().ok()) else {
            return;
        };
        // Source 2: a pager, which window managers always obey.
        let message = ClientMessageEvent::new(32, window, active.atom, [2, x11rb::CURRENT_TIME, 0, 0, 0]);
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        let _ = conn.send_event(false, root, mask, message);
        let _ = conn.flush();
    }
}

#[cfg(target_os = "linux")]
use x11::run;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_section_in_the_menu_exists_in_the_app() {
        for entry in entries() {
            if let Entry::Item(_, _, cmd @ Cmd::Section(_)) = entry {
                assert!(matches!(action(cmd), Some(Action::OpenSection(_))), "{cmd:?}");
            }
        }
    }

    #[test]
    fn quit_and_pause_do_not_need_the_terminal_in_front() {
        assert_eq!(action(Cmd::Show), None);
        assert!(matches!(action(Cmd::Quit), Some(Action::Quit)));
        assert!(matches!(action(Cmd::Pause), Some(Action::TogglePause)));
    }
}
