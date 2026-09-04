//! Semantic theme tokens, dark & light. Mirrors the quiet workbench palette:
//! calm backgrounds, restrained accent, semantic status colors only.

use crate::rgba_const;

/// Re-export the core enum so every layer agrees on one definition.
pub use aitoolplus_core::settings::ThemeMode;

#[derive(Clone, Debug)]
pub struct Theme {
    pub is_dark: bool,
    // surfaces
    pub bg: gpui::Rgba,
    pub sidebar_bg: gpui::Rgba,
    pub card_bg: gpui::Rgba,
    pub card_hover: gpui::Rgba,
    pub card_border: gpui::Rgba,
    pub card_border_hover: gpui::Rgba,
    pub row_hover: gpui::Rgba,
    pub row_selected: gpui::Rgba,
    pub table_header: gpui::Rgba,
    pub input_bg: gpui::Rgba,
    pub input_border: gpui::Rgba,
    // text
    pub text_primary: gpui::Rgba,
    pub text_secondary: gpui::Rgba,
    pub text_muted: gpui::Rgba,
    // accent
    pub accent: gpui::Rgba,
    pub accent_hover: gpui::Rgba,
    pub accent_subtle: gpui::Rgba,
    // status
    pub success: gpui::Rgba,
    pub success_subtle: gpui::Rgba,
    pub warning: gpui::Rgba,
    pub warning_subtle: gpui::Rgba,
    pub danger: gpui::Rgba,
    pub danger_subtle: gpui::Rgba,
    // misc
    pub hover_overlay: gpui::Rgba,
    pub active_overlay: gpui::Rgba,
    pub track_on: gpui::Rgba,
    pub track_off: gpui::Rgba,
    pub thumb: gpui::Rgba,
    pub modal_backdrop: gpui::Rgba,
    pub pill_bg: gpui::Rgba,
    pub pill_border: gpui::Rgba,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            is_dark: true,
            bg: rgba_const(0x0f1115ff),
            sidebar_bg: rgba_const(0x141619ff),
            card_bg: rgba_const(0x1a1d21ff),
            card_hover: rgba_const(0x20242aff),
            card_border: rgba_const(0x2a2f36ff),
            card_border_hover: rgba_const(0x3a414dff),
            row_hover: rgba_const(0x1f2328ff),
            row_selected: rgba_const(0x1c2634ff),
            table_header: rgba_const(0x16191dff),
            input_bg: rgba_const(0x141619ff),
            input_border: rgba_const(0x2a2f36ff),
            text_primary: rgba_const(0xe6e8eaff),
            text_secondary: rgba_const(0x9aa1a9ff),
            text_muted: rgba_const(0x626a72ff),
            accent: rgba_const(0x4c8dffff),
            accent_hover: rgba_const(0x6da2ffff),
            accent_subtle: rgba_const(0x4c8dff1f),
            success: rgba_const(0x34d399ff),
            success_subtle: rgba_const(0x34d3991f),
            warning: rgba_const(0xfbbf24ff),
            warning_subtle: rgba_const(0xfbbf241f),
            danger: rgba_const(0xf87171ff),
            danger_subtle: rgba_const(0xf871711f),
            hover_overlay: rgba_const(0xffffff0a),
            active_overlay: rgba_const(0xffffff14),
            track_on: rgba_const(0x4c8dffff),
            track_off: rgba_const(0x2a2f36ff),
            thumb: rgba_const(0xe6e8eaff),
            modal_backdrop: rgba_const(0x000000a6),
            pill_bg: rgba_const(0x1a1d21f2),
            pill_border: rgba_const(0xffffff14),
        }
    }

    pub fn light() -> Self {
        Self {
            is_dark: false,
            bg: rgba_const(0xf5f6f8ff),
            sidebar_bg: rgba_const(0xedeef1ff),
            card_bg: rgba_const(0xffffffff),
            card_hover: rgba_const(0xf5f7faff),
            card_border: rgba_const(0xdde0e6ff),
            card_border_hover: rgba_const(0xc3c9d4ff),
            row_hover: rgba_const(0xf0f2f6ff),
            row_selected: rgba_const(0xe7effdff),
            table_header: rgba_const(0xf0f1f4ff),
            input_bg: rgba_const(0xffffffff),
            input_border: rgba_const(0xd3d7deff),
            text_primary: rgba_const(0x1a212aff),
            text_secondary: rgba_const(0x535e6aff),
            text_muted: rgba_const(0x8b95a1ff),
            accent: rgba_const(0x1668dcff),
            accent_hover: rgba_const(0x0e5cc4ff),
            accent_subtle: rgba_const(0x1668dc1a),
            success: rgba_const(0x0d9668ff),
            success_subtle: rgba_const(0x0d96681a),
            warning: rgba_const(0xd97706ff),
            warning_subtle: rgba_const(0xd977061a),
            danger: rgba_const(0xdc2626ff),
            danger_subtle: rgba_const(0xdc26261a),
            hover_overlay: rgba_const(0x00000008),
            active_overlay: rgba_const(0x00000012),
            track_on: rgba_const(0x1668dcff),
            track_off: rgba_const(0xd3d7deff),
            thumb: rgba_const(0xffffffff),
            modal_backdrop: rgba_const(0x00000073),
            pill_bg: rgba_const(0xffffffeb),
            pill_border: rgba_const(0x00000014),
        }
    }

    pub fn for_mode(mode: ThemeMode, system_dark: bool) -> Self {
        match mode {
            ThemeMode::Dark => Self::dark(),
            ThemeMode::Light => Self::light(),
            ThemeMode::System => {
                if system_dark {
                    Self::dark()
                } else {
                    Self::light()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_and_light_differ() {
        let d = Theme::dark();
        let l = Theme::light();
        assert!(d.is_dark);
        assert!(!l.is_dark);
        assert_ne!(d.bg, l.bg);
    }

    #[test]
    fn system_resolves() {
        assert!(Theme::for_mode(ThemeMode::System, true).is_dark);
        assert!(!Theme::for_mode(ThemeMode::System, false).is_dark);
    }
}
