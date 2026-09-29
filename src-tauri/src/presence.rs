//! Nhận biết plugin HNH SFX Finder đang mở trong DaVinci Resolve (nhịp tim trong DB dùng chung)
//! và luồng nền của Media Browser: đọc metadata media, xử lý yêu cầu từ plugin.
//!
//! Khi plugin đang mở: MRM tắt Media Browser (UI), tạm dừng việc nặng (metadata, AI) và nhả model AI
//! khỏi VRAM để GPU dành cho Resolve. Plugin ngừng gửi nhịp tim > 15 giây (đóng/crash) -> MRM tự mở lại.

use crate::commands::AppState;
use crate::db;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub const PLUGIN_APP: &str = "hnh-sfx-finder";
pub const STALE_SECS: i64 = 15;

#[derive(Serialize, Clone, Default)]
pub struct Presence {
    pub plugin_active: bool,
    pub version: Option<String>,
    pub last_seen: Option<i64>,
}

pub fn plugin_presence(conn: &Connection) -> Presence {
    let row: Option<(Option<String>, i64)> = conn
        .query_row("SELECT version, last_seen FROM app_presence WHERE app = ?1", [PLUGIN_APP], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .ok()
        .flatten();
    match row {
        Some((version, last_seen)) => Presence { plugin_active: db::now() - last_seen <= STALE_SECS, version, last_seen: Some(last_seen) },
        None => Presence::default(),
    }
}

pub fn plugin_active(conn: &Connection) -> bool {
    plugin_presence(conn).plugin_active
}

#[tauri::command]
pub fn get_presence(state: tauri::State<AppState>) -> Presence {
    plugin_presence(&state.db.lock().unwrap())
}

static WAS_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Quét lại library + lập chỉ mục media (dùng cho yêu cầu từ plugin).
fn rescan_all(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let ids: Vec<i64> = {
        let conn = state.db.lock().unwrap();
        conn.prepare("SELECT id FROM libraries")
            .and_then(|mut s| s.query_map([], |r| r.get(0)).map(|rows| rows.flatten().collect()))
            .unwrap_or_default()
    };
    let mut total = 0;
    for id in ids {
        if !state.scanning.lock().unwrap().insert(id) {
            continue;
        }
        let emitter = app.clone();
        let emit = move |p: crate::scanner::ScanProgress| {
            let _ = emitter.emit("scan-progress", p);
        };
        let r = crate::scanner::scan_library(&state.db, id, false, None, &emit);
        state.scanning.lock().unwrap().remove(&id);
        if r.is_ok() {
            if let Ok(m) = crate::media_index::index_library(&state.db, id, Some(&state.storage_dir())) {
                total += m.found;
            }
            let _ = app.emit("library-updated", ());
        }
    }
    format!("{{\"assets\":{total}}}")
}

/// Luồng nền: chạy suốt vòng đời app.
pub fn start_worker(app: AppHandle) {
    let mut tick: u64 = 0;
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        tick += 1;
        let state = app.state::<AppState>();
        // mỗi ~1 phút gộp WAL vào file chính
        if tick % 30 == 0 {
            db::checkpoint(&state.db.lock().unwrap(), false);
        }
        // 0. Nhịp tim của MRM (plugin dùng để biết MRM có đang chạy để xử lý yêu cầu)
        {
            let conn = state.db.lock().unwrap();
            let _ = conn.execute(
                "INSERT INTO app_presence (app, pid, version, detail, last_seen) VALUES ('mrm', ?1, ?2, 'MRM', ?3)
                 ON CONFLICT(app) DO UPDATE SET pid = excluded.pid, version = excluded.version, last_seen = excluded.last_seen",
                params![std::process::id(), env!("CARGO_PKG_VERSION"), db::now()],
            );
        }

        // 1. Yêu cầu từ plugin (quét lại thư viện)
        let requests: Vec<(i64, String)> = {
            let conn = state.db.lock().unwrap();
            conn.prepare("SELECT id, kind FROM app_requests WHERE handled_at IS NULL ORDER BY id LIMIT 5")
                .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|rows| rows.flatten().collect()))
                .unwrap_or_default()
        };
        for (id, kind) in requests {
            let result = match kind.as_str() {
                "rescan" => rescan_all(&app),
                _ => "{\"error\":\"unknown request\"}".into(),
            };
            let conn = state.db.lock().unwrap();
            let _ = conn.execute("UPDATE app_requests SET handled_at = ?1, result = ?2 WHERE id = ?3", params![db::now(), result, id]);
        }

        // 2. Plugin đang mở trong DaVinci?
        let active = plugin_active(&state.db.lock().unwrap());
        if active != WAS_ACTIVE.swap(active, Ordering::SeqCst) {
            let p = plugin_presence(&state.db.lock().unwrap());
            let _ = app.emit("plugin-presence", p);
            if active {
                // nhả VRAM cho Resolve
                std::thread::spawn(crate::ai::unload_models);
            }
        }
        if active {
            continue; // tạm dừng việc nặng
        }

        // 3. Metadata media (duration, kích thước, codec...) — từng lô nhỏ
        let n = crate::media_index::probe_batch(&state.db, 60);
        if n > 0 {
            let _ = app.emit("media-meta-progress", n);
        } else {
            std::thread::sleep(Duration::from_secs(8));
        }
    });
}

/// MRM thoát: xóa nhịp tim.
pub fn clear_mrm_presence(conn: &Connection) {
    let _ = conn.execute("DELETE FROM app_presence WHERE app = 'mrm'", []);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_freshness() {
        let dir = std::env::temp_dir().join(format!("mrm_presence_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let conn = db::open(&dir.join("t.db")).unwrap();
        assert!(!plugin_active(&conn));
        conn.execute("INSERT INTO app_presence (app, pid, version, last_seen) VALUES (?1, 1, '0.11.0', ?2)", params![PLUGIN_APP, db::now()]).unwrap();
        assert!(plugin_active(&conn));
        conn.execute("UPDATE app_presence SET last_seen = ?1", [db::now() - 60]).unwrap();
        assert!(!plugin_active(&conn), "nhịp tim cũ (plugin đóng/crash) -> không còn hoạt động");
    }
}
