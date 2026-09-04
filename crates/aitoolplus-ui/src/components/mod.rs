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
    let (bg, bg_hover, fg, border) = match variant {
        ButtonVariant::Primary => (t.accent, t.accent_hover, WHITE, None),
        ButtonVariant::Secondary => (t.card_bg, t.card_hover, t.text_primary, Some(t.card_border)),
        ButtonVariant::Danger => (t.danger, t.danger, WHITE, None),
        ButtonVariant::Ghost => (t.bg, t.row_hover, t.text_secondary, None),
    };
    let is_ghost = variant == ButtonVariant::Ghost;

    div()
        .id(id.into())
        .cursor_pointer()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(30.0))
        .px(px(14.0))
        .rounded(px(6.0))
        .text_size(px(13.0))
        .when(!is_ghost, |s| s.bg(bg))
        .map(move |s| match border {
            Some(b) => s.border_1().border_color(b),
            None => s,
        })
        .text_color(fg)
        .font_weight(gpui::FontWeight::MEDIUM)
        .hover(move |h| {
            let mut h2 = h.bg(bg_hover);
            if is_ghost {
                h2 = h2.text_color(t.text_primary);
            }
            h2
        })
        .active(move |a| a.bg(t.active_overlay))
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
    let hover_color = if danger { t.danger_subtle } else { t.row_hover };
    let text_color = if danger { t.danger } else { t.text_secondary };

    div()
        .id(id.into())
        .cursor_pointer()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(28.0))
        .rounded(px(6.0))
        .text_size(px(14.0))
        .text_color(text_color)
        .hover(move |h| {
            h.bg(hover_color)
                .text_color(if danger { t.danger } else { t.text_primary })
        })
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
        let bg = crate::rgba_const(0x1a1a1cf2);
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .bg(bg)
            .text_size(px(11.5))
            .text_color(WHITE)
            .shadow_lg()
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
    let track = if on { t.track_on } else { t.track_off };
    let knob_color = t.thumb;
    let knob_left = if on { px(20.0) } else { px(2.0) };

    div()
        .id(id.into())
        .cursor_pointer()
        .relative()
        .flex_shrink_0()
        .h(px(20.0))
        .w(px(38.0))
        .rounded(px(10.0))
        .bg(track)
        .child(
            div()
                .absolute()
                .top(px(2.0))
                .left(knob_left)
                .size(px(16.0))
                .rounded(px(8.0))
                .bg(knob_color)
                .shadow_sm(),
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
        .p(px(16.0))
        .rounded(px(10.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border);
    for (idx, row) in children.into_iter().enumerate() {
        if idx > 0 {
            builder = builder.child(
                div()
                    .my(px(10.0))
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
                    .text_color(t.text_secondary)
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
                .text_size(px(20.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(t.text_primary)
                .child(title),
        )
        .child(
            div()
                .text_size(px(13.0))
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
    let (fg, bg) = match kind {
        BadgeKind::Success => (t.success, t.success_subtle),
        BadgeKind::Warning => (t.warning, t.warning_subtle),
        BadgeKind::Danger => (t.danger, t.danger_subtle),
        BadgeKind::Neutral => (t.text_secondary, t.hover_overlay),
        BadgeKind::Accent => (t.accent, t.accent_subtle),
    };
    let text: SharedString = text.into();
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .px(px(8.0))
        .h(px(22.0))
        .rounded(px(11.0))
        .bg(bg)
        .child(
            div()
                .size(px(6.0))
                .rounded(px(3.0))
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
