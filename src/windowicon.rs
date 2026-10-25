//! Puts the tubetui icon on the terminal's window (X11 only).
//!
//! Terminals like kitty set their own icon on the window, so the taskbar
//! shows theirs and ignores the `.desktop` file. `_NET_WM_ICON` is set on
//! the window in `$WINDOWID` instead.

/// The window icon, 48x48 PNG.
#[cfg(target_os = "linux")]
pub(crate) const ICON: &[u8] = include_bytes!("../assets/icon-48.png");

/// Sets the icon on the terminal's window. Does nothing (and never fails
/// loudly) off X11, inside a multiplexer or when the window is not ours.
#[cfg(target_os = "linux")]
pub fn set() {
    let _ = try_set();
}

#[cfg(not(target_os = "linux"))]
pub fn set() {}

#[cfg(target_os = "linux")]
fn try_set() -> Option<()> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, PropMode};
    use x11rb::wrapper::ConnectionExt as _;

    if std::env::var_os("TMUX").is_some() || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland") {
        return None;
    }
    let window: u32 = std::env::var("WINDOWID").ok()?.parse().ok()?;
    let data = pixels(ICON)?;
    let (conn, _) = x11rb::connect(None).ok()?;
    let atom = conn.intern_atom(false, b"_NET_WM_ICON").ok()?.reply().ok()?.atom;
    conn.change_property32(PropMode::REPLACE, window, atom, AtomEnum::CARDINAL, &data).ok()?;
    conn.flush().ok()
}

/// The pixels as the server wants them (32 bits per pixel, blue first),
/// blended over the icon's own dark background where it is transparent.
#[cfg(target_os = "linux")]
pub(crate) fn bgrx(img: &image::RgbaImage) -> Vec<u8> {
    const BACKDROP: [u32; 3] = [0x1a, 0x1b, 0x26];
    let mut out = Vec::with_capacity(img.as_raw().len());
    for p in img.pixels() {
        let a = u32::from(p[3]);
        let blend = |c: u8, back: u32| ((u32::from(c) * a + back * (255 - a)) / 255) as u8;
        out.extend([blend(p[2], BACKDROP[2]), blend(p[1], BACKDROP[1]), blend(p[0], BACKDROP[0]), 0]);
    }
    out
}

/// `_NET_WM_ICON` payload: width, height, then one ARGB value per pixel.
#[cfg(target_os = "linux")]
fn pixels(png: &[u8]) -> Option<Vec<u32>> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.to_rgba8();
    let mut data = vec![img.width(), img.height()];
    data.extend(img.pixels().map(|p| u32::from_be_bytes([p[3], p[0], p[1], p[2]])));
    Some(data)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn transparent_pixels_take_the_backdrop_color() {
        let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 0]));
        assert_eq!(bgrx(&img), vec![0x26, 0x1b, 0x1a, 0]);
        let opaque = image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]));
        assert_eq!(bgrx(&opaque), vec![3, 2, 1, 0]);
    }

    #[test]
    fn the_icon_is_48_by_48_argb() {
        let data = pixels(ICON).unwrap();
        assert_eq!(&data[..2], &[48, 48]);
        assert_eq!(data.len(), 2 + 48 * 48);
        // The corner is transparent, the middle (the play button) is opaque.
        assert_eq!(data[2] >> 24, 0);
        assert_eq!(data[2 + 24 * 48 + 24] >> 24, 0xff);
    }
}
