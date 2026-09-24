use std::path::Path;

use aitoolplus_core::skills::{self, Skill, StoreSkillItem};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, div, prelude::*, px, uniform_list};

use crate::components::{
    BadgeKind, ButtonVariant, badge, button_l, button_with_icon_l, button_with_icon_loading_l, input_container,
    section_title, segmented_pill_selector,
};
use crate::text_input::TextInput;
use crate::workspace::Workspace;

use super::{SkillDetailState, SkillsPageTab, SkillStoreSource, modal_scaffold_custom, modal_scaffold_sized};

pub const RECOMMENDED_SKILLS: &[(&str, &str, &str)] = &[
    ("ponytail", "极简开发原则", "https://github.com/ponytail-ai/ponytail.git"),
    ("gpui-kit", "GPUI Kit 设计规范", "https://github.com/gpui-kit/skills.git"),
    ("git-commit-helper", "Git 规范提交助手", "https://github.com/skills/git-commit-helper.git"),
    ("code-reviewer", "代码审查专家", "https://github.com/skills/code-reviewer.git"),
];

fn format_installs(installs: u64) -> String {
    if installs >= 1_000_000 {
        format!("{:.1}M", installs as f64 / 1_000_000.0)
    } else if installs >= 1_000 {
        format!("{:.1}k", installs as f64 / 1_000.0)
    } else {
        installs.to_string()
    }
}

fn format_relative_time(timestamp_ms: i64, i: &crate::i18n::I18n) -> String {
    let now = chrono::Utc::now().timestamp_millis();
    let diff_ms = now - timestamp_ms;
    let diff_secs = if diff_ms < 0 { 0 } else { diff_ms / 1000 };

    if diff_secs < 60 {
        i.t("刚刚", "just now").to_string()
    } else if diff_secs < 3600 {
        let mins = diff_secs / 60;
        i.t(&format!("{mins} 分钟前"), &format!("{mins}m ago")).to_string()
    } else if diff_secs < 86400 {
        let hours = diff_secs / 3600;
        i.t(&format!("{hours} 小时前"), &format!("{hours}h ago")).to_string()
    } else if diff_secs < 30 * 86400 {
        let days = diff_secs / 86400;
        i.t(&format!("{days} 天前"), &format!("{days}d ago")).to_string()
    } else {
        let months = diff_secs / (30 * 86400);
        i.t(&format!("{months} 个月前"), &format!("{months}mo ago")).to_string()
    }
}

fn tag_color(tag: &str, theme: &crate::theme::Theme) -> (gpui::Rgba, gpui::Rgba) {
    let mut hash: u32 = 0x811c9dc5;
    for b in tag.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    let idx = (hash % 8) as usize;
    if theme.is_dark {
        match idx {
            0 => (crate::rgba_const(0x3b82f625), crate::rgba_const(0x93c5fdff)),
            1 => (crate::rgba_const(0x8b5cf625), crate::rgba_const(0xc4b5fdff)),
            2 => (crate::rgba_const(0x10b98125), crate::rgba_const(0x6ee7b7ff)),
            3 => (crate::rgba_const(0xf59e0b25), crate::rgba_const(0xfcd34dff)),
            4 => (crate::rgba_const(0xf43f5e25), crate::rgba_const(0xfda4afff)),
            5 => (crate::rgba_const(0x06b6d425), crate::rgba_const(0x67e8f9ff)),
            6 => (crate::rgba_const(0x6366f125), crate::rgba_const(0xa5b4fcff)),
            _ => (crate::rgba_const(0xf9731625), crate::rgba_const(0xfdba74ff)),
        }
    } else {
        match idx {
            0 => (crate::rgba_const(0xeff6ffef), crate::rgba_const(0x1d4ed8ff)),
            1 => (crate::rgba_const(0xf5f3ffef), crate::rgba_const(0x6d28d9ff)),
            2 => (crate::rgba_const(0xecfdf5ef), crate::rgba_const(0x047857ff)),
            3 => (crate::rgba_const(0xfffbebef), crate::rgba_const(0xb45309ff)),
            4 => (crate::rgba_const(0xfff1f2ef), crate::rgba_const(0xbe123cff)),
            5 => (crate::rgba_const(0xecfeffef), crate::rgba_const(0x0e7490ff)),
            6 => (crate::rgba_const(0xeef2ffef), crate::rgba_const(0x4338caff)),
            _ => (crate::rgba_const(0xfff7edef), crate::rgba_const(0xc2410cff)),
        }
    }
}

fn extract_source_label(source_type: &str, source_ref: Option<&str>, central_path: &str) -> String {
    if source_type == "git" {
        if let Some(ref_str) = source_ref {
            let s = ref_str.trim().trim_end_matches(".git");
            if let Some(pos) = s.find("github.com/") {
                return s[pos + 11..].to_string();
            }
            if let Some(pos) = s.find("github.com:") {
                return s[pos + 11..].to_string();
            }
            let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
            if parts.len() >= 2 {
                return format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]);
            }
            return s.to_string();
        }
        return "Git".to_string();
    }
    if source_type == "local" {
        if let Some(ref_str) = source_ref {
            let p = Path::new(ref_str);
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                return name.to_string();
            }
        }
        return "Local".to_string();
    }
    central_path.to_string()
}

pub fn render_skills_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    // First visit: scan the central repo and discover existing tool skills.
    if !ws.ui.skills_discovered {
        let (_added, _imported) = {
            let mut store = ws.store.store().skills.clone();
            let added = skills::scan_central(&mut store, &ws.paths);
            let imported = skills::scan_and_import_existing(&ws.paths, &mut store)
                .unwrap_or_default();
            let _ = ws.store.update(|db| db.skills = store);
            (added, imported)
        };
        ws.persist_store();
        ws.ui.skills_discovered = true;
    }

    let tab_selector = segmented_pill_selector(
        "skills-page-tab",
        vec![
            (
                SkillsPageTab::Installed,
                Some(crate::icons::PACKAGE_SVG),
                i.t("已安装技能", "Installed Skills"),
            ),
            (
                SkillsPageTab::Store,
                Some(crate::icons::SPARKLES_SVG),
                i.t("发现技能", "Discover Skills"),
            ),
        ],
        ws.ui.skills_page_tab,
        &t,
        cx,
        |ws, tab, _, cx| {
            ws.ui.skills_page_tab = tab;
            cx.notify();
        },
    );

    let content = match ws.ui.skills_page_tab {
        SkillsPageTab::Installed => render_installed_tab(ws, cx),
        SkillsPageTab::Store => render_store_tab(ws, cx),
    };

    div()
        .relative()
        .size_full()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(tab_selector)
        .child(content)
        .into_any_element()
}

fn tool_icon(tool: ToolId) -> &'static [u8] {
    crate::icons::tool_icon(tool)
}

fn render_virtual_skill_card(
    skill: &Skill,
    tools: &[ToolId],
    repo_path: &Path,
    ws_entity: &gpui::Entity<Workspace>,
    t: &crate::theme::Theme,
    i: &crate::i18n::I18n,
) -> gpui::AnyElement {
    let sid = skill.id.clone();
    let s_name = skill.name.clone();
    let skill_folder = repo_path.join(&skill.central_path);
    let skill_folder_for_reveal = skill_folder.clone();

    let is_enabled = skill.management_enabled;
    let description = skill.get_description(repo_path).unwrap_or_default();
    let source_label = extract_source_label(&skill.source_type, skill.source_ref.as_deref(), &skill.central_path);
    let updated_str = format_relative_time(skill.updated_at, i);

    // Source icon
    let source_icon = if skill.source_type == "git" {
        crate::icons::GITHUB_SVG
    } else if skill.source_type == "local" {
        crate::icons::FOLDER_SVG
    } else {
        crate::icons::FILE_TEXT_SVG
    };

    // Card click opens detail drawer
    let sid_for_drawer = sid.clone();
    let ws_entity_drawer = ws_entity.clone();

    // 0. Toggle management enabled (iOS-style switch)
    let sid_toggle = sid.clone();
    let ws_entity_toggle = ws_entity.clone();
    let toggle_btn = div()
        .id(gpui::SharedString::from(format!("vcard-toggle-{}", sid)))
        .w(px(32.0))
        .h(px(18.0))
        .p(px(2.0))
        .rounded_full()
        .cursor_pointer()
        .bg(if is_enabled {
            t.track_on
        } else {
            t.track_off
        })
        .flex()
        .flex_none()
        .items_center()
        .bg(if is_enabled {
            t.track_on
        } else {
            t.track_off
        })
        .hover(move |h| h.opacity(0.92))
        .active(move |a| a.opacity(0.85))
        .when(is_enabled, |s| s.justify_end())
        .when(!is_enabled, |s| s.justify_start())
        .child(
            div()
                .size(px(14.0))
                .rounded_full()
                .bg(t.thumb)
                .shadow_sm(),
        )
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let sid = sid_toggle.clone();
            let _ = ws_entity_toggle.update(cx, |ws, cx| {
                let mut store = ws.store.store().skills.clone();
                let new_state = !is_enabled;
                skills::set_management_enabled(&mut store, &sid, new_state);
                let settings = store.settings.clone();
                if let Some(skill) = store.skills.iter_mut().find(|s| s.id == sid) {
                    let tools_to_sync: Vec<ToolId> = skills::skills_tools()
                        .iter()
                        .copied()
                        .filter(|t| skill.is_enabled_in(*t))
                        .collect();
                    for tool in tools_to_sync {
                        if new_state {
                            let _ = skills::sync_skill_to_tool(&settings, &ws.paths, skill, tool);
                        } else {
                            let _ = skills::unlink_skill_from_tool(&ws.paths, skill, tool);
                        }
                    }
                }
                let _ = ws.store.update(|db| db.skills = store);
                ws.persist_store();
                cx.notify();
            });
        });

    // 1. Reveal folder
    let folder_reveal = skill_folder_for_reveal.clone();
    let reveal_btn = div()
        .id(gpui::SharedString::from(format!("vcard-folder-{}", sid)))
        .w(px(22.0))
        .h(px(22.0))
        .rounded(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover({
            let bg = t.card_hover;
            move |h| h.bg(bg)
        })
        .child(crate::icons::svg_icon(crate::icons::FOLDER_SVG, px(12.0), t.text_secondary))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            super::open_path_in_default_manager(&folder_reveal);
        });

    // 2. Copy source/path
    let copy_val = skill
        .source_ref
        .clone()
        .unwrap_or_else(|| skill_folder.display().to_string());
    let ws_entity_copy = ws_entity.clone();
    let copy_btn = div()
        .id(gpui::SharedString::from(format!("vcard-copy-{}", sid)))
        .w(px(22.0))
        .h(px(22.0))
        .rounded(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover({
            let bg = t.card_hover;
            move |h| h.bg(bg)
        })
        .child(crate::icons::svg_icon(crate::icons::COPY_SVG, px(12.0), t.text_secondary))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_val.clone()));
            let _ = ws_entity_copy.update(cx, |ws, cx| {
                ws.ui.toast(ws.i18n.t("已复制到剪贴板", "Copied to clipboard").to_string(), false);
                cx.notify();
            });
        });

    // 3. Update
    let sid_for_update = sid.clone();
    let is_git = skill.source_type == "git";
    let ws_entity_update = ws_entity.clone();
    let update_btn = div()
        .id(gpui::SharedString::from(format!("vcard-update-{}", sid)))
        .w(px(22.0))
        .h(px(22.0))
        .rounded(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover({
            let bg = t.card_hover;
            move |h| h.bg(bg)
        })
        .child(crate::icons::svg_icon(crate::icons::REFRESH_SVG, px(12.0), t.text_secondary))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let sid = sid_for_update.clone();
            let _ = ws_entity_update.update(cx, |ws, cx| {
                if is_git {
                    match aitoolplus_core::skill_git::update(
                        &mut ws.store.store_mut().skills,
                        &ws.paths,
                        &sid,
                    ) {
                        Ok(()) => {
                            ws.persist_store();
                            ws.ui.toast("Git Skill updated", false);
                        }
                        Err(error) => ws.ui.toast(error, true),
                    }
                } else {
                    let mut store = ws.store.store().skills.clone();
                    skills::scan_central(&mut store, &ws.paths);
                    let _ = ws.store.update(|db| db.skills = store);
                    ws.persist_store();
                    ws.ui.toast("Skill refreshed", false);
                }
                cx.notify();
            });
        });

    // 4. Delete
    let sid_for_del = sid.clone();
    let s_name_for_del = s_name.clone();
    let ws_entity_del = ws_entity.clone();
    let del_btn = div()
        .id(gpui::SharedString::from(format!("vcard-del-{}", sid)))
        .w(px(22.0))
        .h(px(22.0))
        .rounded(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover({
            let bg = t.danger.opacity(0.12);
            move |h| h.bg(bg)
        })
        .child(crate::icons::svg_icon(crate::icons::TRASH_SVG, px(12.0), t.danger))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let sid = sid_for_del.clone();
            let sname = s_name_for_del.clone();
            let _ = ws_entity_del.update(cx, |ws, cx| {
                ws.ui.confirm = Some(super::ConfirmState {
                    title: ws.i18n.t("卸载 Skill", "Uninstall Skill").to_string(),
                    message: ws
                        .i18n
                        .t(
                            &format!("确定要彻底卸载删除 Skill “{}” 吗？", sname),
                            &format!("Are you sure you want to uninstall and delete “{}”?", sname),
                        )
                        .to_string(),
                    action: super::ConfirmAction::DeleteSkill { id: sid },
                });
                cx.notify();
            });
        });

    // Header row
    let header_row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(6.0))
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .min_w(px(0.0))
                .flex_1()
                .child(
                    div()
                        .w(px(8.0))
                        .h(px(8.0))
                        .rounded(px(4.0))
                        .bg(if is_enabled {
                            crate::rgba_const(0x10b981ff)
                        } else {
                            t.card_border
                        }),
                )
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(if is_enabled { t.text_primary } else { t.text_muted })
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(s_name),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(2.0))
                .child(toggle_btn)
                .child(reveal_btn)
                .child(copy_btn)
                .child(update_btn)
                .child(del_btn),
        );

    // Body: strictly single-line description with ellipsis, matching ai-toolbox!
    let desc_text = if description.is_empty() {
        format!("中央仓库托管 Skill ({})", skill.central_path)
    } else {
        description.replace('\n', " ").replace('\r', "")
    };

    let mut body_col = div()
        .flex_1()
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(2.0))
        .min_w(px(0.0))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(t.text_secondary)
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(desc_text),
        );

    // Optional tags / note row (if present)
    if !skill.tags.is_empty() || skill.user_group.is_some() || skill.user_note.is_some() {
        let mut tags_bar = div().flex().items_center().gap(px(4.0)).min_w(px(0.0));
        for tag in skill.tags.iter().take(2) {
            let (bg, fg) = tag_color(tag, t);
            tags_bar = tags_bar.child(
                div()
                    .px(px(6.0))
                    .h(px(18.0))
                    .rounded(px(9.0))
                    .bg(bg)
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(10.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(fg)
                            .whitespace_nowrap()
                            .child(tag.clone()),
                    ),
            );
        }
        if let Some(ref grp) = skill.user_group {
            tags_bar = tags_bar.child(
                div()
                    .px(px(6.0))
                    .h(px(18.0))
                    .rounded(px(9.0))
                    .bg(t.sidebar_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(t.text_muted)
                            .whitespace_nowrap()
                            .child(grp.clone()),
                    ),
            );
        }
        body_col = body_col.child(tags_bar);
    }

    // Footer row: Source + time on left, synced tools on right!
    let mut footer_tools = div().flex().items_center().gap(px(4.0));
    for tool in tools {
        if skill.is_enabled_in(*tool) {
            let tool_id_val = *tool;
            let sid_tool = sid.clone();
            let ws_entity_tool = ws_entity.clone();
            let tool_icon_svg = tool_icon(*tool);
            let tooltip_msg = match *tool {
                ToolId::Agents => i.t(
                    "通用 Agent (~/.agents/skills，一般支持除了 Claude 的所有 Agent)",
                    "Universal Agent (~/.agents/skills, generally supports all agents except Claude)",
                ),
                ToolId::ClaudeCode => i.t("Claude Code (~/.claude/skills)", "Claude Code (~/.claude/skills)"),
                ToolId::Codex => i.t("Codex (~/.codex/skills)", "Codex (~/.codex/skills)"),
                ToolId::Pi => i.t("Pi (~/.pi/agent/skills)", "Pi (~/.pi/agent/skills)"),
                ToolId::OpenCode => i.t("OpenCode (~/.config/opencode/skill)", "OpenCode (~/.config/opencode/skill)"),
                ToolId::OhMyPi => i.t("Oh My Pi (~/.config/oh-my-pi/skills)", "Oh My Pi (~/.config/oh-my-pi/skills)"),
                ToolId::Kimi => i.t("Kimi (~/.kimi-code/skills)", "Kimi (~/.kimi-code/skills)"),
                _ => i.t(tool.name_zh(), tool.name_en()),
            };
            let pill = div()
                .id(gpui::SharedString::from(format!("vcard-tool-{}-{}", sid, tool.key())))
                .w(px(20.0))
                .h(px(20.0))
                .rounded(px(4.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.accent.opacity(0.4))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover({
                    let bg = t.card_hover;
                    move |h| h.bg(bg)
                })
                .tooltip(move |_window, cx| {
                    cx.new(|_| crate::components::Tooltip::new(tooltip_msg.clone())).into()
                })
                .child(crate::icons::svg_icon(tool_icon_svg, px(12.0), t.text_primary))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    let sid = sid_tool.clone();
                    let _ = ws_entity_tool.update(cx, |ws, cx| {
                        let _ = ws.store.update(|db| {
                            skills::toggle_tool(&mut db.skills, &sid, tool_id_val);
                        });
                        let mut store = ws.store.store().skills.clone();
                        let idx = store.skills.iter().position(|s| s.id == sid);
                        if let Some(idx) = idx {
                            let settings = store.settings.clone();
                            let entry = &mut store.skills[idx];
                            let target = entry.is_enabled_in(tool_id_val);
                            let _ = if target {
                                skills::sync_skill_to_tool(&settings, &ws.paths, entry, tool_id_val)
                            } else {
                                skills::remove_skill_from_tool(&settings, &ws.paths, entry, tool_id_val)
                            };
                        }
                        let _ = ws.store.update(|db| db.skills = store);
                        ws.persist_store();
                        cx.notify();
                    });
                });
            footer_tools = footer_tools.child(pill);
        }
    }

    // Plus button to open detail drawer (mirroring ai-toolbox .addToolBtn)
    let sid_plus = sid.clone();
    let ws_entity_plus = ws_entity.clone();
    let plus_btn = div()
        .id(gpui::SharedString::from(format!("vcard-plus-{}", sid)))
        .w(px(20.0))
        .h(px(20.0))
        .rounded(px(4.0))
        .border_1()
        .border_dashed()
        .border_color(t.input_border)
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover({
            let bg = t.card_hover;
            move |h| h.bg(bg)
        })
        .child(crate::icons::svg_icon(crate::icons::PLUS_SVG, px(10.0), t.text_muted))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let sid = sid_plus.clone();
            let _ = ws_entity_plus.update(cx, |ws, cx| {
                ws.ui.selected_skill_id = Some(sid);
                cx.notify();
            });
        });
    footer_tools = footer_tools.child(plus_btn);

    let footer_row = div()
        .pt(px(4.0))
        .border_t_1()
        .border_color(t.card_border.opacity(0.5))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(6.0))
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .min_w(px(0.0))
                .child(crate::icons::svg_icon(source_icon, px(11.0), t.text_secondary))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_secondary)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(source_label),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .whitespace_nowrap()
                        .child(format!("· {updated_str}")),
                ),
        )
        .child(footer_tools);

    let card = div()
        .id(gpui::SharedString::from(format!("v-skill-card-{}", sid)))
        .w_full()
        .h_full()
        .min_w(px(0.0))
        .p(px(8.0))
        .px(px(10.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .shadow_xs()
        .cursor_pointer()
        .hover({
            let bg = t.card_hover;
            let border = t.card_border_hover;
            move |h| h.bg(bg).border_color(border)
        })
        .flex()
        .flex_col()
        .justify_between()
        .child(header_row)
        .child(body_col)
        .child(footer_row)
        .on_click(move |_, _, cx| {
            let sid = sid_for_drawer.clone();
            let _ = ws_entity_drawer.update(cx, |ws, cx| {
                ws.ui.selected_skill_id = Some(sid);
                cx.notify();
            });
        });

    if !is_enabled {
        card.opacity(0.55).into_any_element()
    } else {
        card.into_any_element()
    }
}

fn render_installed_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let store = ws.store.store().skills.clone();
    let repo = skills::central_repo_path(&store.settings, &ws.paths);
    let all_skills = skills::list(&store);
    let tools: Vec<ToolId> = skills::skills_tools().to_vec();

    let mut section = div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .h_full()
        .min_h(px(0.0))
        .gap(px(10.0));

    // Top control section (compact)
    let header_card = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(10.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(section_title(
                    &t,
                    i.t("中央仓库", "Central Repo"),
                    Some(gpui::SharedString::from(repo.display().to_string())),
                ))
                .child({
                    let is_more_open = ws.ui.skills_more_actions_open;
                    let mut actions_bar = div()
                        .relative()
                        .flex()
                        .items_center()
                        .gap(px(6.0));

                    // 1. 导入现有skill (Primary)
                    actions_bar = actions_bar.child(button_with_icon_l(
                        "skills-scan-import",
                        crate::icons::SPARKLES_SVG,
                        i.t("导入现有Skill", "Import Existing Skills"),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        |ws, _, _, cx| scan_and_import_action(ws, cx),
                    ));

                    // 2. 更新全部
                    actions_bar = actions_bar.child(button_with_icon_loading_l(
                        "skills-check-update",
                        crate::icons::REFRESH_SVG,
                        i.t("更新全部", "Update All"),
                        ButtonVariant::Secondary,
                        ws.ui.skills_busy,
                        &t,
                        cx,
                        |ws, _, _, cx| check_and_update_all_action(ws, cx),
                    ));

                    // 3. 从Git安装
                    actions_bar = actions_bar.child(button_with_icon_l(
                        "skills-git-modal-btn",
                        crate::icons::GITHUB_SVG,
                        i.t("从Git安装...", "Install from Git..."),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        |ws, _, _, cx| {
                            let git_input = cx.new(|cx| TextInput::new("https://github.com/owner/repo.git", cx));
                            let git_inp_for_enter = git_input.clone();
                            cx.subscribe(&git_input, move |ws, _emitter, event: &crate::text_input::TextInputEvent, cx| {
                                if let crate::text_input::TextInputEvent::Enter = event {
                                    let url = git_inp_for_enter.read(cx).text().trim().to_string();
                                    if !url.is_empty() {
                                        install_git_skill_action(ws, url, cx);
                                    }
                                }
                            }).detach();
                            ws.ui.skill_git_modal = Some(git_input);
                            cx.notify();
                        },
                    ));

                    // 4. 其他操作 (下拉菜单触发按钮)
                    actions_bar = actions_bar.child(button_with_icon_l(
                        "skills-more-actions-btn",
                        if is_more_open { crate::icons::CHEVRON_UP_SVG } else { crate::icons::CHEVRON_DOWN_SVG },
                        i.t("其他操作", "More Actions"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            if is_more_open {
                                ws.ui.skills_more_actions_open = false;
                            } else {
                                ws.ui.skills_more_actions_open = true;
                            }
                            cx.notify();
                        },
                    ));

                    actions_bar
                }),
        );

    let search_bar = div()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .w(px(320.0))
                .child(input_container(&t, ws.ui.skill_search.clone())),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(t.text_muted)
                .child(format!("共 {} 个已安装技能", all_skills.len())),
        );

    section = section
        .child(header_card)
        .child(search_bar);

    let query = ws.ui.skill_search.read(cx).text().to_lowercase();
    let filtered_skills: Vec<_> = all_skills
        .into_iter()
        .filter(|s| {
            query.is_empty()
                || s.name.to_lowercase().contains(&query)
                || s.user_note.as_deref().is_some_and(|n| n.to_lowercase().contains(&query))
                || s.central_path.to_lowercase().contains(&query)
                || s.description.as_deref().is_some_and(|d| d.to_lowercase().contains(&query))
                || s.tags.iter().any(|tag| tag.to_lowercase().contains(&query))
        })
        .collect();

    if filtered_skills.is_empty() {
        section = section.child(crate::components::empty_state_svg(
            &t,
            crate::icons::SPARKLES_SVG,
            if query.is_empty() {
                i.t("暂无已安装技能", "No installed skills")
            } else {
                i.t("没有找到匹配的 Skill", "No matching skills")
            },
            if query.is_empty() {
                i.t(
                    "点击上方【扫描并导入现有技能】自动收录本地技能，或前往【发现技能】发现安装",
                    "Click 'Scan & Import' to discover local skills, or visit 'Discover Skills'",
                )
            } else {
                i.t("尝试更换搜索关键词", "Try a different search query")
            },
        ));
    } else {
        // Virtualized 2-column grid using uniform_list (matching ai-toolbox defaultRowHeight={108})
        let rows_count = (filtered_skills.len() + 1) / 2;
        let skills_arc = std::sync::Arc::new(filtered_skills);
        let tools_arc = std::sync::Arc::new(tools);
        let repo_arc = std::sync::Arc::new(repo.clone());
        let ws_entity = cx.entity();
        let t_clone = t.clone();
        let i_clone = i.clone();

        let v_list = uniform_list(
            "installed-skills-vlist",
            rows_count,
            move |range: std::ops::Range<usize>, _window: &mut gpui::Window, _cx: &mut gpui::App| -> Vec<gpui::AnyElement> {
                let mut row_elements = Vec::with_capacity(range.len());
                for row_idx in range {
                    let first_idx = row_idx * 2;
                    let second_idx = first_idx + 1;

                    let mut row = div()
                        .h(px(108.0))
                        .pb(px(8.0))
                        .flex()
                        .gap(px(10.0))
                        .w_full();

                    if let Some(skill1) = skills_arc.get(first_idx) {
                        row = row.child(
                            div()
                                .flex_1()
                                .h_full()
                                .min_w(px(0.0))
                                .child(render_virtual_skill_card(
                                    skill1,
                                    &tools_arc,
                                    &repo_arc,
                                    &ws_entity,
                                    &t_clone,
                                    &i_clone,
                                )),
                        );
                    }

                    if let Some(skill2) = skills_arc.get(second_idx) {
                        row = row.child(
                            div()
                                .flex_1()
                                .h_full()
                                .min_w(px(0.0))
                                .child(render_virtual_skill_card(
                                    skill2,
                                    &tools_arc,
                                    &repo_arc,
                                    &ws_entity,
                                    &t_clone,
                                    &i_clone,
                                )),
                        );
                    } else {
                        row = row.child(div().flex_1().h_full().min_w(px(0.0)));
                    }

                    row_elements.push(row.into_any_element());
                }
                row_elements
            },
        )
        .size_full();

        section = section.child(
            div()
                .w_full()
                .flex_1()
                .h_full()
                .min_h(px(0.0))
                .overflow_hidden()
                .child(v_list),
        );
    }

    if ws.ui.skills_more_actions_open {
        let repo_for_open = repo.clone();
        let backdrop = div()
            .id("skills-more-backdrop")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_click(cx.listener(|ws, _, _, cx| {
                ws.ui.skills_more_actions_open = false;
                cx.notify();
            }));

        let mut menu = div()
            .id("skills-more-actions-menu")
            .occlude()
            .absolute()
            .top(px(45.0))
            .right(px(11.0))
            .w(px(168.0))
            .p(px(4.0))
            .rounded(px(6.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border)
            .shadow_xl()
            .flex()
            .flex_col()
            .gap(px(2.0));

        // 选项 1: 全部同步
        menu = menu.child(
            div()
                .id("skills-menu-sync-all")
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .text_size(px(12.5))
                .text_color(t.text_primary)
                .hover(|h| h.bg(t.card_hover))
                .child(crate::icons::svg_icon(crate::icons::REFRESH_SVG, px(13.0), t.text_secondary))
                .child(i.t("全部同步", "Sync All"))
                .on_click(cx.listener(|ws, _, _, cx| {
                    ws.ui.skills_more_actions_open = false;
                    sync_all_action(ws, cx);
                })),
        );

        // 选项 2: 还原实体目录
        menu = menu.child(
            div()
                .id("skills-menu-restore")
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .text_size(px(12.5))
                .text_color(t.text_primary)
                .hover(|h| h.bg(t.card_hover))
                .child(crate::icons::svg_icon(crate::icons::REPEAT_SVG, px(13.0), t.text_secondary))
                .child(i.t("还原实体目录", "Restore to Plain"))
                .on_click(cx.listener(|ws, _, _, cx| {
                    ws.ui.skills_more_actions_open = false;
                    ws.ui.confirm = Some(super::ConfirmState {
                        title: ws.i18n.t("还原为实体目录", "Restore to Plain Folders").to_string(),
                        message: ws.i18n.t(
                            "确定要将所有已同步工具目录中的技能超链（Junction）还原为独立的普通实体文件夹吗？\n还原后各个工具目录将拥有独立的文件副本，不再依赖中央仓库。",
                            "Are you sure you want to restore all skill junctions in tool directories to standalone plain folders?\nAfter restoration, each tool directory will have independent file copies and will no longer depend on the central repository.",
                        ).to_string(),
                        action: super::ConfirmAction::RestoreSkillsToPlain,
                    });
                    cx.notify();
                })),
        );

        // 选项 3: 手动导入目录...
        menu = menu.child(
            div()
                .id("skills-menu-import-dir")
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .text_size(px(12.5))
                .text_color(t.text_primary)
                .hover(|h| h.bg(t.card_hover))
                .child(crate::icons::svg_icon(crate::icons::FOLDER_SVG, px(13.0), t.text_secondary))
                .child(i.t("手动导入目录...", "Import Directory..."))
                .on_click(cx.listener(|ws, _, _, cx| {
                    ws.ui.skills_more_actions_open = false;
                    import_skill_dir(ws, cx);
                })),
        );

        // 选项 4: 从 ZIP 安装...
        menu = menu.child(
            div()
                .id("skills-menu-import-zip")
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .text_size(px(12.5))
                .text_color(t.text_primary)
                .hover(|h| h.bg(t.card_hover))
                .child(crate::icons::svg_icon(crate::icons::FILE_TEXT_SVG, px(13.0), t.text_secondary))
                .child(i.t("从 ZIP 安装...", "Install from ZIP..."))
                .on_click(cx.listener(|ws, _, _, cx| {
                    ws.ui.skills_more_actions_open = false;
                    install_from_zip_action(ws, cx);
                })),
        );

        // 选项 5: 打开Skill目录
        menu = menu.child(
            div()
                .id("skills-menu-open-dir")
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .text_size(px(12.5))
                .text_color(t.text_primary)
                .hover(|h| h.bg(t.card_hover))
                .child(crate::icons::svg_icon(crate::icons::EXTERNAL_LINK_SVG, px(13.0), t.text_secondary))
                .child(i.t("打开Skill目录", "Open Skills Directory"))
                .on_click(cx.listener(move |ws, _, _, cx| {
                    ws.ui.skills_more_actions_open = false;
                    super::open_path_in_default_manager(&repo_for_open);
                    cx.notify();
                })),
        );

        section = section.child(backdrop).child(menu);
    }

    section.into_any_element()
}

/// Right-Side Detail Drawer (mirrors ai-toolbox `SkillDetailPanel`)
pub(crate) fn render_skill_detail_drawer(
    selected_id: &str,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let store = ws.store.store().skills.clone();
    let Some(skill) = store.skills.iter().find(|s| s.id == selected_id).cloned() else {
        return div().into_any_element();
    };

    let repo_path = skills::central_repo_path(&store.settings, &ws.paths);
    let skill_folder = repo_path.join(&skill.central_path);
    let skill_folder_for_reveal = skill_folder.clone();
    let skill_folder_for_central = repo_path.clone();
    let documents = skills::read_skill_documents(&skill_folder);

    let is_enabled = skill.management_enabled;
    let description = skill.get_description(&repo_path).unwrap_or_default();
    let source_label = extract_source_label(&skill.source_type, skill.source_ref.as_deref(), &skill.central_path);
    let updated_str = format_relative_time(skill.updated_at, &i);

    // Source icon
    let source_icon = if skill.source_type == "git" {
        crate::icons::GITHUB_SVG
    } else if skill.source_type == "local" {
        crate::icons::FOLDER_SVG
    } else {
        crate::icons::FILE_TEXT_SVG
    };

    // Active doc for tabs
    let active_doc_name = ws.ui.skill_detail_active_doc.clone().unwrap_or_else(|| {
        documents.first().map(|d| d.filename.clone()).unwrap_or_else(|| "SKILL.md".to_string())
    });

    let current_doc_content = documents
        .iter()
        .find(|d| d.filename == active_doc_name)
        .map(|d| d.content.clone())
        .unwrap_or_else(|| {
            if skill_folder.join("SKILL.md").exists() {
                std::fs::read_to_string(skill_folder.join("SKILL.md")).unwrap_or_default()
            } else {
                i.t("（未找到 SKILL.md 文档）", "(no SKILL.md found)").to_string()
            }
        });

    // Tool sync summary
    let tools: Vec<ToolId> = skills::skills_tools().to_vec();
    let synced_count = tools.iter().filter(|tool| skill.is_enabled_in(**tool)).count();
    let total_count = tools.len();

    // Backdrop mask (clicking outside closes drawer)
    let backdrop = div()
        .id("drawer-backdrop")
        .absolute()
        .inset_0()
        .occlude()
        .bg(gpui::rgba(0x00000038))
        .on_scroll_wheel(|_, _, cx| {
            cx.stop_propagation();
        })
        .on_click(cx.listener(|ws, _, _, cx| {
            ws.ui.selected_skill_id = None;
            cx.notify();
        }));

    // Close button
    let close_btn = button_with_icon_l(
        "drawer-close-btn",
        crate::icons::X_SVG,
        "",
        ButtonVariant::Ghost,
        &t,
        cx,
        |ws, _, _, cx| {
            ws.ui.selected_skill_id = None;
            cx.notify();
        },
    );

    // Header title row
    let header = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .p(px(16.0))
        .border_b_1()
        .border_color(t.card_border)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .w(px(9.0))
                                .h(px(9.0))
                                .rounded(px(4.5))
                                .bg(if is_enabled {
                                    crate::rgba_const(0x10b981ff)
                                } else {
                                    t.card_border
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(18.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(t.text_primary)
                                .child(skill.name.clone()),
                        ),
                )
                .child(close_btn),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    crate::icons::svg_icon(source_icon, px(13.0), t.text_secondary),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_secondary)
                        .child(source_label),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_muted)
                        .child(format!("· {updated_str}")),
                ),
        );

    // Central Path mono box
    let central_path_str = skill_folder.display().to_string();
    let copy_path_val = central_path_str.clone();
    let central_path_row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .p(px(8.0))
        .rounded(px(6.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.input_border)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .min_w(px(0.0))
                .child(crate::icons::svg_icon(crate::icons::FOLDER_SVG, px(13.0), t.text_muted))
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_weight(gpui::FontWeight::NORMAL)
                        .text_color(t.text_secondary)
                        .child(central_path_str),
                ),
        )
        .child(
            div()
                .flex()
                .gap(px(4.0))
                .child(button_with_icon_l(
                    "drawer-copy-path",
                    crate::icons::COPY_SVG,
                    i.t("复制", "Copy"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_path_val.clone()));
                        ws.ui.toast(ws.i18n.t("已复制路径", "Path copied").to_string(), false);
                        cx.notify();
                    },
                ))
                .child(button_with_icon_l(
                    "drawer-reveal-path",
                    crate::icons::EXTERNAL_LINK_SVG,
                    i.t("定位", "Reveal"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |_, _, _, _| {
                        super::open_path_in_default_manager(&skill_folder_for_reveal);
                    },
                )),
        );

    // Metadata card (分组 & 备注)
    let sid_for_meta = skill.id.clone();
    let group_str = skill.user_group.clone().unwrap_or_default();
    let note_str = skill.user_note.clone().unwrap_or_default();
    let meta_card = div()
        .flex()
        .items_center()
        .justify_between()
        .p(px(10.0))
        .rounded(px(6.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.input_border)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(t.text_muted)
                                .child(i.t("分组：", "Group:")),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_primary)
                                .child(if group_str.is_empty() {
                                    i.t("未分组", "Ungrouped").to_string()
                                } else {
                                    group_str.clone()
                                }),
                        ),
                )
                .children((!note_str.is_empty()).then(|| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(t.text_muted)
                                .child(i.t("备注：", "Note:")),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(t.text_secondary)
                                .child(note_str.clone()),
                        )
                })),
        )
        .child(button_with_icon_l(
            "drawer-edit-meta",
            crate::icons::PENCIL_SVG,
            i.t("编辑", "Edit"),
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                let g_inp = cx.new(|cx| TextInput::new(&group_str, cx));
                let n_inp = cx.new(|cx| TextInput::new(&note_str, cx));
                ws.ui.skill_editing_metadata = Some((sid_for_meta.clone(), g_inp, n_inp));
                cx.notify();
            },
        ));

    // Tags Row
    let sid_for_add_tag = skill.id.clone();
    let mut tags_section = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_wrap();

    for tag in &skill.tags {
        let tag_clone = tag.clone();
        let sid_del_tag = skill.id.clone();
        let (bg, fg) = tag_color(tag, &t);
        let tag_pill_id = format!("del-tag-{}-{}", sid_del_tag, tag_clone);
        tags_section = tags_section.child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(px(7.0))
                .py(px(2.5))
                .rounded(px(4.0))
                .bg(bg)
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(fg)
                        .child(tag.clone()),
                )
                .child(
                    div()
                        .id(gpui::SharedString::from(tag_pill_id))
                        .cursor_pointer()
                        .child(crate::icons::svg_icon(crate::icons::X_SVG, px(10.0), fg))
                        .on_click(cx.listener(move |ws, _, _, cx| {
                            let mut store = ws.store.store().skills.clone();
                            if let Some(s) = store.skills.iter_mut().find(|s| s.id == sid_del_tag) {
                                s.tags.retain(|t| t != &tag_clone);
                                s.touch();
                            }
                            let _ = ws.store.update(|db| db.skills = store);
                            ws.persist_store();
                            cx.notify();
                        })),
                ),
        );
    }

    tags_section = tags_section.child(button_with_icon_l(
        "drawer-add-tag-btn",
        crate::icons::PLUS_SVG,
        i.t("添加标签", "Add Tag"),
        ButtonVariant::Secondary,
        &t,
        cx,
        move |ws, _, _, cx| {
            let t_inp = cx.new(|cx| TextInput::new("", cx));
            ws.ui.skill_adding_tag = Some((sid_for_add_tag.clone(), t_inp));
            cx.notify();
        },
    ));

    // Tool Sync Grid (4 columns, matching ai-toolbox .toolGrid)
    let mut tool_grid = div().flex().flex_wrap().gap(px(8.0));
    for tool in tools {
        let is_on = skill.is_enabled_in(tool);
        let tool_name = i.t(tool.name_zh(), tool.name_en());
        let sid_tool = skill.id.clone();
        let grid_pill_id = format!("drawer-tool-cell-{}-{}", skill.id, tool.key());
        let tooltip_msg = match tool {
            ToolId::Agents => i.t(
                "通用 Agent 技能目录 (~/.agents/skills)，一般支持除了 Claude 的所有 Agent 工具",
                "Universal Agent skills directory (~/.agents/skills), generally supports all Agent tools except Claude",
            ),
            ToolId::ClaudeCode => i.t("Claude Code 技能目录 (~/.claude/skills)", "Claude Code skills directory (~/.claude/skills)"),
            ToolId::Codex => i.t("Codex 技能目录 (~/.codex/skills)", "Codex skills directory (~/.codex/skills)"),
            ToolId::Pi => i.t("Pi 技能目录 (~/.pi/agent/skills)", "Pi skills directory (~/.pi/agent/skills)"),
            ToolId::OpenCode => i.t("OpenCode 技能目录 (~/.config/opencode/skill)", "OpenCode skills directory (~/.config/opencode/skill)"),
            ToolId::OhMyPi => i.t("Oh My Pi 技能目录 (~/.config/oh-my-pi/skills)", "Oh My Pi skills directory (~/.config/oh-my-pi/skills)"),
            ToolId::Kimi => i.t("Kimi 技能目录 (~/.kimi-code/skills)", "Kimi skills directory (~/.kimi-code/skills)"),
            _ => i.t(tool.name_zh(), tool.name_en()),
        };

        let dot_color = if is_on {
            crate::rgba_const(0x10b981ff)
        } else {
            t.card_border
        };

        tool_grid = tool_grid.child(
            div()
                .id(gpui::SharedString::from(grid_pill_id))
                .w(px(116.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .p(px(8.0))
                .rounded(px(6.0))
                .bg(if is_on { t.accent.opacity(0.08) } else { t.input_bg })
                .border_1()
                .border_color(if is_on { t.accent.opacity(0.5) } else { t.input_border })
                .cursor_pointer()
                .hover(|h| h.bg(t.card_hover))
                .tooltip(move |_window, cx| {
                    cx.new(|_| crate::components::Tooltip::new(tooltip_msg.clone())).into()
                })
                .child(
                    div()
                        .w(px(6.0))
                        .h(px(6.0))
                        .rounded(px(3.0))
                        .bg(dot_color),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(if is_on { gpui::FontWeight::MEDIUM } else { gpui::FontWeight::NORMAL })
                        .text_color(if is_on { t.text_primary } else { t.text_secondary })
                        .child(tool_name),
                )
                .on_click(cx.listener(move |ws, _, _, cx| {
                    let _ = ws.store.update(|db| {
                        skills::toggle_tool(&mut db.skills, &sid_tool, tool);
                    });
                    let mut store = ws.store.store().skills.clone();
                    let idx = store.skills.iter().position(|s| s.id == sid_tool);
                    if let Some(idx) = idx {
                        let settings = store.settings.clone();
                        let entry = &mut store.skills[idx];
                        let target = entry.is_enabled_in(tool);
                        let _ = if target {
                            skills::sync_skill_to_tool(&settings, &ws.paths, entry, tool)
                        } else {
                            skills::remove_skill_from_tool(&settings, &ws.paths, entry, tool)
                        };
                    }
                    let _ = ws.store.update(|db| db.skills = store);
                    ws.persist_store();
                    cx.notify();
                })),
        );
    }

    // Document tabs
    let mut doc_tabs = div().flex().items_center().gap(px(6.0));
    if documents.is_empty() {
        doc_tabs = doc_tabs.child(
            div()
                .px(px(8.0))
                .py(px(3.0))
                .rounded(px(4.0))
                .bg(t.accent)
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(crate::rgba_const(0xffffffff))
                .child("SKILL.md"),
        );
    } else {
        for doc in &documents {
            let fname = doc.filename.clone();
            let is_active = fname == active_doc_name;
            let tab_id = format!("doc-tab-{}", fname);
            let fname_clone = fname.clone();
            doc_tabs = doc_tabs.child(
                div()
                    .id(gpui::SharedString::from(tab_id))
                    .px(px(8.0))
                    .py(px(3.0))
                    .rounded(px(4.0))
                    .bg(if is_active { t.accent } else { t.input_bg })
                    .border_1()
                    .border_color(if is_active { t.accent } else { t.input_border })
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .font_weight(if is_active { gpui::FontWeight::MEDIUM } else { gpui::FontWeight::NORMAL })
                    .text_color(if is_active { crate::rgba_const(0xffffffff) } else { t.text_secondary })
                    .child(fname)
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        ws.ui.skill_detail_active_doc = Some(fname_clone.clone());
                        cx.notify();
                    })),
            );
        }
    }

    // Footer actions
    let sid_for_toggle = skill.id.clone();
    let toggle_btn = button_with_icon_l(
        "drawer-toggle-mgmt",
        if is_enabled { crate::icons::POWER_OFF_SVG } else { crate::icons::POWER_SVG },
        if is_enabled { i.t("停用技能", "Disable Skill") } else { i.t("启用技能", "Enable Skill") },
        ButtonVariant::Secondary,
        &t,
        cx,
        move |ws, _, _, cx| {
            let mut store = ws.store.store().skills.clone();
            let new_state = !is_enabled;
            skills::set_management_enabled(&mut store, &sid_for_toggle, new_state);
            let settings = store.settings.clone();
            if let Some(skill) = store.skills.iter_mut().find(|s| s.id == sid_for_toggle) {
                let tools_to_sync: Vec<ToolId> = skills::skills_tools()
                    .iter()
                    .copied()
                    .filter(|t| skill.is_enabled_in(*t))
                    .collect();
                for tool in tools_to_sync {
                    if new_state {
                        let _ = skills::sync_skill_to_tool(&settings, &ws.paths, skill, tool);
                    } else {
                        let _ = skills::unlink_skill_from_tool(&ws.paths, skill, tool);
                    }
                }
            }
            let _ = ws.store.update(|db| db.skills = store);
            ws.persist_store();
            ws.ui.toast(
                if new_state {
                    ws.i18n.t("已启用技能", "Skill enabled").to_string()
                } else {
                    ws.i18n.t("已停用技能", "Skill disabled").to_string()
                },
                false,
            );
            cx.notify();
        },
    );

    let sid_for_del = skill.id.clone();
    let s_name_for_del = skill.name.clone();
    let del_btn = button_with_icon_l(
        "drawer-del-skill",
        crate::icons::TRASH_SVG,
        i.t("删除", "Delete"),
        ButtonVariant::Danger,
        &t,
        cx,
        move |ws, _, _, cx| {
            ws.ui.confirm = Some(super::ConfirmState {
                title: ws.i18n.t("卸载 Skill", "Uninstall Skill").to_string(),
                message: ws
                    .i18n
                    .t(
                        &format!("确定要彻底卸载删除 Skill “{}” 吗？", s_name_for_del),
                        &format!(
                            "Are you sure you want to uninstall and delete “{}”?",
                            s_name_for_del
                        ),
                    )
                    .to_string(),
                action: super::ConfirmAction::DeleteSkill {
                    id: sid_for_del.clone(),
                },
            });
            ws.ui.selected_skill_id = None;
            cx.notify();
        },
    );

    let sid_for_update = skill.id.clone();
    let is_git = skill.source_type == "git";
    let update_btn = button_with_icon_l(
        "drawer-update-skill",
        crate::icons::REFRESH_SVG,
        i.t("更新", "Update"),
        ButtonVariant::Secondary,
        &t,
        cx,
        move |ws, _, _, cx| {
            if is_git {
                match aitoolplus_core::skill_git::update(
                    &mut ws.store.store_mut().skills,
                    &ws.paths,
                    &sid_for_update,
                ) {
                    Ok(()) => {
                        ws.persist_store();
                        ws.ui.toast("Git Skill updated", false);
                    }
                    Err(error) => ws.ui.toast(error, true),
                }
            } else {
                let mut store = ws.store.store().skills.clone();
                skills::scan_central(&mut store, &ws.paths);
                let _ = ws.store.update(|db| db.skills = store);
                ws.persist_store();
                ws.ui.toast("Skill refreshed", false);
            }
            cx.notify();
        },
    );

    let open_data_dir = button_with_icon_l(
        "drawer-open-data-dir",
        crate::icons::FOLDER_SVG,
        i.t("打开数据目录", "Open Data Dir"),
        ButtonVariant::Secondary,
        &t,
        cx,
        move |_, _, _, _| {
            super::open_path_in_default_manager(&skill_folder_for_central);
        },
    );

    // Right-docked drawer panel
    let panel = div()
        .id("drawer-panel")
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .h_full()
        .w(px(540.0))
        .occlude()
        .bg(t.card_bg)
        .border_l_1()
        .border_color(t.card_border)
        .shadow_lg()
        .flex()
        .flex_col()
        .overflow_hidden()
        .on_scroll_wheel(|_, _, cx| {
            cx.stop_propagation();
        })
        .child(header)
        // Scrollable content
        .child(
            div()
                .id("drawer-scroll-container")
                .flex_1()
                .overflow_y_scroll()
                .p(px(16.0))
                .flex()
                .flex_col()
                .gap(px(14.0))
                .children((!description.is_empty()).then(|| {
                    div()
                        .text_size(px(13.0))
                        .text_color(t.text_secondary)
                        .line_height(gpui::relative(1.4))
                        .child(description)
                }))
                .child(central_path_row)
                .child(meta_card)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_primary)
                                .child(i.t("标签", "Tags")),
                        )
                        .child(tags_section),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(t.text_primary)
                                        .child(i.t("同步状态", "Tool Sync Status")),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(t.text_muted)
                                        .child(format!("已同步 {} / {} 个工具", synced_count, total_count)),
                                ),
                        )
                        .child(tool_grid),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(t.text_primary)
                                        .child(i.t("文档预览", "Documents")),
                                )
                                .child(doc_tabs),
                        )
                        .child(
                            div()
                                .id("drawer-doc-content-scroll")
                                .p(px(12.0))
                                .rounded(px(8.0))
                                .bg(t.input_bg)
                                .border_1()
                                .border_color(t.input_border)
                                .max_h(px(320.0))
                                .overflow_y_scroll()
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .line_height(gpui::relative(1.4))
                                        .text_color(t.text_primary)
                                        .child(current_doc_content),
                                ),
                        ),
                ),
        )
        // Footer actions
        .child(
            div()
                .p(px(12.0))
                .border_t_1()
                .border_color(t.card_border)
                .bg(t.card_bg)
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(open_data_dir)
                        .child(update_btn)
                        .child(toggle_btn),
                )
                .child(del_btn),
        );

    div()
        .id("skill-detail-drawer-overlay")
        .absolute()
        .inset_0()
        .size_full()
        .occlude()
        .on_scroll_wheel(|_, _, cx| {
            cx.stop_propagation();
        })
        .child(backdrop)
        .child(panel)
        .into_any_element()
}

/// Metadata edit modal (Group & Note)
pub(crate) fn render_skill_metadata_modal(
    skill_id: String,
    group_input: gpui::Entity<TextInput>,
    note_input: gpui::Entity<TextInput>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let sid = skill_id.clone();
    let g_in = group_input.clone();
    let n_in = note_input.clone();

    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .w(px(400.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(i.t("技能分组 (Group)", "Skill Group")),
                )
                .child(input_container(&t, group_input)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(i.t("技能备注 (Note)", "Skill Note")),
                )
                .child(input_container(&t, note_input)),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "skill-meta-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.skill_editing_metadata = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "skill-meta-save",
                    i.t("保存", "Save"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let grp = g_in.read(cx).text().trim().to_string();
                        let nte = n_in.read(cx).text().trim().to_string();
                        let mut store = ws.store.store().skills.clone();
                        skills::update_metadata(
                            &mut store,
                            &sid,
                            Some(grp),
                            Some(nte),
                            None,
                        );
                        let _ = ws.store.update(|db| db.skills = store);
                        ws.persist_store();
                        ws.ui.skill_editing_metadata = None;
                        ws.ui.toast(ws.i18n.t("元数据已更新", "Metadata saved").to_string(), false);
                        cx.notify();
                    },
                )),
        );

    modal_scaffold_sized(
        &t,
        &i.t("编辑技能分组与备注", "Edit Skill Group & Note"),
        px(440.0),
        None,
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.skill_editing_metadata = None;
            cx.notify();
        },
    )
}

/// Tag add modal
pub(crate) fn render_skill_add_tag_modal(
    skill_id: String,
    tag_input: gpui::Entity<TextInput>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let sid = skill_id.clone();
    let t_in = tag_input.clone();

    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .w(px(320.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(i.t("输入新标签名称", "Enter new tag name")),
                )
                .child(input_container(&t, tag_input)),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "skill-tag-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.skill_adding_tag = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "skill-tag-save",
                    i.t("添加", "Add"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let new_tag = t_in.read(cx).text().trim().to_string();
                        if !new_tag.is_empty() {
                            let mut store = ws.store.store().skills.clone();
                            if let Some(s) = store.skills.iter_mut().find(|s| s.id == sid) {
                                if !s.tags.contains(&new_tag) {
                                    s.tags.push(new_tag);
                                    s.touch();
                                }
                            }
                            let _ = ws.store.update(|db| db.skills = store);
                            ws.persist_store();
                        }
                        ws.ui.skill_adding_tag = None;
                        cx.notify();
                    },
                )),
        );

    modal_scaffold_sized(
        &t,
        &i.t("添加技能标签", "Add Skill Tag"),
        px(360.0),
        None,
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.skill_adding_tag = None;
            cx.notify();
        },
    )
}

/// Backward compatibility stub for workspace.rs
pub fn render_skill_detail_dialog(
    detail: SkillDetailState,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let title = i.t(
        &format!("Skill 详情 - {}", detail.name),
        &format!("Skill Details - {}", detail.name),
    );

    let path_clone = detail.path.clone();

    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .w(px(680.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(t.text_muted)
                        .child(detail.path.display().to_string()),
                )
                .child(button_l(
                    "skill-dlg-reveal",
                    i.t("在文件管理器中定位", "Reveal in File Manager"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |_, _, _, _| {
                        super::open_path_in_default_manager(&path_clone);
                    },
                )),
        )
        .child(
            div()
                .id("skill-detail-scroll")
                .flex()
                .flex_col()
                .gap(px(4.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.input_border)
                .max_h(px(400.0))
                .overflow_y_scroll()
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_primary)
                        .child(if detail.skill_md.is_empty() {
                            i.t("（未找到 SKILL.md 文档）", "(no SKILL.md found)").to_string()
                        } else {
                            detail.skill_md
                        }),
                ),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .child(button_l(
                    "skill-dlg-close",
                    i.t("关闭", "Close"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.skill_detail_dialog = None;
                        cx.notify();
                    },
                )),
        );

    modal_scaffold_custom(&t, &title, px(720.0), body.into_any_element(), cx, |ws, _, _, cx| {
        ws.ui.skill_detail_dialog = None;
        cx.notify();
    })
}

fn install_git_skill_action(ws: &mut Workspace, url: String, cx: &mut Context<Workspace>) {
    ws.ui.skill_git_modal = None;
    ws.ui.toast(
        ws.i18n
            .t("正在克隆并安装 Git 技能...", "Cloning and installing Git skill...")
            .to_string(),
        false,
    );
    let paths = ws.paths.clone();
    let settings = ws.store.store().skills.settings.clone();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let result = cx
            .background_spawn(async move {
                let mut temporary = aitoolplus_core::skills::SkillsStore {
                    settings,
                    ..Default::default()
                };
                aitoolplus_core::skill_git::install(
                    &mut temporary,
                    &paths,
                    &url,
                    None,
                )
                .map(|name| (name, temporary.skills))
            })
            .await;
        let _ = weak.update(cx, |ws, cx| {
            match result {
                Ok((name, mut skills)) => {
                    for skill in skills.drain(..) {
                        aitoolplus_core::skills::upsert(
                            &mut ws.store.store_mut().skills,
                            skill,
                        );
                    }
                    ws.persist_store();
                    ws.ui.toast(format!("installed {name}"), false);
                }
                Err(error) => ws.ui.toast(error, true),
            }
            cx.notify();
        });
    })
    .detach();
    cx.notify();
}

pub(crate) fn render_skill_git_modal(
    git_input: gpui::Entity<TextInput>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let git_action_input = git_input.clone();

    let body = div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .w(px(460.0))
        .child(
            div()
                .text_size(px(12.5))
                .text_color(t.text_secondary)
                .line_height(gpui::relative(1.4))
                .child(i.t(
                    "输入 Git 仓库 URL，将自动克隆到中央仓库并收录 SKILL.md 技能定义。",
                    "Enter a Git repository URL to clone into central repo and discover SKILL.md.",
                )),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(i.t("Git 仓库地址 (URL)", "Git Repository URL")),
                )
                .child(input_container(&t, git_input)),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .pt(px(4.0))
                .child(button_l(
                    "skill-git-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.skill_git_modal = None;
                        cx.notify();
                    },
                ))
                .child(button_with_icon_l(
                    "skill-git-submit",
                    crate::icons::DOWNLOAD_SVG,
                    i.t("克隆并安装", "Clone & Install"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let url = git_action_input.read(cx).text().trim().to_string();
                        if url.is_empty() {
                            ws.ui.toast(
                                ws.i18n
                                    .t("请输入 Git 仓库地址", "Git URL is required")
                                    .to_string(),
                                true,
                            );
                            cx.notify();
                            return;
                        }
                        install_git_skill_action(ws, url, cx);
                    },
                )),
        );

    modal_scaffold_sized(
        &t,
        &i.t("从 Git 克隆并安装技能", "Clone & Install Git Skill"),
        px(500.0),
        None,
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.skill_git_modal = None;
            cx.notify();
        },
    )
}

fn render_store_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let mut store = ws.store.store().skills.clone();

    // Ensure default repos exist in store and merge any new defaults
    let mut repos_changed = false;
    for def_repo in skills::default_skill_repos() {
        if !store.settings.repos.iter().any(|r| r.owner.eq_ignore_ascii_case(&def_repo.owner) && r.name.eq_ignore_ascii_case(&def_repo.name)) {
            store.settings.repos.push(def_repo);
            repos_changed = true;
        }
    }
    if repos_changed {
        let _ = ws.store.update(|db| db.skills = store.clone());
        ws.persist_store();
    }

    let current_source = ws.ui.skill_store_source;

    // Initial load: if skills.sh results are empty and not loading and no query, load curated
    if current_source == SkillStoreSource::SkillsSh
        && ws.ui.skill_store_results.is_empty()
        && !ws.ui.skill_store_loading
        && ws.ui.skill_store_query.is_empty()
    {
        ws.ui.skill_store_results = skills::curated_skills();
    }

    let mut section = div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .h_full()
        .min_h(px(0.0))
        .gap(px(12.0));

    // Top source switcher
    let source_switcher = div()
        .flex()
        .items_center()
        .gap(px(2.0))
        .p(px(2.0))
        .rounded(px(6.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.card_border)
        .child(button_l(
            "store-source-repos",
            i.t("仓库", "Repositories"),
            if current_source == SkillStoreSource::Repos {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Ghost
            },
            &t,
            cx,
            |ws, _, _, cx| {
                let sh_query = ws.ui.skill_store_search.read(cx).text().trim().to_string();
                if !sh_query.is_empty() {
                    ws.ui.skill_store_repos_search.update(cx, |inp, cx| inp.set_text_silent(sh_query, cx));
                }
                ws.ui.skill_store_source = SkillStoreSource::Repos;
                cx.notify();
            },
        ))
        .child(button_l(
            "store-source-skillssh",
            "skills.sh",
            if current_source == SkillStoreSource::SkillsSh {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Ghost
            },
            &t,
            cx,
            |ws, _, _, cx| {
                let repo_query = ws.ui.skill_store_repos_search.read(cx).text().trim().to_string();
                if !repo_query.is_empty() {
                    ws.ui.skill_store_search.update(cx, |inp, cx| inp.set_text_silent(repo_query.clone(), cx));
                    trigger_store_search(ws, repo_query, cx);
                }
                ws.ui.skill_store_source = SkillStoreSource::SkillsSh;
                cx.notify();
            },
        ));

    let header_row = div()
        .flex()
        .items_center()
        .justify_between()
        .child(section_title(
            &t,
            i.t("发现技能", "Discover Skills"),
            Some(i.t(
                "浏览社区仓库或检索公共技能库，一键安装至本地中心仓库并支持多工具同步",
                "Browse community repos or search public skills, 1-click install & sync across tools",
            )),
        ))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(source_switcher),
        );

    section = section.child(header_row);

    match current_source {
        SkillStoreSource::Repos => {
            let repos_search_input = ws.ui.skill_store_repos_search.clone();
            let repo_filter_val = ws.ui.skill_store_repo_filter.clone();
            let status_filter_val = ws.ui.skill_store_status_filter.clone();

            let repo_btn_label = if repo_filter_val == "all" {
                i.t("全部仓库", "All Repos").to_string()
            } else {
                repo_filter_val.clone()
            };

            let status_btn_label = match status_filter_val.as_str() {
                "installed" => i.t("已安装", "Installed").to_string(),
                "uninstalled" => i.t("未安装", "Uninstalled").to_string(),
                _ => i.t("全部状态", "All Status").to_string(),
            };

            let is_repo_open = ws.ui.skill_store_repo_dropdown_open;
            let is_status_open = ws.ui.skill_store_status_dropdown_open;

            let repo_dropdown_btn = button_with_icon_l(
                "store-repo-filter-btn",
                if is_repo_open {
                    crate::icons::CHEVRON_UP_SVG
                } else {
                    crate::icons::CHEVRON_DOWN_SVG
                },
                repo_btn_label,
                ButtonVariant::Secondary,
                &t,
                cx,
                move |ws, _, _, cx| {
                    if is_repo_open {
                        ws.ui.skill_store_repo_dropdown_open = false;
                    } else {
                        ws.ui.skill_store_repo_dropdown_open = true;
                        ws.ui.skill_store_status_dropdown_open = false;
                    }
                    cx.notify();
                },
            );

            let status_dropdown_btn = button_with_icon_l(
                "store-status-filter-btn",
                if is_status_open {
                    crate::icons::CHEVRON_UP_SVG
                } else {
                    crate::icons::CHEVRON_DOWN_SVG
                },
                status_btn_label,
                ButtonVariant::Secondary,
                &t,
                cx,
                move |ws, _, _, cx| {
                    if is_status_open {
                        ws.ui.skill_store_status_dropdown_open = false;
                    } else {
                        ws.ui.skill_store_status_dropdown_open = true;
                        ws.ui.skill_store_repo_dropdown_open = false;
                    }
                    cx.notify();
                },
            );

            let manage_repos_btn = button_with_icon_l(
                "store-manage-repos-btn",
                crate::icons::SETTINGS_SVG,
                i.t("管理仓库", "Manage Repos"),
                ButtonVariant::Secondary,
                &t,
                cx,
                |ws, _, _, cx| {
                    ws.ui.skill_store_repo_manager_open = true;
                    cx.notify();
                },
            );

            let refresh_btn = button_with_icon_l(
                "store-repos-refresh-btn",
                crate::icons::REFRESH_SVG,
                i.t("重置", "Reset"),
                ButtonVariant::Secondary,
                &t,
                cx,
                |ws, _, _, cx| {
                    ws.ui.skill_store_repos_search.update(cx, |inp, cx| inp.set_text_silent("", cx));
                    ws.ui.skill_store_repo_filter = "all".to_string();
                    ws.ui.skill_store_status_filter = "all".to_string();
                    cx.notify();
                },
            );

            let repos_toolbar = div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(div().flex_1().child(input_container(&t, repos_search_input)))
                .child(repo_dropdown_btn)
                .child(status_dropdown_btn)
                .child(manage_repos_btn)
                .child(refresh_btn);

            section = section.child(repos_toolbar);

            // Filter items
            let query = ws.ui.skill_store_repos_search.read(cx).text().trim().to_string();
            let discovered_skills = skills::discover_repo_skills(
                &store,
                Some(&ws.ui.skill_store_repo_filter),
                Some(&ws.ui.skill_store_status_filter),
                Some(&query),
            );

            if discovered_skills.is_empty() {
                let query_str = query.clone();
                let mut empty_container = div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(14.0))
                    .py(px(32.0));

                if !query_str.is_empty() {
                    let q_switch = query_str.clone();
                    empty_container = empty_container
                        .child(crate::components::empty_state_svg(
                            &t,
                            crate::icons::SEARCH_SVG,
                            i.t("未在已配置的仓库中找到匹配技能", "No matching skills found in configured repositories"),
                            i.t(
                                &format!("当前仅在已添加的 GitHub 仓库中检索；若要查找全球公开发布的“{}”，请切换至 skills.sh", query_str),
                                &format!("Only configured repositories were searched. To find \"{}\" worldwide, switch to skills.sh", query_str),
                            ),
                        ))
                        .child(button_with_icon_l(
                            "switch-to-skillssh-btn",
                            crate::icons::SEARCH_SVG,
                            i.t(
                                &format!("前往 skills.sh 全球技能库搜索 “{}”", query_str),
                                &format!("Search \"{}\" on skills.sh", query_str),
                            ),
                            ButtonVariant::Primary,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                ws.ui.skill_store_source = SkillStoreSource::SkillsSh;
                                ws.ui.skill_store_search.update(cx, |inp, cx| inp.set_text_silent(q_switch.clone(), cx));
                                trigger_store_search(ws, q_switch.clone(), cx);
                            },
                        ));
                } else {
                    empty_container = empty_container.child(crate::components::empty_state_svg(
                        &t,
                        crate::icons::SEARCH_SVG,
                        i.t("未找到匹配的仓库技能", "No skills found in repositories"),
                        i.t("点击【管理仓库】添加更多 GitHub 技能源", "Click 'Manage Repos' to add GitHub repositories"),
                    ));
                }

                section = section.child(empty_container);
            } else {
                // 2-column virtual list
                let rows_count = (discovered_skills.len() + 1) / 2;
                let skills_arc = std::sync::Arc::new(discovered_skills);
                let store_arc = std::sync::Arc::new(store.clone());
                let ws_entity = cx.entity();
                let t_clone = t.clone();
                let i_clone = i.clone();
                let installing_id = ws.ui.skill_store_installing.clone();

                let v_list = uniform_list(
                    "store-repos-vlist",
                    rows_count,
                    move |range: std::ops::Range<usize>, _window: &mut gpui::Window, _cx: &mut gpui::App| -> Vec<gpui::AnyElement> {
                        let mut row_elements = Vec::with_capacity(range.len());
                        for row_idx in range {
                            let idx1 = row_idx * 2;
                            let idx2 = idx1 + 1;

                            let mut row = div()
                                .h(px(116.0))
                                .pb(px(8.0))
                                .flex()
                                .gap(px(10.0))
                                .w_full();

                            if let Some(item1) = skills_arc.get(idx1) {
                                let is_inst1 = store_arc.skills.iter().any(|s| {
                                    s.name.eq_ignore_ascii_case(&item1.skill_id)
                                        || s.name.eq_ignore_ascii_case(&item1.name)
                                        || s.central_path.eq_ignore_ascii_case(&item1.skill_id)
                                });
                                let is_ing1 = installing_id.as_deref() == Some(&item1.id);
                                row = row.child(
                                    div()
                                        .flex_1()
                                        .h_full()
                                        .min_w(px(0.0))
                                        .child(render_virtual_store_skill_card(
                                            item1,
                                            is_inst1,
                                            is_ing1,
                                            &ws_entity,
                                            &t_clone,
                                            &i_clone,
                                        )),
                                );
                            }

                            if let Some(item2) = skills_arc.get(idx2) {
                                let is_inst2 = store_arc.skills.iter().any(|s| {
                                    s.name.eq_ignore_ascii_case(&item2.skill_id)
                                        || s.name.eq_ignore_ascii_case(&item2.name)
                                        || s.central_path.eq_ignore_ascii_case(&item2.skill_id)
                                });
                                let is_ing2 = installing_id.as_deref() == Some(&item2.id);
                                row = row.child(
                                    div()
                                        .flex_1()
                                        .h_full()
                                        .min_w(px(0.0))
                                        .child(render_virtual_store_skill_card(
                                            item2,
                                            is_inst2,
                                            is_ing2,
                                            &ws_entity,
                                            &t_clone,
                                            &i_clone,
                                        )),
                                );
                            } else {
                                row = row.child(div().flex_1().h_full().min_w(px(0.0)));
                            }

                            row_elements.push(row.into_any_element());
                        }
                        row_elements
                    },
                )
                .size_full();

                section = section.child(
                    div()
                        .w_full()
                        .flex_1()
                        .h_full()
                        .min_h(px(0.0))
                        .overflow_hidden()
                        .child(v_list),
                );
            }

            // Dropdown menus
            if ws.ui.skill_store_repo_dropdown_open {
                let backdrop = div()
                    .id("store-repo-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_click(cx.listener(|ws, _, _, cx| {
                        ws.ui.skill_store_repo_dropdown_open = false;
                        cx.notify();
                    }));

                let mut menu = div()
                    .id("store-repo-menu")
                    .occlude()
                    .absolute()
                    .top(px(85.0))
                    .right(px(240.0))
                    .w(px(250.0))
                    .max_h(px(280.0))
                    .overflow_y_scroll()
                    .p(px(4.0))
                    .rounded(px(6.0))
                    .bg(t.card_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .shadow_xl()
                    .flex()
                    .flex_col()
                    .gap(px(2.0));

                let is_all = ws.ui.skill_store_repo_filter == "all";
                menu = menu.child(
                    div()
                        .id("store-repo-opt-all")
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(10.0))
                        .py(px(6.0))
                        .rounded(px(4.0))
                        .text_size(px(12.5))
                        .text_color(if is_all { t.accent } else { t.text_primary })
                        .bg(if is_all { t.card_hover } else { gpui::rgba(0x00000000) })
                        .hover(|h| h.bg(t.card_hover))
                        .child(i.t("全部仓库", "All Repositories"))
                        .children(is_all.then(|| crate::icons::svg_icon(crate::icons::CHECK_SVG, px(12.0), t.accent)))
                        .on_click(cx.listener(|ws, _, _, cx| {
                            ws.ui.skill_store_repo_filter = "all".to_string();
                            ws.ui.skill_store_repo_dropdown_open = false;
                            cx.notify();
                        })),
                );

                for repo in &store.settings.repos {
                    let repo_tag = format!("{}/{}", repo.owner, repo.name);
                    let repo_tag_click = repo_tag.clone();
                    let is_sel = ws.ui.skill_store_repo_filter == repo_tag;
                    let opt_id = format!("store-repo-opt-{}", repo_tag);
                    menu = menu.child(
                        div()
                            .id(gpui::SharedString::from(opt_id))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(px(4.0))
                            .text_size(px(12.5))
                            .text_color(if is_sel { t.accent } else { t.text_primary })
                            .bg(if is_sel { t.card_hover } else { gpui::rgba(0x00000000) })
                            .hover(|h| h.bg(t.card_hover))
                            .child(repo_tag)
                            .children(is_sel.then(|| crate::icons::svg_icon(crate::icons::CHECK_SVG, px(12.0), t.accent)))
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                ws.ui.skill_store_repo_filter = repo_tag_click.clone();
                                ws.ui.skill_store_repo_dropdown_open = false;
                                cx.notify();
                            })),
                    );
                }

                section = section.child(backdrop).child(menu);
            }

            if ws.ui.skill_store_status_dropdown_open {
                let backdrop = div()
                    .id("store-status-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_click(cx.listener(|ws, _, _, cx| {
                        ws.ui.skill_store_status_dropdown_open = false;
                        cx.notify();
                    }));

                let mut menu = div()
                    .id("store-status-menu")
                    .occlude()
                    .absolute()
                    .top(px(85.0))
                    .right(px(160.0))
                    .w(px(140.0))
                    .p(px(4.0))
                    .rounded(px(6.0))
                    .bg(t.card_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .shadow_xl()
                    .flex()
                    .flex_col()
                    .gap(px(2.0));

                let status_options = [
                    ("all", i.t("全部状态", "All Status")),
                    ("installed", i.t("已安装", "Installed")),
                    ("uninstalled", i.t("未安装", "Uninstalled")),
                ];

                for (opt_val, opt_label) in status_options {
                    let is_sel = ws.ui.skill_store_status_filter == opt_val;
                    let opt_val_str = opt_val.to_string();
                    let opt_id = format!("store-status-opt-{}", opt_val);
                    menu = menu.child(
                        div()
                            .id(gpui::SharedString::from(opt_id))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(px(4.0))
                            .text_size(px(12.5))
                            .text_color(if is_sel { t.accent } else { t.text_primary })
                            .bg(if is_sel { t.card_hover } else { gpui::rgba(0x00000000) })
                            .hover(|h| h.bg(t.card_hover))
                            .child(opt_label)
                            .children(is_sel.then(|| crate::icons::svg_icon(crate::icons::CHECK_SVG, px(12.0), t.accent)))
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                ws.ui.skill_store_status_filter = opt_val_str.clone();
                                ws.ui.skill_store_status_dropdown_open = false;
                                cx.notify();
                            })),
                    );
                }

                section = section.child(backdrop).child(menu);
            }
        }
        SkillStoreSource::SkillsSh => {
            let store_search_input = ws.ui.skill_store_search.clone();
            let store_search_action_input = store_search_input.clone();

            let search_bar = div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .child(input_container(&t, store_search_input)),
                )
                .child(button_with_icon_l(
                    "store-search-btn",
                    crate::icons::SEARCH_SVG,
                    i.t("搜索", "Search"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let query = store_search_action_input.read(cx).text().trim().to_string();
                        trigger_store_search(ws, query, cx);
                    },
                ))
                .child(button_with_icon_l(
                    "store-refresh-btn",
                    crate::icons::REFRESH_SVG,
                    i.t("重置推荐", "Reset"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.skill_store_search.update(cx, |inp, cx| inp.set_text_silent("", cx));
                        ws.ui.skill_store_query = String::new();
                        ws.ui.skill_store_results = skills::curated_skills();
                        cx.notify();
                    },
                ));

            // Category tag chips
            const STORE_CHIPS: &[(&str, &str)] = &[
                ("热门精选", ""),
                ("Git 规范", "git"),
                ("前端与 UI", "frontend"),
                ("代码审查", "review"),
                ("深度研究", "research"),
                ("Rust 工程", "rust"),
                ("Claude", "claude"),
                ("文档助手", "notion"),
            ];

            let mut chips_bar = div().flex().items_center().gap(px(6.0)).flex_wrap().child(
                div()
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(t.text_secondary)
                    .child(i.t("热门分类：", "Categories:")),
            );

            for (chip_label, chip_query) in STORE_CHIPS {
                let q_str = chip_query.to_string();
                let chip_id = format!("chip-{}", chip_label);
                let is_active = ws.ui.skill_store_query == q_str;
                chips_bar = chips_bar.child(button_l(
                    gpui::SharedString::from(chip_id),
                    i.t(chip_label, chip_label),
                    if is_active {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        ws.ui.skill_store_search.update(cx, |inp, cx| inp.set_text_silent(q_str.clone(), cx));
                        trigger_store_search(ws, q_str.clone(), cx);
                    },
                ));
            }

            section = section.child(search_bar).child(chips_bar);

            if ws.ui.skill_store_loading && ws.ui.skill_store_results.is_empty() {
                section = section.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .py(px(40.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .text_size(px(13.5))
                                .text_color(t.text_secondary)
                                .child(i.t("正在从 skills.sh 检索技能库…", "Searching skills.sh...").to_string()),
                        ),
                );
            } else if let Some(ref err_msg) = ws.ui.skill_store_error {
                let err_display = err_msg.clone();
                let retry_query = ws.ui.skill_store_query.clone();
                section = section.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(14.0))
                        .py(px(40.0))
                        .child(crate::components::empty_state_svg(
                            &t,
                            crate::icons::INFO_SVG,
                            i.t("检索技能库失败", "Failed to search skills.sh"),
                            err_display,
                        ))
                        .child(button_with_icon_l(
                            "store-retry-search-btn",
                            crate::icons::REFRESH_SVG,
                            i.t("重试检索", "Retry Search"),
                            ButtonVariant::Primary,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                trigger_store_search(ws, retry_query.clone(), cx);
                            },
                        )),
                );
            } else if ws.ui.skill_store_results.is_empty() {
                section = section.child(crate::components::empty_state_svg(
                    &t,
                    crate::icons::SEARCH_SVG,
                    i.t("未找到匹配的技能", "No skills found"),
                    i.t("请尝试更换关键词，例如：git, review, rust, claude...", "Try a different search query"),
                ));
            } else {
                let results = ws.ui.skill_store_results.clone();
                let rows_count = (results.len() + 1) / 2;
                let skills_arc = std::sync::Arc::new(results);
                let store_arc = std::sync::Arc::new(store.clone());
                let ws_entity = cx.entity();
                let t_clone = t.clone();
                let i_clone = i.clone();
                let installing_id = ws.ui.skill_store_installing.clone();

                let v_list = uniform_list(
                    "store-skillssh-vlist",
                    rows_count,
                    move |range: std::ops::Range<usize>, _window: &mut gpui::Window, _cx: &mut gpui::App| -> Vec<gpui::AnyElement> {
                        let mut row_elements = Vec::with_capacity(range.len());
                        for row_idx in range {
                            let idx1 = row_idx * 2;
                            let idx2 = idx1 + 1;

                            let mut row = div()
                                .h(px(116.0))
                                .pb(px(8.0))
                                .flex()
                                .gap(px(10.0))
                                .w_full();

                            if let Some(item1) = skills_arc.get(idx1) {
                                let is_inst1 = store_arc.skills.iter().any(|s| {
                                    s.name.eq_ignore_ascii_case(&item1.skill_id)
                                        || s.name.eq_ignore_ascii_case(&item1.name)
                                        || s.central_path.eq_ignore_ascii_case(&item1.skill_id)
                                });
                                let is_ing1 = installing_id.as_deref() == Some(&item1.id);
                                row = row.child(
                                    div()
                                        .flex_1()
                                        .h_full()
                                        .min_w(px(0.0))
                                        .child(render_virtual_store_skill_card(
                                            item1,
                                            is_inst1,
                                            is_ing1,
                                            &ws_entity,
                                            &t_clone,
                                            &i_clone,
                                        )),
                                );
                            }

                            if let Some(item2) = skills_arc.get(idx2) {
                                let is_inst2 = store_arc.skills.iter().any(|s| {
                                    s.name.eq_ignore_ascii_case(&item2.skill_id)
                                        || s.name.eq_ignore_ascii_case(&item2.name)
                                        || s.central_path.eq_ignore_ascii_case(&item2.skill_id)
                                });
                                let is_ing2 = installing_id.as_deref() == Some(&item2.id);
                                row = row.child(
                                    div()
                                        .flex_1()
                                        .h_full()
                                        .min_w(px(0.0))
                                        .child(render_virtual_store_skill_card(
                                            item2,
                                            is_inst2,
                                            is_ing2,
                                            &ws_entity,
                                            &t_clone,
                                            &i_clone,
                                        )),
                                );
                            } else {
                                row = row.child(div().flex_1().h_full().min_w(px(0.0)));
                            }

                            row_elements.push(row.into_any_element());
                        }
                        row_elements
                    },
                )
                .size_full();

                section = section.child(
                    div()
                        .w_full()
                        .flex_1()
                        .h_full()
                        .min_h(px(0.0))
                        .overflow_hidden()
                        .child(v_list),
                );

                // Footer with Load More + Powered by skills.sh
                let mut footer = div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pt(px(2.0))
                    .px(px(4.0));

                if ws.ui.skill_store_has_more {
                    footer = footer.child(button_with_icon_l(
                        "store-load-more-btn",
                        crate::icons::CHEVRON_DOWN_SVG,
                        if ws.ui.skill_store_loading {
                            i.t("加载中…", "Loading…")
                        } else {
                            i.t("加载更多", "Load More")
                        },
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        |ws, _, _, cx| {
                            load_more_store_search(ws, cx);
                        },
                    ));
                } else {
                    footer = footer.child(div());
                }

                footer = footer.child(
                    div()
                        .text_size(px(11.5))
                        .text_color(t.text_muted)
                        .child(i.t("由 skills.sh 提供公共检索", "Powered by skills.sh")),
                );

                section = section.child(footer);
            }
        }
    }

    section.into_any_element()
}

fn render_virtual_store_skill_card(
    item: &StoreSkillItem,
    is_installed: bool,
    is_installing: bool,
    ws_entity: &gpui::Entity<Workspace>,
    t: &crate::theme::Theme,
    i: &crate::i18n::I18n,
) -> gpui::AnyElement {
    let item_clone = item.clone();
    let id_str = item.id.clone();
    let show_subpath = !item.skill_id.is_empty() && !item.skill_id.eq_ignore_ascii_case(&item.name);
    let github_url = format!("https://github.com/{}/{}", item.repo_owner, item.repo_name);
    let gh_url_for_open = github_url.clone();

    let mut action_btns = div().flex().items_center().gap(px(6.0));

    // GitHub 查看 button
    action_btns = action_btns.child(
        div()
            .id(gpui::SharedString::from(format!("vcard-gh-{}", id_str)))
            .cursor_pointer()
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(5.0))
            .text_size(px(11.5))
            .text_color(t.text_secondary)
            .bg(t.card_hover)
            .border_1()
            .border_color(t.card_border)
            .hover(|h| h.bg(t.input_bg))
            .child(crate::icons::svg_icon(crate::icons::EXTERNAL_LINK_SVG, px(12.0), t.text_secondary))
            .child(i.t("查看", "View"))
            .on_click(move |_, _, _| {
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("rundll32")
                        .arg("url.dll,FileProtocolHandler")
                        .arg(&gh_url_for_open)
                        .spawn();
                }
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open").arg(&gh_url_for_open).spawn();
                }
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("xdg-open").arg(&gh_url_for_open).spawn();
                }
            }),
    );

    // Install / Installed status
    if is_installed {
        action_btns = action_btns.child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(5.0))
                .text_size(px(11.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(crate::rgba_const(0x10b981ff))
                .bg(crate::rgba_const(0x10b98118))
                .border_1()
                .border_color(crate::rgba_const(0x10b98140))
                .child(crate::icons::svg_icon(crate::icons::CHECK_SVG, px(11.0), crate::rgba_const(0x10b981ff)))
                .child(i.t("已安装", "Installed")),
        );
    } else if is_installing {
        action_btns = action_btns.child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(5.0))
                .text_size(px(11.5))
                .text_color(t.text_muted)
                .bg(t.card_hover)
                .child(i.t("安装中…", "Installing…")),
        );
    } else {
        let ws_entity_install = ws_entity.clone();
        let item_for_install = item_clone.clone();
        action_btns = action_btns.child(
            div()
                .id(gpui::SharedString::from(format!("vcard-install-{}", id_str)))
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(px(9.0))
                .py(px(4.0))
                .rounded(px(5.0))
                .text_size(px(11.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(gpui::rgb(0xffffff))
                .bg(t.accent)
                .hover(|h| h.opacity(0.9))
                .child(crate::icons::svg_icon(crate::icons::PLUS_SVG, px(12.0), gpui::rgb(0xffffff)))
                .child(i.t("一键安装", "Install"))
                .on_click(move |_, _, cx| {
                    let item_to_install = item_for_install.clone();
                    ws_entity_install.update(cx, |ws, cx| {
                        install_store_skill_action(ws, item_to_install, cx);
                    });
                }),
        );
    }

    div()
        .id(gpui::SharedString::from(format!("store-card-{}", id_str)))
        .flex()
        .flex_col()
        .justify_between()
        .h_full()
        .p(px(10.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(if is_installed { crate::rgba_const(0x10b98140) } else { t.card_border })
        .hover(|h| h.bg(t.card_hover).border_color(t.accent.opacity(0.4)))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
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
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .text_size(px(13.5))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(t.text_primary)
                                        .truncate()
                                        .child(item.name.clone()),
                                )
                                .children(show_subpath.then(|| {
                                    div()
                                        .text_size(px(11.0))
                                        .font_family(".AppleSystemUIFontMonospaced, Consolas, monospace")
                                        .text_color(t.text_muted)
                                        .truncate()
                                        .child(item.skill_id.clone())
                                })),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .flex_shrink_0()
                                .children((item.installs > 0).then(|| {
                                    badge(
                                        t,
                                        format!("🔥 {}", format_installs(item.installs)),
                                        BadgeKind::Neutral,
                                    )
                                })),
                        ),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_secondary)
                        .line_height(gpui::relative(1.3))
                        .max_h(px(32.0))
                        .overflow_hidden()
                        .child(if item.description.trim().is_empty() {
                            format!("来自 {}/{}", item.repo_owner, item.repo_name)
                        } else {
                            item.description.trim().to_string()
                        }),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .pt(px(6.0))
                .border_t_1()
                .border_color(t.card_border.opacity(0.5))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(crate::icons::svg_icon(crate::icons::GITHUB_SVG, px(12.0), t.text_muted))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child(format!("{}/{}", item.repo_owner, item.repo_name)),
                        ),
                )
                .child(action_btns),
        )
        .into_any_element()
}

fn parse_repo_owner_name(input: &str) -> Option<(String, String)> {
    let mut s = input.trim();
    if let Some(rest) = s.strip_prefix("https://github.com/") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix("http://github.com/") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix("git@github.com:") {
        s = rest;
    }
    if let Some(rest) = s.strip_suffix(".git") {
        s = rest;
    }
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

pub(crate) fn render_skill_repo_manager_modal(
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let store = ws.store.store().skills.clone();
    let configured_repos = if store.settings.repos.is_empty() {
        skills::default_skill_repos()
    } else {
        store.settings.repos.clone()
    };

    let url_input = ws.ui.skill_store_new_repo_url.clone();
    let branch_input = ws.ui.skill_store_new_repo_branch.clone();
    let url_input_action = url_input.clone();
    let branch_input_action = branch_input.clone();

    // 1. Add repo form card
    let add_form = div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(14.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(t.text_primary)
                .child(i.t("添加自定义 GitHub 技能仓库", "Add GitHub Skill Repository")),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_secondary)
                        .child(i.t("仓库地址 (URL 或 owner/repo)", "Repository URL")),
                )
                .child(input_container(&t, url_input)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(t.text_secondary)
                                .child(i.t("默认分支 (默认 main)", "Branch")),
                        )
                        .child(input_container(&t, branch_input)),
                )
                .child(
                    div()
                        .pt(px(18.0))
                        .child(button_with_icon_l(
                            "add-repo-btn",
                            crate::icons::PLUS_SVG,
                            i.t("添加仓库", "Add Repo"),
                            ButtonVariant::Primary,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                let raw_url = url_input_action.read(cx).text().trim().to_string();
                                let mut branch = branch_input_action.read(cx).text().trim().to_string();
                                if branch.is_empty() {
                                    branch = "main".to_string();
                                }
                                if let Some((owner, name)) = parse_repo_owner_name(&raw_url) {
                                    let mut store = ws.store.store().skills.clone();
                                    skills::add_skill_repo(
                                        &mut store.settings,
                                        skills::SkillRepo {
                                            owner: owner.clone(),
                                            name: name.clone(),
                                            branch,
                                            enabled: true,
                                        },
                                    );
                                    let _ = ws.store.update(|db| db.skills = store);
                                    ws.persist_store();
                                    url_input_action.update(cx, |inp, cx| inp.set_text_silent("", cx));
                                    ws.ui.toast(format!("成功添加仓库 {}/{}", owner, name), false);
                                    cx.notify();
                                } else {
                                    ws.ui.toast(
                                        ws.i18n.t(
                                            "请输入合法的 GitHub 仓库地址 (如 owner/repo 或完整 GitHub URL)",
                                            "Please enter a valid GitHub repository (owner/repo or URL)",
                                        ).to_string(),
                                        true,
                                    );
                                    cx.notify();
                                }
                            },
                        )),
                ),
        );

    // 2. Repos list card
    let mut repo_items = div().flex().flex_col().gap(px(6.0));
    for repo in configured_repos.clone() {
        let owner = repo.owner.clone();
        let name = repo.name.clone();
        let branch = repo.branch.clone();
        let repo_tag = format!("{}/{}", owner, name);
        let repo_url = format!("https://github.com/{}/{}", owner, name);
        let repo_url_open = repo_url.clone();

        // Skill count for this repo
        let skill_count = skills::discover_repo_skills(&store, Some(&repo_tag), Some("all"), None).len();

        let owner_for_del = owner.clone();
        let name_for_del = name.clone();

        let row = div()
            .flex()
            .items_center()
            .justify_between()
            .p(px(10.0))
            .rounded(px(6.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border)
            .hover(|h| h.bg(t.card_hover))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(crate::icons::svg_icon(crate::icons::GITHUB_SVG, px(15.0), t.text_primary))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(t.text_primary)
                                    .child(repo_tag),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(t.text_muted)
                                            .child(format!("分支: {branch}")),
                                    )
                                    .child(badge(
                                        &t,
                                        format!("{skill_count} 个技能"),
                                        BadgeKind::Neutral,
                                    )),
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
                            .id(gpui::SharedString::from(format!("repo-open-{}", repo_url)))
                            .cursor_pointer()
                            .p(px(6.0))
                            .rounded(px(4.0))
                            .hover(|h| h.bg(t.input_bg))
                            .child(crate::icons::svg_icon(crate::icons::EXTERNAL_LINK_SVG, px(13.0), t.text_secondary))
                            .on_click(move |_, _, _| {
                                #[cfg(target_os = "windows")]
                                {
                                    let _ = std::process::Command::new("rundll32")
                                        .arg("url.dll,FileProtocolHandler")
                                        .arg(&repo_url_open)
                                        .spawn();
                                }
                                #[cfg(target_os = "macos")]
                                {
                                    let _ = std::process::Command::new("open").arg(&repo_url_open).spawn();
                                }
                                #[cfg(target_os = "linux")]
                                {
                                    let _ = std::process::Command::new("xdg-open").arg(&repo_url_open).spawn();
                                }
                            }),
                    )
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!("repo-del-{}-{}", owner, name)))
                            .cursor_pointer()
                            .p(px(6.0))
                            .rounded(px(4.0))
                            .hover(|h| h.bg(crate::rgba_const(0xef444420)))
                            .child(crate::icons::svg_icon(crate::icons::TRASH_SVG, px(13.0), crate::rgba_const(0xef4444ff)))
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                let mut store = ws.store.store().skills.clone();
                                skills::remove_skill_repo(&mut store.settings, &owner_for_del, &name_for_del);
                                let _ = ws.store.update(|db| db.skills = store);
                                ws.persist_store();
                                ws.ui.toast(format!("已移除仓库 {}/{}", owner_for_del, name_for_del), false);
                                cx.notify();
                            })),
                    ),
            );

        repo_items = repo_items.child(row);
    }

    let repos_list_card = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.text_primary)
                        .child(format!("已配置仓库（{}）", configured_repos.len())),
                ),
        )
        .child(
            div()
                .id("repo-manager-items-scroll")
                .max_h(px(260.0))
                .overflow_y_scroll()
                .child(repo_items),
        );

    let body = div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .w(px(520.0))
        .child(
            div()
                .text_size(px(12.5))
                .text_color(t.text_secondary)
                .line_height(gpui::relative(1.4))
                .child(i.t(
                    "配置常用技能源仓库。在「仓库」模式下可快速筛选和一键导入这些仓库中的 Skills。",
                    "Configure upstream repositories. You can filter and 1-click install skills from these repos in Repos mode.",
                )),
        )
        .child(add_form)
        .child(repos_list_card)
        .child(
            div()
                .flex()
                .justify_end()
                .pt(px(4.0))
                .child(button_l(
                    "repo-manager-close-btn",
                    i.t("完成", "Done"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.skill_store_repo_manager_open = false;
                        cx.notify();
                    },
                )),
        );

    modal_scaffold_sized(
        &t,
        &i.t("技能仓库管理", "Manage Skill Repositories"),
        px(560.0),
        None,
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.skill_store_repo_manager_open = false;
            cx.notify();
        },
    )
}

fn scan_and_import_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let i = ws.i18n;
    let mut store = ws.store.store().skills.clone();
    match skills::scan_and_import_existing(&ws.paths, &mut store) {
        Ok(imported) => {
            let _ = ws.store.update(|db| db.skills = store);
            ws.persist_store();
            if imported.is_empty() {
                ws.ui.toast(
                    i.t(
                        "扫描完成：所有已知工具的技能已全部收录",
                        "Scan complete: all tool skills are already imported",
                    )
                    .to_string(),
                    false,
                );
            } else {
                let count = imported.len();
                let sample = if imported.len() > 3 {
                    format!("{} 等", imported[..3].join(", "))
                } else {
                    imported.join(", ")
                };
                ws.ui.toast(
                    i.t(
                        &format!("成功自动扫描并导入 {count} 个技能：{sample}"),
                        &format!("Discovered and imported {count} skills: {sample}"),
                    )
                    .to_string(),
                    false,
                );
            }
        }
        Err(e) => {
            ws.ui.toast(format!("扫描导入失败: {e}"), true);
        }
    }
    cx.notify();
}

pub(crate) fn trigger_store_search(ws: &mut Workspace, query: String, cx: &mut Context<Workspace>) {
    ws.ui.skill_store_query = query.clone();
    ws.ui.skill_store_offset = 0;
    ws.ui.skill_store_loading = true;
    ws.ui.skill_store_error = None;
    ws.ui.skill_store_results.clear();
    cx.notify();

    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let q = query.clone();
        let res = cx
            .background_spawn(async move { skills::search_skills_store(&q, 20, 0) })
            .await;

        let _ = weak.update(cx, |ws, cx| {
            ws.ui.skill_store_loading = false;
            match res {
                Ok(result) => {
                    ws.ui.skill_store_error = None;
                    ws.ui.skill_store_has_more = result.skills.len() >= 20;
                    ws.ui.skill_store_results = result.skills;
                }
                Err(e) => {
                    ws.ui.skill_store_error = Some(e.clone());
                    ws.ui.toast(format!("搜索失败: {e}"), true);
                }
            }
            cx.notify();
        });
    })
    .detach();
}

pub(crate) fn load_more_store_search(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if ws.ui.skill_store_loading {
        return;
    }
    let next_offset = ws.ui.skill_store_offset + 20;
    ws.ui.skill_store_offset = next_offset;
    ws.ui.skill_store_loading = true;
    cx.notify();

    let weak = cx.entity().downgrade();
    let query = ws.ui.skill_store_query.clone();
    cx.spawn(async move |_this, cx| {
        let q = query.clone();
        let res = cx
            .background_spawn(async move { skills::search_skills_store(&q, 20, next_offset) })
            .await;

        let _ = weak.update(cx, |ws, cx| {
            ws.ui.skill_store_loading = false;
            match res {
                Ok(result) => {
                    let len = result.skills.len();
                    ws.ui.skill_store_has_more = len >= 20;
                    for skill in result.skills {
                        if !ws.ui.skill_store_results.iter().any(|s| s.id == skill.id) {
                            ws.ui.skill_store_results.push(skill);
                        }
                    }
                }
                Err(e) => {
                    ws.ui.toast(format!("加载更多失败: {e}"), true);
                }
            }
            cx.notify();
        });
    })
    .detach();
}

fn install_store_skill_action(
    ws: &mut Workspace,
    item: StoreSkillItem,
    cx: &mut Context<Workspace>,
) {
    let id = item.id.clone();
    let name = item.name.clone();
    ws.ui.skill_store_installing = Some(id);
    ws.ui.toast(format!("正在下载并安装技能 {}...", name), false);
    cx.notify();

    let weak = cx.entity().downgrade();
    let paths = ws.paths.clone();
    let settings = ws.store.store().skills.settings.clone();

    cx.spawn(async move |_this, cx| {
        let item_clone = item.clone();
        let result = cx
            .background_spawn(async move {
                let mut temporary = skills::SkillsStore {
                    settings,
                    ..Default::default()
                };
                skills::install_from_store(&paths, &mut temporary, &item_clone)
                    .map(|installed_name| (installed_name, temporary.skills))
            })
            .await;

        let _ = weak.update(cx, |ws, cx| {
            ws.ui.skill_store_installing = None;
            match result {
                Ok((installed_name, mut new_skills)) => {
                    for s in new_skills.drain(..) {
                        skills::upsert(&mut ws.store.store_mut().skills, s);
                    }
                    skills::scan_central(&mut ws.store.store_mut().skills, &ws.paths);
                    ws.persist_store();
                    ws.ui.toast(
                        format!("技能 “{}” 安装成功！已添加至中央仓库", installed_name),
                        false,
                    );
                }
                Err(e) => {
                    ws.ui.toast(format!("安装失败: {e}"), true);
                }
            }
            cx.notify();
        });
    })
    .detach();
}

fn import_skill_dir(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let weak = cx.entity().downgrade();
    let paths = ws.paths.clone();
    let settings = ws.store.store().skills.settings.clone();

    cx.spawn(async move |_this, cx| {
        let dialog = rfd::AsyncFileDialog::new().set_title("选择包含 SKILL.md 的目录");
        if let Some(folder) = dialog.pick_folder().await {
            let path = folder.path().to_path_buf();
            let result = cx
                .background_spawn(async move {
                    skills::install_into_central(&settings, &paths, &path)
                })
                .await;

            let _ = weak.update(cx, |ws, cx| {
                match result {
                    Ok(name) => {
                        skills::scan_central(&mut ws.store.store_mut().skills, &ws.paths);
                        ws.persist_store();
                        ws.ui.toast(format!("已导入技能 “{name}”"), false);
                    }
                    Err(e) => {
                        ws.ui.toast(format!("导入失败: {e}"), true);
                    }
                }
                cx.notify();
            });
        }
    })
    .detach();
}

fn install_from_zip_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if ws.ui.skills_busy {
        return;
    }
    let weak = cx.entity().downgrade();
    let paths = ws.paths.clone();

    cx.spawn(async move |_this, cx| {
        let dialog = rfd::AsyncFileDialog::new()
            .set_title("选择包含 Skill 的 ZIP 压缩包")
            .add_filter("ZIP 压缩包 (*.zip)", &["zip"]);
        if let Some(file_handle) = dialog.pick_file().await {
            let zip_path = file_handle.path().to_path_buf();
            let store = weak.update(cx, |ws, cx| {
                if ws.ui.skills_busy {
                    return None;
                }
                ws.ui.skills_busy = true;
                cx.notify();
                let skills = ws.store.store().skills.clone();
                Some((skills.clone(), skills))
            });
            let Some((original, mut store)) = store.ok().flatten() else {
                return;
            };
            let result = cx
                .background_spawn(async move {
                    skills::install_from_zip(&paths, &mut store, &zip_path)
                        .map(|installed| (installed, store))
                })
                .await;
            let _ = weak.update(cx, |ws, cx| {
                ws.ui.skills_busy = false;
                match result {
                    Ok((installed, store)) => {
                        let _ = ws.store.update(|db| {
                            merge_skill_snapshot(&mut db.skills, store, &original);
                        });
                        ws.persist_store();
                        let count = installed.len();
                        let names = installed.join(", ");
                        ws.ui.toast(format!("成功从 ZIP 安装 {count} 个技能: {names}"), false);
                    }
                    Err(e) => ws.ui.toast(format!("ZIP 安装失败: {e}"), true),
                }
                cx.notify();
            });
        }
    })
    .detach();
}

fn take_if_untouched<T: PartialEq + Clone>(current: &mut T, before: &T, after: &T) {
    if current == before {
        *current = after.clone();
    }
}

/// Copy fields the background job changed, but keep a field the user edited
/// after the snapshot was taken.
fn merge_skill_record(current: &mut skills::Skill, before: &skills::Skill, after: &skills::Skill) {
    take_if_untouched(&mut current.name, &before.name, &after.name);
    take_if_untouched(&mut current.source_type, &before.source_type, &after.source_type);
    take_if_untouched(&mut current.source_ref, &before.source_ref, &after.source_ref);
    take_if_untouched(&mut current.central_path, &before.central_path, &after.central_path);
    take_if_untouched(&mut current.content_hash, &before.content_hash, &after.content_hash);
    take_if_untouched(&mut current.updated_at, &before.updated_at, &after.updated_at);
    take_if_untouched(&mut current.last_sync_at, &before.last_sync_at, &after.last_sync_at);
    take_if_untouched(&mut current.status, &before.status, &after.status);
    take_if_untouched(&mut current.sort_index, &before.sort_index, &after.sort_index);
    take_if_untouched(&mut current.user_group, &before.user_group, &after.user_group);
    take_if_untouched(&mut current.user_note, &before.user_note, &after.user_note);
    take_if_untouched(
        &mut current.management_enabled,
        &before.management_enabled,
        &after.management_enabled,
    );
    take_if_untouched(&mut current.tags, &before.tags, &after.tags);
    take_if_untouched(&mut current.enabled_tools, &before.enabled_tools, &after.enabled_tools);
    take_if_untouched(&mut current.sync_details, &before.sync_details, &after.sync_details);
    take_if_untouched(&mut current.description, &before.description, &after.description);
    take_if_untouched(&mut current.origin_tool, &before.origin_tool, &after.origin_tool);
}

fn merge_skill_snapshot(
    current: &mut skills::SkillsStore,
    result: skills::SkillsStore,
    before: &skills::SkillsStore,
) {
    let before_ids: std::collections::HashSet<&str> =
        before.skills.iter().map(|skill| skill.id.as_str()).collect();
    for after in result.skills {
        if let Some(slot) = current.skills.iter_mut().find(|item| item.id == after.id) {
            if let Some(original) = before.skills.iter().find(|item| item.id == after.id) {
                merge_skill_record(slot, original, &after);
            }
        } else if !before_ids.contains(after.id.as_str()) {
            current.skills.push(after);
        }
    }
}

fn sync_all_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if ws.ui.skills_busy {
        return;
    }
    ws.ui.skills_busy = true;
    cx.notify();
    let paths = ws.paths.clone();
    let mut store = ws.store.store().skills.clone();
    let original = store.clone();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let report = cx
            .background_spawn(async move {
                let report = skills::sync_all(&mut store, &paths);
                (report, store)
            })
            .await;
        let _ = weak.update(cx, |ws, cx| {
            ws.ui.skills_busy = false;
            let (report, store) = report;
            let _ = ws.store.update(|db| {
                merge_skill_snapshot(&mut db.skills, store, &original);
            });
            ws.persist_store();
            let total_ok: usize = report.iter().map(|(_, ok, _)| ok).sum();
            let total_failed: usize = report.iter().map(|(_, _, f)| f).sum();
            let msg = if total_failed > 0 {
                ws.i18n.t(
                    &format!("同步完成：成功 {total_ok}，失败 {total_failed}"),
                    &format!("sync done: {total_ok} ok, {total_failed} failed"),
                )
            } else {
                ws.i18n.t(
                    &format!("同步完成：所有已启用工具同步成功 ({total_ok})"),
                    &format!("sync done: {total_ok} ok"),
                )
            };
            ws.ui.toast(msg.to_string(), total_failed > 0);
            cx.notify();
        });
    })
    .detach();
}

fn check_and_update_all_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if ws.ui.skills_busy {
        return;
    }
    ws.ui.skills_busy = true;
    cx.notify();
    let paths = ws.paths.clone();
    let mut store = ws.store.store().skills.clone();
    let original = store.clone();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let result = cx
            .background_spawn(async move {
                let res = skills::update_all_skills(&mut store, &paths);
                (res, store)
            })
            .await;
        let _ = weak.update(cx, |ws, cx| {
            ws.ui.skills_busy = false;
            let ((updated, failed), store) = result;
            let _ = ws.store.update(|db| {
                merge_skill_snapshot(&mut db.skills, store, &original);
            });
            ws.persist_store();
            let msg = if updated > 0 {
                ws.i18n
                    .t(
                        &format!("检查完成：已更新 {updated} 个技能"),
                        &format!("Check complete: updated {updated} skills"),
                    )
                    .to_string()
            } else if failed > 0 {
                ws.i18n
                    .t(
                        &format!("检查完成：{failed} 个技能更新失败，请检查网络"),
                        &format!("Check complete: {failed} skills failed to update"),
                    )
                    .to_string()
            } else {
                ws.i18n
                    .t(
                        "检查完成：所有已安装技能均为最新版本",
                        "Check complete: all skills are up to date",
                    )
                    .to_string()
            };
            ws.ui.toast(msg, failed > 0);
            cx.notify();
        });
    })
    .detach();
}

