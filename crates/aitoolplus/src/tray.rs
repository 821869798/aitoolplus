//! System tray icon + menu (cc-switch parity).
//!
//! Shares the UI thread's Win32 message pump rather than running one of its own.
//! `tray-icon` and `muda` both create ordinary Win32 windows on the calling thread,
//! and GPUI's event loop dispatches messages for every window on its thread —
//! so building the tray from inside the GPUI run loop is all the integration needed.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use gpui::App as GpuiApp;

pub enum TrayAction {
    ShowWindow,
    Quit,
    /// Apply this provider for the tool (grouped quick-switch).
    ApplyProvider {
        tool: &'static str,
        provider_id: String,
        provider_name: String,
    },
    /// Rebuild menu from fresh snapshot.
    UpdateMenu(ToolGroupSnapshot),
}

/// Snapshot of (tool, provider-id, provider-name, is_applied).
pub type ToolGroupSnapshot = Vec<(&'static str, String, String, bool)>;

static MAIN_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static TRAY_SENDER: OnceLock<async_channel::Sender<TrayAction>> = OnceLock::new();

pub fn set_main_thread_id(tid: u32) {
    MAIN_THREAD_ID.store(tid, Ordering::Release);
}

/// Wake up GPUI's Win32 message pump if sleeping in GetMessageW.
fn wake_ui_thread() {
    #[cfg(windows)]
    {
        let tid = MAIN_THREAD_ID.load(Ordering::Acquire);
        if tid != 0 {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                    tid,
                    windows::Win32::UI::WindowsAndMessaging::WM_NULL,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
        }
    }
}

pub fn post_tray_action(action: TrayAction) {
    if let Some(tx) = TRAY_SENDER.get() {
        let _ = tx.try_send(action);
        wake_ui_thread();
    }
}

/// Cloneable handle for pushing menu snapshots into the tray.
#[derive(Clone)]
pub struct TrayMenuUpdater {
    tx: async_channel::Sender<TrayAction>,
}

impl TrayMenuUpdater {
    /// Push a fresh snapshot; the menu updates on the UI thread.
    pub fn update_groups(&self, groups: ToolGroupSnapshot) {
        let _ = self.tx.try_send(TrayAction::UpdateMenu(groups));
        wake_ui_thread();
    }
}

pub fn init_tray(initial_groups: ToolGroupSnapshot, cx: &mut GpuiApp) -> TrayMenuUpdater {
    #[cfg(windows)]
    set_main_thread_id(unsafe { windows::Win32::System::Threading::GetCurrentThreadId() });

    let (tx, rx) = async_channel::unbounded::<TrayAction>();
    TRAY_SENDER.set(tx.clone()).ok();
    let updater = TrayMenuUpdater { tx: tx.clone() };

    let groups_state = Arc::new(Mutex::new(initial_groups.clone()));
    let groups_for_events = groups_state.clone();

    #[cfg(target_os = "windows")]
    let image = tray_icon::Icon::from_resource(1, Some((32, 32)))
        .or_else(|_| {
            tray_icon::Icon::from_rgba(crate::icon::rgba(), crate::icon::SIZE, crate::icon::SIZE)
        })
        .expect("infallible icon");

    #[cfg(not(target_os = "windows"))]
    let image = tray_icon::Icon::from_rgba(crate::icon::rgba(), crate::icon::SIZE, crate::icon::SIZE)
        .expect("infallible icon");

    let menu = build_menu(&initial_groups).unwrap_or_default();
    let tray_icon = match tray_icon::TrayIconBuilder::new()
        .with_tooltip("AI ToolPlus")
        .with_icon(image)
        .with_menu(Box::new(menu))
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("tray build failed: {e}");
            return updater;
        }
    };

    muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
        let ev_id = event.id().0.clone();
        match ev_id.as_str() {
            "app:open" => post_tray_action(TrayAction::ShowWindow),
            "app:quit" => post_tray_action(TrayAction::Quit),
            value => {
                if let Some((tool, id)) = decode_provider_item_id(value) {
                    let name = current_name(&groups_for_events, tool, &id);
                    post_tray_action(TrayAction::ApplyProvider {
                        tool,
                        provider_id: id,
                        provider_name: name,
                    });
                }
            }
        }
    }));

    tray_icon::TrayIconEvent::set_event_handler(Some(move |event: tray_icon::TrayIconEvent| {
        if matches!(
            event,
            tray_icon::TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } | tray_icon::TrayIconEvent::DoubleClick {
                button: tray_icon::MouseButton::Left,
                ..
            }
        ) {
            post_tray_action(TrayAction::ShowWindow);
        }
    }));

    let updater_for_pump = updater.clone();
    cx.spawn(async move |cx| {
        while let Ok(action) = rx.recv().await {
            match action {
                TrayAction::ShowWindow => {
                    if let Some(window) = ensure_workspace_window(cx, &updater_for_pump) {
                        let _ = window.update(cx, |_, window, _cx| {
                            crate::app::restore_window_from_tray(window);
                        });
                    }
                }
                TrayAction::Quit => {
                    crate::app::request_quit();
                    tray_icon.set_visible(false).ok();
                    cx.update(|cx| cx.quit());
                    return;
                }
                TrayAction::ApplyProvider {
                    tool,
                    provider_id,
                    provider_name,
                } => {
                    if let Some(handle) = ensure_workspace_window(cx, &updater_for_pump) {
                        let _ = handle.update(cx, |root, window, cx| {
                            if let Ok(ws) = root.view().clone().downcast::<aitoolplus_ui::Workspace>() {
                                ws.update(cx, |ws, cx| {
                                    if let Some(tool_id) = aitoolplus_core::ToolId::from_key(tool) {
                                        ws.apply_provider(tool_id, &provider_id, cx);
                                        let i = ws.i18n;
                                        let msg = i
                                            .t(
                                                &format!("已从托盘切换：{provider_name}"),
                                                &format!("switched via tray: {provider_name}"),
                                            )
                                            .to_string();
                                        ws.ui.toast(msg, false);
                                    }
                                });
                            }
                            crate::app::restore_window_from_tray(window);
                        });
                    }
                }
                TrayAction::UpdateMenu(new_groups) => {
                    if let Ok(mut guard) = groups_state.lock() {
                        *guard = new_groups.clone();
                    }
                    if let Ok(menu) = build_menu(&new_groups) {
                        tray_icon.set_menu(Some(Box::new(menu)));
                    }
                }
            }
        }
    })
    .detach();

    updater
}

fn build_menu(groups: &ToolGroupSnapshot) -> Result<muda::Menu, String> {
    let menu = muda::Menu::new();
    let open = muda::MenuItem::with_id("app:open", "打开 AI ToolPlus / Open", true, None);
    let quit = muda::MenuItem::with_id("app:quit", "退出 / Quit", true, None);
    let separator = muda::PredefinedMenuItem::separator();
    menu.append_items(&[&open, &quit, &separator])
        .map_err(|e| e.to_string())?;

    if groups.is_empty() {
        return Ok(menu);
    }

    let header = muda::MenuItem::new("供应商 / Providers", false, None);
    let header_sep = muda::PredefinedMenuItem::separator();
    menu.append_items(&[&header, &header_sep])
        .map_err(|e| e.to_string())?;

    // preserve insertion order of tools
    let mut order: Vec<&'static str> = vec![];
    for (tool, _, _, _) in groups.iter() {
        if !order.contains(tool) {
            order.push(tool);
        }
    }
    for tool in order {
        // collect this tool's menu items
        let items: Vec<muda::MenuItem> = groups
            .iter()
            .filter(|(t, _, _, _)| t == &tool)
            .map(|(_, id, name, applied)| {
                let display = if *applied {
                    format!("✓ {name}")
                } else {
                    name.clone()
                };
                muda::MenuItem::with_id(encode_provider_item_id(tool, id), display, true, None)
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        let refs: Vec<&dyn muda::IsMenuItem> =
            items.iter().map(|i| i as &dyn muda::IsMenuItem).collect();
        let submenu = muda::Submenu::with_items(pretty_tool_name(tool), true, &refs)
            .map_err(|e| e.to_string())?;
        menu.append(&submenu).map_err(|e| e.to_string())?;
    }
    Ok(menu)
}

fn encode_provider_item_id(tool: &str, id: &str) -> String {
    format!("t:{tool}|{id}")
}

fn decode_provider_item_id(value: &str) -> Option<(&'static str, String)> {
    let rest = value.strip_prefix("t:")?;
    let (tool, id) = rest.split_once('|')?;
    let known: &'static str = match tool {
        "claude_code" => "claude_code",
        "codex" => "codex",
        "gemini_cli" => "gemini_cli",
        "grok" => "grok",
        "kimi" => "kimi",
        "opencode" => "opencode",
        "openclaw" => "openclaw",
        "pi" => "pi",
        "oh_my_pi" => "oh_my_pi",
        "claude_desktop" => "claude_desktop",
        "hermes" => "hermes",
        "dsh" => "dsh",
        _ => return None,
    };
    Some((known, id.to_string()))
}

fn current_name(groups: &Mutex<ToolGroupSnapshot>, tool: &str, id: &str) -> String {
    groups
        .lock()
        .ok()
        .and_then(|g| {
            g.iter()
                .find(|(t, pid, _, _)| *t == tool && pid == id)
                .map(|(_, _, name, _)| name.clone())
        })
        .unwrap_or_default()
}

fn pretty_tool_name(tool: &str) -> &'static str {
    const NAMES: [(&str, &str); 12] = [
        ("claude_code", "Claude Code"),
        ("codex", "Codex"),
        ("gemini_cli", "Gemini CLI"),
        ("grok", "Grok"),
        ("kimi", "Kimi"),
        ("opencode", "OpenCode"),
        ("openclaw", "OpenClaw"),
        ("pi", "Pi"),
        ("oh_my_pi", "Oh My Pi"),
        ("claude_desktop", "Claude Desktop"),
        ("hermes", "Hermes"),
        ("dsh", "DSH"),
    ];
    NAMES
        .iter()
        .find(|(k, _)| *k == tool)
        .map(|(_, v)| *v)
        .unwrap_or("Tool")
}

fn ensure_workspace_window(
    cx: &mut gpui::AsyncApp,
    updater: &TrayMenuUpdater,
) -> Option<gpui::WindowHandle<gpui_kit::component::Root>> {
    cx.update(|cx| {
        if let Some(existing) = cx
            .windows()
            .into_iter()
            .find_map(|window| window.downcast::<gpui_kit::component::Root>())
        {
            return Some(existing);
        }
        let mut application = match crate::app::App::load() {
            Ok(app) => app,
            Err(error) => {
                tracing::error!("failed to reload app from tray: {error}");
                return None;
            }
        };
        match crate::app::open_main_window(&mut application, updater.clone(), cx) {
            Ok(window) => Some(window),
            Err(error) => {
                tracing::error!("failed to reopen main window from tray: {error}");
                None
            }
        }
    })
}

/// Build the tray snapshot from the store: every non-disabled provider per
/// tool (applied marked, order preserved).
pub fn snapshot_from_store(store: &aitoolplus_core::store::Store) -> ToolGroupSnapshot {
    let mut out = ToolGroupSnapshot::new();
    for tool in aitoolplus_core::ToolId::ALL {
        let section = store.tool(tool);
        let providers = aitoolplus_core::providers::list(&section.providers);
        for p in providers {
            if p.is_disabled {
                continue;
            }
            out.push((tool.key(), p.id.clone(), p.name.clone(), p.is_applied));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_item_ids_round_trip() {
        for (tool, expected) in [
            ("claude_code", "claude_code"),
            ("kimi", "kimi"),
            ("pi", "pi"),
            ("dsh", "dsh"),
        ] {
            let id = encode_provider_item_id(tool, "abc-123");
            assert_eq!(id, format!("t:{tool}|abc-123"));
            let (decoded_tool, decoded_id) = decode_provider_item_id(&id).unwrap();
            assert_eq!(decoded_tool, expected);
            assert_eq!(decoded_id, "abc-123");
        }
        // unknown ids and malformed values are rejected
        assert!(decode_provider_item_id("t:unknown|id").is_none());
        assert!(decode_provider_item_id("app:open").is_none());
        assert!(decode_provider_item_id("t:nopipe").is_none());
    }

    #[test]
    fn snapshot_marks_applied_and_skips_disabled() {
        let mut store = aitoolplus_core::store::Store::new();
        let section = store.tool_mut(aitoolplus_core::ToolId::ClaudeCode);
        let a = aitoolplus_core::providers::create(&mut section.providers, "A", "custom");
        let _b = aitoolplus_core::providers::create(&mut section.providers, "B", "custom");
        let c = aitoolplus_core::providers::create(&mut section.providers, "C", "custom");
        aitoolplus_core::providers::select(&mut section.providers, &a.id);
        aitoolplus_core::providers::toggle_disabled(&mut section.providers, &c.id);

        let snap = snapshot_from_store(&store);
        assert_eq!(snap.len(), 2); // disabled C skipped
        let applied = snap.iter().find(|(_, _, name, _)| name == "A");
        assert!(
            applied.is_some_and(|(_, _, _, a)| *a),
            "A must be marked applied"
        );
        assert!(snap.iter().any(|(_, _, name, _)| name == "B"));
        assert!(!snap.iter().any(|(_, _, name, _)| name == "C"));
    }

    #[test]
    fn pretty_names_cover_all_tools() {
        for tool in aitoolplus_core::ToolId::ALL {
            assert_ne!(
                pretty_tool_name(tool.key()),
                tool.key(),
                "name must differ from key"
            );
        }
    }
}
