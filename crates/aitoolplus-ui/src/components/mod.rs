//! Shared GPUI components: buttons, toggles, cards, badges, modals,
//! dropdowns, tabs, empty states. All are stateless render helpers bound to
//! listeners via `cx.listener`; state lives in the hosting view (GPUI rules
//! from flyclip's guidelines).

use gpui::{Context, IntoElement, MouseButton, Rgba, SharedString, Window, div, prelude::*, px};

use crate::theme::Theme;

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Danger,
    Ghost,
    Outline,
}

const WHITE: Rgba = crate::rgba_const(0xffffffff);

/// View-bound button. `on_click` receives (view, event, window, cx).
pub fn button_l<V: 'static>(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    variant: ButtonVariant,
    theme: &Theme,
    cx: &mut Context<V>,
    on_click: impl Fn(&mut V, &gpui::MouseDownEvent, &mut Window, &mut Context<V>) + 'static,
) -> gpui::AnyElement {
    let t = theme.clone();
    let label: SharedString = label.into();

    let (bg, bg_hover, fg, border, has_shadow) = match variant {
        ButtonVariant::Primary => (
            t.accent,
            t.accent_hover,
            WHITE,
            Some(crate::rgba_const(0xffffff26)),
            true,
        ),
        ButtonVariant::Secondary => (
            t.card_bg,
            t.card_hover,
            t.text_primary,
            Some(t.card_border),
            true,
        ),
        ButtonVariant::Danger => (
            t.danger_subtle,
            t.danger,
            t.danger,
            Some(crate::rgba_const(0xf8717140)),
            false,
        ),
        ButtonVariant::Ghost => (
            crate::rgba_const(0x00000000),
            t.row_hover,
            t.text_secondary,
            None,
            false,
        ),
        ButtonVariant::Outline => (
            crate::rgba_const(0x00000000),
            t.row_hover,
            t.text_primary,
            Some(t.card_border),
            false,
        ),
    };

    let is_danger = variant == ButtonVariant::Danger;
    let is_ghost = variant == ButtonVariant::Ghost;

    div()
        .id(id.into())
        .cursor_pointer()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(28.0))
        .px(px(12.0))
        .rounded(px(6.0))
        .text_size(px(12.5))
        .font_weight(gpui::FontWeight::MEDIUM)
        .bg(bg)
        .text_color(fg)
        .when_some(border, |s, b| s.border_1().border_color(b))
        .when(has_shadow, |s| s.shadow_xs())
        .hover(move |h| {
            let mut h = h.bg(bg_hover);
            if is_danger {
                h = h
                    .text_color(WHITE)
                    .border_color(t.danger)
                    .shadow_xs();
            } else if is_ghost {
                h = h.text_color(t.text_primary);
            } else if variant == ButtonVariant::Secondary || variant == ButtonVariant::Outline {
                h = h.border_color(t.card_border_hover);
            }
            h
        })
        .active(move |a| a.opacity(0.85))
        .on_mouse_down(MouseButton::Left, cx.listener(on_click))
        .child(label)
        .into_any_element()
}

/// Small square icon/emoji button (28px).
pub fn icon_button_l<V: 'static>(
    id: impl Into<gpui::ElementId>,
    icon: impl Into<SharedString>,
    title: impl Into<SharedString>,
    danger: bool,
    theme: &Theme,
    cx: &mut Context<V>,
    on_click: impl Fn(&mut V, &gpui::MouseDownEvent, &mut Window, &mut Context<V>) + 'static,
) -> gpui::AnyElement {
    let t = theme.clone();
    let icon: SharedString = icon.into();
    let title: SharedString = title.into();
    let hover_bg = if danger { t.danger_subtle } else { t.row_hover };
    let text_color = if danger { t.danger } else { t.text_muted };
    let hover_text = if danger { t.danger } else { t.text_primary };

    div()
        .id(id.into())
        .cursor_pointer()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(28.0))
        .rounded(px(6.0))
        .text_size(px(13.5))
        .text_color(text_color)
        .hover(move |h| h.bg(hover_bg).text_color(hover_text))
        .active(|a| a.opacity(0.8))
        .tooltip(move |_window, cx| {
            let tip = title.clone();
            cx.new(|_| Tooltip::new(tip)).into()
        })
        .on_mouse_down(MouseButton::Left, cx.listener(on_click))
        .child(icon)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Tooltip (top overlay channel)
// ---------------------------------------------------------------------------

pub struct Tooltip {
    text: SharedString,
}

impl Tooltip {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into() }
    }
}

impl gpui::Render for Tooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let bg = crate::rgba_const(0x18181be6);
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .bg(bg)
            .border_1()
            .border_color(crate::rgba_const(0xffffff1a))
            .text_size(px(11.5))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(WHITE)
            .shadow_md()
            .whitespace_nowrap()
            .child(self.text.clone())
    }
}

// ---------------------------------------------------------------------------
// Toggle switch
// ---------------------------------------------------------------------------

pub fn toggle<V: 'static>(
    id: impl Into<gpui::ElementId>,
    on: bool,
    theme: &Theme,
    cx: &mut Context<V>,
    on_toggle: impl Fn(&mut V, &gpui::MouseDownEvent, &mut Window, &mut Context<V>) + 'static,
) -> gpui::AnyElement {
    let t = theme.clone();
    let (track_bg, track_border) = if on {
        (t.accent, crate::rgba_const(0xffffff22))
    } else {
        (t.track_off, t.card_border)
    };
    let knob_left = if on { px(18.0) } else { px(2.0) };

    div()
        .id(id.into())
        .cursor_pointer()
        .relative()
        .flex_shrink_0()
        .h(px(20.0))
        .w(px(36.0))
        .rounded(px(10.0))
        .bg(track_bg)
        .border_1()
        .border_color(track_border)
        .hover(|h| h.opacity(0.92))
        .child(
            div()
                .absolute()
                .top(px(1.0))
                .left(knob_left)
                .size(px(16.0))
                .rounded(px(8.0))
                .bg(WHITE)
                .shadow_xs(),
        )
        .on_mouse_down(MouseButton::Left, cx.listener(on_toggle))
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Card / section / page header
// ---------------------------------------------------------------------------

pub fn card(theme: &Theme, children: Vec<gpui::AnyElement>) -> gpui::AnyElement {
    let t = theme.clone();
    let mut builder = div()
        .flex()
        .flex_col()
        .w_full()
        .p(px(14.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .shadow_xs();
    for (idx, row) in children.into_iter().enumerate() {
        if idx > 0 {
            builder = builder.child(
                div()
                    .my(px(8.0))
                    .h(px(1.0))
                    .w_full()
                    .bg(t.card_border)
                    .into_any_element(),
            );
        }
        builder = builder.child(row);
    }
    builder.into_any_element()
}

pub fn section_title(
    theme: &Theme,
    title: impl Into<SharedString>,
    subtitle: Option<SharedString>,
) -> gpui::AnyElement {
    let t = theme.clone();
    let title: SharedString = title.into();
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(t.text_primary)
                .child(title),
        )
        .when_some(subtitle, |s, sib| {
            s.child(
                div()
                    .text_size(px(12.0))
                    .text_color(t.text_muted)
                    .child(sib),
            )
        })
        .into_any_element()
}

pub fn page_header(
    theme: &Theme,
    title: impl Into<SharedString>,
    subtitle: impl Into<SharedString>,
) -> gpui::AnyElement {
    let t = theme.clone();
    let title: SharedString = title.into();
    let subtitle: SharedString = subtitle.into();
    div()
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(
            div()
                .text_size(px(18.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(t.text_primary)
                .child(title),
        )
        .child(
            div()
                .text_size(px(12.5))
                .text_color(t.text_secondary)
                .child(subtitle),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Badge / status pill
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    Success,
    Warning,
    Danger,
    Neutral,
    Accent,
}

pub fn badge(theme: &Theme, text: impl Into<SharedString>, kind: BadgeKind) -> gpui::AnyElement {
    let t = theme.clone();
    let (fg, bg, border) = match kind {
        BadgeKind::Success => (t.success, t.success_subtle, crate::rgba_const(0x34d39940)),
        BadgeKind::Warning => (t.warning, t.warning_subtle, crate::rgba_const(0xfbbf2440)),
        BadgeKind::Danger => (t.danger, t.danger_subtle, crate::rgba_const(0xf8717140)),
        BadgeKind::Neutral => (t.text_secondary, t.hover_overlay, t.card_border),
        BadgeKind::Accent => (t.accent, t.accent_subtle, crate::rgba_const(0x4c8dff40)),
    };
    let text: SharedString = text.into();
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(5.0))
        .px(px(8.0))
        .h(px(20.0))
        .rounded(px(10.0))
        .bg(bg)
        .border_1()
        .border_color(border)
        .child(
            div()
                .size(px(5.0))
                .rounded(px(2.5))
                .bg(fg)
                .into_any_element(),
        )
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(fg)
                .child(text),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Empty state
// ---------------------------------------------------------------------------

pub fn empty_state(
    theme: &Theme,
    icon: &str,
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
) -> gpui::AnyElement {
    let t = theme.clone();
    let title: SharedString = title.into();
    let hint: SharedString = hint.into();
    let icon: SharedString = icon.into();
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(10.0))
        .py(px(48.0))
        .child(
            div()
                .text_size(px(36.0))
                .text_color(t.text_muted)
                .child(icon),
        )
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(t.text_secondary)
                .child(title),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(t.text_muted)
                .child(hint),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Input & TextArea Containers
// ---------------------------------------------------------------------------

pub fn input_container(theme: &Theme, child: impl IntoElement) -> gpui::Div {
    let t = theme.clone();
    div()
        .w_full()
        .h(px(32.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .rounded(px(6.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.input_border)
        .shadow_xs()
        .hover(move |h| h.border_color(t.card_border_hover))
        .child(child)
}

pub fn textarea_container(theme: &Theme, child: impl IntoElement) -> gpui::Div {
    let t = theme.clone();
    div()
        .w_full()
        .p(px(10.0))
        .rounded(px(6.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.input_border)
        .shadow_xs()
        .hover(move |h| h.border_color(t.card_border_hover))
        .child(child)
}
