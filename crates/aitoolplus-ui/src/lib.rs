//! aitoolplus-ui: the GPUI interface.

#![allow(clippy::type_complexity, clippy::collapsible_if)]

extern crate gpui_kit as gpui;

pub mod components;
pub mod i18n;
pub mod layout;
pub mod pages;
pub mod text_area;
pub mod text_input;
pub mod theme;
pub mod workspace;

pub use i18n::I18n;
pub use theme::Theme;
pub use workspace::{Workspace, WorkspaceCallbacks};

/// [`gpui::rgba`], as a `const fn` — GPUI's own is not, and colour tables
/// want to live in constants. (Same trick as flyclip.)
pub(crate) const fn rgba_const(hex: u32) -> gpui::Rgba {
    let [r, g, b, a] = hex.to_be_bytes();
    gpui::Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a as f32 / 255.0,
    }
}
