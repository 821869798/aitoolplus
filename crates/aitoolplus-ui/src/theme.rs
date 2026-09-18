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
    pub sidebar_border: gpui::Rgba,
    pub header_bg: gpui::Rgba,
    pub card_bg: gpui::Rgba,
    pub card_hover: gpui::Rgba,
    pub card_border: gpui::Rgba,
    pub card_border_hover: gpui::Rgba,
    pub row_hover: gpui::Rgba,
    pub row_selected: gpui::Rgba,
    pub table_header: gpui::Rgba,
    pub input_bg: gpui::Rgba,
    pub input_border: gpui::Rgba,
    // tab bar
    pub tab_bar_bg: gpui::Rgba,
    pub tab_active_bg: gpui::Rgba,
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
            bg: rgba_const(0x0c0e12ff),
            sidebar_bg: rgba_const(0x101318ff),
            sidebar_border: rgba_const(0x1c212aff),
            header_bg: rgba_const(0x101318ff),
            card_bg: rgba_const(0x14181fff),
            card_hover: rgba_const(0x191e27ff),
            card_border: rgba_const(0x222936ff),
            card_border_hover: rgba_const(0x2f394aff),
            row_hover: rgba_const(0x191e27ff),
            row_selected: rgba_const(0x4f87ff24),
            table_header: rgba_const(0x101318ff),
            input_bg: rgba_const(0x0e1115ff),
            input_border: rgba_const(0x222936ff),
            tab_bar_bg: rgba_const(0x0e1115ff),
            tab_active_bg: rgba_const(0x1c222cff),
            text_primary: rgba_const(0xf1f5f9ff),
            text_secondary: rgba_const(0x94a3b8ff),
            text_muted: rgba_const(0x64748bff),
            accent: rgba_const(0x4f87ffff),
            accent_hover: rgba_const(0x6ba0ffff),
            accent_subtle: rgba_const(0x4f87ff24),
            success: rgba_const(0x10b981ff),
            success_subtle: rgba_const(0x10b98124),
            warning: rgba_const(0xf59e0bff),
            warning_subtle: rgba_const(0xf59e0b24),
            danger: rgba_const(0xef4444ff),
            danger_subtle: rgba_const(0xef444424),
            hover_overlay: rgba_const(0xffffff0a),
            active_overlay: rgba_const(0xffffff14),
            track_on: rgba_const(0x4f87ffff),
            track_off: rgba_const(0x222936ff),
            thumb: rgba_const(0xf1f5f9ff),
            modal_backdrop: rgba_const(0x000000d9),
            pill_bg: rgba_const(0x14181ff2),
            pill_border: rgba_const(0xffffff14),
        }
    }

    pub fn light() -> Self {
        Self {
            is_dark: false,
            bg: rgba_const(0xf8fafcff),
            sidebar_bg: rgba_const(0xf1f5f9ff),
            sidebar_border: rgba_const(0xe2e8f0ff),
            header_bg: rgba_const(0xffffffff),
            card_bg: rgba_const(0xffffffff),
            card_hover: rgba_const(0xf8fafcff),
            card_border: rgba_const(0xe2e8f0ff),
            card_border_hover: rgba_const(0xcbd5e1ff),
            row_hover: rgba_const(0xf1f5f9ff),
            row_selected: rgba_const(0xeff6ffff),
            table_header: rgba_const(0xf1f5f9ff),
            input_bg: rgba_const(0xffffffff),
            input_border: rgba_const(0xd1d5dbff),
            tab_bar_bg: rgba_const(0xe2e8f0ff),
            tab_active_bg: rgba_const(0xffffffff),
            text_primary: rgba_const(0x0f172aff),
            text_secondary: rgba_const(0x475569ff),
            text_muted: rgba_const(0x94a3b8ff),
            accent: rgba_const(0x2563ebff),
            accent_hover: rgba_const(0x1d4ed8ff),
            accent_subtle: rgba_const(0x2563eb18),
            success: rgba_const(0x10b981ff),
            success_subtle: rgba_const(0x10b98118),
            warning: rgba_const(0xd97706ff),
            warning_subtle: rgba_const(0xd9770618),
            danger: rgba_const(0xdc2626ff),
            danger_subtle: rgba_const(0xdc262618),
            hover_overlay: rgba_const(0x00000008),
            active_overlay: rgba_const(0x00000012),
            track_on: rgba_const(0x2563ebff),
            track_off: rgba_const(0xd1d5dbff),
            thumb: rgba_const(0xffffffff),
            modal_backdrop: rgba_const(0x00000080),
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
