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
            .flex()
            .items_center()
            .px(px(20.0))
            .border_b_1()
            .border_color(t.sidebar_border)
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(t.text_primary)
                    .child("AI ToolPlus"),
            );

        // 2. Scrollable Navigation List
        let mut nav_list = div()
            .id("sidebar-nav")
            .flex_1()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .py(px(6.0));

        nav_list = nav_list.child(self.sidebar_group(i.t("layout.coding_tools")));

        let mut ag_rendered = false;
        let show_ag = self.settings.visible_tools.is_empty()
            || self.settings.visible_tools.iter().any(|key| key == "antigravity");

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

            // Right after Codex, show Antigravity if visible
            if tool == aitoolplus_core::ToolId::Codex && show_ag {
                nav_list = nav_list.child(self.sidebar_item(cx, Page::Antigravity));
                ag_rendered = true;
            }
        }

        // If Codex was hidden or not rendered, but Antigravity is enabled, render it in Coding Tools
        if show_ag && !ag_rendered {
            nav_list = nav_list.child(self.sidebar_item(cx, Page::Antigravity));
        }

        nav_list = nav_list.child(self.sidebar_group(i.t("layout.shared_resources")));
        nav_list = nav_list.child(self.sidebar_item(cx, Page::Mcp));
        nav_list = nav_list.child(self.sidebar_item(cx, Page::Skills));

        // 3. Docked Settings at Bottom
        let footer = div()
            .p(px(8.0))
            .border_t_1()
            .border_color(t.sidebar_border)
            .child(self.sidebar_item(cx, Page::Settings));

        div()
            .id("sidebar")
            .w(px(200.0))
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
            Page::Tool(tool) => (crate::icons::tool_icon(tool), i.raw(tool.name_zh(), tool.name_en())),
            Page::Mcp => (crate::icons::MCP_SVG, i.t("layout.mcp_servers")),
            Page::Skills => (crate::icons::SPARKLES_SVG, i.t("layout.skills")),
            Page::Antigravity => (crate::icons::GEMINI_SVG, i.t("layout.antigravity")),
            Page::Settings => (crate::icons::SETTINGS_SVG, i.t("layout.settings")),
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
            .when(
                page == Page::Settings
                    && self
                        .ui
                        .update_info
                        .as_ref()
                        .map(|u| u.update_available)
                        .unwrap_or(false),
                |s| {
                    s.child(
                        div()
                            .ml_auto()
                            .px(px(6.0))
                            .py(px(1.5))
                            .rounded(px(10.0))
                            .bg(t.accent)
                            .text_color(crate::rgba_const(0xffffffff))
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("NEW"),
                    )
                },
            )
            .into_any_element()
    }

    /// Top bar: active page title + subtitle + proxy status.
    pub(crate) fn topbar(&self, _cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = self.theme.clone();
        let i = self.i18n;
        let page = self.page;

        let (page_title, page_subtitle) = match page {
            Page::Tool(tool) => (
                i.raw(tool.name_zh(), tool.name_en()),
                i.t("layout.providers_runtime_configuratio"),
            ),
            Page::Mcp => (
                i.t("layout.mcp_servers"),
                i.t("layout.manage_model_context_protocol"),
            ),
            Page::Skills => (
                i.t("layout.skills"),
                i.t("layout.system_prompts_agent_tool"),
            ),
            Page::Antigravity => {
                if self.ui.antigravity_tab == crate::pages::AntigravityPageTab::Sessions {
                    (
                        i.t("layout.antigravity_sessions"),
                        i.t("layout.browse_resume_and_manage"),
                    )
                } else {
                    (
                        i.t("layout.antigravity_accounts"),
                        i.t("layout.account_quota_inspection_and"),
                    )
                }
            }
            Page::Settings => (
                i.t("layout.settings"),
                i.t("layout.preferences_paths_backups"),
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
                                .child(i.t("layout.proxy_active")),
                        )
                    }),
            )
            .child(div())
            .into_any_element()
    }
}
