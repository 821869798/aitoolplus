use aitoolplus_core::pi_extensions::{PiExtensionKind, PiExtensionScope};
use aitoolplus_core::pi_pages::PiModelSettings;
use aitoolplus_core::providers::{CATEGORIES, ProviderRecord};
use aitoolplus_core::session::{self, SessionMeta};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, deferred, div, prelude::*, px, uniform_list};
use gpui_kit::base::{Align, ElementExt as _, Placement, Positioner, POPUP_PRIORITY};
use serde_json::Value;

use crate::components::{
    self, BadgeKind, ButtonVariant, Tooltip, badge, button_l, button_with_icon_l,
    button_with_icon_loading_l, input_container, page_header, section_title, text_area_scroll_container, textarea_container,
};
use crate::i18n::I18n;
use crate::text_area::TextArea;
use crate::text_input::TextInput;
use crate::theme::Theme;
use crate::workspace::Workspace;

use crate::pages::{PromptDialogState, ProviderDialogState, ToolTab, modal_scaffold_custom, modal_scaffold_sized};

use super::common::{open_in_browser, plugin_tag, spawn_tool_action};

pub struct BuiltinPromptPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub content: &'static str,
}

pub const BUILTIN_PROMPTS: &[BuiltinPromptPreset] = &[
    BuiltinPromptPreset {
        id: "architect",
        name: "资深架构师",
        desc: "高内聚低耦合系统架构、SOLID 原则与健壮扩展设计",
        content: "# 资深系统架构师工作准则\n\n你是一名拥有 15 年以上大型分布式与系统级软件工程经验的资深架构师。\n\n## 核心原则\n1. **SOLID & DRY 原则**：所有设计必须保证单一职责、高内聚、依赖倒置，消除不必要的重复代码。\n2. **模块化与关注点分离**：严格划分清晰的接口契约与数据模型边界，禁止跨层乱调。\n3. **健壮性与并发安全**：针对边界输入、空指针、并发锁竞争、网络重试和超时进行严密防护。\n4. **演进式架构**：避免过度设计（YAGNI），但在关键抽象层预留可扩展的钩子。\n\n## 输出要求\n- 给出代码前先简明扼要说明设计思路与权衡（Trade-offs）。\n- 交付生产级、类型安全、带关键设计注释的高质量代码。",
    },
    BuiltinPromptPreset {
        id: "reviewer",
        name: "严格代码审查员",
        desc: "深度挖掘边界漏洞、竞态死锁、内存泄露与代码坏味道",
        content: "# 严格代码审查员工作准则\n\n你是一名偏执而严苛的代码审查专家，以零容忍的态度对待潜在漏洞与坏味道。\n\n## 审查维度\n1. **正确性与边界情况**：检查所有可能发生溢出、下标越界、并发竞态、死锁、资源泄露的地方。\n2. **安全性 (OWASP)**：检查敏感信息硬编码、命令注入、未经校验的用户输入与权限漏洞。\n3. **代码异味 (Code Smells)**：指出长函数、巨型类、魔法数字、循环调用与无效抽象。\n4. **性能瓶颈**：指出高频内存分配、不必要的深拷贝、低效算法复杂度。\n\n## 输出规范\n- 采用 `[致命/严重/建议]` 评级结构化列出发现的问题。\n- 为每一个问题提供具体的定位和立即可用的修复后代码示例。",
    },
    BuiltinPromptPreset {
        id: "chinese-dev",
        name: "中文全栈开发专家",
        desc: "母语级中文沟通、思考链路清晰、直接产出高质量完整代码",
        content: "# 中文全栈开发专家\n\n你是一名资深全栈研发工程师，擅长以流畅地ฏ地道的中文进行技术交流与方案落地。\n\n## 工作规范\n1. **中文优先**：全程使用专业、清晰、精炼的简体中文沟通，术语保留业界通用英文。\n2. **思考清晰**：给出方案前，快速分析需求核心与潜在影响范围。\n3. **直接交付完整代码**：拒绝省略或伪代码，提供完整可运行、格式工整、开箱即用的解决方案。\n4. **现代最佳实践**：遵循现代语言规范（如 TypeScript Strict、Rust 2024、Python 3.12+）。",
    },
    BuiltinPromptPreset {
        id: "ponytail",
        name: "极简开发者 (Ponytail)",
        desc: "最懒最快的方案，拒绝过度设计，优先标准库，一行顶五十行",
        content: "# 极简实用主义开发者 (Ponytail Style)\n\n你是一位经验丰富、崇尚“最少代码办成最多事”的资深开发老兵。\n\n## 极简宪章\n1. **YAGNI 原则**：不需要的功能坚决不写，不解决未发生的问题，代码越少 Bug 越少。\n2. **标准库优先**：能用语言标准库和系统原生 API 搞定的，坚决不引入第三方依赖。\n3. **扁平化结构**：拒绝层层无意义的封装、Speculative Factory 和抽象层，直接用最简短直白的方式实现。\n4. **能删则删**：发现多余的脚手架、胶水代码和样板代码，果断重构成短小精炼的实现。",
    },
    BuiltinPromptPreset {
        id: "tdd",
        name: "TDD 单元测试专家",
        desc: "测试驱动开发，全覆盖边缘用例、Mock 隔离与清晰断言",
        content: "# TDD 单元测试专家准则\n\n你是一名严格践行测试驱动开发 (TDD) 的工程专家。\n\n## 测试准则\n1. **红-绿-重构循环**：先根据接口契约写测试，再编写满足测试的最简实现，最后重构。\n2. **边界全覆盖**：覆盖正常流、空值、极值、非法输入、超时重试与异常路径。\n3. **外部依赖 Mock 隔离**：网络、数据库、时钟等外部环境必须通过 Mock/Stub 隔离，确保测试快速且确定。\n4. **清晰的 Given-When-Then 结构**：每个测试用例意图明确，失败提示具备可诊断性。",
    },
];

pub(super) fn prompts_section(
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let prompts = aitoolplus_core::prompt::list(&ws.store.store().tool(tool).prompts);

    let query = ws.ui.prompt_search.read(cx).text().to_lowercase();
    let filtered_prompts: Vec<_> = prompts
        .into_iter()
        .filter(|p| {
            query.is_empty()
                || p.name.to_lowercase().contains(&query)
                || p.content.to_lowercase().contains(&query)
        })
        .collect();

    let adapter = aitoolplus_core::adapters::adapter_for(tool);
    let target_file_str = adapter
        .prompt_file(&ws.paths)
        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
        .unwrap_or_else(|| "Prompt".into());

    let add_label = i.t("新增提示词", "Add Prompt");

    // Presets quick-add bar
    let mut presets_bar = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_wrap()
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(t.text_secondary)
                .child(i.t("快捷预设模板：", "Quick Presets:")),
        );

    for preset in BUILTIN_PROMPTS {
        let preset_name = preset.name;
        let preset_content = preset.content;
        let p_id = preset.id;
        presets_bar = presets_bar.child(button_with_icon_l(
            gpui::SharedString::from(format!("prompt-preset-chip-{p_id}")),
            crate::icons::PLUS_SVG,
            preset_name,
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                let name_str = preset_name.to_string();
                let content_str = preset_content.to_string();
                let name_inp = cx.new(|cx| {
                    let mut inp = TextInput::new("名称", cx);
                    inp.set_text_silent(name_str, cx);
                    inp
                });
                let content_ta = cx.new(|cx| {
                    let mut ta = TextArea::new("提示词内容…", cx);
                    ta.set_text_silent(content_str, cx);
                    ta
                });
                ws.ui.prompt_dialog = Some(PromptDialogState {
                    editing_id: None,
                    tool,
                    name: name_inp,
                    content: content_ta,
                });
                cx.notify();
            },
        ));
    }

    // Top hint block aligned with ai-toolbox
    let hint_block = div()
        .p(px(12.0))
        .rounded(px(6.0))
        .bg(t.card_bg)
        .border_l_4()
        .border_color(t.accent)
        .text_size(px(12.5))
        .text_color(t.text_secondary)
        .child(i.t(
            "全局提示词将在与 AI 对话时自动作为系统提示词或前置上下文生效。您可以创建多个提示词模板并随时切换或启用。",
            "Global prompts will be injected as system instructions or leading context during conversations. You can create multiple templates and switch or enable them at any time.",
        ));

    let header = div()
        .flex()
        .items_center()
        .justify_between()
        .child(section_title(
            &t,
            i.t("全局提示词", "Global Prompts"),
            Some(i.t(
                &format!("应用后写入工具的提示词文件（{}）", target_file_str),
                &format!("Applied prompts are written to {}", target_file_str),
            )),
        ))
        .child(button_with_icon_l(
            "prompt-add",
            crate::icons::PLUS_SVG,
            add_label,
            ButtonVariant::Primary,
            &t,
            cx,
            move |ws, _, window, cx| {
                open_prompt_dialog(None, tool, ws, cx);
                if let Some(dlg) = &ws.ui.prompt_dialog {
                    dlg.name.update(cx, |name, cx| {
                        name.focus_handle.focus(window, cx);
                        name.start_blink(cx);
                    });
                }
            },
        ));

    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(hint_block)
        .child(header)
        .child(presets_bar)
        .child(
            div()
                .w(px(320.0))
                .child(input_container(&t, ws.ui.prompt_search.clone())),
        );

    if filtered_prompts.is_empty() {
        section = section.child(crate::components::empty_state_svg(
            &t,
            crate::icons::FILE_TEXT_SVG,
            if query.is_empty() {
                i.t("还没有全局提示词", "No prompts yet")
            } else {
                i.t("没有找到匹配的提示词", "No matching prompts")
            },
            if query.is_empty() {
                i.t(
                    "点击上方快捷预设模板，或新增一条可一键应用的全局提示词",
                    "Click a preset above or add a new global prompt",
                )
            } else {
                i.t("尝试更换搜索关键词", "Try a different search term")
            },
        ));
    } else {
        let mut list = div().flex().flex_col().gap(px(8.0));
        for p in &filtered_prompts {
            let pid = p.id.clone();
            let pid_unapply = p.id.clone();
            let pid2 = p.id.clone();
            let pid3 = p.id.clone();
            let pid4 = p.id.clone();
            let pid5 = p.id.clone();
            let pid_toggle = p.id.clone();
            let p_content = p.content.clone();
            let p_name = p.name.clone();
            let is_expanded = ws.ui.expanded_prompts.contains(&p.id);

            // Card header row
            let mut top_row = div().flex().items_center().justify_between().gap(px(10.0)).w_full();

            let left = div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .min_w(px(0.0))
                .flex_1()
                .child(
                    div()
                        .size(px(14.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_grab()
                        .child(
                            gpui::svg()
                                .data(crate::icons::GRIP_VERTICAL_SVG)
                                .size(px(14.0))
                                .text_color(t.text_muted),
                        ),
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.text_primary)
                        .child(p.name.clone()),
                )
                .children(p.is_applied.then(|| {
                    badge(&t, i.t("已应用", "Applied"), BadgeKind::Success)
                }))
                .child(badge(
                    &t,
                    gpui::SharedString::from(target_file_str.clone()),
                    BadgeKind::Neutral,
                ));

            let mut actions = div().flex().items_center().gap(px(6.0)).flex_shrink_0();

            if p.is_applied {
                actions = actions.child(button_with_icon_l(
                    gpui::SharedString::from(format!("prompt-unapply-{pid_unapply}")),
                    crate::icons::EYE_OFF_SVG,
                    i.t("停用", "Disable"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |ws, _, _, cx| unapply_prompt(tool, &pid_unapply, ws, cx),
                ));
            } else {
                let pid_apply = pid.clone();
                actions = actions.child(button_with_icon_l(
                    gpui::SharedString::from(format!("prompt-apply-{pid_apply}")),
                    crate::icons::CHECK_SVG,
                    i.t("应用", "Apply"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| apply_prompt(tool, &pid_apply, ws, cx),
                ));
            }

            actions = actions
                .child(crate::components::icon_button_svg(
                    gpui::SharedString::from(format!("prompt-copy-{pid4}")),
                    crate::icons::COPY_SVG,
                    i.t("复制内容", "Copy Content"),
                    false,
                    &t,
                    cx,
                    {
                        let content_copy = p_content.clone();
                        move |ws, _, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                content_copy.clone(),
                            ));
                            let msg = ws
                                .i18n
                                .t(
                                    "提示词内容已复制到剪贴板",
                                    "prompt copied to clipboard",
                                )
                                .to_string();
                            ws.ui.toast(msg, false);
                            cx.notify();
                        }
                    },
                ))
                .child(crate::components::icon_button_svg(
                    gpui::SharedString::from(format!("prompt-dup-{pid5}")),
                    crate::icons::SPARKLES_SVG,
                    i.t("创建副本", "Duplicate"),
                    false,
                    &t,
                    cx,
                    {
                        let dup_name = format!("{} (副本)", p_name);
                        let dup_content = p_content.clone();
                        move |ws, _, _, cx| {
                            let _ = ws.store.update(|store| {
                                let s = store.tool_mut(tool);
                                aitoolplus_core::prompt::create(
                                    &mut s.prompts,
                                    &dup_name,
                                    &dup_content,
                                );
                            });
                            ws.persist_store();
                            let msg = ws
                                .i18n
                                .t("已创建提示词副本", "prompt duplicate created")
                                .to_string();
                            ws.ui.toast(msg, false);
                            cx.notify();
                        }
                    },
                ))
                .child(crate::components::icon_button_svg(
                    gpui::SharedString::from(format!("prompt-edit-{pid2}")),
                    crate::icons::PENCIL_SVG,
                    i.t("编辑", "Edit"),
                    false,
                    &t,
                    cx,
                    move |ws, _, window, cx| {
                        let id = pid2.clone();
                        open_prompt_dialog(Some(id), tool, ws, cx);
                        if let Some(dlg) = &ws.ui.prompt_dialog {
                            dlg.name.update(cx, |name, cx| {
                                name.focus_handle.focus(window, cx);
                                name.start_blink(cx);
                            });
                        }
                    },
                ))
                .child(crate::components::icon_button_svg(
                    gpui::SharedString::from(format!("prompt-del-{pid3}")),
                    crate::icons::TRASH_SVG,
                    i.t("删除", "Delete"),
                    true,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        ws.ui.confirm = Some(crate::pages::ConfirmState {
                            title: ws.i18n.t("删除提示词", "Delete Prompt").to_string(),
                            message: ws
                                .i18n
                                .t("确定要删除这条提示词吗？", "Delete this prompt?")
                                .to_string(),
                            action: crate::pages::ConfirmAction::DeletePrompt {
                                tool,
                                id: pid3.clone(),
                            },
                        });
                        cx.notify();
                    },
                ));

            top_row = top_row.child(left).child(actions);

            // Content preview / expand section
            let content_elem = if is_expanded {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .w_full()
                    .child(
                        div()
                            .p(px(10.0))
                            .rounded(px(6.0))
                            .bg(t.sidebar_bg)
                            .border_1()
                            .border_color(t.card_border)
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .text_color(t.text_secondary)
                            .child(p.content.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .child(
                                button_with_icon_l(
                                    gpui::SharedString::from(format!("prompt-collapse-{pid_toggle}")),
                                    crate::icons::CHEVRON_UP_SVG,
                                    i.t("收起 ▴", "Collapse ▴"),
                                    ButtonVariant::Ghost,
                                    &t,
                                    cx,
                                    move |ws, _, _, cx| {
                                        ws.ui.expanded_prompts.remove(&pid_toggle);
                                        cx.notify();
                                    },
                                ),
                            ),
                    )
                    .into_any_element()
            } else {
                let single_line: String = p
                    .content
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .w_full()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(px(12.0))
                            .text_color(t.text_muted)
                            .child(single_line),
                    )
                    .child(
                        button_with_icon_l(
                            gpui::SharedString::from(format!("prompt-expand-{pid_toggle}")),
                            crate::icons::CHEVRON_DOWN_SVG,
                            i.t("展开 ▾", "Expand ▾"),
                            ButtonVariant::Ghost,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                ws.ui.expanded_prompts.insert(pid_toggle.clone());
                                cx.notify();
                            },
                        ),
                    )
                    .into_any_element()
            };

            let card = div()
                .id(gpui::SharedString::from(format!("prompt-{pid}")))
                .flex()
                .flex_col()
                .w_full()
                .min_w(px(0.0))
                .gap(px(8.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(if p.is_applied {
                    t.accent
                } else {
                    t.card_border
                })
                .child(top_row)
                .child(content_elem);

            list = list.child(card);
        }
        section = section.child(list);
    }

    section.into_any_element()
}

pub(super) fn unapply_prompt(tool: ToolId, _id: &str, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let i = ws.i18n;
    let _ = ws.store.update(|store| {
        let s = store.tool_mut(tool);
        aitoolplus_core::prompt::unapply_all(&mut s.prompts);
    });
    let adapter = aitoolplus_core::adapters::adapter_for(tool);
    if let Some(file) = adapter.prompt_file(&ws.paths) {
        if file.exists() {
            let _ = std::fs::remove_file(&file);
        }
        let msg = i.t("已停用全局提示词", "Global prompt disabled").to_string();
        ws.ui.toast(msg, false);
    }
    ws.persist_store();
    cx.notify();
}

pub(super) fn apply_prompt(tool: ToolId, id: &str, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let i = ws.i18n;
    let _ = ws.store.update(|store| {
        let s = store.tool_mut(tool);
        aitoolplus_core::prompt::select(&mut s.prompts, id);
    });
    let section = ws.store.store().tool(tool);
    let prompt = section.prompts.iter().find(|p| p.id == id).cloned();
    if let Some(prompt) = prompt {
        let adapter = aitoolplus_core::adapters::adapter_for(tool);
        if let Some(file) = adapter.prompt_file(&ws.paths) {
            if let Some(parent) = file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&file, &prompt.content) {
                let msg = format!("write prompt failed: {e}");
                ws.ui.toast(msg, true);
            } else {
                let msg = i
                    .t(
                        &format!("已应用 Prompt 到 {}", file.display()),
                        &format!("prompt applied to {}", file.display()),
                    )
                    .to_string();
                ws.ui.toast(msg, false);
            }
        } else {
            let msg = i
                .t("该工具没有 Prompt 文件", "tool has no prompt file")
                .to_string();
            ws.ui.toast(msg, true);
        }
    }
    ws.persist_store();
    cx.notify();
}

// ---------------------------------------------------------------------------
// Runtime files tab
// ---------------------------------------------------------------------------

pub fn open_prompt_dialog(
    editing_id: Option<String>,
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
    let i = ws.i18n;
    let existing = editing_id.as_ref().and_then(|id| {
        ws.store
            .store()
            .tool(tool)
            .prompts
            .iter()
            .find(|p| p.id == *id)
            .cloned()
    });

    let name = cx.new(|cx| {
        let mut input = TextInput::new(i.t("名称", "Name"), cx);
        if let Some(p) = &existing {
            input.set_text_silent(p.name.clone(), cx);
        }
        input
    });
    let content = cx.new(|cx| {
        let mut ta = TextArea::new(i.t("Prompt 内容…", "Prompt content…"), cx);
        if let Some(p) = &existing {
            ta.set_text_silent(p.content.clone(), cx);
        }
        ta
    });

    ws.ui.prompt_dialog = Some(PromptDialogState {
        editing_id,
        tool,
        name,
        content,
    });
    cx.notify();
}

pub fn render_prompt_dialog(
    state: PromptDialogState,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let PromptDialogState {
        editing_id,
        tool,
        name,
        content,
    } = state;

    let title = if editing_id.is_some() {
        i.t("编辑全局提示词", "Edit Global Prompt")
    } else {
        i.t("添加全局提示词", "Add Global Prompt")
    };

    let field_label = |label: gpui::SharedString| -> gpui::AnyElement {
        div()
            .text_size(px(12.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(t.text_secondary)
            .child(label)
            .into_any_element()
    };

    let tool_owned = tool;
    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .children((editing_id.is_none()).then(|| {
            let mut row = div().flex().items_center().gap(px(6.0)).flex_wrap();
            row = row.child(div().text_size(px(11.5)).text_color(t.text_secondary).child(i.t("快捷填入预设：", "Quick Presets:")));
            for preset in BUILTIN_PROMPTS {
                let p_name = preset.name;
                let p_content = preset.content;
                let name_clone = name.clone();
                let content_clone = content.clone();
                row = row.child(button_l(
                    gpui::SharedString::from(format!("dlg-preset-{}", preset.id)),
                    p_name,
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |_ws, _, _, cx| {
                        name_clone.update(cx, |inp, cx| inp.set_text_silent(p_name.to_string(), cx));
                        content_clone.update(cx, |ta, cx| ta.set_text_silent(p_content.to_string(), cx));
                        cx.notify();
                    },
                ));
            }
            row.into_any_element()
        }))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("提示词名称", "Prompt Name")))
                .child(input_container(&t, name.clone())),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("提示词内容（Markdown）", "Prompt Content (Markdown)")))
                .child({
                    let scroll_handle = content.read(cx).scroll_handle.clone();
                    let focus_handle = content.read(cx).focus_handle.clone();
                    text_area_scroll_container(
                        "prompt-content-editor-wrap",
                        "prompt-content-scrollbar",
                        &t,
                        px(320.0),
                        &scroll_handle,
                        &focus_handle,
                        content.clone(),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "prompt-dlg-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.prompt_dialog = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "prompt-dlg-save",
                    i.t("保存", "Save"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let name_txt: String = name.update(cx, |inp, _| inp.text().to_string());
                        let content_txt: String = content.update(cx, |ta, _| ta.text().to_string());
                        if name_txt.trim().is_empty() {
                            let msg = ws.i18n.t("名称不能为空", "name is required").to_string();
                            ws.ui.toast(msg, true);
                            cx.notify();
                            return;
                        }
                        let _ = ws.store.update(|store| {
                            let section = store.tool_mut(tool_owned);
                            match editing_id.clone() {
                                Some(id) => {
                                    aitoolplus_core::prompt::update(
                                        &mut section.prompts,
                                        &id,
                                        |p| {
                                             p.name = name_txt.clone();
                                            p.content = content_txt.clone();
                                        },
                                    );
                                }
                                None => {
                                    section.prompts.push(
                                        aitoolplus_core::prompt::PromptRecord::new(
                                            name_txt.clone(),
                                            content_txt.clone(),
                                        ),
                                    );
                                }
                            }
                        });
                        ws.persist_store();
                        ws.ui.prompt_dialog = None;
                        let msg = ws.i18n.t("已保存", "saved").to_string();
                        ws.ui.toast(msg, false);
                        cx.notify();
                    },
                )),
        );

    modal_scaffold_sized(
        &t,
        title.as_ref(),
        px(780.0),
        None,
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.prompt_dialog = None;
            cx.notify();
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn test_claude_code_provider_settings_building() {
        let form = ProviderFormData {
            tool: ToolId::ClaudeCode,
            category: "custom",
            base_url: "https://api.krill.io/v1",
            api_key: "sk-krill-secret",
            api_format: "openai",
            model: "",
            sonnet_model: "anthropic/claude-3-7-sonnet",
            sonnet_name: "Krill Sonnet 3.7",
            opus_model: "anthropic/claude-3-opus",
            opus_name: "Krill Opus",
            haiku_model: "anthropic/claude-3-5-haiku",
            haiku_name: "Krill Haiku",
            fable_model: "",
            fable_name: "",
            subagent_model: "anthropic/claude-3-5-haiku",
            sonnet_1m: true,
            opus_1m: false,
            haiku_1m: false,
            fable_1m: false,
            subagent_1m: true,
            pi_provider_key: "",
            pi_api_format: "",
            pi_models: &[],
            codex_wire_api: "",
            codex_reasoning_effort: "",
            codex_catalog_models: &[],
            custom_user_agent: "claude-cli/2.1.237 (external, cli)",
            custom_headers: "X-Krill-Custom: 123",
            headers_map: &[],
            raw_settings_json: "{}",
        };

        let settings_str = build_provider_settings(&form).expect("should build Claude settings");
        let parsed: Value = serde_json::from_str(&settings_str).expect("must be valid JSON");
        let env = parsed.get("env").expect("must contain env object");

        assert_eq!(
            env.get("USER_AGENT").and_then(Value::as_str),
            Some("claude-cli/2.1.237 (external, cli)")
        );
        assert_eq!(
            env.get("CUSTOM_HEADERS").and_then(Value::as_str),
            Some("X-Krill-Custom: 123")
        );
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str),
            Some("https://api.krill.io/v1")
        );
        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN").and_then(Value::as_str),
            Some("sk-krill-secret")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_SONNET_MODEL").and_then(Value::as_str),
            Some("anthropic/claude-3-7-sonnet[1M]"),
            "1M suffix should be appended when sonnet_1m is true"
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME").and_then(Value::as_str),
            Some("Krill Sonnet 3.7")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").and_then(Value::as_str),
            Some("anthropic/claude-3-opus")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_SUBAGENT_MODEL").and_then(Value::as_str),
            Some("anthropic/claude-3-5-haiku[1M]")
        );
        assert_eq!(
            env.get("API_FORMAT").and_then(Value::as_str),
            Some("openai")
        );
        assert_eq!(
            env.get("CUSTOM_HEADERS").and_then(Value::as_str),
            Some("X-Krill-Custom: 123")
        );
    }

    #[test]
    fn test_pi_provider_settings_building() {
        let pi_models = vec![
            serde_json::json!({
                "id": "deepseek-chat",
                "name": "DeepSeek V3",
                "reasoning": false,
                "input": ["text"],
                "contextWindow": 128000,
                "maxTokens": 16384
            }),
            serde_json::json!({
                "id": "deepseek-reasoner",
                "name": "DeepSeek R1",
                "reasoning": true,
                "input": ["text", "image"],
                "contextWindow": 64000,
                "maxTokens": 8192
            }),
        ];

        let form = ProviderFormData {
            tool: ToolId::Pi,
            category: "custom",
            base_url: "https://api.deepseek.com",
            api_key: "sk-deepseek-test",
            api_format: "openai",
            model: "",
            sonnet_model: "",
            sonnet_name: "",
            opus_model: "",
            opus_name: "",
            haiku_model: "",
            haiku_name: "",
            fable_model: "",
            fable_name: "",
            subagent_model: "",
            sonnet_1m: false,
            opus_1m: false,
            haiku_1m: false,
            fable_1m: false,
            subagent_1m: false,
            pi_provider_key: "deepseek-test",
            pi_api_format: "openai-completions",
            pi_models: &pi_models,
            codex_wire_api: "",
            codex_reasoning_effort: "",
            codex_catalog_models: &[],
            custom_user_agent: "Kilo-Code/1.0",
            custom_headers: "",
            headers_map: &[("X-Title".to_string(), "DeepSeekApp".to_string())],
            raw_settings_json: "{}",
        };

        let settings_str = build_provider_settings(&form).expect("should build Pi settings");
        let parsed: Value = serde_json::from_str(&settings_str).expect("must be valid JSON");

        assert_eq!(
            parsed.get("baseUrl").and_then(Value::as_str),
            Some("https://api.deepseek.com")
        );
        assert_eq!(
            parsed.get("apiKey").and_then(Value::as_str),
            Some("sk-deepseek-test")
        );
        assert_eq!(
            parsed.get("api").and_then(Value::as_str),
            Some("openai-completions")
        );
        assert_eq!(
            parsed.get("_providerKey").and_then(Value::as_str),
            Some("deepseek-test")
        );

        let headers = parsed.get("headers").expect("headers object");
        assert_eq!(headers.get("User-Agent").and_then(Value::as_str), Some("Kilo-Code/1.0"));
        assert_eq!(headers.get("X-Title").and_then(Value::as_str), Some("DeepSeekApp"));

        let models = parsed.get("models").and_then(Value::as_array).expect("models array");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["id"], "deepseek-chat");
        assert_eq!(models[0]["name"], "DeepSeek V3");
        assert_eq!(models[0]["contextWindow"], 128000);
        assert_eq!(models[1]["id"], "deepseek-reasoner");
        assert_eq!(models[1]["reasoning"], true);
        assert_eq!(models[1]["input"], serde_json::json!(["text", "image"]));
    }

    #[test]
    fn test_codex_provider_settings_building() {
        let catalog = vec![
            serde_json::json!({
                "displayName": "DeepSeek V3",
                "model": "deepseek-chat",
                "contextWindow": "64000",
                "reasoningLevels": ["none"]
            })
        ];
        let form = ProviderFormData {
            tool: ToolId::Codex,
            category: "custom",
            base_url: "https://api.deepseek.com/v1",
            api_key: "sk-test-codex",
            api_format: "openai_responses",
            model: "deepseek-chat",
            sonnet_model: "",
            sonnet_name: "",
            opus_model: "",
            opus_name: "",
            haiku_model: "",
            haiku_name: "",
            fable_model: "",
            fable_name: "",
            subagent_model: "",
            sonnet_1m: false,
            opus_1m: false,
            haiku_1m: false,
            fable_1m: false,
            subagent_1m: false,
            pi_provider_key: "",
            pi_api_format: "",
            pi_models: &[],
            codex_wire_api: "responses",
            codex_reasoning_effort: "low",
            codex_catalog_models: &catalog,
            custom_user_agent: "",
            custom_headers: "",
            headers_map: &[],
            raw_settings_json: "{}",
        };

        let settings_str = build_provider_settings(&form).expect("should build Codex settings");
        let parsed: Value = serde_json::from_str(&settings_str).expect("must be valid JSON");
        assert_eq!(parsed["auth"]["OPENAI_API_KEY"], "sk-test-codex");
        let toml_str = parsed["config"].as_str().unwrap();
        assert!(toml_str.contains("model = \"deepseek-chat\""));
        assert!(toml_str.contains("base_url = \"https://api.deepseek.com/v1\""));
        assert_eq!(parsed["modelCatalog"]["models"][0]["displayName"], "DeepSeek V3");
    }

    #[test]
    fn test_model_search_filtering_logic() {
        let models = vec![
            aitoolplus_core::api_hub::FetchedModel {
                id: "anthropic/claude-3-7-sonnet".to_string(),
                display_name: Some("Claude 3.7 Sonnet".to_string()),
                owned_by: Some("Anthropic".to_string()),
                ..Default::default()
            },
            aitoolplus_core::api_hub::FetchedModel {
                id: "deepseek-ai/DeepSeek-V3".to_string(),
                display_name: Some("DeepSeek V3".to_string()),
                owned_by: Some("DeepSeek".to_string()),
                ..Default::default()
            },
        ];

        let query = "sonnet".to_lowercase();
        let matched: Vec<_> = models
            .iter()
            .filter(|m| {
                m.id.to_lowercase().contains(&query)
                    || m.owned_by.as_deref().unwrap_or("").to_lowercase().contains(&query)
                    || m.display_name.as_deref().unwrap_or("").to_lowercase().contains(&query)
            })
            .collect();

        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].id, "anthropic/claude-3-7-sonnet");

        let query_vendor = "deepseek".to_lowercase();
        let matched_vendor: Vec<_> = models
            .iter()
            .filter(|m| {
                m.id.to_lowercase().contains(&query_vendor)
                    || m.owned_by.as_deref().unwrap_or("").to_lowercase().contains(&query_vendor)
                    || m.display_name.as_deref().unwrap_or("").to_lowercase().contains(&query_vendor)
            })
            .collect();

        assert_eq!(matched_vendor.len(), 1);
        assert_eq!(matched_vendor[0].id, "deepseek-ai/DeepSeek-V3");
    }
}

