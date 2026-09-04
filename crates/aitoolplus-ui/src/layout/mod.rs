//! Sidebar + top bar for the main window.

use gpui::{Context, IntoElement, SharedString, div, prelude::*, px};

use crate::components::Tooltip;
use crate::pages::Page;
use crate::workspace::Workspace;

impl Workspace {
    /// Left navigation: workbench sections + tools + settings.
    pub(crate) fn sidebar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = &self.theme;
        let i = &self.i18n;

        let mut col = div()
            .w(px(170.0))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(t.sidebar_bg)
            .border_r_1()
            .border_color(t.card_border)
            .pt(px(6.0))
            .pb(px(6.0));

        col = col.child(self.sidebar_group(i.t("编码工具", "Coding Tools")));

        for tool in aitoolplus_core::ToolId::ALL {
            if !self.settings.visible_tools.is_empty()
                && !self
                    .settings
                    .visible_tools
                    .iter()
                    .any(|key| key == tool.key())
            {
                continue;
            }
            let page = Page::Tool(tool);
            col = col.child(self.sidebar_item(cx, page));
        }

        col = col.child(self.sidebar_group(i.t("共享资源", "Shared")));
        col = col.child(self.sidebar_item(cx, Page::Mcp));
        col = col.child(self.sidebar_item(cx, Page::Skills));
        col = col.child(self.sidebar_item(cx, Page::Sessions));

        col = col.child(self.sidebar_group(i.t("设置", "Settings")));
        col = col.child(self.sidebar_item(cx, Page::Settings));

        col.into_any_element()
    }

    fn sidebar_group(&self, label: SharedString) -> impl IntoElement {
        let t = &self.theme;
        div()
            .px(px(14.0))
            .pt(px(12.0))
            .pb(px(4.0))
            .text_size(px(10.5))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(t.text_muted)
            .child(label)
    }

    fn sidebar_item(&self, cx: &mut Context<Self>, page: Page) -> impl IntoElement {
        let t = &self.theme;
        let i = &self.i18n;
        let is_active = self.page == page;

        let (icon, label): (&'static str, SharedString) = match page {
            Page::Tool(tool) => ("◈", i.t(tool.name_zh(), tool.name_en())),
            Page::Mcp => ("⬡", i.t("MCP 服务器", "MCP Servers")),
            Page::Skills => ("✦", i.t("Skills 技能", "Skills")),
            Page::Sessions => ("☰", i.t("会话管理", "Sessions")),
            Page::Settings => ("⚙", i.t("设置", "Settings")),
        };

        div()
            .id(gpui::ElementId::Name(
                format!("sidebar-item-{}", page.key()).into(),
            ))
            .cursor_pointer()
            .flex()
            .items_center()
            .gap(px(8.0))
            .mx(px(6.0))
            .my(px(1.0))
            .px(px(8.0))
            .h(px(28.0))
            .rounded(px(6.0))
            .when(is_active, |s| {
                s.bg(t.row_selected)
                    .text_color(t.accent)
                    .font_weight(gpui::FontWeight::MEDIUM)
            })
            .when(!is_active, |s| s.text_color(t.text_secondary))
            .hover(|h| if is_active { h } else { h.bg(t.row_hover) })
            .on_click(
                cx.listener(move |this, _ev: &gpui::ClickEvent, _window, cx| {
                    this.navigate(page, cx);
                }),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .w(px(18.0))
                    .flex_shrink_0()
                    .when(is_active, |s| s.text_color(t.accent))
                    .when(!is_active, |s| s.text_color(t.text_muted))
                    .child(icon),
            )
            .child(div().text_size(px(13.0)).child(label).into_any_element())
            .into_any_element()
    }

    /// Top bar: app title + status summary + theme/language quick switches.
    pub(crate) fn topbar(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = self.theme.clone();
        let i = self.i18n;
        let theme_mode = self.settings.theme_mode;
        let lang = self.settings.language;

        let is_dark_now = t.is_dark;
        let _ = is_dark_now;

        div()
            .h(px(44.0))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .px(px(16.0))
            .bg(t.sidebar_bg)
            .border_b_1()
            .border_color(t.card_border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(t.text_primary)
                            .child(i.t("AI ToolPlus", "AI ToolPlus")),
                    )
                    .child(
                        div().text_size(px(11.0)).text_color(t.text_muted).child(
                            i.t("AI 编程助手配置工作台", "Coding assistant config workbench"),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .id("theme-btn")
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .h(px(28.0))
                            .px(px(8.0))
                            .rounded(px(6.0))
                            .text_size(px(13.0))
                            .text_color(t.text_secondary)
                            .hover(|h| h.bg(t.row_hover).text_color(t.text_primary))
                            .tooltip(move |_w, cx| {
                                let label = match theme_mode {
                                    aitoolplus_core::settings::ThemeMode::Dark => "暗色 / Dark",
                                    aitoolplus_core::settings::ThemeMode::Light => "亮色 / Light",
                                    aitoolplus_core::settings::ThemeMode::System => {
                                        "跟随系统 / System"
                                    }
                                };
                                cx.new(|_| Tooltip::new(label)).into()
                            })
                            .on_click(cx.listener(|this, _ev: &gpui::ClickEvent, _w, cx| {
                                this.cycle_theme(cx);
                            }))
                            .child(match theme_mode {
                                aitoolplus_core::settings::ThemeMode::Dark => "🌙",
                                aitoolplus_core::settings::ThemeMode::Light => "☀",
                                aitoolplus_core::settings::ThemeMode::System => "◐",
                            }),
                    )
                    .child(
                        div()
                            .id("lang-btn")
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .h(px(28.0))
                            .px(px(8.0))
                            .rounded(px(6.0))
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.text_secondary)
                            .hover(|h| h.bg(t.row_hover).text_color(t.text_primary))
                            .tooltip(move |_w, cx| {
                                let label = match lang {
                                    aitoolplus_core::settings::Language::Zh => "中文",
                                    aitoolplus_core::settings::Language::En => "English",
                                    aitoolplus_core::settings::Language::System => {
                                        "跟随系统 / System"
                                    }
                                };
                                cx.new(|_| Tooltip::new(label)).into()
                            })
                            .on_click(cx.listener(|this, _ev: &gpui::ClickEvent, _w, cx| {
                                this.cycle_language(cx);
                            }))
                            .child(match lang {
                                aitoolplus_core::settings::Language::Zh => "中",
                                aitoolplus_core::settings::Language::En => "EN",
                                aitoolplus_core::settings::Language::System => "AUTO",
                            }),
                    ),
            )
            .into_any_element()
    }
}
