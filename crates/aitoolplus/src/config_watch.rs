//! Runtime configuration change watcher.
//!
//! Watches existing tool roots recursively and debounces bursts. External CLI
//! edits trigger provider re-discovery plus a GPUI redraw; app-data is not
//! watched, preventing save→watch feedback loops.

use std::path::PathBuf;
use std::sync::mpsc;

use notify::{RecursiveMode, Watcher};

pub fn spawn(paths: &aitoolplus_core::Paths) -> mpsc::Receiver<Vec<PathBuf>> {
    let (output_sender, output_receiver) = mpsc::channel();
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
                paths.sort();
                paths.dedup();
                if output_sender.send(paths).is_err() {
                    break;
                }
            }
        });
    output_receiver
}

pub fn pump(receiver: mpsc::Receiver<Vec<PathBuf>>, cx: &mut gpui::App) {
    cx.spawn(async move |cx| {
        loop {
            while let Ok(changed) = receiver.try_recv() {
                let handle = cx.update(|cx| {
                    cx.windows()
                        .into_iter()
                        .find_map(|window| window.downcast::<aitoolplus_ui::Workspace>())
                });
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
            cx.background_executor()
                .timer(std::time::Duration::from_millis(200))
                .await;
        }
    })
    .detach();
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
