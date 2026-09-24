//! Runtime configuration change watcher.
//!
//! Watches existing tool roots recursively and debounces bursts. External CLI
//! edits trigger provider re-discovery plus a GPUI redraw; app-data is not
//! watched, preventing save→watch feedback loops.

use std::path::PathBuf;
use std::sync::mpsc;

use notify::{RecursiveMode, Watcher};

pub fn spawn(paths: &aitoolplus_core::Paths) -> async_channel::Receiver<Vec<PathBuf>> {
    let (output_sender, output_receiver) = async_channel::unbounded();
    let roots: Vec<PathBuf> = aitoolplus_core::ToolId::ALL
        .into_iter()
        .map(|tool| paths.tool_root(tool))
        .filter(|root| root.is_dir())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    let _ = std::thread::Builder::new()
        .name("config-watch".into())
        .spawn(move || {
            let (raw_sender, raw_receiver) = mpsc::channel();
            let mut watcher = match notify::recommended_watcher(move |event| {
                let _ = raw_sender.send(event);
            }) {
                Ok(watcher) => watcher,
                Err(error) => {
                    tracing::warn!("config watcher unavailable: {error}");
                    return;
                }
            };
            for root in roots {
                if let Err(error) = watcher.watch(&root, RecursiveMode::Recursive) {
                    tracing::debug!(path = %root.display(), "watch skipped: {error}");
                }
            }

            loop {
                let event = match raw_receiver.recv() {
                    Ok(Ok(event)) => event,
                    Ok(Err(error)) => {
                        tracing::debug!("config watch event error: {error}");
                        continue;
                    }
                    Err(_) => break,
                };
                let mut paths = event.paths;
                // Debounce events produced by one atomic write/rename.
                while let Ok(next) =
                    raw_receiver.recv_timeout(std::time::Duration::from_millis(350))
                {
                    if let Ok(next) = next {
                        paths.extend(next.paths);
                    }
                }
                paths.retain(|path| is_runtime_config(path));
                paths.sort();
                paths.dedup();
                if paths.is_empty() {
                    continue;
                }
                if output_sender.send_blocking(paths).is_err() {
                    break;
                }
            }
        });
    output_receiver
}

pub fn pump(receiver: async_channel::Receiver<Vec<PathBuf>>, cx: &mut gpui::App) {
    // The watcher thread blocks on notify and send_blocking. This await parks
    // the UI task; it does not hold a thread-pool worker.
    cx.spawn(async move |cx| {
        while let Ok(changed) = receiver.recv().await {
            let handle = cx.update(|cx| {
                cx.windows()
                    .into_iter()
                    .find_map(|window| window.downcast::<aitoolplus_ui::Workspace>())
            });
            if aitoolplus_ui::restore_in_flight() {
                continue;
            }
            if let Some(handle) = handle {
                let _ = handle.update(cx, |workspace, _window, cx| {
                    refresh_runtime(workspace);
                    workspace.ui.toast(
                        workspace
                            .i18n
                            .t(
                                &format!("检测到 {} 个配置文件变化，已刷新", changed.len()),
                                &format!(
                                    "{} config changes detected; refreshed",
                                    changed.len()
                                ),
                            )
                            .to_string(),
                        false,
                    );
                    cx.notify();
                });
            }
        }
    })
    .detach();
}

/// Session logs and other junk under a tool root must not wake the UI.
/// `refresh_runtime` only rereads these config filenames.
fn is_runtime_config(path: &std::path::Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(
            "settings.json"
                | "auth.json"
                | "config.toml"
                | ".env"
                | "opencode.json"
                | "opencode.jsonc"
                | "openclaw.json"
                | "models.yml"
                | "config.yml"
                | "claude_desktop_config.json"
                | "config.yaml"
                | "settings.yaml"
                | "models.json"
                | ".claude.json"
        )
    )
}

fn refresh_runtime(workspace: &mut aitoolplus_ui::Workspace) {
    let paths = workspace.paths.clone();
    let store = workspace.store.store_mut();
    let _ = aitoolplus_core::import_current::import_claude_current(
        &paths,
        &mut store
            .tool_mut(aitoolplus_core::ToolId::ClaudeCode)
            .providers,
    );
    let _ = aitoolplus_core::import_current::import_codex_current(
        &paths,
        &mut store.tool_mut(aitoolplus_core::ToolId::Codex).providers,
    );
    let _ = aitoolplus_core::import_current::import_gemini_current(
        &paths,
        &mut store.tool_mut(aitoolplus_core::ToolId::GeminiCli).providers,
    );
    let _ = aitoolplus_core::import_current::import_opencode_current(
        &paths,
        &mut store.tool_mut(aitoolplus_core::ToolId::OpenCode).providers,
    );
    let _ = aitoolplus_core::import_current::import_grok_current(
        &paths,
        &mut store.tool_mut(aitoolplus_core::ToolId::Grok).providers,
    );
    let _ = aitoolplus_core::pi_runtime::import_runtime(
        &paths,
        &mut store.tool_mut(aitoolplus_core::ToolId::Pi).providers,
    );
    let _ = workspace.store.save();
}
