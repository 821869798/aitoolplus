//! Left sidebar navigation + top contextual bar for the main window.
//! Architectural layout matching Sonora, Pure-Clash and NavOp.

use gpui::{Context, IntoElement, SharedString, div, prelude::*, px};

use crate::pages::Page;
use crate::workspace::Workspace;

impl Workspace {
    /// Left sidebar: brand header + coding tools + shared resources + docked settings.
    pub(crate) fn sidebar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = &self.theme;
        let i = &self.i18n;

        // 1. Top Brand Header
        let brand = div()
            .id("sidebar-brand")
            .h(px(54.0))
            .px(px(16.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .border_b_1()
            .border_color(t.sidebar_border)
            .child(
                div()
                    .size(px(28.0))
                    .rounded(px(8.0))
                    .bg(t.accent)
                    .flex()
                    .items_center()
                    .justify_center()
                    .shadow_xs()
                    .child(
                        gpui::svg()
                            .data(crate::icons::SPARKLES_SVG)
                            .size(px(15.0))
                            .text_color(crate::rgba_const(0xffffffff))
                            .flex_none(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(t.text_primary)
                            .child("AI ToolPlus"),
                    )
                    .child(
                        div()
                            .px(px(5.0))
                            .py(px(1.5))
                            .rounded(px(4.0))
                            .bg(t.accent_subtle)
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(t.accent)
                            .child("PRO"),
                    ),
            );

        // 2. Scrollable Navigation List
        let mut nav_list = div()
            .id("sidebar-nav")
            .flex_1()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .py(px(6.0));

        nav_list = nav_list.child(self.sidebar_group(i.t("编码工具", "Coding Tools")));

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
            nav_list = nav_list.child(self.sidebar_item(cx, page));
        }

        nav_list = nav_list.child(self.sidebar_group(i.t("共享资源", "Shared Resources")));
        nav_list = nav_list.child(self.sidebar_item(cx, Page::Mcp));
        nav_list = nav_list.child(self.sidebar_item(cx, Page::Skills));
        nav_list = nav_list.child(self.sidebar_item(cx, Page::Sessions));

        // 3. Docked Settings at Bottom
        let footer = div()
            .p(px(8.0))
            .border_t_1()
            .border_color(t.sidebar_border)
            .child(self.sidebar_item(cx, Page::Settings));

        div()
            .id("sidebar")
            .w(px(220.0))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(t.sidebar_bg)
            .border_r_1()
            .border_color(t.sidebar_border)
            .child(brand)
            .child(nav_list)
            .child(footer)
            .into_any_element()
    }

    fn sidebar_group(&self, label: SharedString) -> impl IntoElement {
        let t = &self.theme;
        div()
            .px(px(16.0))
            .pt(px(14.0))
            .pb(px(4.0))
            .text_size(px(11.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(t.text_muted)
            .child(label)
    }

    fn sidebar_item(&self, cx: &mut Context<Self>, page: Page) -> impl IntoElement {
        let t = &self.theme;
        let i = &self.i18n;
        let is_active = self.page == page;

        let (icon_svg, label): (&'static [u8], SharedString) = match page {
            Page::Tool(tool) => match tool {
                aitoolplus_core::ToolId::ClaudeCode | aitoolplus_core::ToolId::ClaudeDesktop => {
                    (crate::icons::CLAUDE_SVG, i.t(tool.name_zh(), tool.name_en()))
                }
                aitoolplus_core::ToolId::Codex => {
                    (crate::icons::OPENAI_SVG, i.t(tool.name_zh(), tool.name_en()))
                }
                aitoolplus_core::ToolId::Grok => {
                    (crate::icons::GROK_SVG, i.t(tool.name_zh(), tool.name_en()))
                }
                aitoolplus_core::ToolId::Kimi => {
                    (crate::icons::KIMI_SVG, i.t(tool.name_zh(), tool.name_en()))
                }
                _ => (crate::icons::TERMINAL_SVG, i.t(tool.name_zh(), tool.name_en())),
            },
            Page::Mcp => (crate::icons::MCP_SVG, i.t("MCP 服务器", "MCP Servers")),
            Page::Skills => (crate::icons::SPARKLES_SVG, i.t("Skills 技能", "Skills")),
            Page::Sessions => (crate::icons::HISTORY_SVG, i.t("会话管理", "Sessions")),
            Page::Settings => (crate::icons::SETTINGS_SVG, i.t("系统设置", "Settings")),
        };

        div()
            .id(gpui::ElementId::Name(
                format!("sidebar-item-{}", page.key()).into(),
            ))
            .cursor_pointer()
            .flex()
            .items_center()
            .gap(px(10.0))
            .mx(px(8.0))
            .my(px(1.5))
            .px(px(10.0))
            .h(px(36.0))
            .rounded(px(8.0))
            .when(is_active, |s| {
                s.bg(t.accent_subtle)
                    .text_color(t.accent)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
            })
            .when(!is_active, |s| s.text_color(t.text_secondary))
            .hover(|h| {
                if is_active {
                    h
                } else {
                    h.bg(t.card_hover).text_color(t.text_primary)
                }
            })
            .on_click(cx.listener(move |this, _ev: &gpui::ClickEvent, _window, cx| {
                this.navigate(page, cx);
            }))
            .child(
                div()
                    .size(px(20.0))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .child(
                        gpui::svg()
                            .data(icon_svg)
                            .size(px(15.0))
                            .when(is_active, |s| s.text_color(t.accent))
                            .when(!is_active, |s| s.text_color(t.text_muted))
                            .flex_none(),
                    ),
            )
            .child(div().text_size(px(13.0)).child(label).into_any_element())
            .into_any_element()
    }

    /// Top bar: active page title + subtitle + proxy status.
    pub(crate) fn topbar(&self, _cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = self.theme.clone();
        let i = self.i18n;
        let page = self.page;

        let (page_title, page_subtitle) = match page {
            Page::Tool(tool) => (
                i.t(tool.name_zh(), tool.name_en()),
                i.t("模型供应商与运行时配置", "Providers & Runtime Configuration"),
            ),
            Page::Mcp => (
                i.t("MCP 服务器", "MCP Servers"),
                i.t("统一管理各工具的 Model Context Protocol 协议服务", "Manage Model Context Protocol servers"),
            ),
            Page::Skills => (
                i.t("Skills 技能", "Skills"),
                i.t("扩展各 AI 工具的系统 Prompt 与函数技能库", "System prompts & agent tool skills"),
            ),
            Page::Sessions => (
                i.t("会话管理", "Sessions"),
                i.t("浏览、恢复与导出各 CLI 工具的历史对话会话", "Browse and export CLI chat sessions"),
            ),
            Page::Settings => (
                i.t("系统设置", "Settings"),
                i.t("个性化偏好、CLI 根目录与自动备份管理", "Preferences, paths & backups"),
            ),
        };

        let has_proxy = !self.settings.proxy_url.trim().is_empty();

        div()
            .id("top-header")
            .h(px(54.0))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .px(px(24.0))
            .bg(t.header_bg)
            .border_b_1()
            .border_color(t.sidebar_border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(15.5))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(t.text_primary)
                            .child(page_title),
                    )
                    .child(
                        div()
                            .h(px(12.0))
                            .border_l_1()
                            .border_color(t.card_border),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(t.text_muted)
                            .child(page_subtitle),
                    )
                    .when(has_proxy, |p| {
                        p.child(
                            div()
                                .px(px(7.0))
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .bg(t.success_subtle)
                                .text_size(px(11.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.success)
                                .child(i.t("代理开启", "Proxy Active")),
                        )
                    }),
            )
            .child(div())
            .into_any_element()
    }
}
