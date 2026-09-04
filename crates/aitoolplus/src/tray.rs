//! System tray icon + menu (cc-switch parity).
//!
//! Menu structure (rebuilt whenever the store changes):
//!   打开 AI ToolPlus / Open
//!   退出 / Quit
//!   ─────────────
//!   供应商 / Providers
//!     Claude Code
//!       ✓ Provider A   (applied)
//!         Provider B   ← click = apply + switch
//!     Codex
//!       …
//!
//! muda 0.19 note: menus have no item-clear API, so a store change rebuilds
//! and swaps the whole tray menu (`TrayIcon::set_menu`). Event ids are
//! compared by string; provider entries use `t:<tool>|<provider-id>`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use gpui::App as GpuiApp;

pub enum TrayEvent {
    ShowWindow,
    Quit,
    /// Apply this provider for the tool (grouped quick-switch).
    ApplyProvider {
        tool: &'static str,
        provider_id: String,
        provider_name: String,
    },
}

/// Snapshot of (tool, provider-id, provider-name, is_applied).
pub type ToolGroupSnapshot = Vec<(&'static str, String, String, bool)>;

/// Cloneable handle for pushing menu snapshots into the tray thread.
/// (The full `TrayHandle` owns the one-shot event receiver and stays with
/// the GPUI pump.)
#[derive(Clone)]
pub struct TrayMenuUpdater {
    menu_dirty: Arc<AtomicBool>,
    groups: Arc<Mutex<ToolGroupSnapshot>>,
}

impl TrayMenuUpdater {
    /// Push a fresh snapshot; the tray thread swaps the menu on its next tick.
    pub fn update_groups(&self, groups: ToolGroupSnapshot) {
        if let Ok(mut guard) = self.groups.lock() {
            *guard = groups;
        }
        self.menu_dirty.store(true, Ordering::Relaxed);
    }
}

pub struct TrayHandle {
    pub events: mpsc::Receiver<TrayEvent>,
    menu_dirty: Arc<AtomicBool>,
    groups: Arc<Mutex<ToolGroupSnapshot>>,
    /// Keeps the tray thread alive; dropped with the handle.
    _lifeline: mpsc::Sender<()>,
}

impl TrayHandle {
    /// Cloneable updater for callers that only need to refresh the menu.
    pub fn updater(&self) -> TrayMenuUpdater {
        TrayMenuUpdater {
            menu_dirty: self.menu_dirty.clone(),
            groups: self.groups.clone(),
        }
    }

    /// Push a fresh snapshot; the tray thread swaps the menu on its next tick.
    pub fn update_groups(&self, groups: ToolGroupSnapshot) {
        self.updater().update_groups(groups);
    }
}

pub fn spawn_tray() -> TrayHandle {
    let (tx, rx) = mpsc::channel::<TrayEvent>();
    // lifeline: dropped when TrayHandle drops -> thread exits
    let (life_tx, life_rx) = mpsc::channel::<()>();
    let menu_dirty = Arc::new(AtomicBool::new(true));
    let groups = Arc::new(Mutex::new(Vec::new()));
    let dirty_for_thread = menu_dirty.clone();
    let groups_for_thread = groups.clone();
    let life_tx_thread = life_tx.clone();
    let _ = life_rx; // kept alive by the thread loop below

    thread::Builder::new()
        .name("tray".into())
        .spawn(move || {
            let icon =
                tray_icon::Icon::from_rgba(include_bytes!("../assets/icon.rgba").to_vec(), 32, 32)
                    .unwrap_or_else(|_| {
                        tray_icon::Icon::from_rgba(vec![0u8; 32 * 32 * 4], 32, 32)
                            .expect("infallible rgba")
                    });

            let tray = match tray_icon::TrayIconBuilder::new()
                .with_tooltip("AI ToolPlus")
                .with_icon(icon)
                .with_menu(Box::new(build_menu(&groups_for_thread).unwrap_or_default()))
                .build()
            {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!("tray build failed: {e}");
                    return;
                }
            };

            let menu_channel = muda::MenuEvent::receiver();
            loop {
                if dirty_for_thread.swap(false, Ordering::Relaxed)
                    && let Ok(menu) = build_menu(&groups_for_thread)
                {
                    // set_menu swaps the whole native menu
                    tray.set_menu(Some(Box::new(menu)));
                }
                if let Ok(ev) = menu_channel.try_recv() {
                    let ev_id = ev.id().0.clone();
                    match ev_id.as_str() {
                        "app:open" => {
                            let _ = tx.send(TrayEvent::ShowWindow);
                        }
                        "app:quit" => {
                            let _ = tx.send(TrayEvent::Quit);
                            tray.set_visible(false).ok();
                            break;
                        }
                        value => {
                            if let Some((tool, id)) = decode_provider_item_id(value) {
                                let name = current_name(&groups_for_thread, tool, &id);
                                let _ = tx.send(TrayEvent::ApplyProvider {
                                    tool,
                                    provider_id: id,
                                    provider_name: name,
                                });
                            }
                        }
                    }
                }
                // lifeline probe: life_tx errors once TrayHandle is dropped
                if life_tx_thread.send(()).is_err() {
                    tray.set_visible(false).ok();
                    break;
                }
                thread::sleep(std::time::Duration::from_millis(60));
            }
        })
        .expect("failed to spawn tray thread");

    TrayHandle {
        events: rx,
        menu_dirty,
        groups,
        _lifeline: life_tx,
    }
}

fn build_menu(groups: &Mutex<ToolGroupSnapshot>) -> Result<muda::Menu, String> {
    let menu = muda::Menu::new();
    let open = muda::MenuItem::with_id("app:open", "打开 AI ToolPlus / Open", true, None);
    let quit = muda::MenuItem::with_id("app:quit", "退出 / Quit", true, None);
    let separator = muda::PredefinedMenuItem::separator();
    menu.append_items(&[&open, &quit, &separator])
        .map_err(|e| e.to_string())?;

    let groups = groups.lock().map_err(|e| e.to_string())?;
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
) -> Option<gpui::WindowHandle<aitoolplus_ui::Workspace>> {
    cx.update(|cx| {
        if let Some(existing) = cx
            .windows()
            .into_iter()
            .find_map(|window| window.downcast::<aitoolplus_ui::Workspace>())
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

/// Poll tray events inside GPUI's event loop.
pub fn pump_tray_events(handle: TrayHandle, cx: &mut GpuiApp) {
    let updater = handle.updater();
    let TrayHandle { events, .. } = handle;

    cx.spawn(async move |cx| {
        loop {
            while let Ok(ev) = events.try_recv() {
                match ev {
                    TrayEvent::ShowWindow => {
                        if let Some(window) = ensure_workspace_window(cx, &updater) {
                            let _ = window.update(cx, |_, window, _cx| {
                                window.activate_window();
                            });
                        }
                    }
                    TrayEvent::Quit => {
                        crate::app::request_quit();
                        cx.update(|cx| cx.quit());
                        return;
                    }
                    TrayEvent::ApplyProvider {
                        tool,
                        provider_id,
                        provider_name,
                    } => {
                        // Reopen the workspace if the main window was closed,
                        // then apply through the same verified UI path.
                        if let Some(handle) = ensure_workspace_window(cx, &updater) {
                            let _ = handle.update(cx, |ws, window, cx| {
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
                                window.activate_window();
                            });
                        }
                    }
                }
            }
            cx.background_executor()
                .timer(std::time::Duration::from_millis(120))
                .await;
        }
    })
    .detach();
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
