//! Phase 4 — Theo dõi thay đổi thư mục (ReadDirectoryChangesW qua crate `notify`) và trạng thái ổ ngoài.
//! Khi có thay đổi: gom sự kiện 3 giây rồi quét lại đúng các resource bị ảnh hưởng.

use crate::commands::AppState;
use crate::db;
use crate::scanner::{self, ScanProgress};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const QUIET: Duration = Duration::from_secs(3);
const STATUS_EVERY: Duration = Duration::from_secs(20);

#[derive(Default)]
struct Pending {
    last: Option<Instant>,
    rels: HashSet<String>,
    rescan_all: bool,
}

#[derive(Default)]
pub struct WatchManager {
    watchers: Mutex<HashMap<i64, RecommendedWatcher>>,
    pending: std::sync::Arc<Mutex<HashMap<i64, Pending>>>,
    online: Mutex<HashMap<i64, bool>>,
}

#[derive(Serialize, Clone)]
struct LibraryEvent {
    library_id: i64,
    reason: String,
}

/// Map đường dẫn thay đổi -> rel_path của (các) ứng viên resource có thể bị ảnh hưởng.
/// Chế độ Tự động (depth = 0): mọi thư mục tổ tiên, vì chưa biết tầng nào là gói.
pub fn candidate_rels(root: &Path, changed: &Path, depth: usize) -> Vec<String> {
    let Ok(rel) = changed.strip_prefix(root) else { return vec![] };
    let comps: Vec<_> = rel.components().collect();
    if comps.is_empty() {
        return vec![];
    }
    let upto: Vec<usize> = if depth == 0 { (1..=comps.len().min(10)).collect() } else { vec![comps.len().min(depth)] };
    upto.into_iter()
        .map(|n| comps[..n].iter().collect::<PathBuf>().to_string_lossy().to_string())
        .collect()
}

fn libraries(app: &AppHandle) -> Vec<(i64, String, usize)> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    let mut s = match conn.prepare("SELECT id, path, scan_depth FROM libraries") {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as usize)))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

pub fn enabled(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    db::get_setting(&conn, "auto_watch").map(|v| v != "0").unwrap_or(true)
}

/// Gắn/gỡ watcher cho khớp danh sách library hiện tại.
pub fn refresh(app: &AppHandle) {
    let wm = app.state::<WatchManager>();
    let on = enabled(app);
    let libs = libraries(app);
    let storage = app.state::<AppState>().storage_dir();
    let mut watchers = wm.watchers.lock().unwrap();
    let wanted: HashSet<i64> = if on { libs.iter().filter(|l| Path::new(&l.1).is_dir()).map(|l| l.0).collect() } else { HashSet::new() };
    watchers.retain(|id, _| wanted.contains(id));
    for (id, path, depth) in libs {
        if !wanted.contains(&id) || watchers.contains_key(&id) {
            continue;
        }
        let pending = wm.pending.clone();
        let root = PathBuf::from(&path);
        let root_cb = root.clone();
        let storage = storage.clone();
        let handler = move |res: notify::Result<notify::Event>| {
            let Ok(ev) = res else { return };
            let mut p = pending.lock().unwrap();
            let entry = p.entry(id).or_default();
            if ev.need_rescan() {
                entry.rescan_all = true;
            }
            for path in &ev.paths {
                if path.starts_with(&storage) {
                    continue; // bỏ qua cache của chính app
                }
                entry.rels.extend(candidate_rels(&root_cb, path, depth));
            }
            if entry.rescan_all || !entry.rels.is_empty() {
                entry.last = Some(Instant::now());
            }
        };
        match notify::recommended_watcher(handler) {
            Ok(mut w) => {
                if w.watch(&root, RecursiveMode::Recursive).is_ok() {
                    watchers.insert(id, w);
                }
            }
            Err(err) => eprintln!("watcher error for {path}: {err}"),
        }
    }
}

fn run_scan(app: &AppHandle, id: i64, force: Option<HashSet<String>>, full: bool) {
    let state = app.state::<AppState>();
    if !state.scanning.lock().unwrap().insert(id) {
        return;
    }
    let emitter = app.clone();
    let emit = move |p: ScanProgress| {
        let _ = emitter.emit("scan-progress", p);
    };
    let res = scanner::scan_library(&state.db, id, full, force.as_ref(), &emit);
    state.scanning.lock().unwrap().remove(&id);
    if res.is_ok() {
        let _ = crate::media_index::index_library(&state.db, id, Some(&state.storage_dir()));
    }
    if let Ok(r) = res {
        *state.learn.lock().unwrap() = None;
        let _ = app.emit("library-updated", r);
    }
}

/// Vòng lặp nền: xử lý sự kiện đã lắng + kiểm tra ổ online/offline định kỳ.
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        refresh(&app);
        let mut last_status = Instant::now() - STATUS_EVERY;
        loop {
            std::thread::sleep(Duration::from_millis(700));

            // 1. Sự kiện file đã yên 3 giây
            let ready: Vec<(i64, Pending)> = {
                let wm = app.state::<WatchManager>();
                let mut p = wm.pending.lock().unwrap();
                let ids: Vec<i64> = p.iter().filter(|(_, v)| v.last.map(|t| t.elapsed() >= QUIET).unwrap_or(false)).map(|(k, _)| *k).collect();
                ids.into_iter().filter_map(|id| p.remove(&id).map(|v| (id, v))).collect()
            };
            for (id, pend) in ready {
                if app.state::<AppState>().scanning.lock().unwrap().contains(&id) {
                    // đang quét: trả lại để xử lý sau
                    let wm = app.state::<WatchManager>();
                    let mut p = wm.pending.lock().unwrap();
                    let e = p.entry(id).or_default();
                    e.rels.extend(pend.rels);
                    e.rescan_all |= pend.rescan_all;
                    e.last = Some(Instant::now());
                    continue;
                }
                run_scan(&app, id, Some(pend.rels), pend.rescan_all);
            }

            // 2. Ổ ngoài cắm/rút
            if last_status.elapsed() >= STATUS_EVERY {
                last_status = Instant::now();
                let mut changed = false;
                for (id, path, _) in libraries(&app) {
                    let now_online = {
                        let state = app.state::<AppState>();
                        let conn = state.db.lock().unwrap();
                        scanner::check_library(&conn, id).unwrap_or(false)
                    };
                    let prev = {
                        let wm = app.state::<WatchManager>();
                        let mut online = wm.online.lock().unwrap();
                        online.insert(id, now_online)
                    };
                    match prev {
                        Some(was) if was != now_online => {
                            changed = true;
                            let _ = app.emit(
                                "library-status",
                                LibraryEvent { library_id: id, reason: if now_online { "online".into() } else { "offline".into() } },
                            );
                            // quét lại: online -> cập nhật thay đổi trong lúc rút ổ; offline -> đánh dấu unavailable
                            run_scan(&app, id, None, false);
                            let _ = path;
                        }
                        _ => {}
                    }
                }
                if changed {
                    crate::commands::allow_asset_dirs(&app);
                    refresh(&app);
                }
            }
        }
    });
}

#[tauri::command]
pub fn set_auto_watch(app: AppHandle, enabled: bool) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        db::set_setting(&state.db.lock().unwrap(), "auto_watch", if enabled { "1" } else { "0" }).map_err(|e| e.to_string())?;
    }
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn get_auto_watch(app: AppHandle) -> bool {
    enabled(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_changed_path_to_candidate() {
        let root = Path::new("D:\\Res");
        assert_eq!(candidate_rels(root, Path::new("D:\\Res\\Pack\\sub\\a.wav"), 1), vec!["Pack"]);
        assert_eq!(candidate_rels(root, Path::new("D:\\Res\\LUTs\\Kodak\\a.cube"), 2), vec!["LUTs\\Kodak"]);
        assert_eq!(candidate_rels(root, Path::new("D:\\Res\\x.zip"), 2), vec!["x.zip"]);
        assert!(candidate_rels(root, Path::new("D:\\Other\\x.zip"), 1).is_empty());
        assert_eq!(candidate_rels(root, Path::new("D:\\Res\\A\\B\\c.wav"), 0), vec!["A", "A\\B", "A\\B\\c.wav"]);
    }
}
