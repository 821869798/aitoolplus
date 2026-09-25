//! Shared GPUI components: buttons, toggles, cards, badges, modals,
//! dropdowns, tabs, empty states. All are stateless render helpers bound to
//! listeners via `cx.listener`; state lives in the hosting view (GPUI rules
//! from flyclip's guidelines).

use gpui::{Animation, AnimationExt, ClickEvent, Context, IntoElement, Rgba, SharedString, Window, div, prelude::*, px};
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};

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
    on_click: impl Fn(&mut V, &ClickEvent, &mut Window, &mut Context<V>) + 'static,
) -> gpui::AnyElement {
    let t = theme.clone();
    let label: SharedString = label.into();
    let id = id.into();
    let on_click = std::rc::Rc::new(on_click);

    let base = div()
        .id(id)
        .cursor_pointer()
        .h(px(30.0))
        .px(px(12.0))
        .rounded(px(6.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .text_size(px(12.5))
        .font_weight(gpui::FontWeight::MEDIUM)
        .whitespace_nowrap();

    let styled = match variant {
        ButtonVariant::Primary => base
            .bg(t.accent)
            .text_color(WHITE)
            .shadow_xs()
            .hover(move |h| h.bg(t.accent_hover))
            .active(move |a| a.opacity(0.88)),
        ButtonVariant::Secondary => base
            .bg(t.tab_active_bg)
            .border_1()
            .border_color(t.card_border)
            .text_color(t.text_primary)
            .hover(move |h| h.bg(t.card_hover).border_color(t.card_border_hover))
            .active(move |a| a.bg(t.row_hover)),
        ButtonVariant::Ghost => base
            .text_color(t.text_secondary)
            .hover(move |h| h.bg(t.card_hover).text_color(t.text_primary))
            .active(move |a| a.bg(t.row_hover)),
        ButtonVariant::Outline => base
            .border_1()
            .border_color(t.card_border)
            .text_color(t.text_primary)
            .hover(move |h| h.bg(t.card_hover).border_color(t.card_border_hover))
            .active(move |a| a.bg(t.row_hover)),
        ButtonVariant::Danger => base
            .bg(t.danger_subtle)
            .border_1()
            .border_color(t.danger_subtle)
            .text_color(t.danger)
            .hover(move |h| h.bg(t.danger).text_color(WHITE).border_color(t.danger))
            .active(move |a| a.opacity(0.88)),
    };

    styled
        .child(label)
        .on_click(cx.listener(move |view, ev: &ClickEvent, window, cx| {
            on_click(view, ev, window, cx);
        }))
        .into_any_element()
}

pub fn button_with_icon_l<V: 'static>(
    id: impl Into<gpui::ElementId>,
    icon_svg: &'static [u8],
    label: impl Into<SharedString>,
    variant: ButtonVariant,
    theme: &Theme,
    cx: &mut Context<V>,
    on_click: impl Fn(&mut V, &ClickEvent, &mut Window, &mut Context<V>) + 'static,
) -> gpui::AnyElement {
    button_with_icon_loading_l(id, icon_svg, label, variant, false, theme, cx, on_click)
}

pub fn button_with_icon_loading_l<V: 'static>(
    id: impl Into<gpui::ElementId>,
    icon_svg: &'static [u8],
    label: impl Into<SharedString>,
    variant: ButtonVariant,
    loading: bool,
    theme: &Theme,
    cx: &mut Context<V>,
    on_click: impl Fn(&mut V, &ClickEvent, &mut Window, &mut Context<V>) + 'static,
) -> gpui::AnyElement {
    let t = theme.clone();
    let label: SharedString = label.into();
    let spin_id = SharedString::from(format!("{label}-spin"));
    let id = id.into();
    let on_click = std::rc::Rc::new(on_click);

    let icon_fg = match variant {
        ButtonVariant::Primary => WHITE,
        ButtonVariant::Danger => t.danger,
        _ => t.text_secondary,
    };

    let base = div()
        .id(id)
        .cursor_pointer()
        .h(px(30.0))
        .px(px(12.0))
        .rounded(px(6.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .text_size(px(12.5))
        .font_weight(gpui::FontWeight::MEDIUM)
        .whitespace_nowrap();

    let styled = match variant {
        ButtonVariant::Primary => base
            .bg(t.accent)
            .text_color(WHITE)
            .shadow_xs()
            .hover(move |h| h.bg(t.accent_hover))
            .active(move |a| a.opacity(0.88)),
        ButtonVariant::Secondary => base
            .bg(t.tab_active_bg)
            .border_1()
            .border_color(t.card_border)
            .text_color(t.text_primary)
            .hover(move |h| h.bg(t.card_hover).border_color(t.card_border_hover))
            .active(move |a| a.bg(t.row_hover)),
        ButtonVariant::Ghost => base
            .text_color(t.text_secondary)
            .hover(move |h| h.bg(t.card_hover).text_color(t.text_primary))
            .active(move |a| a.bg(t.row_hover)),
        ButtonVariant::Outline => base
            .border_1()
            .border_color(t.card_border)
            .text_color(t.text_primary)
            .hover(move |h| h.bg(t.card_hover).border_color(t.card_border_hover))
            .active(move |a| a.bg(t.row_hover)),
        ButtonVariant::Danger => base
            .bg(t.danger_subtle)
            .border_1()
            .border_color(t.danger_subtle)
            .text_color(t.danger)
            .hover(move |h| h.bg(t.danger).text_color(WHITE).border_color(t.danger))
            .active(move |a| a.opacity(0.88)),
    };

    styled
        .when(!loading, |this| {
            this.child(
                gpui::svg()
                    .data(icon_svg)
                    .size(px(14.0))
                    .text_color(icon_fg),
            )
        })
        .when(loading, |this| this.child(spinner(spin_id, icon_fg)))
        .child(label)
        .when(!loading, |this| {
            this.on_click(cx.listener(move |view, ev: &ClickEvent, window, cx| {
                on_click(view, ev, window, cx);
            }))
        })
        .into_any_element()
}

pub(crate) fn spinner(id: SharedString, color: Rgba) -> gpui::AnyElement {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    div()
        .w(px(14.0))
        .h(px(14.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(14.0))
        .text_color(color)
        .with_animation(
            id,
            Animation::new(std::time::Duration::from_millis(700))
                .repeat()
                .with_max_fps(12.0),
            move |this, delta| {
                let index = ((delta * FRAMES.len() as f32) as usize) % FRAMES.len();
                this.child(FRAMES[index])
            },
        )
        .into_any_element()
}

pub fn icon_button_svg<V: 'static>(
    id: impl Into<gpui::ElementId>,
    svg_data: &'static [u8],
    title: impl Into<SharedString>,
    danger: bool,
    theme: &Theme,
    cx: &mut Context<V>,
    on_click: impl Fn(&mut V, &ClickEvent, &mut Window, &mut Context<V>) + 'static,
) -> gpui::AnyElement {
    let t = theme.clone();
    let id = id.into();
    let on_click = std::rc::Rc::new(on_click);
    let title: SharedString = title.into();
    let fg = if danger { t.danger } else { t.text_secondary };

    div()
        .id(id)
        .cursor_pointer()
        .size(px(28.0))
        .rounded(px(6.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .text_color(fg)
        .tooltip(move |_window, cx| cx.new(|_| Tooltip::new(title.clone())).into())
        .when(danger, |this| {
            this.hover(move |h| h.bg(t.danger_subtle).text_color(t.danger))
                .active(move |a| a.opacity(0.8))
        })
        .when(!danger, |this| {
            this.hover(move |h| h.bg(t.card_hover).text_color(t.text_primary))
                .active(move |a| a.bg(t.row_hover))
        })
        .child(
            gpui::svg()
                .data(svg_data)
                .size(px(14.5))
                .text_color(fg),
        )
        .on_click(cx.listener(move |view, ev: &ClickEvent, window, cx| {
            on_click(view, ev, window, cx);
        }))
        .into_any_element()
}

pub fn icon_button_l<V: 'static>(
    id: impl Into<gpui::ElementId>,
    icon: impl Into<SharedString>,
    title: impl Into<SharedString>,
    danger: bool,
    theme: &Theme,
    cx: &mut Context<V>,
    on_click: impl Fn(&mut V, &ClickEvent, &mut Window, &mut Context<V>) + 'static,
) -> gpui::AnyElement {
    let t = theme.clone();
    let id = id.into();
    let on_click = std::rc::Rc::new(on_click);
    let title: SharedString = title.into();
    let icon: SharedString = icon.into();
    let fg = if danger { t.danger } else { t.text_secondary };

    div()
        .id(id)
        .cursor_pointer()
        .size(px(28.0))
        .rounded(px(6.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .text_size(px(13.0))
        .text_color(fg)
        .tooltip(move |_window, cx| cx.new(|_| Tooltip::new(title.clone())).into())
        .when(danger, |this| {
            this.hover(move |h| h.bg(t.danger_subtle).text_color(t.danger))
                .active(move |a| a.opacity(0.8))
        })
        .when(!danger, |this| {
            this.hover(move |h| h.bg(t.card_hover).text_color(t.text_primary))
                .active(move |a| a.bg(t.row_hover))
        })
        .child(icon)
        .on_click(cx.listener(move |view, ev: &ClickEvent, window, cx| {
            on_click(view, ev, window, cx);
        }))
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Tooltip (top overlay channel)
// ---------------------------------------------------------------------------

pub struct Tooltip {
    text: SharedString,
    max_w: Option<gpui::Pixels>,
}

impl Tooltip {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            max_w: None,
        }
    }

    pub fn with_max_width(text: impl Into<SharedString>, max_w: gpui::Pixels) -> Self {
        Self {
            text: text.into(),
            max_w: Some(max_w),
        }
    }
}

impl gpui::Render for Tooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let bg = crate::rgba_const(0x18181bee);
        let mut el = div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .bg(bg)
            .border_1()
            .border_color(crate::rgba_const(0xffffff1a))
            .text_size(px(11.5))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(WHITE)
            .shadow_md();

        if let Some(max_w) = self.max_w {
            el = el.max_w(max_w).whitespace_normal().line_height(gpui::relative(1.35));
        } else {
            el = el.whitespace_nowrap();
        }

        el.child(self.text.clone())
    }
}

// ---------------------------------------------------------------------------
// Universal Error Floating Tooltip & Banners
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ParsedGenericError {
    /// 1-line short summary suitable for badges and single-line strips.
    pub summary: String,
    /// Formatted multi-line detail for floating tooltips.
    pub detail: String,
    /// Extracted URL for actions (e.g. appeal link or docs).
    pub action_url: Option<String>,
    /// Raw unparsed error text.
    pub raw: String,
}

/// Parse any raw error string (JSON or plaintext) into a structured summary, detail, and optional link.
pub fn parse_generic_error(raw: &str) -> ParsedGenericError {
    let raw_trimmed = raw.trim();
    if raw_trimmed.is_empty() {
        return ParsedGenericError {
            summary: "未知错误".to_string(),
            detail: "未知错误，未返回具体错误内容。".to_string(),
            action_url: None,
            raw: String::new(),
        };
    }

    // 1. Try parsing as JSON (Google Cloud Quota, OpenAI, Anthropic, or standard JSON API)
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw_trimmed) {
        let mut reason_code = String::new();
        let mut message = String::new();
        let mut appeal_url = None;

        if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
            message = msg.to_string();
        } else if let Some(msg) = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
            message = msg.to_string();
        } else if let Some(msg) = v.get("error").and_then(|e| e.as_str()) {
            message = msg.to_string();
        }

        if let Some(status) = v.get("status").and_then(|s| s.as_str()) {
            reason_code = status.to_string();
        } else if let Some(status) = v.get("error").and_then(|e| e.get("status")).and_then(|s| s.as_str()) {
            reason_code = status.to_string();
        } else if let Some(code) = v.get("error").and_then(|e| e.get("code")).and_then(|c| c.as_str()) {
            reason_code = code.to_string();
        } else if let Some(code) = v.get("code").and_then(|c| c.as_str()) {
            reason_code = code.to_string();
        } else if let Some(code) = v.get("error").and_then(|e| e.get("type")).and_then(|t| t.as_str()) {
            reason_code = code.to_string();
        }

        let details = v.get("details")
            .or_else(|| v.get("error").and_then(|e| e.get("details")))
            .and_then(|d| d.as_array());
        if let Some(details) = details {
            for d in details {
                if let Some(r) = d.get("reason").and_then(|r| r.as_str()) {
                    if !r.is_empty() {
                        reason_code = r.to_string();
                    }
                }
                if let Some(meta) = d.get("metadata") {
                    if let Some(url) = meta.get("appeal_url").and_then(|u| u.as_str()) {
                        appeal_url = Some(url.to_string());
                    }
                }
            }
        }

        if appeal_url.is_none() {
            if let Some(url) = v.get("appeal_url").and_then(|u| u.as_str()) {
                appeal_url = Some(url.to_string());
            } else {
                appeal_url = extract_first_url(raw_trimmed);
            }
        }

        let summary = if reason_code == "TOS_VIOLATION" || message.contains("Terms of Service") {
            "违反服务条款被封禁 (TOS_VIOLATION)".to_string()
        } else if reason_code == "PERMISSION_DENIED" || message.contains("does not have permission") {
            "权限不足或凭据失效 (PERMISSION_DENIED)".to_string()
        } else if reason_code == "RESOURCE_EXHAUSTED" || message.contains("Quota exceeded") || message.contains("rate_limit_exceeded") {
            "配额已耗尽 (RESOURCE_EXHAUSTED)".to_string()
        } else if reason_code == "UNAUTHENTICATED" || message.contains("invalid_token") {
            "未认证或凭证无效 (UNAUTHENTICATED)".to_string()
        } else if !reason_code.is_empty() {
            if !message.is_empty() {
                format!("{reason_code}: {}", message.chars().take(36).collect::<String>())
            } else {
                format!("接口受限 ({reason_code})")
            }
        } else if !message.is_empty() {
            message.chars().take(40).collect()
        } else {
            "接口响应异常".to_string()
        };

        let mut detail = format!("【错误分类】{}", if reason_code.is_empty() { "API_ERROR" } else { &reason_code });
        if !message.is_empty() {
            detail.push_str(&format!("\n【详细说明】{message}"));
        }
        if let Some(url) = &appeal_url {
            detail.push_str(&format!("\n【官方链接】{url}"));
        }
        detail.push_str("\n\n(提示：点击错误条可复制完整错误信息)");

        return ParsedGenericError {
            summary,
            detail,
            action_url: appeal_url,
            raw: raw_trimmed.to_string(),
        };
    }

    // 2. Text error parsing
    let action_url = extract_first_url(raw_trimmed);
    let first_line = raw_trimmed.lines().next().unwrap_or("未知错误").trim();

    let summary = if raw_trimmed.contains("timed out") || raw_trimmed.contains("Timeout") || raw_trimmed.contains("DeadlineExceeded") {
        "网络请求超时 (Timeout)".to_string()
    } else if raw_trimmed.contains("Connection refused") || raw_trimmed.contains("Failed to connect") {
        "连接被拒绝，服务未启动或地址不可达".to_string()
    } else if raw_trimmed.contains("dns error") || raw_trimmed.contains("Could not resolve host") {
        "DNS 解析失败，请检查网络或域名".to_string()
    } else if raw_trimmed.contains("certificate") || raw_trimmed.contains("SSL") || raw_trimmed.contains("tls") {
        "SSL/TLS 证书验证失败".to_string()
    } else if raw_trimmed.contains("401") || raw_trimmed.contains("Unauthorized") {
        "身份认证失败 (401 Unauthorized)".to_string()
    } else if raw_trimmed.contains("403") || raw_trimmed.contains("Forbidden") {
        "访问被拒绝 (403 Forbidden)".to_string()
    } else if raw_trimmed.contains("404") || raw_trimmed.contains("Not Found") {
        "资源不存在 (404 Not Found)".to_string()
    } else if raw_trimmed.contains("429") || raw_trimmed.contains("Too Many Requests") {
        "请求过频 (429 Too Many Requests)".to_string()
    } else if raw_trimmed.contains("500") || raw_trimmed.contains("Internal Server Error") {
        "服务器内部错误 (500 Internal Error)".to_string()
    } else if raw_trimmed.contains("502") || raw_trimmed.contains("Bad Gateway") {
        "网关错误 (502 Bad Gateway)".to_string()
    } else if raw_trimmed.contains("503") || raw_trimmed.contains("Service Unavailable") {
        "服务暂时不可用 (503 Service Unavailable)".to_string()
    } else if raw_trimmed.contains("504") || raw_trimmed.contains("Gateway Timeout") {
        "网关超时 (504 Gateway Timeout)".to_string()
    } else {
        first_line.chars().take(42).collect::<String>()
    };

    let mut detail = format!("【错误摘要】{summary}\n【完整信息】\n{raw_trimmed}");
    if let Some(url) = &action_url {
        detail.push_str(&format!("\n\n【相关链接】{url}"));
    }
    detail.push_str("\n\n(提示：点击错误条可复制完整错误信息)");

    ParsedGenericError {
        summary,
        detail,
        action_url,
        raw: raw_trimmed.to_string(),
    }
}

fn extract_first_url(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        let clean = word.trim_matches(|c| {
            c == '(' || c == ')' || c == '[' || c == ']' || c == '{' || c == '}'
                || c == '<' || c == '>' || c == '"' || c == '\'' || c == ','
                || c == ';' || c == ':' || c == '.'
        });
        if clean.starts_with("https://") || clean.starts_with("http://") {
            return Some(clean.to_string());
        }
    }
    None
}

/// Compact inline error badge with warning icon and floating tooltip.
pub fn error_badge_tooltip(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    theme: &Theme,
) -> gpui::AnyElement {
    let t = theme.clone();
    let label: SharedString = label.into();
    let detail: SharedString = detail.into();

    div()
        .id(id.into())
        .h(px(22.0))
        .px(px(6.0))
        .rounded(px(4.0))
        .bg(t.danger_subtle)
        .border_1()
        .border_color(t.danger.opacity(0.35))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.0))
        .text_size(px(11.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(t.danger)
        .cursor_pointer()
        .overflow_hidden()
        .tooltip(move |_window, cx| {
            cx.new(|_| Tooltip::with_max_width(detail.clone(), px(380.0))).into()
        })
        .child(
            gpui::svg()
                .data(crate::icons::ALERT_SVG)
                .size(px(11.0))
                .text_color(t.danger)
                .flex_none(),
        )
        .child(
            div()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(label),
        )
        .into_any_element()
}

/// Single-line error strip with strict height limit (26px), truncation, floating tooltip on hover,
/// one-click copy of full error details, and optional action button.
pub fn error_strip<V: 'static>(
    id: impl Into<gpui::ElementId>,
    prefix: impl Into<SharedString>,
    raw_error: &str,
    theme: &Theme,
    cx: &mut Context<V>,
    action_button: Option<gpui::AnyElement>,
) -> gpui::AnyElement {
    error_strip_action(id, prefix, raw_error, theme, cx, action_button, |_view, _raw, _cx| {})
}

/// Single-line error strip with custom on_copy callback (e.g. for toast notifications).
pub fn error_strip_action<V: 'static>(
    id: impl Into<gpui::ElementId>,
    prefix: impl Into<SharedString>,
    raw_error: &str,
    theme: &Theme,
    cx: &mut Context<V>,
    action_button: Option<gpui::AnyElement>,
    on_copy: impl Fn(&mut V, &str, &mut Context<V>) + 'static,
) -> gpui::AnyElement {
    let t = theme.clone();
    let prefix: SharedString = prefix.into();
    let parsed = parse_generic_error(raw_error);
    let tooltip_detail = parsed.detail.clone();
    let raw_for_copy = parsed.raw.clone();
    let on_copy = std::rc::Rc::new(on_copy);

    let mut strip = div()
        .id(id.into())
        .h(px(26.0))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .px(px(8.0))
        .rounded(px(6.0))
        .bg(t.danger_subtle)
        .border_1()
        .border_color(t.danger.opacity(0.40))
        .overflow_hidden()
        .cursor_pointer()
        .hover(move |h| h.bg(t.danger.opacity(0.18)))
        .tooltip(move |_window, cx| {
            cx.new(|_| Tooltip::with_max_width(tooltip_detail.clone(), px(440.0))).into()
        })
        .on_click(cx.listener(move |view, _, _, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(raw_for_copy.clone()));
            on_copy(view, &raw_for_copy, cx);
        }))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .overflow_hidden()
                .child(
                    gpui::svg()
                        .data(crate::icons::ALERT_SVG)
                        .size(px(12.0))
                        .text_color(t.danger)
                        .flex_none(),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.danger)
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(format!("{}: {}", prefix, parsed.summary)),
                ),
        );

    if let Some(btn) = action_button {
        strip = strip.child(
            div()
                .id(gpui::ElementId::NamedInteger("err-strip-act".into(), 1))
                .flex_none()
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .child(btn),
        );
    }

    strip.into_any_element()
}

/// Helper button to open external links (e.g. appeal or documentation URL) from error components.
pub fn error_action_link_button<V: 'static>(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    url: String,
    theme: &Theme,
    cx: &mut Context<V>,
) -> gpui::AnyElement {
    let t = theme.clone();
    let label: SharedString = label.into();
    div()
        .id(id.into())
        .cursor_pointer()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(3.0))
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(4.0))
        .bg(t.danger.opacity(0.18))
        .hover(move |h| h.bg(t.danger.opacity(0.32)))
        .text_size(px(10.5))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(t.danger)
        .child(label)
        .on_click(cx.listener(move |_view, _, _, cx| {
            cx.open_url(&url);
        }))
        .into_any_element()
}

/// Block-level error banner for dialogs or details modals.
/// Displays structured error title, summary, action buttons (e.g. appeal or dismiss),
/// copy button, and a bounded monospace scrollable detail container.
pub fn error_banner<V: 'static>(
    id_prefix: impl Into<SharedString>,
    title: impl Into<SharedString>,
    raw_error: &str,
    theme: &Theme,
    cx: &mut Context<V>,
    action_button: Option<gpui::AnyElement>,
) -> gpui::AnyElement {
    let t = theme.clone();
    let id_str: SharedString = id_prefix.into();
    let title: SharedString = title.into();
    let parsed = parse_generic_error(raw_error);
    let raw_for_copy = parsed.raw.clone();
    let tooltip_detail = parsed.detail.clone();

    div()
        .id(gpui::ElementId::Name(format!("{id_str}-banner").into()))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(12.0))
        .rounded(px(8.0))
        .bg(t.danger_subtle)
        .border_1()
        .border_color(t.danger.opacity(0.45))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(t.danger)
                        .child(
                            gpui::svg()
                                .data(crate::icons::ALERT_SVG)
                                .size(px(14.0))
                                .text_color(t.danger)
                                .flex_none(),
                        )
                        .child(title),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .children(action_button)
                        .child(button_l(
                            format!("{id_str}-copy-btn"),
                            "复制完整错误",
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            move |_view, _, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(raw_for_copy.clone()));
                            },
                        )),
                ),
        )
        .child(
            div()
                .id(gpui::ElementId::Name(format!("{id_str}-status-row").into()))
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(12.0))
                .text_color(t.danger)
                .font_weight(gpui::FontWeight::MEDIUM)
                .tooltip(move |_window, cx| {
                    cx.new(|_| Tooltip::with_max_width(tooltip_detail.clone(), px(440.0))).into()
                })
                .child(format!("错误状态: {}", parsed.summary)),
        )
        .child(
            div()
                .id(gpui::ElementId::Name(format!("{id_str}-detail-scroll").into()))
                .max_h(px(110.0))
                .overflow_y_scroll()
                .p(px(8.0))
                .rounded(px(6.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(t.card_border)
                .text_size(px(11.0))
                .font_family("Consolas, monospace")
                .text_color(t.text_secondary)
                .child(parsed.detail),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Toggle switch
// ---------------------------------------------------------------------------

pub fn toggle<V: 'static>(
    id: impl Into<gpui::ElementId>,
    on: bool,
    theme: &Theme,
    cx: &mut Context<V>,
    on_toggle: impl Fn(&mut V, &ClickEvent, &mut Window, &mut Context<V>) + 'static,
) -> gpui::AnyElement {
    let t = theme.clone();
    let id = id.into();
    let on_toggle = std::rc::Rc::new(on_toggle);

    div()
        .id(id)
        .cursor_pointer()
        .w(px(38.0))
        .h(px(22.0))
        .p(px(2.5))
        .rounded_full()
        .flex()
        .flex_none()
        .items_center()
        .bg(if on { t.track_on } else { t.track_off })
        .hover(move |h| h.opacity(0.92))
        .active(move |a| a.opacity(0.85))
        .when(on, |s| s.justify_end())
        .when(!on, |s| s.justify_start())
        .child(
            div()
                .size(px(17.0))
                .rounded_full()
                .bg(t.thumb)
                .shadow_sm(),
        )
        .on_click(cx.listener(move |view, ev: &ClickEvent, window, cx| {
            on_toggle(view, ev, window, cx);
        }))
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
        .p(px(16.0))
        .rounded(px(10.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .shadow_xs();
    for (idx, row) in children.into_iter().enumerate() {
        if idx > 0 {
            builder = builder.child(
                gpui_kit::component::separator::Separator::horizontal()
                    .my(px(10.0))
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
                .text_size(px(14.5))
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
        .gap(px(4.0))
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
// Segmented Pill Selector & Settings Card Components
// ---------------------------------------------------------------------------

/// A modern segmented pill selector that supports optional SVG icons and high-contrast active state.
pub fn segmented_pill_selector<V: 'static, T: PartialEq + Copy + 'static>(
    id_prefix: &'static str,
    items: Vec<(T, Option<&'static [u8]>, SharedString)>,
    active_val: T,
    theme: &Theme,
    cx: &mut Context<V>,
    on_select: impl Fn(&mut V, T, &mut Window, &mut Context<V>) + 'static + Copy,
) -> gpui::AnyElement {
    let t = theme.clone();

    let mut row = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(3.0))
        .p(px(3.0))
        .rounded(px(8.0))
        .bg(t.tab_bar_bg)
        .border_1()
        .border_color(t.card_border);

    for (item_val, icon_svg, label) in items {
        let is_active = item_val == active_val;
        let tab_theme = t.clone();

        let mut pill = div()
            .id(gpui::ElementId::Name(format!("{}-pill-{}", id_prefix, label).into()))
            .cursor_pointer()
            .h(px(28.0))
            .px(px(12.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .text_size(px(12.0))
            .when(is_active, |s| {
                s.bg(tab_theme.tab_active_bg)
                    .border_1()
                    .border_color(if tab_theme.is_dark {
                        tab_theme.card_border_hover
                    } else {
                        tab_theme.card_border
                    })
                    .text_color(tab_theme.text_primary)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .shadow_xs()
            })
            .when(!is_active, |s| {
                s.border_1()
                    .border_color(gpui::transparent_black())
                    .text_color(tab_theme.text_secondary)
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .hover(move |h| {
                        h.text_color(tab_theme.text_primary)
                            .bg(tab_theme.card_hover)
                    })
            });

        if let Some(svg_data) = icon_svg {
            let icon_color = if is_active {
                tab_theme.accent
            } else {
                tab_theme.text_muted
            };
            pill = pill.child(
                gpui::svg()
                    .data(svg_data)
                    .size(px(13.0))
                    .text_color(icon_color),
            );
        }

        pill = pill.child(label).on_click(cx.listener(move |view, _ev: &gpui::ClickEvent, window, cx| {
            on_select(view, item_val, window, cx);
        }));

        row = row.child(pill);
    }

    row.into_any_element()
}

/// Settings section card with Sonora / pure-clash design language.
pub fn settings_card(
    theme: &Theme,
    title: impl Into<SharedString>,
    subtitle: Option<SharedString>,
    rows: Vec<gpui::AnyElement>,
) -> gpui::AnyElement {
    let t = theme.clone();
    let title: SharedString = title.into();

    let mut card_box = div()
        .flex()
        .flex_col()
        .w_full()
        .rounded(px(10.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .shadow_xs();

    for (idx, row) in rows.into_iter().enumerate() {
        if idx > 0 {
            card_box = card_box.child(
                div()
                    .w_full()
                    .h(px(1.0))
                    .bg(t.card_border),
            );
        }
        card_box = card_box.child(row);
    }

    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .w_full()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .px(px(4.0))
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.text_primary)
                        .child(title),
                )
                .when_some(subtitle, |s, sub| {
                    s.child(
                        div()
                            .text_size(px(11.5))
                            .text_color(t.text_muted)
                            .child(sub),
                    )
                }),
        )
        .child(card_box)
        .into_any_element()
}

/// Settings row: left title + subtitle, right interactive control.
pub fn settings_row(
    theme: &Theme,
    title: impl Into<SharedString>,
    subtitle: Option<SharedString>,
    control: gpui::AnyElement,
) -> gpui::AnyElement {
    let t = theme.clone();
    let title: SharedString = title.into();

    div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .min_h(px(52.0))
        .px(px(16.0))
        .py(px(12.0))
        .gap(px(16.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .flex_1()
                .min_w(px(0.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(title),
                )
                .when_some(subtitle, |s, sub| {
                    s.child(
                        div()
                            .text_size(px(11.5))
                            .text_color(t.text_muted)
                            .child(sub),
                    )
                }),
        )
        .child(control)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Segmented Tab Bar
// ---------------------------------------------------------------------------

/// A modern segmented pill tab bar that fits its content (like Sonora TabBar / Linear / Raycast).
pub fn segmented_tab_bar<V: 'static, T: PartialEq + Copy + 'static>(
    id_prefix: &'static str,
    tabs: Vec<(T, SharedString)>,
    active_tab: T,
    theme: &Theme,
    cx: &mut Context<V>,
    on_select: impl Fn(&mut V, T, &mut Window, &mut Context<V>) + 'static + Copy,
) -> gpui::AnyElement {
    segmented_tab_bar_with_dots(
        id_prefix,
        tabs.into_iter().map(|(t, l)| (t, l, false)).collect(),
        active_tab,
        theme,
        cx,
        on_select,
    )
}

/// Segmented tab bar with optional notification dot per tab.
pub fn segmented_tab_bar_with_dots<V: 'static, T: PartialEq + Copy + 'static>(
    id_prefix: &'static str,
    tabs: Vec<(T, SharedString, bool)>,
    active_tab: T,
    theme: &Theme,
    cx: &mut Context<V>,
    on_select: impl Fn(&mut V, T, &mut Window, &mut Context<V>) + 'static + Copy,
) -> gpui::AnyElement {
    let t = theme.clone();

    let mut row = div()
        .flex()
        .flex_none()
        .self_start()
        .items_center()
        .gap(px(3.0))
        .p(px(3.0))
        .rounded(px(8.0))
        .bg(t.tab_bar_bg)
        .border_1()
        .border_color(t.card_border);

    for (tab_val, label, show_dot) in tabs {
        let is_active = tab_val == active_tab;
        let tab_theme = t.clone();

        let tab_btn = div()
            .id(gpui::ElementId::Name(format!("{}-tab-{}", id_prefix, label).into()))
            .cursor_pointer()
            .h(px(28.0))
            .px(px(14.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .text_size(px(12.5))
            .when(is_active, |s| {
                s.bg(tab_theme.tab_active_bg)
                    .text_color(tab_theme.text_primary)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .shadow_xs()
            })
            .when(!is_active, |s| {
                s.text_color(tab_theme.text_secondary)
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .hover(move |h| {
                        h.text_color(tab_theme.text_primary)
                            .bg(tab_theme.card_hover)
                    })
            })
            .child(label)
            .when(show_dot, |s| {
                s.child(
                    div()
                        .w(px(6.5))
                        .h(px(6.5))
                        .rounded_full()
                        .bg(tab_theme.danger),
                )
            })
            .on_click(cx.listener(move |view, _ev: &gpui::ClickEvent, window, cx| {
                on_select(view, tab_val, window, cx);
            }));

        row = row.child(tab_btn);
    }

    row.into_any_element()
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

pub fn badge(_theme: &Theme, text: impl Into<SharedString>, kind: BadgeKind) -> gpui::AnyElement {
    let tag = match kind {
        BadgeKind::Success => gpui_kit::component::tag::Tag::success(),
        BadgeKind::Warning => gpui_kit::component::tag::Tag::warning(),
        BadgeKind::Danger => gpui_kit::component::tag::Tag::danger(),
        BadgeKind::Neutral => gpui_kit::component::tag::Tag::secondary(),
        BadgeKind::Accent => gpui_kit::component::tag::Tag::primary(),
    };
    tag.child(text.into()).into_any_element()
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

pub fn empty_state_svg(
    theme: &Theme,
    svg_data: &'static [u8],
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
) -> gpui::AnyElement {
    let t = theme.clone();
    let title: SharedString = title.into();
    let hint: SharedString = hint.into();
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(12.0))
        .py(px(48.0))
        .child(
            gpui::svg()
                .data(svg_data)
                .size(px(40.0))
                .text_color(crate::rgba_const(0xffffff28))
                .flex_none(),
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
        .flex()
        .items_center()
        .rounded(px(6.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.input_border)
        .shadow_xs()
        .overflow_hidden()
        .cursor_text()
        .hover(move |h| h.border_color(t.card_border_hover))
        .child(child)
}

pub fn textarea_container(theme: &Theme, child: impl IntoElement) -> gpui::Stateful<gpui::Div> {
    let t = theme.clone();
    div()
        .id("textarea-container")
        .w_full()
        .min_h(px(120.0))
        .max_h(px(320.0))
        .overflow_y_scroll()
        .rounded(px(6.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.input_border)
        .shadow_xs()
        .cursor_text()
        .hover(move |h| h.border_color(t.card_border_hover))
        .child(child)
}

/// A scrollable container for multi-line text areas with a vertical progress/scrollbar indicator.
/// When the text lines exceed the container height, the scrollbar automatically appears on the right edge.
pub fn text_area_scroll_container(
    wrap_id: impl Into<gpui::ElementId>,
    bar_id: impl Into<gpui::ElementId>,
    theme: &Theme,
    height: gpui::Pixels,
    scroll_handle: &gpui::ScrollHandle,
    focus_handle: &gpui::FocusHandle,
    child: impl IntoElement,
) -> gpui::Div {
    let t = theme.clone();
    let scrollbar = Scrollbar::vertical(scroll_handle)
        .id(bar_id)
        .mode(ScrollbarMode::Always);

    div()
        .relative()
        .w_full()
        .h(height)
        .rounded(px(6.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.input_border)
        .shadow_xs()
        .cursor_text()
        .track_focus(focus_handle)
        .focus(|s| s.border_color(crate::rgba_const(0x3b82f6cc)))
        .hover(move |h| h.border_color(t.card_border_hover))
        .child(
            div()
                .id(wrap_id)
                .size_full()
                .overflow_y_scroll()
                .track_scroll(scroll_handle)
                .child(child),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .child(scrollbar),
        )
}

// ---------------------------------------------------------------------------
// Menu dropdown. Trigger toggles from the state captured at press time.
// Clicking outside closes on the next frame, so an item click commits first.
// ---------------------------------------------------------------------------

pub struct MenuDrop<'a, 'v, V: 'static> {
    id: SharedString,
    open: bool,
    t: &'a Theme,
    cx: &'a mut Context<'v, V>,
    trigger: Option<gpui::AnyElement>,
    menu: Option<gpui::AnyElement>,
    align_end: bool,
    menu_w: Option<f32>,
}

impl<'a, 'v, V: 'static> MenuDrop<'a, 'v, V> {
    pub fn new(
        id: impl Into<SharedString>,
        open: bool,
        t: &'a Theme,
        cx: &'a mut Context<'v, V>,
    ) -> Self {
        Self {
            id: id.into(),
            open,
            t,
            cx,
            trigger: None,
            menu: None,
            align_end: false,
            menu_w: None,
        }
    }

    pub fn trigger(mut self, trigger: impl IntoElement) -> Self {
        self.trigger = Some(trigger.into_any_element());
        self
    }

    pub fn menu(mut self, menu: impl IntoElement) -> Self {
        self.menu = Some(menu.into_any_element());
        self
    }

    pub fn align_end(mut self, align_end: bool) -> Self {
        self.align_end = align_end;
        self
    }

    pub fn menu_width(mut self, width: f32) -> Self {
        self.menu_w = Some(width);
        self
    }

    pub fn render(
        self,
        on_toggle: impl Fn(&mut V, bool, &mut gpui::App) + 'static,
        on_close: impl Fn(&mut V, &mut gpui::App) + 'static,
    ) -> gpui::AnyElement {
        let open = self.open;
        let id = self.id.clone();
        let mut root = div().relative().child(
            div()
                .id(id)
                .cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, {
                    let entity = self.cx.entity().clone();
                    move |_ev, _, cx| {
                        cx.stop_propagation();
                        let _ = entity.update(cx, |view, cx| on_toggle(view, open, cx));
                        cx.notify(entity.entity_id());
                    }
                })
                .child(self.trigger.unwrap_or_else(|| div().into_any_element())),
        );
        if open {
            if let Some(menu) = self.menu {
                let entity = self.cx.entity().clone();
                let on_close = std::rc::Rc::new(on_close);
                let mut panel = div()
                    .id(SharedString::from(format!("{}-menu", self.id)))
                    .occlude()
                    .absolute()
                    .top(px(36.0))
                    .min_w(px(self.menu_w.unwrap_or(160.0)))
                    .bg(self.t.card_bg)
                    .border_1()
                    .border_color(self.t.card_border)
                    .rounded(px(6.0))
                    .shadow_xl()
                    .p(px(4.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .on_mouse_down_out({
                        let on_close = on_close.clone();
                        move |_ev, window, cx| {
                            let entity = entity.clone();
                            let on_close = on_close.clone();
                            window.defer(cx, move |window, cx| {
                                let _ = entity.update(cx, |view, cx| on_close(view, cx));
                                cx.notify(entity.entity_id());
                                window.refresh();
                            });
                        }
                    })
                    .child(menu);
                panel = if self.align_end {
                    panel.right_0()
                } else {
                    panel.left_0()
                };
                root = root.child(gpui::deferred(panel));
            }
        }
        root.into_any_element()
    }
}

// ---------------------------------------------------------------------------
// Fused Combobox (Searchable Select / Autocomplete)
// ---------------------------------------------------------------------------

/// A reusable fused combobox component (input field + dropdown options fused into one).
/// The trigger box itself is the editable text input with clear '✕' and toggle chevron '∨'/'∧'.
/// The floating options list uses `gpui::deferred` to overlay above all subsequent content
/// without pushing down elements below.
pub fn fused_combobox<V: 'static>(
    id: impl Into<SharedString>,
    label: Option<impl Into<SharedString>>,
    input_entity: gpui::Entity<crate::text_input::TextInput>,
    is_open: bool,
    is_typing: bool,
    options: Vec<String>,
    empty_hint: Option<impl Into<SharedString>>,
    theme: &Theme,
    cx: &mut Context<V>,
    on_open: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    on_close: impl Fn(&mut V, &mut Context<V>) + 'static,
    on_clear: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    on_select: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + 'static,
) -> gpui::AnyElement {
    let t = theme.clone();
    let id_str: SharedString = id.into();
    let current_text = input_entity.read(cx).text().trim().to_string();
    let has_val = !current_text.is_empty();

    let on_open = std::rc::Rc::new(on_open);
    let on_close = std::rc::Rc::new(on_close);
    let on_clear = std::rc::Rc::new(on_clear);
    let on_select = std::rc::Rc::new(on_select);

    let mut col = div()
        .id(SharedString::from(format!("{id_str}-col")))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .flex_1()
        .min_w(px(0.0))
        .relative();

    if let Some(lbl) = label {
        col = col.child(
            div()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(t.text_secondary)
                .child(lbl.into()),
        );
    }

    let on_open_click = on_open.clone();
    let on_close_click = on_close.clone();
    let mut trigger_box = div()
        .id(id_str.clone())
        .h(px(34.0))
        .w_full()
        .rounded(px(6.0))
        .border_1()
        .border_color(if is_open { t.accent } else { t.input_border })
        .bg(t.input_bg)
        .flex()
        .items_center()
        .justify_between()
        .hover(|h| h.border_color(if is_open { t.accent } else { t.card_border_hover }))
        .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |view, _, window, cx| {
            cx.stop_propagation();
            if is_open {
                on_close_click(view, cx);
            } else {
                on_open_click(view, window, cx);
            }
        }));

    let input_wrapper = div()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .flex()
        .items_center()
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .child(input_entity.clone());

    let mut right_icons = div()
        .flex()
        .items_center()
        .gap(px(2.0))
        .pr(px(6.0))
        .flex_none();

    if has_val {
        let on_clear_click = on_clear.clone();
        right_icons = right_icons.child(
            div()
                .id(SharedString::from(format!("{id_str}-clear-btn")))
                .cursor_pointer()
                .px(px(4.0))
                .py(px(2.0))
                .rounded(px(3.0))
                .text_size(px(12.0))
                .text_color(t.text_muted)
                .hover(|h| h.text_color(t.text_primary))
                .child("✕")
                .on_click(cx.listener(move |view, _, window, cx| {
                    cx.stop_propagation();
                    on_clear_click(view, window, cx);
                })),
        );
    }

    let on_open_chevron = on_open.clone();
    let on_close_chevron = on_close.clone();
    right_icons = right_icons.child(
        div()
            .id(SharedString::from(format!("{id_str}-chevron-btn")))
            .cursor_pointer()
            .px(px(4.0))
            .py(px(4.0))
            .flex()
            .items_center()
            .justify_center()
            .child(
                gpui::svg()
                    .data(if is_open {
                        crate::icons::CHEVRON_UP_SVG
                    } else {
                        crate::icons::CHEVRON_DOWN_SVG
                    })
                    .size(px(12.0))
                    .text_color(if is_open { t.accent } else { t.text_muted }),
            )
            .on_click(cx.listener(move |view, _, window, cx| {
                cx.stop_propagation();
                if is_open {
                    on_close_chevron(view, cx);
                } else {
                    on_open_chevron(view, window, cx);
                }
            })),
    );

    trigger_box = trigger_box.child(input_wrapper).child(right_icons);
    col = col.child(trigger_box);

    if is_open {
        let on_close_out = on_close.clone();
        let mut dropdown_menu = div()
            .id(SharedString::from(format!("{id_str}-menu")))
            .occlude()
            .absolute()
            .top(px(60.0))
            .left_0()
            .w_full()
            .rounded(px(8.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.accent)
            .shadow_xl()
            .p(px(6.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .on_mouse_down_out({
                let entity = cx.entity().clone();
                move |_ev, window, cx| {
                    let entity = entity.clone();
                    let on_close_out = on_close_out.clone();
                    window.defer(cx, move |window, cx| {
                        let _ = entity.update(cx, |view, cx| {
                            on_close_out(view, cx);
                        });
                        window.refresh();
                    });
                }
            });

        let filtered_options: Vec<String> = if !is_typing {
            options
        } else {
            let search_lower = current_text.to_lowercase();
            options
                .into_iter()
                .filter(|opt| search_lower.is_empty() || opt.to_lowercase().contains(&search_lower))
                .collect()
        };

        let mut list_container = div()
            .id(SharedString::from(format!("{id_str}-list")))
            .max_h(px(200.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.0));

        if filtered_options.is_empty() {
            let no_matches = empty_hint
                .map(|h| h.into())
                .unwrap_or_else(|| SharedString::from("无匹配项"));
            list_container = list_container.child(
                div()
                    .py(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.0))
                    .text_color(t.text_muted)
                    .child(no_matches),
            );
        } else {
            for (idx, opt) in filtered_options.into_iter().enumerate() {
                let is_sel = current_text == opt;
                let opt_for_click = opt.clone();
                let on_select_click = on_select.clone();
                let mut opt_row = div()
                    .id(SharedString::from(format!("{id_str}-opt-{idx}")))
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .text_color(if is_sel { t.accent } else { t.text_primary })
                    .bg(if is_sel { t.accent_subtle } else { t.card_bg })
                    .hover(|h| h.bg(t.row_hover))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .font_weight(if is_sel {
                                gpui::FontWeight::SEMIBOLD
                            } else {
                                gpui::FontWeight::NORMAL
                            })
                            .child(opt.clone()),
                    );

                if is_sel {
                    opt_row = opt_row.child(
                        gpui::svg()
                            .data(crate::icons::CHECK_SVG)
                            .size(px(12.0))
                            .text_color(t.accent)
                            .flex_none(),
                    );
                }

                opt_row = opt_row.on_click(cx.listener(move |view, _, window, cx| {
                    on_select_click(view, opt_for_click.clone(), window, cx);
                }));

                list_container = list_container.child(opt_row);
            }
        }

        dropdown_menu = dropdown_menu.child(list_container);
        col = col.child(gpui::deferred(dropdown_menu));
    }

    col.into_any_element()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_google_cloud_tos_violation() {
        let json_err = r#"{
            "error": {
                "code": 403,
                "message": "User is prohibited from accessing the service due to Terms of Service violations.",
                "status": "PERMISSION_DENIED",
                "details": [
                    {
                        "@type": "type.googleapis.com/google.rpc.ErrorInfo",
                        "reason": "TOS_VIOLATION",
                        "domain": "googleapis.com",
                        "metadata": {
                            "appeal_url": "https://support.google.com/accounts/contact/suspended"
                        }
                    }
                ]
            }
        }"#;
        let parsed = parse_generic_error(json_err);
        assert!(parsed.summary.contains("TOS_VIOLATION"));
        assert!(parsed.detail.contains("TOS_VIOLATION") || parsed.detail.contains("API_ERROR"));
        assert_eq!(
            parsed.action_url.as_deref(),
            Some("https://support.google.com/accounts/contact/suspended")
        );
    }

    #[test]
    fn test_parse_google_cloud_permission_denied() {
        let json_err = r#"{
            "error": {
                "code": 403,
                "message": "The caller does not have permission",
                "status": "PERMISSION_DENIED"
            }
        }"#;
        let parsed = parse_generic_error(json_err);
        assert!(parsed.summary.contains("PERMISSION_DENIED"));
        assert!(parsed.action_url.is_none());
    }

    #[test]
    fn test_parse_openai_error() {
        let json_err = r#"{
            "error": {
                "message": "You exceeded your current quota, please check your plan and billing details.",
                "type": "insufficient_quota",
                "code": "insufficient_quota"
            }
        }"#;
        let parsed = parse_generic_error(json_err);
        assert!(parsed.summary.contains("RESOURCE_EXHAUSTED") || parsed.summary.contains("quota"));
        assert!(parsed.detail.contains("insufficient_quota"));
    }

    #[test]
    fn test_parse_plaintext_timeout() {
        let err = "error sending request for url (https://api.openai.com/v1/models): operation timed out after 30000ms";
        let parsed = parse_generic_error(err);
        assert!(parsed.summary.contains("超时") || parsed.summary.contains("Timeout"));
        assert_eq!(parsed.action_url.as_deref(), Some("https://api.openai.com/v1/models"));
    }

    #[test]
    fn test_parse_plaintext_connection_refused() {
        let err = "Failed to connect to 127.0.0.1:11434: Connection refused";
        let parsed = parse_generic_error(err);
        assert!(parsed.summary.contains("连接被拒绝"));
    }

    #[test]
    fn test_parse_empty_error() {
        let parsed = parse_generic_error("");
        assert_eq!(parsed.summary, "未知错误");
    }
}

