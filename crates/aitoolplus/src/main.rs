//! AI ToolPlus — a Rust + GPUI desktop workbench to manage coding-assistant
//! CLI configurations (providers, prompts, MCP, skills, sessions), mirroring
//! the feature set of coulsontl/ai-toolbox.

mod app;
mod autostart;
mod config_watch;
mod log;
mod single_instance;
mod tray;

use aitoolplus_core::paths::Paths;

fn main() {
    let paths = Paths::system();
    let _ = std::fs::create_dir_all(&paths.app_data);
    log::init(&paths.app_data.join("aitoolplus.log"));

    let incoming = std::env::args()
        .skip(1)
        .find(|argument| argument.starts_with("aitoolbox://"));

    // Single instance: second launches forward their URL/activation payload.
    let mut instance = match single_instance::acquire() {
        Some(instance) => instance,
        None => {
            let message = incoming.as_deref().unwrap_or("activate");
            if let Err(error) = single_instance::forward_to_existing(message) {
                tracing::warn!("failed to forward to running instance: {error}");
            }
            return;
        }
    };
    let message_receiver = instance.take_receiver();
    if let Err(error) = single_instance::register_protocol() {
        tracing::debug!("deep-link protocol registration failed: {error}");
    }

    let mut application = app::App::load().expect("failed to load app state");
    let config_events = config_watch::spawn(&application.paths);
    if let Some(url) = incoming {
        match aitoolplus_core::deeplink::import_into_store(&url, application.store.store_mut()) {
            Ok((tool, name)) => {
                let _ = application.store.save();
                application.settings.last_page = tool.key().to_string();
                let _ = application
                    .settings
                    .save(&application.paths.settings_file());
                tracing::info!(
                    url = %aitoolplus_core::deeplink::redact(&url),
                    provider = %name,
                    "cold-start deep-link import completed"
                );
            }
            Err(error) => tracing::warn!(
                url = %aitoolplus_core::deeplink::redact(&url),
                "deep-link import failed: {error}"
            ),
        }
    }

    tracing::info!("starting aitoolplus (data at {})", paths.app_data.display());

    // Tray on its own thread + GPUI pump for its events.
    let tray_handle = tray::spawn_tray();
    tray_handle.update_groups(tray::snapshot_from_store(application.store.store()));
    let tray_for_window = tray_handle.updater();
    let protocol_updater = tray_handle.updater();

    gpui_platform::application().run(move |cx| {
        // Keep running with no windows: GPUI must not quit when the
        // last window closes; the tray reopens it.
        cx.set_quit_mode(gpui::QuitMode::Explicit);

        if let Err(e) = app::open_main_window(&mut application, tray_for_window, cx) {
            tracing::error!("failed to open main window: {e}");
            cx.quit();
            return;
        }

        single_instance::pump_messages(message_receiver, protocol_updater, cx);
        config_watch::pump(config_events, cx);
        tray::pump_tray_events(tray_handle, cx);
    });

    drop(instance);
}
