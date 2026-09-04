//! Application assembly: window creation and state loading.

use std::sync::Arc;

use aitoolplus_core::paths::Paths;
use aitoolplus_core::settings::AppSettings;
use aitoolplus_core::store::StoreHandle;
use aitoolplus_ui::{Workspace, WorkspaceCallbacks};
use gpui::{Focusable, TitlebarOptions, WindowBounds, WindowKind, WindowOptions, prelude::*};

static ALLOW_WINDOW_CLOSE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn request_quit() {
    ALLOW_WINDOW_CLOSE.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// gpui's App context, aliased to avoid colliding with our own `App`.
use gpui::App as GpuiAppContext;

/// Persisted settings.json path.
pub fn settings_path(paths: &Paths) -> std::path::PathBuf {
    paths.settings_file()
}

/// Everything the app needs at startup.
pub struct App {
    pub paths: Arc<Paths>,
    pub settings: AppSettings,
    pub store: StoreHandle,
}

impl App {
    pub fn load() -> anyhow::Result<Self> {
        let mut paths = Paths::system();
        std::fs::create_dir_all(&paths.app_data)?;
        let mut settings = AppSettings::load(&settings_path(&paths));
        for (key, value) in &settings.tool_root_overrides {
            if let Some(tool) = aitoolplus_core::ToolId::from_key(key)
                && !value.trim().is_empty()
            {
                paths
                    .tool_roots
                    .insert(tool, std::path::PathBuf::from(value));
            }
        }
        // Startup is single-threaded here, so process environment mutation is
        // safe before GPUI/tray/watcher threads are spawned.
        unsafe {
            match settings.proxy_mode {
                aitoolplus_core::settings::ProxyMode::Custom
                    if !settings.proxy_url.trim().is_empty() =>
                {
                    std::env::set_var("HTTP_PROXY", &settings.proxy_url);
                    std::env::set_var("HTTPS_PROXY", &settings.proxy_url);
                }
                aitoolplus_core::settings::ProxyMode::Direct => {
                    std::env::remove_var("HTTP_PROXY");
                    std::env::remove_var("HTTPS_PROXY");
                    std::env::remove_var("ALL_PROXY");
                }
                _ => {}
            }
            std::env::set_var(
                "AITOOLPLUS_CODEX_PRESERVE_AUTH",
                if settings.codex_preserve_official_auth_on_switch {
                    "1"
                } else {
                    "0"
                },
            );
            std::env::set_var(
                "AITOOLPLUS_OMO_LEGACY",
                if settings.opencode_use_legacy_oh_my_config {
                    "1"
                } else {
                    "0"
                },
            );
            std::env::set_var(
                "AITOOLPLUS_OMO_DUAL_REASONING_VARIANT",
                if settings.opencode_dual_write_reasoning_variant {
                    "1"
                } else {
                    "0"
                },
            );
            for (command, path) in &settings.cli_manual_paths {
                std::env::set_var(
                    format!("AITOOLPLUS_CLI_{}", command.to_ascii_uppercase()),
                    path,
                );
            }
        }
        // The registry is authoritative for the current launch-at-login state.
        settings.start_with_system = crate::autostart::is_enabled();
        match aitoolplus_core::backup::run_auto_backup_if_due(&paths, &mut settings) {
            Ok(Some(report)) => {
                tracing::info!(
                    path = %report.output.display(),
                    files = report.file_count,
                    "automatic backup completed"
                );
                let _ = settings.save(&settings_path(&paths));
            }
            Ok(None) => {}
            Err(error) => tracing::warn!("automatic backup failed: {error}"),
        }
        if settings.auto_update_check_enabled {
            let due = settings
                .last_update_check_time
                .as_deref()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                .map(|last| {
                    chrono::Utc::now()
                        .signed_duration_since(last.with_timezone(&chrono::Utc))
                        .num_hours()
                        >= 24
                })
                .unwrap_or(true);
            if due {
                match aitoolplus_core::updater::check_latest(env!("CARGO_PKG_VERSION")) {
                    Ok(update) if update.update_available => tracing::info!(
                        latest = %update.latest_version,
                        url = %update.release_url,
                        "application update available"
                    ),
                    Ok(_) => tracing::debug!("application is up to date"),
                    Err(error) => tracing::debug!("automatic update check failed: {error}"),
                }
                settings.last_update_check_time = Some(chrono::Utc::now().to_rfc3339());
                let _ = settings.save(&settings_path(&paths));
            }
        }
        let store =
            StoreHandle::open(&paths).map_err(|e| anyhow::anyhow!("failed to open store: {e}"))?;
        Ok(Self {
            paths: Arc::new(paths),
            settings,
            store,
        })
    }
}

/// Launch the main window hosting the workspace view.
pub fn open_main_window(
    application: &mut App,
    tray: crate::tray::TrayMenuUpdater,
    cx: &mut GpuiAppContext,
) -> anyhow::Result<gpui::WindowHandle<Workspace>> {
    let paths = application.paths.clone();
    let store = std::mem::replace(&mut application.store, StoreHandle::open(&paths)?);
    let settings = application.settings.clone();
    let settings_file = settings_path(&paths);

    let bounds = match settings.window_bounds {
        Some((x, y, w, h)) => gpui::Bounds::new(
            gpui::point(gpui::px(x as f32), gpui::px(y as f32)),
            gpui::size(gpui::px(w as f32), gpui::px(h as f32)),
        ),
        None => gpui::Bounds::centered(None, gpui::size(gpui::px(1180.0), gpui::px(760.0)), cx),
    };

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("AI ToolPlus".into()),
            ..Default::default()
        }),
        focus: true,
        show: true,
        kind: WindowKind::Normal,
        ..Default::default()
    };

    let start_minimized = settings.start_minimized;
    let handle = cx.open_window(options, |window, cx| {
        // Persist settings on each change (KB file; sync write is fine).
        let settings_file = settings_file.clone();
        let save_settings: Box<dyn Fn(&AppSettings)> = Box::new(move |s| {
            if let Err(e) = s.save(&settings_file) {
                tracing::warn!("failed to save settings: {e}");
            }
            if let Err(e) = crate::autostart::set_enabled(s.start_with_system) {
                tracing::warn!("failed to update autostart: {e}");
            }
        });

        // Workspace owns persistence; this hook remains available for hosts
        // that mirror saved state elsewhere.
        let save_store: Box<dyn Fn(&aitoolplus_core::store::Store)> = Box::new(|_| {});

        let notify: Box<dyn Fn(String)> = Box::new(|msg| {
            // Toasts render inside the workspace; log for the record.
            tracing::debug!("ui notification: {msg}");
        });
        // Tray refresh: the workspace passes the just-saved store; push a
        // fresh provider snapshot to the quick-switch menu.
        let store_changed: Box<dyn Fn(&aitoolplus_core::store::Store)> = Box::new(move |store| {
            tray.update_groups(crate::tray::snapshot_from_store(store));
        });

        let workspace = cx.new(|cx| {
            Workspace::new(
                paths.clone(),
                store,
                settings,
                WorkspaceCallbacks {
                    save_settings,
                    save_store,
                    notify,
                    store_changed,
                },
                cx,
            )
        });
        let weak_workspace = workspace.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            if ALLOW_WINDOW_CLOSE.load(std::sync::atomic::Ordering::SeqCst) {
                return true;
            }
            let minimize = weak_workspace
                .update(cx, |workspace, _| {
                    workspace.settings.minimize_to_tray_on_close
                })
                .unwrap_or(false);
            if minimize {
                window.minimize_window();
                false
            } else {
                true
            }
        });
        window.focus(&workspace.read(cx).focus_handle(cx), cx);
        workspace
    })?;

    if start_minimized {
        let _ = handle.update(cx, |_, window, _| window.minimize_window());
    }
    Ok(handle)
}
