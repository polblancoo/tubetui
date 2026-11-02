//! Video thumbnails for the preview panel. One background thread downloads
//! YouTube's small JPEGs (320×180, about 15 KB); kitty draws the selected one
//! with its graphics protocol, scaled to the cells the panel leaves free.
//! Besides kitty's protocol, iTerm2's inline images (iTerm2, WezTerm) and
//! Sixel (foot, mlterm, contour, ...) are drawn too.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

use base64::Engine;
use image::ImageEncoder;
use ratatui::layout::Rect;

/// Our image in kitty, apart from the ones mpv draws.
const IMAGE_ID: u32 = 0x7475_6265;
/// Decoded thumbnails kept in memory (about 170 KB each).
const CACHE: usize = 64;
/// kitty takes the image in base64 chunks of at most this many bytes.
const CHUNK: usize = 4096;

/// How the terminal can draw images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Graphics {
    Kitty,
    Iterm2,
    Sixel,
}

impl Graphics {
    /// Picks the protocol from the environment. `TUBETUI_GRAPHICS`
    /// (`kitty`, `iterm2`, `sixel` or `none`) overrides the detection, even
    /// inside a multiplexer; otherwise multiplexers get no images.
    pub fn detect(env: impl Fn(&str) -> Option<String>) -> Option<Self> {
        if let Some(forced) = env("TUBETUI_GRAPHICS") {
            return match forced.to_lowercase().as_str() {
                "kitty" => Some(Self::Kitty),
                "iterm2" | "iterm" => Some(Self::Iterm2),
                "sixel" => Some(Self::Sixel),
                _ => None,
            };
        }
        if crate::player::in_multiplexer(&env) {
            return None;
        }
        let term = env("TERM").unwrap_or_default().to_lowercase();
        let program = env("TERM_PROGRAM").unwrap_or_default().to_lowercase();
        if env("KITTY_WINDOW_ID").is_some() || term == "xterm-kitty" {
            Some(Self::Kitty)
        } else if program == "iterm.app" || program == "wezterm" || env("LC_TERMINAL").as_deref() == Some("iTerm2") {
            Some(Self::Iterm2)
        } else if term.starts_with("foot")
            || term.starts_with("mlterm")
            || term.starts_with("contour")
            || term.contains("sixel")
            || term == "yaft-256color"
        {
            Some(Self::Sixel)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    pub width: u32,
    pub height: u32,
    /// 8-bit RGB, row by row.
    pub rgb: Vec<u8>,
}

pub struct Thumbnails {
    cache: HashMap<String, Thumb>,
    order: VecDeque<String>,
    requested: HashSet<String>,
    jobs: mpsc::Sender<String>,
    done: mpsc::Receiver<(String, Option<Thumb>)>,
    /// What is on screen now.
    shown: Option<(String, Rect)>,
    graphics: Graphics,
    /// Pixels of one terminal cell (Sixel draws pixels, not cells).
    cell: (u16, u16),
}

impl Default for Thumbnails {
    fn default() -> Self {
        Self::new(Graphics::Kitty, (10, 20))
    }
}

impl Thumbnails {
    pub fn new(graphics: Graphics, cell: (u16, u16)) -> Self {
        let (jobs, queue) = mpsc::channel::<String>();
        let (finished, done) = mpsc::channel();
        std::thread::spawn(move || {
            for id in queue {
                if finished.send((id.clone(), download(&id).ok())).is_err() {
                    return;
                }
            }
        });
        Self { cache: HashMap::new(), order: VecDeque::new(), requested: HashSet::new(), jobs, done, shown: None, graphics, cell }
    }

    pub fn set_graphics(&mut self, graphics: Graphics, cell: (u16, u16)) {
        if (self.graphics, self.cell) != (graphics, cell) {
            self.graphics = graphics;
            self.cell = cell;
            self.shown = None;
        }
    }

    /// Downloads these thumbnails, in order, unless they are here or on the way.
    pub fn want<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>) {
        for id in ids {
            if !self.cache.contains_key(id) && self.requested.insert(id.to_owned()) {
                let _ = self.jobs.send(id.to_owned());
            }
        }
    }

    /// Takes the finished downloads; whether any arrived.
    pub fn receive(&mut self) -> bool {
        let mut any = false;
        while let Ok((id, thumb)) = self.done.try_recv() {
            self.requested.remove(&id);
            if let Some(thumb) = thumb {
                self.order.push_back(id.clone());
                self.cache.insert(id, thumb);
                any = true;
            }
        }
        while self.order.len() > CACHE {
            if let Some(old) = self.order.pop_front() {
                self.cache.remove(&old);
            }
        }
        any
    }

    /// Shows the thumbnail of `target` (video id, cells), or none. Writes to
    /// the terminal only when that changes. Returns whether an image that
    /// kitty cannot delete (iTerm2, Sixel) was left on the cells: the caller
    /// must repaint the screen, and call this again.
    pub fn show(&mut self, target: Option<(String, Rect)>) -> bool {
        let target = target.filter(|(id, _)| self.cache.contains_key(id));
        if target == self.shown {
            return false;
        }
        if self.graphics != Graphics::Kitty {
            let stale = self.shown.as_ref().is_some_and(|(_, old)| target.as_ref().is_none_or(|(_, new)| new != old));
            if stale {
                self.shown = None;
                return true;
            }
        }
        let mut out = if self.graphics == Graphics::Kitty { delete() } else { String::new() };
        if let Some((id, area)) = &target {
            out.push_str(&match self.graphics {
                Graphics::Kitty => place(&self.cache[id], *area),
                Graphics::Iterm2 => place_iterm2(&self.cache[id], *area),
                Graphics::Sixel => place_sixel(&self.cache[id], *area, self.cell),
            });
        }
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(out.as_bytes()).and_then(|()| stdout.flush());
        self.shown = target;
        false
    }

    /// Something removed every image from the screen: draw it again next time.
    pub fn forget(&mut self) {
        self.shown = None;
    }
}

fn download(id: &str) -> Result<Thumb, String> {
    let url = format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg");
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into();
    let mut jpeg = Vec::new();
    let reply = agent.get(&url).call().map_err(|e| e.to_string())?;
    reply.into_body().into_reader().take(1 << 20).read_to_end(&mut jpeg).map_err(|e| e.to_string())?;
    decode(&jpeg)
}

fn decode(jpeg: &[u8]) -> Result<Thumb, String> {
    let image = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).map_err(|e| e.to_string())?;
    let rgb = image.to_rgb8();
    Ok(Thumb { width: rgb.width(), height: rgb.height(), rgb: rgb.into_raw() })
}

/// Removes our image (and frees it in kitty).
fn delete() -> String {
    format!("\x1b_Ga=d,d=I,i={IMAGE_ID},q=2\x1b\\")
}

/// Sends `thumb` and places it scaled to `area`, leaving the cursor where it was.
fn place(thumb: &Thumb, area: Rect) -> String {
    let data = base64::engine::general_purpose::STANDARD.encode(&thumb.rgb);
    let chunks: Vec<&[u8]> = data.as_bytes().chunks(CHUNK).collect();
    let mut out = format!("\x1b7\x1b[{};{}H", area.y + 1, area.x + 1);
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        let chunk = std::str::from_utf8(chunk).unwrap_or_default();
        if i == 0 {
            out.push_str(&format!(
                "\x1b_Ga=T,f=24,s={},v={},i={IMAGE_ID},c={},r={},C=1,q=2,m={more};{chunk}\x1b\\",
                thumb.width, thumb.height, area.width, area.height
            ));
        } else {
            out.push_str(&format!("\x1b_Gm={more};{chunk}\x1b\\"));
        }
    }
    out.push_str("\x1b8");
    out
}

/// iTerm2's inline image protocol: a PNG scaled to the cells.
fn place_iterm2(thumb: &Thumb, area: Rect) -> String {
    let mut png = Vec::new();
    let encoded = image::codecs::png::PngEncoder::new(&mut png).write_image(
        &thumb.rgb,
        thumb.width,
        thumb.height,
        image::ExtendedColorType::Rgb8,
    );
    if encoded.is_err() {
        return String::new();
    }
    let data = base64::engine::general_purpose::STANDARD.encode(&png);
    format!(
        "\x1b7\x1b[{};{}H\x1b]1337;File=inline=1;size={};width={};height={};preserveAspectRatio=0:{data}\x07\x1b8",
        area.y + 1,
        area.x + 1,
        png.len(),
        area.width,
        area.height
    )
}

/// Sixel: the thumbnail scaled to the area's pixels, in 256 colors (3-3-2 bits).
fn place_sixel(thumb: &Thumb, area: Rect, cell: (u16, u16)) -> String {
    let width = (u32::from(area.width) * u32::from(cell.0)).max(1);
    let height = (u32::from(area.height) * u32::from(cell.1)).max(1);
    let Some(source) = image::RgbImage::from_raw(thumb.width, thumb.height, thumb.rgb.clone()) else {
        return String::new();
    };
    let scaled = image::imageops::resize(&source, width, height, image::imageops::FilterType::Triangle);
    let index = |p: &image::Rgb<u8>| (p[0] >> 5) << 5 | (p[1] >> 5) << 2 | p[2] >> 6;
    let mut out = format!("\x1b7\x1b[{};{}H\x1bP0;1;0q\"1;1;{width};{height}", area.y + 1, area.x + 1);
    // Palette: percentages 0-100 per channel.
    for i in 0..=255_u32 {
        let (r, g, b) = ((i >> 5) & 7, (i >> 2) & 7, i & 3);
        out.push_str(&format!("#{i};2;{};{};{}", r * 100 / 7, g * 100 / 7, b * 100 / 3));
    }
    for band in (0..height).step_by(6) {
        let rows = (height - band).min(6);
        // For each color of the band, the sixel bit pattern of every column.
        let mut masks: HashMap<u8, Vec<u8>> = HashMap::new();
        for x in 0..width {
            for dy in 0..rows {
                let color = index(scaled.get_pixel(x, band + dy));
                masks.entry(color).or_insert_with(|| vec![0; width as usize])[x as usize] |= 1 << dy;
            }
        }
        let mut colors: Vec<_> = masks.into_iter().collect();
        colors.sort_by_key(|(c, _)| *c);
        for (n, (color, mask)) in colors.iter().enumerate() {
            out.push_str(&format!("#{color}"));
            let mut x = 0;
            while x < mask.len() {
                let run = mask[x..].iter().take_while(|&&m| m == mask[x]).count();
                let ch = (63 + mask[x]) as char;
                if run > 3 {
                    out.push_str(&format!("!{run}{ch}"));
                } else {
                    out.extend(std::iter::repeat_n(ch, run));
                }
                x += run;
            }
            // `$` returns to the band's start for the next color, `-` goes down.
            out.push(if n + 1 < colors.len() { '$' } else { '-' });
        }
    }
    out.push_str("\x1b\\\x1b8");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thumb(width: u32, height: u32) -> Thumb {
        Thumb { width, height, rgb: vec![7; (width * height * 3) as usize] }
    }

    #[test]
    fn placing_sends_chunked_rgb_scaled_to_the_cells() {
        let out = place(&thumb(64, 36), Rect::new(10, 4, 40, 11));
        assert!(out.starts_with("\x1b7\x1b[5;11H\x1b_Ga=T,f=24,s=64,v=36,i="), "{out:.60}");
        assert!(out.contains(",c=40,r=11,C=1,q=2,m=1;"), "first chunk says more follow");
        assert!(out.contains("\x1b_Gm=0;"), "last chunk ends it");
        assert!(out.ends_with("\x1b\\\x1b8"), "cursor restored");
        let chunks = out.matches("\x1b_G").count();
        assert_eq!(chunks, (64 * 36 * 3_usize).div_ceil(3) * 4 / CHUNK + 1);
    }

    #[test]
    fn small_images_go_in_one_chunk() {
        let out = place(&thumb(2, 2), Rect::new(0, 0, 4, 2));
        assert!(out.contains(",m=0;"));
        assert_eq!(out.matches("\x1b_G").count(), 1);
    }

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| vars.iter().find(|(k, _)| *k == key).map(|(_, v)| (*v).to_owned())
    }

    #[test]
    fn detects_the_protocol_of_the_terminal() {
        assert_eq!(Graphics::detect(env(&[("KITTY_WINDOW_ID", "1")])), Some(Graphics::Kitty));
        assert_eq!(Graphics::detect(env(&[("TERM_PROGRAM", "iTerm.app")])), Some(Graphics::Iterm2));
        assert_eq!(Graphics::detect(env(&[("TERM_PROGRAM", "WezTerm")])), Some(Graphics::Iterm2));
        assert_eq!(Graphics::detect(env(&[("TERM", "foot")])), Some(Graphics::Sixel));
        assert_eq!(Graphics::detect(env(&[("TERM", "xterm-256color")])), None);
        assert_eq!(Graphics::detect(env(&[("TERM", "foot"), ("TMUX", "1")])), None);
        assert_eq!(Graphics::detect(env(&[("TUBETUI_GRAPHICS", "sixel"), ("TMUX", "1")])), Some(Graphics::Sixel));
        assert_eq!(Graphics::detect(env(&[("TUBETUI_GRAPHICS", "none"), ("TERM", "foot")])), None);
    }

    #[test]
    fn iterm2_sends_a_png_scaled_to_the_cells() {
        let out = place_iterm2(&thumb(4, 4), Rect::new(1, 2, 20, 5));
        assert!(out.starts_with("\x1b7\x1b[3;2H\x1b]1337;File=inline=1;size="), "{out:.60}");
        assert!(out.contains(";width=20;height=5;preserveAspectRatio=0:"));
        assert!(out.ends_with("\x07\x1b8"));
    }

    #[test]
    fn sixel_draws_bands_of_six_pixels() {
        // 2x1 cells of 4x12 px: 8x12 pixels, two bands.
        let out = place_sixel(&thumb(4, 4), Rect::new(0, 0, 2, 1), (4, 12));
        assert!(out.starts_with("\x1b7\x1b[1;1H\x1bP0;1;0q\"1;1;8;12#0;2;0;0;0"), "{out:.60}");
        assert!(out.ends_with("\x1b\\\x1b8"));
        assert_eq!(out.matches('-').count(), 2, "one line feed per band");
        assert!(out.contains("!8~"), "a flat color is one run of full sixels");
    }

    #[test]
    fn only_downloaded_thumbnails_are_shown() {
        let mut thumbs = Thumbnails::default();
        thumbs.show(Some(("missing".into(), Rect::new(0, 0, 4, 2))));
        assert_eq!(thumbs.shown, None);
        thumbs.cache.insert("a".into(), thumb(2, 2));
        thumbs.show(Some(("a".into(), Rect::new(0, 0, 4, 2))));
        assert_eq!(thumbs.shown, Some(("a".into(), Rect::new(0, 0, 4, 2))));
        thumbs.forget();
        assert_eq!(thumbs.shown, None);
    }

    #[test]
    fn decodes_jpeg() {
        let mut jpeg = Vec::new();
        let image = image::RgbImage::from_pixel(8, 4, image::Rgb([200, 10, 10]));
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg).encode_image(&image).ok();
        if jpeg.is_empty() {
            return; // encoder not built with this feature set
        }
        let thumb = decode(&jpeg).unwrap();
        assert_eq!((thumb.width, thumb.height, thumb.rgb.len()), (8, 4, 96));
    }
}
