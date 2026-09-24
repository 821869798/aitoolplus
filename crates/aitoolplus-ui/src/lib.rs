//! aitoolplus-ui: the GPUI interface.

#![allow(clippy::type_complexity, clippy::collapsible_if)]

extern crate gpui_kit as gpui;

pub mod components;
pub mod i18n;
pub mod icons;
pub mod layout;
pub mod pages;
pub mod text_area;
pub mod text_input;
pub mod theme;
pub mod workspace;

pub use i18n::I18n;
pub use theme::Theme;
pub use workspace::{Workspace, WorkspaceCallbacks};

use std::sync::atomic::{AtomicBool, Ordering};

/// False while the main window is hidden to the tray. Blink tasks read this
/// so a focused field does not redraw a window nobody can see.
static WINDOW_ON_SCREEN: AtomicBool = AtomicBool::new(true);

pub fn set_window_on_screen(visible: bool) {
    WINDOW_ON_SCREEN.store(visible, Ordering::Relaxed);
}

pub(crate) fn window_on_screen() -> bool {
    WINDOW_ON_SCREEN.load(Ordering::Relaxed)
}

/// True while a backup restore is rewriting tool configs and `store.json`.
/// The config watcher must not import that half-written tree back over the restore.
static RESTORE_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

pub fn restore_in_flight() -> bool {
    RESTORE_IN_FLIGHT.load(Ordering::Relaxed)
}

pub fn set_restore_in_flight(active: bool) {
    RESTORE_IN_FLIGHT.store(active, Ordering::Relaxed);
}

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
