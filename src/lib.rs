//! tubetui — YouTube in your terminal.
//!
//! The core is pure and synchronous: [`app::State::update`] turns an
//! [`app::Action`] into state changes plus [`app::Effect`]s, and the binary
//! performs the effects (spawning mpv, running searches). Everything that
//! touches processes or the terminal stays at the edges.

pub mod anim;
pub mod app;
pub mod config;
pub mod doctor;
pub mod domain;
pub mod i18n;
pub mod keymap;
pub mod media;
pub mod mouse;
pub mod player;
pub mod relocate;
pub mod session;
pub mod sponsorblock;
pub mod store;
pub mod theme;
pub mod thumbs;
pub mod ui;
pub mod update;
pub mod youtube;
