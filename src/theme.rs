//! Colour themes and border styles. The rendering code asks for a colour by
//! role (`palette().blue`) and the current theme answers; `ui::draw` selects
//! the theme from the config before every frame.

use std::sync::atomic::{AtomicU8, Ordering};

use ratatui::style::Color;
use ratatui::widgets::BorderType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Tokyo,
    Catppuccin,
    Gruvbox,
    Light,
}

impl Theme {
    pub fn next(self) -> Self {
        match self {
            Self::Tokyo => Self::Catppuccin,
            Self::Catppuccin => Self::Gruvbox,
            Self::Gruvbox => Self::Light,
            Self::Light => Self::Tokyo,
        }
    }

    /// Value as written in `config.toml`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tokyo => "tokyo",
            Self::Catppuccin => "catppuccin",
            Self::Gruvbox => "gruvbox",
            Self::Light => "light",
        }
    }

    /// The theme's name for the settings screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Tokyo => "Tokyo Night",
            Self::Catppuccin => "Catppuccin",
            Self::Gruvbox => "Gruvbox",
            Self::Light => "Light",
        }
    }

    fn palette(self) -> &'static Palette {
        match self {
            Self::Tokyo => &TOKYO,
            Self::Catppuccin => &CATPPUCCIN,
            Self::Gruvbox => &GRUVBOX,
            Self::Light => &LIGHT,
        }
    }

    fn index(self) -> u8 {
        self as u8
    }

    fn from_index(index: u8) -> Self {
        [Self::Tokyo, Self::Catppuccin, Self::Gruvbox, Self::Light].get(usize::from(index)).copied().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Border {
    #[default]
    Rounded,
    Plain,
    Double,
    Thick,
}

impl Border {
    pub fn next(self) -> Self {
        match self {
            Self::Rounded => Self::Plain,
            Self::Plain => Self::Double,
            Self::Double => Self::Thick,
            Self::Thick => Self::Rounded,
        }
    }

    /// Value as written in `config.toml`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rounded => "rounded",
            Self::Plain => "plain",
            Self::Double => "double",
            Self::Thick => "thick",
        }
    }

    pub fn border_type(self) -> BorderType {
        match self {
            Self::Rounded => BorderType::Rounded,
            Self::Plain => BorderType::Plain,
            Self::Double => BorderType::Double,
            Self::Thick => BorderType::Thick,
        }
    }

    fn index(self) -> u8 {
        self as u8
    }

    fn from_index(index: u8) -> Self {
        [Self::Rounded, Self::Plain, Self::Double, Self::Thick].get(usize::from(index)).copied().unwrap_or_default()
    }
}

/// One colour per role.
#[derive(Debug)]
pub struct Palette {
    /// Paints the whole screen; dark themes leave the terminal's own background.
    pub background: Option<Color>,
    pub fg: Color,
    pub muted: Color,
    pub highlight: Color,
    /// Separator lines inside dialogs: visible but quieter than text.
    pub rule: Color,
    pub blue: Color,
    pub cyan: Color,
    pub magenta: Color,
    pub green: Color,
    pub orange: Color,
    pub yellow: Color,
    pub red: Color,
    pub teal: Color,
    pub purple: Color,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

static TOKYO: Palette = Palette {
    background: None,
    fg: rgb(192, 202, 245),
    muted: rgb(86, 95, 137),
    highlight: rgb(41, 46, 66),
    rule: rgb(59, 66, 97),
    blue: rgb(122, 162, 247),
    cyan: rgb(125, 207, 255),
    magenta: rgb(187, 154, 247),
    green: rgb(158, 206, 106),
    orange: rgb(255, 158, 100),
    yellow: rgb(224, 175, 104),
    red: rgb(247, 118, 142),
    teal: rgb(115, 218, 202),
    purple: rgb(157, 124, 216),
};

static CATPPUCCIN: Palette = Palette {
    background: None,
    fg: rgb(205, 214, 244),
    muted: rgb(127, 132, 156),
    highlight: rgb(49, 50, 68),
    rule: rgb(69, 71, 90),
    blue: rgb(137, 180, 250),
    cyan: rgb(137, 220, 235),
    magenta: rgb(203, 166, 247),
    green: rgb(166, 227, 161),
    orange: rgb(250, 179, 135),
    yellow: rgb(249, 226, 175),
    red: rgb(243, 139, 168),
    teal: rgb(148, 226, 213),
    purple: rgb(180, 190, 254),
};

static GRUVBOX: Palette = Palette {
    background: None,
    fg: rgb(235, 219, 178),
    muted: rgb(146, 131, 116),
    highlight: rgb(60, 56, 54),
    rule: rgb(80, 73, 69),
    blue: rgb(131, 165, 152),
    cyan: rgb(142, 192, 124),
    magenta: rgb(211, 134, 155),
    green: rgb(184, 187, 38),
    orange: rgb(254, 128, 25),
    yellow: rgb(250, 189, 47),
    red: rgb(251, 73, 52),
    teal: rgb(104, 157, 106),
    purple: rgb(177, 98, 134),
};

static LIGHT: Palette = Palette {
    background: Some(rgb(225, 226, 231)),
    fg: rgb(52, 59, 88),
    muted: rgb(97, 114, 176),
    highlight: rgb(196, 200, 218),
    rule: rgb(168, 174, 203),
    blue: rgb(46, 125, 233),
    cyan: rgb(0, 113, 151),
    magenta: rgb(152, 84, 241),
    green: rgb(88, 117, 57),
    orange: rgb(177, 92, 0),
    yellow: rgb(140, 108, 62),
    red: rgb(245, 42, 101),
    teal: rgb(17, 140, 116),
    purple: rgb(120, 71, 189),
};

static THEME: AtomicU8 = AtomicU8::new(0);
static BORDER: AtomicU8 = AtomicU8::new(0);

/// Make `theme` and `border` the ones every colour and frame comes from.
pub fn select(theme: Theme, border: Border) {
    THEME.store(theme.index(), Ordering::Relaxed);
    BORDER.store(border.index(), Ordering::Relaxed);
}

pub fn theme() -> Theme {
    Theme::from_index(THEME.load(Ordering::Relaxed))
}

pub fn palette() -> &'static Palette {
    theme().palette()
}

pub fn border() -> Border {
    Border::from_index(BORDER.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_and_border_comes_round() {
        let (mut theme, mut border) = (Theme::Tokyo, Border::Rounded);
        for _ in 0..4 {
            theme = theme.next();
            border = border.next();
        }
        assert_eq!((theme, border), (Theme::Tokyo, Border::Rounded));
    }

    #[test]
    fn indexes_round_trip() {
        for theme in [Theme::Tokyo, Theme::Catppuccin, Theme::Gruvbox, Theme::Light] {
            assert_eq!(Theme::from_index(theme.index()), theme);
        }
        for border in [Border::Rounded, Border::Plain, Border::Double, Border::Thick] {
            assert_eq!(Border::from_index(border.index()), border);
        }
    }
}
