//! Short animations (tachyonfx) when the screen changes: a page appearing,
//! a dialog opening. They last a fraction of a second and never run over an
//! embedded video, so they do not compete with mpv for the terminal.

use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;
use tachyonfx::{EffectManager, Interpolation, Motion, fx};

use crate::app::State;
use crate::{theme, ui};

/// The terminal background of the theme, where text fades in from.
fn background() -> Color {
    theme::palette().background.unwrap_or(Color::Rgb(26, 27, 38))
}

fn accent() -> Color {
    theme::palette().blue
}

/// What is on screen, as far as animations care.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scene {
    active: usize,
    /// Titles of the lists on Inicio.
    pages: Vec<String>,
    dialog: bool,
    sidebar: bool,
    /// The opening logo.
    splash: bool,
}

impl Scene {
    pub fn of(state: &State) -> Self {
        Self {
            active: state.active,
            pages: state.stack.iter().map(|l| l.title.clone()).collect(),
            dialog: state.help || state.confirm.is_some(),
            sidebar: state.sidebar,
            splash: state.splash,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// The opening logo appears.
    Splash,
    /// The app just started (or its logo went away).
    Startup,
    /// Another list, section or tab in the content area.
    Page,
    /// Help or a confirmation opened.
    Dialog,
}

/// Which animation, if any, a change of scene deserves.
pub fn transition(previous: Option<&Scene>, next: &Scene) -> Option<Transition> {
    let Some(previous) = previous else {
        return Some(if next.splash { Transition::Splash } else { Transition::Startup });
    };
    if previous.splash && !next.splash {
        Some(Transition::Startup)
    } else if next.splash {
        None
    } else if next.dialog && !previous.dialog {
        Some(Transition::Dialog)
    } else if !next.dialog && (next.active != previous.active || next.pages != previous.pages) {
        Some(Transition::Page)
    } else {
        None
    }
}

#[derive(Default)]
pub struct Animations {
    effects: EffectManager<()>,
    scene: Option<Scene>,
    last_frame: Option<Instant>,
}

impl Animations {
    pub fn running(&self) -> bool {
        self.effects.is_running()
    }

    /// Starts an animation if the screen changed since the last frame.
    pub fn observe(&mut self, state: &State, screen: Rect) {
        let scene = Scene::of(state);
        let change = transition(self.scene.as_ref(), &scene);
        self.scene = Some(scene);
        let effects = &state.config.effects;
        let enabled = |change| match change {
            Transition::Splash | Transition::Startup => effects.startup,
            Transition::Page => effects.pages,
            Transition::Dialog => effects.dialogs,
        };
        let Some(change) = change.filter(|c| enabled(*c)) else { return };
        // Never draw over the video: mpv owns those cells.
        if ui::video_area(state, screen).is_some() {
            return;
        }
        let areas = ui::areas(screen, state.sidebar);
        let timer = |ms| (ms, Interpolation::QuadOut);
        match change {
            Transition::Splash => {
                let flash = fx::fade_from_fg(background(), timer(700));
                self.add(fx::parallel(&[fx::coalesce(timer(600)), flash]).with_area(screen));
            }
            Transition::Startup => {
                if let Some(sidebar) = areas.sidebar {
                    self.add(fx::sweep_in(Motion::UpToDown, 8, 0, background(), timer(450)).with_area(sidebar));
                }
                self.add(fx::coalesce(timer(500)).with_area(areas.content));
            }
            Transition::Page => {
                self.add(fx::sweep_in(Motion::LeftToRight, 12, 0, background(), timer(300)).with_area(areas.content));
            }
            Transition::Dialog => {
                if let Some(dialog) = ui::dialog_area(state, screen) {
                    let flash = fx::fade_from_fg(accent(), timer(350));
                    let reveal = fx::sweep_in(Motion::UpToDown, 6, 0, background(), timer(250));
                    self.add(fx::parallel(&[reveal, flash]).with_area(dialog));
                }
            }
        }
    }

    /// Applies the running animations to the frame just drawn.
    pub fn render(&mut self, frame: &mut Frame) {
        let now = Instant::now();
        let elapsed = self.last_frame.map_or(Duration::ZERO, |t| now - t);
        self.last_frame = self.running().then_some(now);
        let area = frame.area();
        self.effects.process_effects(elapsed.into(), frame.buffer_mut(), area);
    }

    fn add(&mut self, effect: tachyonfx::Effect) {
        self.effects.add_effect(effect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(active: usize, pages: &[&str], dialog: bool) -> Scene {
        Scene { active, pages: pages.iter().map(|p| p.to_string()).collect(), dialog, sidebar: true, splash: false }
    }

    #[test]
    fn first_frame_is_the_startup() {
        assert_eq!(transition(None, &scene(0, &[], false)), Some(Transition::Startup));
    }

    #[test]
    fn the_logo_comes_first_then_the_menu() {
        let logo = Scene { splash: true, ..scene(0, &[], false) };
        assert_eq!(transition(None, &logo), Some(Transition::Splash));
        assert_eq!(transition(Some(&logo), &logo), None);
        assert_eq!(transition(Some(&logo), &scene(0, &[], false)), Some(Transition::Startup));
    }

    #[test]
    fn new_pages_and_tabs_animate_the_content() {
        let home = scene(0, &["Inicio"], false);
        assert_eq!(transition(Some(&home), &scene(0, &["Inicio", "Canal"], false)), Some(Transition::Page));
        assert_eq!(transition(Some(&home), &scene(1, &["Inicio"], false)), Some(Transition::Page));
        assert_eq!(transition(Some(&home), &home), None, "moving the selection is not a page");
    }

    #[test]
    fn dialogs_animate_when_they_open_not_when_they_close() {
        let home = scene(0, &[], false);
        let help = scene(0, &[], true);
        assert_eq!(transition(Some(&home), &help), Some(Transition::Dialog));
        assert_eq!(transition(Some(&help), &home), None);
    }

    #[test]
    fn disabled_or_over_video_nothing_runs() {
        use crate::app::Action;
        use crate::config::Config;
        let screen = Rect::new(0, 0, 100, 30);
        let mut config = Config::default();
        config.effects.startup = false;
        let mut animations = Animations::default();
        animations.observe(&State::new(config, Vec::new()), screen);
        assert!(!animations.running());

        let mut state = State::new(Config::default(), Vec::new());
        let mut animations = Animations::default();
        animations.observe(&state, screen);
        assert!(animations.running(), "startup animates");
        let mut animations = Animations::default();
        state.menu = crate::app::Section::ALL.iter().position(|s| *s == crate::app::Section::Suscripciones).unwrap();
        let id = match state.update(Action::Enter).as_slice() {
            [crate::app::Effect::Load { id, .. }] => *id, // Suscripciones loads
            other => panic!("{other:?}"),
        };
        state.update(Action::Loaded(id, Ok(crate::youtube::Items::Videos(vec![crate::domain::video("a")]))));
        state.update(Action::Enter); // plays it in a tab
        animations.observe(&state, screen);
        assert!(!animations.running(), "the video owns the screen");
    }
}
