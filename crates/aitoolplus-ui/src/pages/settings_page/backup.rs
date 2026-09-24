use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{
    ButtonVariant, button_l, button_with_icon_l, button_with_icon_loading_l, card, input_container, section_title,
    segmented_pill_selector, settings_card, settings_row, toggle,
};
use crate::text_input::TextInput;
use crate::workspace::Workspace;

use crate::pages::SettingsTab;

pub(super) struct LocalBackupEntry {
    path: std::path::PathBuf,
    name: String,
    size_bytes: u64,
    date_str: String,
}

pub(super) fn list_local_backups(paths: &aitoolplus_core::paths::Paths) -> Vec<LocalBackupEntry> {
    let snapshots_dir = paths.local_snapshots_dir();
    let _ = std::fs::create_dir_all(&snapshots_dir);

    // Auto-migrate any legacy backup files into the dedicated snapshots directory
    let backup_dir = paths.app_data.join("backups");
    let legacy_dirs = [
        backup_dir.join("manual"),
        backup_dir.join("auto"),
        backup_dir.join("automatic"),
    ];
    for dir in legacy_dirs {
        if let Ok(read_dir) = std::fs::read_dir(&dir) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("zip") {
                    if let Some(name) = path.file_name() {
                        let target = snapshots_dir.join(name);
                        if !target.exists() {
                            let _ = std::fs::rename(&path, &target);
                        }
                    }
                }
            }
        }
    }
    // Also migrate loose snapshots directly in backups/
    if let Ok(read_dir) = std::fs::read_dir(&backup_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("zip") {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if file_name.starts_with("aitoolplus-backup-") || file_name.starts_with("aitoolplus-auto-") {
                    let target = snapshots_dir.join(file_name);
                    if !target.exists() {
                        let _ = std::fs::rename(&path, &target);
                    }
                }
            }
        }
    }

    let mut entries = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(&snapshots_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("zip") {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("backup.zip")
                    .to_string();
                let (size_bytes, date_str) = if let Ok(meta) = path.metadata() {
                    let size = meta.len();
                    let date = meta
                        .modified()
                        .ok()
                        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| {
                            let dt = chrono::DateTime::from_timestamp(d.as_secs() as i64, 0);
                            dt.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                                .unwrap_or_default()
                        })
                        .unwrap_or_default();
                    (size, date)
                } else {
                    (0, String::new())
                };
                entries.push(LocalBackupEntry {
                    path,
                    name,
                    size_bytes,
                    date_str,
                });
            }
        }
    }
    entries.sort_by(|a, b| {
        let meta_b = b.path.metadata().and_then(|m| m.modified()).ok();
        let meta_a = a.path.metadata().and_then(|m| m.modified()).ok();
        meta_b.cmp(&meta_a).then_with(|| b.name.cmp(&a.name))
    });
    entries
}

pub(super) fn get_backup_display_name(filename: &str) -> String {
    if let Some(rest) = filename.strip_prefix("aitoolplus-backup-").and_then(|s| s.strip_suffix(".zip")) {
        if rest.len() == 15 && rest.chars().nth(8) == Some('-') {
            let (d, t) = rest.split_at(8);
            let t = &t[1..];
            if d.len() == 8 && t.len() == 6 {
                return format!("{}-{}-{} {}:{}:{} (快照)", &d[0..4], &d[4..6], &d[6..8], &t[0..2], &t[2..4], &t[4..6]);
            }
        }
    }
    if let Some(rest) = filename.strip_prefix("aitoolplus-auto-").and_then(|s| s.strip_suffix(".zip")) {
        if rest.len() == 15 && rest.chars().nth(8) == Some('-') {
            let (d, t) = rest.split_at(8);
            let t = &t[1..];
            if d.len() == 8 && t.len() == 6 {
                return format!("{}-{}-{} {}:{}:{} (自动)", &d[0..4], &d[4..6], &d[6..8], &t[0..2], &t[2..4], &t[4..6]);
            }
        }
    }
    filename.strip_suffix(".zip").unwrap_or(filename).to_string()
}

pub(super) fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub(super) fn reload_after_restore(
    paths: &aitoolplus_core::paths::Paths,
) -> Option<(
    aitoolplus_core::store::StoreHandle,
    aitoolplus_core::settings::AppSettings,
)> {
    let mut store = aitoolplus_core::store::StoreHandle::open(paths).ok()?;
    let mut skills_store = store.store().skills.clone();
    let _ = aitoolplus_core::skills::scan_and_import_existing(paths, &mut skills_store);
    let _ = aitoolplus_core::skills::sync_all(&mut skills_store, paths);
    let _ = store.update(|db| db.skills = skills_store);
    let settings = aitoolplus_core::settings::AppSettings::load(&paths.settings_file());
    Some((store, settings))
}

pub(super) fn apply_restored(
    ws: &mut Workspace,
    loaded: Option<(
        aitoolplus_core::store::StoreHandle,
        aitoolplus_core::settings::AppSettings,
    )>,
) {
    crate::set_restore_in_flight(false);
    if let Some((store, settings)) = loaded {
        ws.store = store;
        ws.settings = settings;
        ws.persist_store();
        ws.ui.skills_discovered = true;
    }
}

pub(super) enum CloudRestoreError {
    Empty,
    List(String),
    Restore(String),
}

/// Run blocking IO off the UI thread, then apply the result on it.
pub(super) fn defer_io<T, W, A>(cx: &mut Context<Workspace>, work: W, apply: A)
where
    T: Send + 'static,
    W: FnOnce() -> T + Send + 'static,
    A: FnOnce(&mut Workspace, &mut Context<Workspace>, T) + 'static,
{
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let value = cx.background_spawn(async move { work() }).await;
        let _ = weak.update(cx, |ws, cx| apply(ws, cx, value));
    })
    .detach();
}

pub(super) fn backup_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let backup_type = ws.settings.backup_type;
    let webdav_config = ws.settings.webdav.clone();
    let webdav_inputs = ws.ui.webdav_inputs(&webdav_config, cx);
    let dav_url = webdav_inputs.url.clone();
    let dav_user = webdav_inputs.username.clone();
    let dav_password = webdav_inputs.password.clone();
    let dav_directory = webdav_inputs.remote_directory.clone();
    let s3_config = ws.settings.s3.clone();
    let s3_inputs = ws.ui.s3_inputs(&s3_config, cx);
    let s3_endpoint = s3_inputs.endpoint.clone();
    let s3_region = s3_inputs.region.clone();
    let s3_bucket = s3_inputs.bucket.clone();
    let s3_access_key = s3_inputs.access_key_id.clone();
    let s3_secret_key = s3_inputs.secret_access_key.clone();
    let s3_prefix = s3_inputs.prefix.clone();

    let transport_row = segmented_pill_selector(
        "backup-type",
        vec![
            (
                aitoolplus_core::settings::BackupType::Local,
                Some(crate::icons::FOLDER_SVG),
                i.t("本地快照", "Local"),
            ),
            (
                aitoolplus_core::settings::BackupType::Webdav,
                Some(crate::icons::GLOBE_SVG),
                i.t("WebDAV 云端", "WebDAV"),
            ),
            (
                aitoolplus_core::settings::BackupType::S3,
                Some(crate::icons::DOWNLOAD_SVG),
                i.t("S3 兼容存储", "S3 Storage"),
            ),
        ],
        backup_type,
        &t,
        cx,
        |ws, b_type, _, cx| {
            ws.settings.backup_type = b_type;
            (ws.callbacks.save_settings)(&ws.settings);
            cx.notify();
        },
    );

    let local_panel = (backup_type == aitoolplus_core::settings::BackupType::Local).then(|| {
        let local_backups = list_local_backups(&ws.paths);

        // 1. Policy Settings (Auto-backup Interval & Retain Count)
        let interval_val = ws.settings.backup_interval_hours;
        let interval_selector = segmented_pill_selector(
            "backup-interval",
            vec![
                (0u32, None, i.t("禁用", "Disabled")),
                (1u32, None, i.t("1小时", "1h")),
                (6u32, None, i.t("6小时", "6h")),
                (12u32, None, i.t("12小时", "12h")),
                (24u32, None, i.t("24小时", "24h")),
                (168u32, None, i.t("7天", "7d")),
            ],
            interval_val,
            &t,
            cx,
            |ws, val, _, cx| {
                ws.settings.backup_interval_hours = val;
                ws.settings.auto_backup_enabled = val > 0;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        );

        let retain_val = ws.settings.backup_retain_count;
        let retain_selector = segmented_pill_selector(
            "backup-retain",
            vec![
                (5usize, None, i.t("5个", "5")),
                (10usize, None, i.t("10个", "10")),
                (20usize, None, i.t("20个", "20")),
                (50usize, None, i.t("50个", "50")),
            ],
            retain_val,
            &t,
            cx,
            |ws, val, _, cx| {
                ws.settings.backup_retain_count = val;
                ws.settings.auto_backup_max_keep = val as u32;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        );

        let policy_section = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(settings_row(
                &t,
                i.t("自动备份间隔", "Auto-backup Interval"),
                Some(i.t(
                    "定时自动在本地创建快照的时间间隔（设为禁用则关闭自动备份）",
                    "Time interval for auto backup (Disabled to turn off)",
                )),
                interval_selector,
            ))
            .child(settings_row(
                &t,
                i.t("备份保留数量", "Backup Retention"),
                Some(i.t(
                    "本地自动与手动快照最多保留的数量，超出将自动清理最旧快照",
                    "Maximum number of backup snapshots to keep before pruning oldest",
                )),
                retain_selector,
            ));

        // 2. Action buttons row: 立即备份、从zip恢复备份、打开备份目录
        let buttons_row = div()
            .flex()
            .gap(px(8.0))
            .flex_wrap()
            .child(button_with_icon_loading_l(
                "backup-create-now",
                crate::icons::DOWNLOAD_SVG,
                i.t("立即备份", "Backup Now"),
                ButtonVariant::Primary,
                ws.ui.backup_busy,
                &t,
                cx,
                |ws, _, _, cx| {
                    if ws.ui.backup_busy {
                        return;
                    }
                    ws.ui.backup_busy = true;
                    cx.notify();
                    let dir = ws.paths.local_snapshots_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    let file_name = format!(
                        "aitoolplus-backup-{}.zip",
                        chrono::Local::now().format("%Y%m%d-%H%M%S")
                    );
                    let output = dir.join(&file_name);
                    let paths = ws.paths.clone();
                    let settings = ws.settings.clone();
                    let retain = ws.settings.backup_retain_count.max(1);
                    let dir_for_prune = dir.clone();
                    defer_io(
                        cx,
                        move || {
                            aitoolplus_core::backup::create_backup(&paths, &settings, &output).map(
                                |report| {
                                    let _ = aitoolplus_core::backup::prune_local_backups(
                                        &dir_for_prune,
                                        retain,
                                    );
                                    report
                                },
                            )
                        },
                        move |ws, cx, result| {
                            ws.ui.backup_busy = false;
                            match result {
                                Ok(report) => ws.ui.toast(
                                    ws.i18n
                                        .t(
                                            &format!(
                                                "备份创建成功：已保存至 {}（{} 个文件）",
                                                file_name, report.file_count
                                            ),
                                            &format!(
                                                "Backup created: {} ({} files)",
                                                file_name, report.file_count
                                            ),
                                        )
                                        .to_string(),
                                    false,
                                ),
                                Err(e) => ws.ui.toast(format!("创建备份失败: {e}"), true),
                            }
                            cx.notify();
                        },
                    );
                },
            ))
            .child(button_with_icon_loading_l(
                "backup-restore-zip",
                crate::icons::UPLOAD_SVG,
                i.t("从zip恢复备份", "Restore from ZIP Backup"),
                ButtonVariant::Secondary,
                ws.ui.backup_busy,
                &t,
                cx,
                |_ws, _, _, cx| {
                    let dialog = rfd::AsyncFileDialog::new().add_filter("ZIP", &["zip"]);
                    let weak = cx.entity().downgrade();
                    cx.spawn(async move |_this, cx| {
                        if let Some(file) = dialog.pick_file().await {
                            let archive = file.path().to_path_buf();
                            let prepared = weak.update(cx, |ws: &mut Workspace, cx| {
                                if ws.ui.backup_busy {
                                    return None;
                                }
                                ws.ui.backup_busy = true;
                                cx.notify();
                                Some((
                                    ws.paths.clone(),
                                    aitoolplus_core::backup::RestoreOptions {
                                        allow_custom_absolute: ws.ui.restore_allow_custom_absolute,
                                        conflict_strategy: ws.ui.restore_conflict_strategy,
                                    },
                                ))
                            });
                            let Some((paths, options)) = prepared.ok().flatten() else {
                                return;
                            };
                            let result = cx
                                .background_spawn(async move {
                                    crate::set_restore_in_flight(true);
                                    aitoolplus_core::backup::restore_backup_with_options(
                                        &paths, &archive, &options,
                                    )
                                    .map(|report| {
                                        let reloaded = reload_after_restore(&paths);
                                        (report, reloaded)
                                    })
                                })
                                .await;
                            let _ = weak.update(cx, |ws: &mut Workspace, cx| {
                                ws.ui.backup_busy = false;
                                match result {
                                    Ok((report, reloaded)) => {
                                        apply_restored(ws, reloaded);
                                        ws.ui.toast(
                                            ws.i18n
                                                .t(
                                                    &format!(
                                                        "从 ZIP 恢复完成：已恢复 {} 个文件 (覆盖 {}, 副本 {})",
                                                        report.restored,
                                                        report.overwritten,
                                                        report.copies.len()
                                                    ),
                                                    &format!(
                                                        "restore complete: {} files (overwritten {}, copies {})",
                                                        report.restored,
                                                        report.overwritten,
                                                        report.copies.len()
                                                    ),
                                                )
                                                .to_string(),
                                            false,
                                        );
                                    }
                                    Err(e) => {
                                        crate::set_restore_in_flight(false);
                                        ws.ui.toast(format!("从 ZIP 恢复失败: {e}"), true);
                                    }
                                }
                                cx.notify();
                            });
                        }
                    })
                    .detach();
                },
            ))
            .child(button_with_icon_l(
                "backup-open-folder",
                crate::icons::FOLDER_SVG,
                i.t("打开备份目录", "Open Backup Directory"),
                ButtonVariant::Secondary,
                &t,
                cx,
                |ws, _, _, _| {
                    let dir = ws.paths.local_snapshots_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = open_dir_in_explorer(&dir);
                },
            ));

        // 3. Backup list section
        let mut list_section = div().flex().flex_col().gap(px(6.0)).child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(i.t("本地备份列表", "Local Backup List")),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(format!("{} {}", local_backups.len(), i.t("个快照", "snapshots"))),
                ),
        );

        if local_backups.is_empty() {
            list_section = list_section.child(
                div()
                    .text_size(px(11.5))
                    .text_color(t.text_muted)
                    .p(px(8.0))
                    .child(i.t(
                        "暂无本地备份文件，点击上方【立即备份】即可创建首个快照",
                        "No local backups found. Click 'Backup Now' to create one.",
                    )),
            );
        } else {
            for b in local_backups {
                let file_path_for_restore = b.path.clone();
                let file_path_for_rename = b.path.clone();
                let file_path_for_save_as = b.path.clone();
                let file_path_for_delete = b.path.clone();
                let file_name = b.name.clone();
                let display_title = get_backup_display_name(&file_name);
                let file_name_for_restore_id = b.name.clone();
                let file_name_for_rename_id = b.name.clone();
                let file_name_for_save_as_id = b.name.clone();
                let file_name_for_save_as_dialog = b.name.clone();
                let file_name_for_delete_id = b.name.clone();
                let file_size_str = format_file_size(b.size_bytes);
                let date_str = b.date_str.clone();

                list_section = list_section.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .p(px(8.0))
                        .rounded(px(6.0))
                        .bg(t.card_bg)
                        .border_1()
                        .border_color(t.card_border)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(t.text_primary)
                                        .child(display_title),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(t.text_muted)
                                        .child(format!("{file_size_str} · {date_str} · {file_name}")),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(button_with_icon_loading_l(
                                    gpui::SharedString::from(format!("local-restore-{}", file_name_for_restore_id)),
                                    crate::icons::REFRESH_SVG,
                                    i.t("恢复", "Restore"),
                                    ButtonVariant::Secondary,
                                    ws.ui.backup_busy,
                                    &t,
                                    cx,
                                    move |ws, _, _, cx| {
                                        if ws.ui.backup_busy {
                                            return;
                                        }
                                        ws.ui.backup_busy = true;
                                        cx.notify();
                                        let paths = ws.paths.clone();
                                        let options = aitoolplus_core::backup::RestoreOptions {
                                            allow_custom_absolute: ws
                                                .ui
                                                .restore_allow_custom_absolute,
                                            conflict_strategy: ws.ui.restore_conflict_strategy,
                                        };
                                        let archive = file_path_for_restore.clone();
                                        defer_io(
                                            cx,
                                            move || {
                                                crate::set_restore_in_flight(true);
                                                aitoolplus_core::backup::restore_backup_with_options(
                                                    &paths, &archive, &options,
                                                )
                                                .map(|report| {
                                                    let reloaded = reload_after_restore(&paths);
                                                    (report, reloaded)
                                                })
                                            },
                                            |ws, cx, result| {
                                                ws.ui.backup_busy = false;
                                                match result {
                                                    Ok((report, reloaded)) => {
                                                        apply_restored(ws, reloaded);
                                                        ws.ui.toast(
                                                            ws.i18n
                                                                .t(
                                                                    &format!(
                                                                        "恢复完成：{} 个文件",
                                                                        report.restored
                                                                    ),
                                                                    &format!(
                                                                        "restore complete: {} files",
                                                                        report.restored
                                                                    ),
                                                                )
                                                                .to_string(),
                                                            false,
                                                        );
                                                    }
                                                    Err(e) => {
                                                        crate::set_restore_in_flight(false);
                                                        ws.ui.toast(format!("恢复失败: {e}"), true);
                                                    }
                                                }
                                                cx.notify();
                                            },
                                        );
                                    },
                                ))
                                .child(button_with_icon_l(
                                    gpui::SharedString::from(format!("local-rename-{}", file_name_for_rename_id)),
                                    crate::icons::PENCIL_SVG,
                                    i.t("重命名", "Rename"),
                                    ButtonVariant::Secondary,
                                    &t,
                                    cx,
                                    move |ws, _, _, cx| {
                                        let stem = file_name.strip_suffix(".zip").unwrap_or(&file_name).to_string();
                                        let input = cx.new(|cx| TextInput::new(stem, cx));
                                        ws.ui.backup_rename_dialog = Some((file_path_for_rename.clone(), input));
                                        cx.notify();
                                    },
                                ))
                                .child(button_with_icon_l(
                                    gpui::SharedString::from(format!("local-save-as-{}", file_name_for_save_as_id)),
                                    crate::icons::DOWNLOAD_SVG,
                                    i.t("另存为", "Save As"),
                                    ButtonVariant::Secondary,
                                    &t,
                                    cx,
                                    move |_ws, _, _, cx| {
                                        let dialog = rfd::AsyncFileDialog::new()
                                            .set_file_name(&file_name_for_save_as_dialog)
                                            .add_filter("ZIP", &["zip"]);
                                        let src = file_path_for_save_as.clone();
                                        let weak = cx.entity().downgrade();
                                        cx.spawn(async move |_this, cx| {
                                            if let Some(file) = dialog.save_file().await {
                                                let dst = file.path().to_path_buf();
                                                let _ = std::fs::copy(&src, &dst);
                                                let _ = weak.update(cx, |ws, cx| {
                                                    ws.ui.toast(
                                                        ws.i18n.t("文件已另存为指定位置", "Saved as specified").to_string(),
                                                        false,
                                                    );
                                                    cx.notify();
                                                });
                                            }
                                        }).detach();
                                    },
                                ))
                                .child(button_with_icon_l(
                                    gpui::SharedString::from(format!("local-delete-{}", file_name_for_delete_id)),
                                    crate::icons::TRASH_SVG,
                                    i.t("删除", "Delete"),
                                    ButtonVariant::Danger,
                                    &t,
                                    cx,
                                    move |ws, _, _, cx| {
                                        let _ = std::fs::remove_file(&file_path_for_delete);
                                        ws.ui.toast(
                                            ws.i18n.t("本地备份文件已删除", "Backup file deleted").to_string(),
                                            false,
                                        );
                                        cx.notify();
                                    },
                                )),
                        ),
                );
            }
        }

        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(t.card_border)
            .child(section_title(
                &t,
                i.t("本地快照", "Local Snapshots"),
                Some(i.t(
                    "配置自动备份策略，创建本地快照与从 ZIP 恢复数据",
                    "Configure auto-backup policy, create snapshot and restore",
                )),
            ))
            .child(policy_section)
            .child(buttons_row)
            .child(list_section)
            .into_any_element()
    });

    let webdav_panel = (backup_type == aitoolplus_core::settings::BackupType::Webdav).then(|| {
        let test_url = dav_url.clone();
        let test_user = dav_user.clone();
        let test_password = dav_password.clone();
        let test_directory = dav_directory.clone();

        let save_url = dav_url.clone();
        let save_user = dav_user.clone();
        let save_password = dav_password.clone();
        let save_directory = dav_directory.clone();

        let upload_url = dav_url.clone();
        let upload_user = dav_user.clone();
        let upload_password = dav_password.clone();
        let upload_directory = dav_directory.clone();

        let download_url = dav_url.clone();
        let download_user = dav_user.clone();
        let download_password = dav_password.clone();
        let download_directory = dav_directory.clone();

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(t.card_border)
            .child(dav_url.clone())
            .child(dav_user.clone())
            .child(dav_password.clone())
            .child(dav_directory.clone())
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .flex_wrap()
                    .child(button_with_icon_l(
                        "webdav-test",
                        crate::icons::GLOBE_SVG,
                        i.t("测试连接", "Test Connection"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let config = aitoolplus_core::settings::WebDavConfig {
                                url: test_url.update(cx, |input, _| input.text().trim().to_string()),
                                username: test_user.update(cx, |input, _| input.text().trim().to_string()),
                                password: test_password.update(cx, |input, _| input.text().to_string()),
                                remote_directory: test_directory.update(cx, |input, _| input.text().trim().to_string()),
                            };
                            match aitoolplus_core::webdav::test_connection(&config) {
                                Ok(()) => ws.ui.toast(
                                    ws.i18n.t("WebDAV 连接成功", "WebDAV connection succeeded").to_string(),
                                    false,
                                ),
                                Err(error) => ws.ui.toast(format!("WebDAV 连接失败: {error}"), true),
                            }
                            cx.notify();
                        },
                    ))
                    .child(button_with_icon_l(
                        "webdav-save",
                        crate::icons::CHECK_SVG,
                        i.t("保存设置", "Save Settings"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            ws.settings.webdav.url = save_url.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.username = save_user.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.password = save_password.update(cx, |input, _| input.text().to_string());
                            ws.settings.webdav.remote_directory = save_directory.update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);
                            ws.ui.toast(ws.i18n.t("已保存", "saved").to_string(), false);
                            cx.notify();
                        },
                    ))
                    .child(button_with_icon_loading_l(
                        "webdav-upload",
                        crate::icons::CLOUD_UPLOAD_SVG,
                        i.t("上传云端", "Upload to Cloud"),
                        ButtonVariant::Primary,
                        ws.ui.backup_busy,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            if ws.ui.backup_busy {
                                return;
                            }
                            ws.ui.backup_busy = true;
                            cx.notify();
                            ws.settings.webdav.url = upload_url.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.username = upload_user.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.password = upload_password.update(cx, |input, _| input.text().to_string());
                            ws.settings.webdav.remote_directory = upload_directory.update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);

                            let directory = ws.paths.app_data.join("backups").join("sync");
                            let output = directory.join("aitoolplus-sync.zip");
                            let paths = ws.paths.clone();
                            let settings = ws.settings.clone();
                            let webdav = ws.settings.webdav.clone();
                            defer_io(
                                cx,
                                move || {
                                    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                                    let report = aitoolplus_core::backup::create_backup(
                                        &paths, &settings, &output,
                                    )?;
                                    aitoolplus_core::webdav::upload(&webdav, &report.output)
                                },
                                |ws, cx, result| {
                                    ws.ui.backup_busy = false;
                                    match result {
                                        Ok(_) => ws.ui.toast(
                                            ws.i18n
                                                .t(
                                                    "上传成功！已同步至 WebDAV 云端",
                                                    "Upload succeeded: Synced to WebDAV cloud",
                                                )
                                                .to_string(),
                                            false,
                                        ),
                                        Err(e) => ws.ui.toast(format!("上传到 WebDAV 失败: {e}"), true),
                                    }
                                    cx.notify();
                                },
                            );
                        },
                    ))
                    .child(button_with_icon_loading_l(
                        "webdav-download",
                        crate::icons::CLOUD_DOWNLOAD_SVG,
                        i.t("从云端下载并恢复", "Download & Restore"),
                        ButtonVariant::Secondary,
                        ws.ui.backup_busy,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            if ws.ui.backup_busy {
                                return;
                            }
                            ws.ui.backup_busy = true;
                            cx.notify();
                            ws.settings.webdav.url = download_url.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.username = download_user.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.password = download_password.update(cx, |input, _| input.text().to_string());
                            ws.settings.webdav.remote_directory = download_directory.update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);

                            let webdav = ws.settings.webdav.clone();
                            let paths = ws.paths.clone();
                            let temporary_dir = ws.paths.app_data.join("backups").join("downloads");
                            let options = aitoolplus_core::backup::RestoreOptions {
                                allow_custom_absolute: ws.ui.restore_allow_custom_absolute,
                                conflict_strategy: ws.ui.restore_conflict_strategy,
                            };
                            defer_io(
                                cx,
                                move || {
                                    crate::set_restore_in_flight(true);
                                    let backups = match aitoolplus_core::webdav::list(&webdav) {
                                        Ok(items) => items,
                                        Err(error) => return Err(CloudRestoreError::List(error)),
                                    };
                                    if backups.is_empty() {
                                        return Err(CloudRestoreError::Empty);
                                    }
                                    let target_name = backups
                                        .iter()
                                        .find(|b| b.name == "aitoolplus-sync.zip")
                                        .map(|b| b.name.clone())
                                        .unwrap_or_else(|| backups[0].name.clone());
                                    let temporary = temporary_dir.join(&target_name);
                                    let path = match aitoolplus_core::webdav::download(
                                        &webdav, &target_name, &temporary,
                                    ) {
                                        Ok(path) => path,
                                        Err(error) => return Err(CloudRestoreError::Restore(error)),
                                    };
                                    let report = match aitoolplus_core::backup::restore_backup_with_options(
                                        &paths, &path, &options,
                                    ) {
                                        Ok(report) => report,
                                        Err(error) => return Err(CloudRestoreError::Restore(error)),
                                    };
                                    Ok((report, reload_after_restore(&paths)))
                                },
                                |ws, cx, result| {
                                    ws.ui.backup_busy = false;
                                    match result {
                                        Err(CloudRestoreError::Empty) => {
                                            crate::set_restore_in_flight(false);
                                            ws.ui.toast(
                                                ws.i18n
                                                    .t(
                                                        "云端暂无备份数据，请先上传",
                                                        "No backup found in cloud, please upload first",
                                                    )
                                                    .to_string(),
                                                true,
                                            );
                                        }
                                        Err(CloudRestoreError::List(error)) => {
                                            crate::set_restore_in_flight(false);
                                            ws.ui.toast(format!("获取 WebDAV 云端备份失败: {error}"), true);
                                        }
                                        Err(CloudRestoreError::Restore(error)) => {
                                            crate::set_restore_in_flight(false);
                                            ws.ui.toast(format!("云端下载恢复失败: {error}"), true);
                                        }
                                        Ok((report, reloaded)) => {
                                            apply_restored(ws, reloaded);
                                            ws.ui.toast(
                                                ws.i18n
                                                    .t(
                                                        &format!(
                                                            "云端恢复完成：已恢复 {} 个文件",
                                                            report.restored
                                                        ),
                                                        &format!(
                                                            "Cloud restore complete: {} files restored",
                                                            report.restored
                                                        ),
                                                    )
                                                    .to_string(),
                                                false,
                                            );
                                        }
                                    }
                                    cx.notify();
                                },
                            );
                        },
                    )),
            )
            .into_any_element()
    });

    let s3_panel = (backup_type == aitoolplus_core::settings::BackupType::S3).then(|| {
        let test_endpoint = s3_endpoint.clone();
        let test_region = s3_region.clone();
        let test_bucket = s3_bucket.clone();
        let test_access_key = s3_access_key.clone();
        let test_secret_key = s3_secret_key.clone();
        let test_prefix = s3_prefix.clone();

        let save_endpoint = s3_endpoint.clone();
        let save_region = s3_region.clone();
        let save_bucket = s3_bucket.clone();
        let save_access_key = s3_access_key.clone();
        let save_secret_key = s3_secret_key.clone();
        let save_prefix = s3_prefix.clone();

        let upload_endpoint = s3_endpoint.clone();
        let upload_region = s3_region.clone();
        let upload_bucket = s3_bucket.clone();
        let upload_access_key = s3_access_key.clone();
        let upload_secret_key = s3_secret_key.clone();
        let upload_prefix = s3_prefix.clone();

        let download_endpoint = s3_endpoint.clone();
        let download_region = s3_region.clone();
        let download_bucket = s3_bucket.clone();
        let download_access_key = s3_access_key.clone();
        let download_secret_key = s3_secret_key.clone();
        let download_prefix = s3_prefix.clone();

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(t.card_border)
            .child(s3_endpoint.clone())
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(div().flex_1().child(s3_region.clone()))
                    .child(div().flex_1().child(s3_bucket.clone())),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(div().flex_1().child(s3_access_key.clone()))
                    .child(div().flex_1().child(s3_secret_key.clone())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(div().flex_1().child(s3_prefix.clone()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(t.text_secondary)
                                    .child(i.t("路径模式 (Path-Style)", "Path-Style")),
                            )
                            .child(toggle(
                                "s3-path-style-toggle",
                                ws.settings.s3.path_style,
                                &t,
                                cx,
                                |ws, _, _, cx| {
                                    ws.settings.s3.path_style = !ws.settings.s3.path_style;
                                    (ws.callbacks.save_settings)(&ws.settings);
                                    cx.notify();
                                },
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .flex_wrap()
                    .child(button_with_icon_l(
                        "s3-test",
                        crate::icons::GLOBE_SVG,
                        i.t("测试连接", "Test Connection"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let config = aitoolplus_core::settings::S3Config {
                                endpoint: test_endpoint.update(cx, |input, _| input.text().trim().to_string()),
                                region: test_region.update(cx, |input, _| input.text().trim().to_string()),
                                bucket: test_bucket.update(cx, |input, _| input.text().trim().to_string()),
                                access_key_id: test_access_key.update(cx, |input, _| input.text().trim().to_string()),
                                secret_access_key: test_secret_key.update(cx, |input, _| input.text().to_string()),
                                prefix: test_prefix.update(cx, |input, _| input.text().trim().to_string()),
                                path_style: ws.settings.s3.path_style,
                            };
                            match aitoolplus_core::s3::test_connection(&config) {
                                Ok(()) => ws.ui.toast(
                                    ws.i18n.t("S3 连接成功", "S3 connection succeeded").to_string(),
                                    false,
                                ),
                                Err(error) => ws.ui.toast(format!("S3 连接失败: {error}"), true),
                            }
                            cx.notify();
                        },
                    ))
                    .child(button_with_icon_l(
                        "s3-save",
                        crate::icons::CHECK_SVG,
                        i.t("保存设置", "Save Settings"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            ws.settings.s3.endpoint = save_endpoint.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.region = save_region.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.bucket = save_bucket.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.access_key_id = save_access_key.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.secret_access_key = save_secret_key.update(cx, |input, _| input.text().to_string());
                            ws.settings.s3.prefix = save_prefix.update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);
                            ws.ui.toast(ws.i18n.t("已保存", "saved").to_string(), false);
                            cx.notify();
                        },
                    ))
                    .child(button_with_icon_loading_l(
                        "s3-upload",
                        crate::icons::CLOUD_UPLOAD_SVG,
                        i.t("上传云端", "Upload to Cloud"),
                        ButtonVariant::Primary,
                        ws.ui.backup_busy,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            if ws.ui.backup_busy {
                                return;
                            }
                            ws.ui.backup_busy = true;
                            cx.notify();
                            ws.settings.s3.endpoint = upload_endpoint.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.region = upload_region.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.bucket = upload_bucket.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.access_key_id = upload_access_key.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.secret_access_key = upload_secret_key.update(cx, |input, _| input.text().to_string());
                            ws.settings.s3.prefix = upload_prefix.update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);

                            let directory = ws.paths.app_data.join("backups").join("sync");
                            let output = directory.join("aitoolplus-sync.zip");
                            let paths = ws.paths.clone();
                            let settings = ws.settings.clone();
                            let s3 = ws.settings.s3.clone();
                            defer_io(
                                cx,
                                move || {
                                    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                                    let report = aitoolplus_core::backup::create_backup(
                                        &paths, &settings, &output,
                                    )?;
                                    aitoolplus_core::s3::upload(&s3, &report.output)
                                },
                                |ws, cx, result| {
                                    ws.ui.backup_busy = false;
                                    match result {
                                        Ok(_) => ws.ui.toast(
                                            ws.i18n
                                                .t(
                                                    "上传成功！已同步至 S3 云端",
                                                    "Upload succeeded: Synced to S3 cloud",
                                                )
                                                .to_string(),
                                            false,
                                        ),
                                        Err(e) => ws.ui.toast(format!("上传到 S3 失败: {e}"), true),
                                    }
                                    cx.notify();
                                },
                            );
                        },
                    ))
                    .child(button_with_icon_loading_l(
                        "s3-download",
                        crate::icons::CLOUD_DOWNLOAD_SVG,
                        i.t("从云端下载并恢复", "Download & Restore"),
                        ButtonVariant::Secondary,
                        ws.ui.backup_busy,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            if ws.ui.backup_busy {
                                return;
                            }
                            ws.ui.backup_busy = true;
                            cx.notify();
                            ws.settings.s3.endpoint = download_endpoint.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.region = download_region.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.bucket = download_bucket.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.access_key_id = download_access_key.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.secret_access_key = download_secret_key.update(cx, |input, _| input.text().to_string());
                            ws.settings.s3.prefix = download_prefix.update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);

                            let s3 = ws.settings.s3.clone();
                            let paths = ws.paths.clone();
                            let temporary_dir = ws.paths.app_data.join("backups").join("downloads");
                            let options = aitoolplus_core::backup::RestoreOptions {
                                allow_custom_absolute: ws.ui.restore_allow_custom_absolute,
                                conflict_strategy: ws.ui.restore_conflict_strategy,
                            };
                            defer_io(
                                cx,
                                move || {
                                    crate::set_restore_in_flight(true);
                                    let backups = match aitoolplus_core::s3::list(&s3) {
                                        Ok(items) => items,
                                        Err(error) => return Err(CloudRestoreError::List(error)),
                                    };
                                    if backups.is_empty() {
                                        return Err(CloudRestoreError::Empty);
                                    }
                                    let target_name = backups
                                        .iter()
                                        .find(|b| b.name == "aitoolplus-sync.zip")
                                        .map(|b| b.name.clone())
                                        .unwrap_or_else(|| backups[0].name.clone());
                                    let temporary = temporary_dir.join(&target_name);
                                    let path = match aitoolplus_core::s3::download(
                                        &s3, &target_name, &temporary,
                                    ) {
                                        Ok(path) => path,
                                        Err(error) => return Err(CloudRestoreError::Restore(error)),
                                    };
                                    let report = match aitoolplus_core::backup::restore_backup_with_options(
                                        &paths, &path, &options,
                                    ) {
                                        Ok(report) => report,
                                        Err(error) => return Err(CloudRestoreError::Restore(error)),
                                    };
                                    Ok((report, reload_after_restore(&paths)))
                                },
                                |ws, cx, result| {
                                    ws.ui.backup_busy = false;
                                    match result {
                                        Err(CloudRestoreError::Empty) => {
                                            crate::set_restore_in_flight(false);
                                            ws.ui.toast(
                                                ws.i18n
                                                    .t(
                                                        "云端暂无备份数据，请先上传",
                                                        "No backup found in cloud, please upload first",
                                                    )
                                                    .to_string(),
                                                true,
                                            );
                                        }
                                        Err(CloudRestoreError::List(error)) => {
                                            crate::set_restore_in_flight(false);
                                            ws.ui.toast(format!("获取 S3 云端备份失败: {error}"), true);
                                        }
                                        Err(CloudRestoreError::Restore(error)) => {
                                            crate::set_restore_in_flight(false);
                                            ws.ui.toast(format!("云端下载恢复失败: {error}"), true);
                                        }
                                        Ok((report, reloaded)) => {
                                            apply_restored(ws, reloaded);
                                            ws.ui.toast(
                                                ws.i18n
                                                    .t(
                                                        &format!(
                                                            "云端恢复完成：已恢复 {} 个文件",
                                                            report.restored
                                                        ),
                                                        &format!(
                                                            "Cloud restore complete: {} files restored",
                                                            report.restored
                                                        ),
                                                    )
                                                    .to_string(),
                                                false,
                                            );
                                        }
                                    }
                                    cx.notify();
                                },
                            );
                        },
                    )),
            )
            .into_any_element()
    });

    let backup_card = card(
        &t,
        vec![
            section_title(
                &t,
                i.t("备份与恢复", "Backup & Restore"),
                Some(i.t(
                    "ZIP 备份包含应用数据、CLI 配置与中央 Skills 仓库",
                    "ZIP bundles include app data, CLI configs and the central Skills repo",
                )),
            ),
            transport_row,
            local_panel.unwrap_or_else(|| div().into_any_element()),
            webdav_panel.unwrap_or_else(|| div().into_any_element()),
            s3_panel.unwrap_or_else(|| div().into_any_element()),
        ],
    );

    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(16.0))
        .child(backup_card)
        .into_any_element()
}

pub(super) fn open_dir_in_explorer(path: &std::path::Path) -> std::io::Result<()> {
    crate::pages::open_path_in_default_manager(path);
    Ok(())
}

/// Local copy of the dark detection (keeps this module self-contained).
pub fn render_backup_rename_dialog(
    path: std::path::PathBuf,
    input: gpui::Entity<TextInput>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(crate::components::input_container(&t, input.clone()))
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "backup-rename-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.backup_rename_dialog = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "backup-rename-save",
                    i.t("保存", "Save"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let new_name: String = input.update(cx, |inp, _| inp.text().trim().to_string());
                        if !new_name.is_empty() {
                            let filename = if new_name.ends_with(".zip") {
                                new_name
                            } else {
                                format!("{new_name}.zip")
                            };
                            if let Some(parent) = path.parent() {
                                let new_path = parent.join(&filename);
                                if new_path.exists() && new_path != path {
                                    ws.ui.toast(
                                        ws.i18n.t("同名备份文件已存在", "Backup with this name already exists").to_string(),
                                        true,
                                    );
                                } else {
                                    match std::fs::rename(&path, &new_path) {
                                        Ok(()) => {
                                            ws.ui.toast(
                                                ws.i18n.t("备份已重命名", "Backup renamed").to_string(),
                                                false,
                                            );
                                        }
                                        Err(e) => {
                                            ws.ui.toast(format!("重命名失败: {e}"), true);
                                        }
                                    }
                                }
                            }
                        }
                        ws.ui.backup_rename_dialog = None;
                        cx.notify();
                    },
                )),
        );

    crate::pages::modal_scaffold(
        &t,
        i.t("重命名备份快照", "Rename Backup Snapshot").as_ref(),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.backup_rename_dialog = None;
            cx.notify();
        },
    )
}
