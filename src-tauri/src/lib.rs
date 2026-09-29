mod ai;
mod asset_ai;
mod assets;
mod autotag;
mod autodetect;
mod classify;
mod commands;
mod db;
mod exclude;
mod matcher;
mod media;
mod media_index;
mod normalize;
mod noteimg;
mod organize;
mod presence;
mod rules;
mod scanner;
mod updater;
mod vision;
mod watcher;

use commands::AppState;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();
    // bản chạy thử (MRM_DATA_DIR) không chiếm quyền "một cửa sổ" của MRM thật
    if std::env::var_os("MRM_DATA_DIR").is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // chỉ một MRM được mở: lần mở thứ hai chỉ đưa cửa sổ cũ lên trước
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }));
    }
    builder
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // MRM_DATA_DIR: chạy thử trên bản sao dữ liệu (dev/test), không đụng database thật
            let data_dir = match std::env::var_os("MRM_DATA_DIR") {
                Some(d) => PathBuf::from(d),
                None => app.path().app_data_dir()?,
            };
            std::fs::create_dir_all(&data_dir)?;
            let (conn, notice) = match db::open_safe(&data_dir.join("nink-vault.db"), &data_dir.join("backups")) {
                Ok(v) => v,
                Err(err) => {
                    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                    app.dialog().message(err.to_string()).title("MRM – Không mở được database").kind(MessageDialogKind::Error).blocking_show();
                    return Err(err.into());
                }
            };
            if let Some(n) = notice {
                eprintln!("{n}");
                let _ = db::set_setting(&conn, "db_notice", &n);
            }
            let storage = db::get_setting(&conn, "storage_dir")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| data_dir.clone());
            let state = AppState {
                db: Mutex::new(conn),
                data_dir,
                scanning: Mutex::new(HashSet::new()),
                storage: Mutex::new(storage),
                media_sem: Arc::new(tokio::sync::Semaphore::new(3)),
                learn: Mutex::new(None),
            };
            if let Err(err) = commands::auto_backup(&state) {
                eprintln!("auto backup failed: {err}");
            }
            app.manage(state);
            app.manage(watcher::WatchManager::default());
            app.manage(ai::AiManager::default());
            let _ = ai::APP.set(app.handle().clone());
            commands::allow_asset_dirs(app.handle());
            watcher::start(app.handle().clone());
            presence::start_worker(app.handle().clone());
            ai::autostart(app.handle());
            vision::start_worker(app.handle());
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let st = handle.state::<AppState>();
                media::enforce_cache_limit(&st);
                noteimg::cleanup_orphans(&st);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_libraries,
            commands::add_library,
            commands::update_library,
            commands::remove_library,
            commands::scan_library,
            commands::scan_all,
            commands::query_resources,
            commands::get_resource,
            commands::update_resource,
            commands::set_favorite,
            commands::set_tag,
            commands::merge_resources,
            commands::split_source,
            commands::reveal_source,
            commands::list_tags,
            commands::create_tag,
            commands::update_tag,
            commands::delete_tag,
            commands::list_match_suggestions,
            commands::resolve_match,
            commands::get_dashboard,
            commands::backup_now,
            commands::list_backups,
            commands::restore_backup,
            commands::export_json,
            commands::open_data_dir,
            commands::get_data_dir,
            commands::get_storage_info,
            commands::list_scan_overrides,
            commands::apply_suggestions,
            commands::confirm_auto_tags,
            commands::get_pref,
            commands::take_db_notice,
            presence::get_presence,
            exclude::get_scan_excludes,
            updater::update_check,
            updater::update_configure,
            updater::update_install,
            exclude::set_scan_excludes,
            assets::query_assets,
            assets::asset_counts,
            assets::asset_facets,
            assets::get_asset,
            assets::set_asset_favorite,
            assets::set_asset_tag,
            assets::list_asset_tag_names,
            assets::use_assets,
            assets::reveal_asset,
            assets::drag_icon,
            assets::resource_asset_counts,
            assets::similar_assets,
            assets::tag_assets_by_query,
            asset_ai::asset_ai_status,
            vision::vision_status,
            vision::vision_set_enabled,
            vision::vision_set_paused,
            commands::set_pref,
            noteimg::add_note_image,
            noteimg::remove_note_image,
            noteimg::note_image_to_front,
            commands::set_scan_override,
            commands::set_storage_dir,
            media::list_source_files,
            media::get_thumbnail,
            media::get_video_sprite,
            media::preview_file,
            media::get_waveform,
            media::get_video_proxy,
            media::get_cover,
            media::set_cover,
            media::cache_stats,
            media::clear_cache,
            media::set_cache_limit,
            rules::list_rules,
            rules::save_rule,
            rules::delete_rule,
            rules::test_rule,
            rules::apply_rule_now,
            organize::find_duplicates,
            organize::dismiss_duplicates,
            organize::check_existing,
            organize::list_version_families,
            organize::mark_outdated_versions,
            organize::add_relation,
            organize::remove_relation,
            watcher::set_auto_watch,
            watcher::get_auto_watch,
            ai::ai_install,
            ai::ai_set_enabled,
            ai::ai_status,
            ai::ai_analyze,
            ai::ai_dismiss,
            ai::ai_apply,
            ai::ai_get_auto_apply,
            ai::ai_set_auto_apply,
            ai::ai_pending_count,
            ai::ai_set_models,
            ai::ai_similar,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let state = app.state::<AppState>();
                let conn = state.db.lock().unwrap();
                presence::clear_mrm_presence(&conn);
                db::checkpoint(&conn, true);
                ai::shutdown(app);
            }
        });
}
