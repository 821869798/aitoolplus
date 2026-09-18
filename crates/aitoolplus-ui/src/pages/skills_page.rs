use aitoolplus_core::skills::{self, Skill};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, div, prelude::*, px};
use serde_json::Value;

use crate::components::{
    BadgeKind, ButtonVariant, badge, button_l, button_with_icon_l, card, input_container,
    section_title,
};
use crate::workspace::Workspace;

use super::{SkillDetailState, modal_scaffold_custom};

pub const RECOMMENDED_SKILLS: &[(&str, &str, &str)] = &[
    ("ponytail", "极简开发原则", "https://github.com/ponytail-ai/ponytail.git"),
    ("gpui-kit", "GPUI Kit 设计规范", "https://github.com/gpui-kit/skills.git"),
    ("git-commit-helper", "Git 规范提交助手", "https://github.com/skills/git-commit-helper.git"),
    ("code-reviewer", "代码审查专家", "https://github.com/skills/code-reviewer.git"),
];


pub fn render_skills_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    // First visit: scan the central repo.
    if !ws.ui.skills_discovered {
        let added = {
            let mut store = ws.store.store().skills.clone();
            let added = skills::scan_central(&mut store, &ws.paths);
            let _ = ws.store.update(|db| db.skills = store);
            added
        };
        if added > 0 {
            ws.persist_store();
        }
        ws.ui.skills_discovered = true;
        let _ = added;
    }

    let store = ws.store.store().skills.clone();
    let repo = skills::central_repo_path(&store.settings, &ws.paths);
    let all_skills = skills::list(&store);
    let tools: Vec<ToolId> = skills::skills_tools().to_vec();

    let sync_label = i.t("全部同步", "Sync All");
    let import_label = i.t("导入 Skill 目录", "Import Skill Dir");

    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(section_title(
                    &t,
                    i.t("中央仓库", "Central Repo"),
                    Some(gpui::SharedString::from(repo.display().to_string())),
                ))
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(button_l(
                            "skills-sync",
                            sync_label,
                            ButtonVariant::Primary,
                            &t,
                            cx,
                            |ws, _, _, cx| sync_all_action(ws, cx),
                        ))
                        .child(button_l(
                            "skills-import",
                            import_label,
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                import_skill_dir(ws, cx);
                            },
                        )),
                ),
        );

    let git_input = ws.ui.skill_git_url.clone();
    let git_action_input = git_input.clone();

    // Recommended presets bar
    let mut rec_bar = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_wrap()
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(t.text_secondary)
                .child(i.t("推荐 Skill：", "Recommended:")),
        );
    for (r_name, r_desc, r_url) in RECOMMENDED_SKILLS {
        let url_str = r_url.to_string();
        let r_id = *r_name;
        let label = format!("{r_name} ({r_desc})");
        rec_bar = rec_bar.child(button_with_icon_l(
            gpui::SharedString::from(format!("rec-skill-chip-{r_id}")),
            crate::icons::PLUS_SVG,
            label,
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                ws.ui.skill_git_url.update(cx, |inp, cx| {
                    inp.set_text_silent(url_str.clone(), cx);
                });
                cx.notify();
            },
        ));
    }

    section = section
        .child(rec_bar)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .p(px(10.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(t.card_border)
                .child(section_title(
                    &t,
                    i.t("从 Git 安装", "Install from Git"),
                    Some(i.t(
                        "仓库根目录必须包含 SKILL.md",
                        "Repository root must contain SKILL.md",
                    )),
                ))
                .child(git_input)
                .child(button_l(
                    "skills-git-install",
                    i.t("克隆并安装", "Clone & Install"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let url =
                            git_action_input.update(cx, |input, _| input.text().trim().to_string());
                        if url.is_empty() {
                            ws.ui.toast("Git URL is required", true);
                            cx.notify();
                            return;
                        }
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
                    },
                )),
        )
        .child(
            div()
                .w(px(320.0))
                .child(input_container(&t, ws.ui.skill_search.clone())),
        );

    let query = ws.ui.skill_search.read(cx).text().to_lowercase();
    let filtered_skills: Vec<_> = all_skills
        .into_iter()
        .filter(|s| {
            query.is_empty()
                || s.name.to_lowercase().contains(&query)
                || s.user_note.as_deref().is_some_and(|n| n.to_lowercase().contains(&query))
                || s.central_path.to_lowercase().contains(&query)
        })
        .collect();

    if filtered_skills.is_empty() {
        section = section.child(crate::components::empty_state_svg(
            &t,
            crate::icons::SPARKLES_SVG,
            if query.is_empty() {
                i.t("中央仓库为空", "Central repo is empty")
            } else {
                i.t("没有找到匹配的 Skill", "No matching skills")
            },
            if query.is_empty() {
                i.t(
                    "从上方推荐 Skill 安装，或导入一个含 SKILL.md 的目录开始",
                    "Install from recommended skills or import a directory containing SKILL.md",
                )
            } else {
                i.t("尝试更换搜索关键词", "Try a different search query")
            },
        ));
    } else {
        let mut list = div().flex().flex_col().gap(px(6.0));
        for skill in &filtered_skills {
            list = list.child(skill_row(skill, tools.clone(), &repo, ws, cx));
        }
        section = section.child(list);
    }

    // per-tool install overview
    let mut overview = div().flex().flex_col().gap(px(8.0)).child(section_title(
        &t,
        i.t("各工具安装状态", "Per-tool Install Status"),
        None,
    ));
    for tool in tools.iter().copied() {
        let store = ws.store.store().skills.clone();
        let names: Vec<String> = store
            .skills
            .iter()
            .filter(|s| s.is_enabled_in(tool))
            .map(|s| s.name.clone())
            .collect();
        overview = overview.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(badge(
                    &t,
                    i.t(tool.name_zh(), tool.name_en()),
                    BadgeKind::Accent,
                ))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(if names.is_empty() {
                            t.text_muted
                        } else {
                            t.text_secondary
                        })
                        .child(if names.is_empty() {
                            i.t("（无）", "(none)").to_string()
                        } else {
                            names.join(", ")
                        }),
                ),
        );
    }
    section = section.child(card(&t, vec![overview.into_any_element()]));

    section.into_any_element()
}

fn skill_row(
    skill: &Skill,
    tools: Vec<ToolId>,
    repo_path: &std::path::Path,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let status_badge = match skill.status.as_str() {
        "ok" => badge(&t, i.t("正常", "OK"), BadgeKind::Success),
        "error" => badge(&t, i.t("异常", "Error"), BadgeKind::Danger),
        _ => badge(&t, i.t("待同步", "Pending"), BadgeKind::Warning),
    };

    let sync_failures: usize = skill
        .sync_details
        .as_ref()
        .and_then(Value::as_object)
        .map(|m| {
            m.values()
                .filter(|v| v.get("status") == Some(&Value::String("error".into())))
                .count()
        })
        .unwrap_or(0);

    let mut tools_row = div().flex().gap(px(6.0)).flex_wrap();
    for tool in tools.clone() {
        let is_on = skill.is_enabled_in(tool);
        let label = i.t(tool.name_zh(), tool.name_en());
        let sid = skill.id.clone();
        tools_row = tools_row.child(button_l(
            gpui::SharedString::from(format!(
                "skill-tool-{}-{}",
                skill.id,
                tool.key()
            )),
            label,
            if is_on {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            &t,
            cx,
            move |ws, _ev, _w, cx| {
                // toggle + immediate install/remove on disk
                let _ = ws.store.update(|db| {
                    skills::toggle_tool(&mut db.skills, &sid, tool);
                });
                // apply on disk right away (single-skill sync)
                let mut store = ws.store.store().skills.clone();
                let idx = store.skills.iter().position(|s| s.id == sid);
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
            },
        ));
    }

    let sid_for_detail = skill.id.clone();
    let s_name = skill.name.clone();
    let skill_folder = repo_path.join(&skill.central_path);
    let skill_folder_for_reveal = skill_folder.clone();
    let tools_for_detail = tools.clone();

    let detail_btn = button_l(
        gpui::SharedString::from(format!("skill-detail-{}", skill.id)),
        i.t("详情", "Details"),
        ButtonVariant::Secondary,
        &t,
        cx,
        move |ws, _, _, cx| {
            let md_path = if skill_folder.join("SKILL.md").exists() {
                skill_folder.join("SKILL.md")
            } else if skill_folder.join("skill.md").exists() {
                skill_folder.join("skill.md")
            } else {
                skill_folder.join("README.md")
            };
            let content = if md_path.exists() {
                std::fs::read_to_string(&md_path).unwrap_or_default()
            } else {
                String::new()
            };
            ws.ui.skill_detail_dialog = Some(SkillDetailState {
                skill_id: sid_for_detail.clone(),
                name: s_name.clone(),
                path: skill_folder.clone(),
                skill_md: content,
                tools: tools_for_detail.clone(),
            });
            cx.notify();
        },
    );

    let reveal_btn = button_l(
        gpui::SharedString::from(format!("skill-reveal-{}", skill.id)),
        i.t("定位", "Reveal"),
        ButtonVariant::Secondary,
        &t,
        cx,
        move |_, _, _, _| {
            #[cfg(target_os = "windows")]
            {
                let _ = std::process::Command::new("explorer")
                    .arg(&skill_folder_for_reveal)
                    .spawn();
            }
        },
    );

    let git_update = (skill.source_type == "git").then(|| {
        let id = skill.id.clone();
        button_l(
            gpui::SharedString::from(format!("skill-git-update-{id}")),
            i.t("更新 Git", "Update Git"),
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                let result = aitoolplus_core::skill_git::update(
                    &mut ws.store.store_mut().skills,
                    &ws.paths,
                    &id,
                );
                match result {
                    Ok(()) => {
                        ws.persist_store();
                        ws.ui.toast("Git Skill updated", false);
                    }
                    Err(error) => ws.ui.toast(error, true),
                }
                cx.notify();
            },
        )
    });

    let sid_for_del = skill.id.clone();
    let s_name_for_del = skill.name.clone();
    let del_btn = button_l(
        gpui::SharedString::from(format!("skill-del-{}", skill.id)),
        i.t("卸载", "Uninstall"),
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
                        &format!("Are you sure you want to uninstall and delete “{}”?", s_name_for_del),
                    )
                    .to_string(),
                action: super::ConfirmAction::DeleteSkill {
                    id: sid_for_del.clone(),
                },
            });
            cx.notify();
        },
    );

    div()
        .id(gpui::SharedString::from(format!("skill-{}", skill.id)))
        .flex()
        .items_center()
        .gap(px(12.0))
        .p(px(12.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(if sync_failures > 0 {
            t.danger
        } else {
            t.card_border
        })
        .hover(|h| h.bg(t.card_hover))
        .child(status_badge)
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(skill.name.clone()),
                )
                .children(skill.user_note.clone().map(|n| {
                    div()
                        .text_size(px(11.5))
                        .text_color(t.text_muted)
                        .child(n)
                        .into_any_element()
                })),
        )
        .child(if sync_failures > 0 {
            badge(
                &t,
                i.t(
                    &format!("{sync_failures} 个失败"),
                    &format!("{sync_failures} failed"),
                ),
                BadgeKind::Danger,
            )
        } else {
            badge(&t, i.t("已同步", "Synced"), BadgeKind::Success)
        })
        .child(tools_row)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(detail_btn)
                .child(reveal_btn)
                .children(git_update)
                .child(del_btn),
        )
        .into_any_element()
}

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
                    i.t("在资源管理器中定位", "Reveal in Explorer"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |_, _, _, _| {
                        #[cfg(target_os = "windows")]
                        {
                            let _ = std::process::Command::new("explorer")
                                .arg(&path_clone)
                                .spawn();
                        }
                    },
                )),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.input_border)
                .id("skill-detail-scroll")
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

fn sync_all_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let i = ws.i18n;
    let report = {
        let mut store = ws.store.store().skills.clone();
        let report = skills::sync_all(&mut store, &ws.paths);
        let _ = ws.store.update(|db| db.skills = store);
        report
    };
    ws.persist_store();

    let total_ok: usize = report.iter().map(|(_, ok, _)| ok).sum();
    let total_failed: usize = report.iter().map(|(_, _, f)| f).sum();
    let msg = if total_failed > 0 {
        i.t(
            &format!("同步完成：成功 {total_ok}，失败 {total_failed}"),
            &format!("sync done: {total_ok} ok, {total_failed} failed"),
        )
        .to_string()
    } else if total_ok > 0 {
        i.t(
            &format!("已同步 {total_ok} 项"),
            &format!("synced {total_ok} entries"),
        )
        .to_string()
    } else {
        i.t("没有启用的 Skill", "no skills enabled").to_string()
    };
    ws.ui.toast(msg, total_failed > 0);
    cx.notify();
}

fn import_skill_dir(_ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let dialog = rfd::AsyncFileDialog::new();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        if let Some(folder) = dialog.pick_folder().await {
            let path = folder.path().to_path_buf();
            let _ = weak.update(cx, |ws: &mut Workspace, cx| {
                let i = ws.i18n;
                let store = ws.store.store().skills.clone();
                match skills::install_into_central(&store.settings, &ws.paths, &path) {
                    Ok(name) => {
                        // rescan so the new skill appears
                        let mut store = ws.store.store().skills.clone();
                        skills::scan_central(&mut store, &ws.paths);
                        let _ = ws.store.update(|db| db.skills = store);
                        ws.persist_store();
                        let msg = i
                            .t(&format!("已导入 {name}"), &format!("imported {name}"))
                            .to_string();
                        ws.ui.toast(msg, false);
                    }
                    Err(e) => {
                        let msg = format!("import failed: {e}");
                        ws.ui.toast(msg, true);
                    }
                }
                cx.notify();
            });
        }
    })
    .detach();
}
