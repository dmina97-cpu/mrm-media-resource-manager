//! Thông tin chẩn đoán: người dùng gặp lỗi thì xuất 1 file JSON gửi nhà phát triển.
//! KHÔNG chứa đường dẫn, tên thư viện, tên file hay nội dung ghi chú — chỉ số đếm, phiên bản, trạng thái và lỗi gần đây
//! (đường dẫn trong thông báo lỗi được che).
use crate::commands::AppState;
use crate::db;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

type Res<T> = Result<T, String>;

static RECENT: Mutex<VecDeque<(i64, String)>> = Mutex::new(VecDeque::new());

/// Che đường dẫn kiểu `D:\...` / `C:/...` / `\\server\...` trong thông báo.
pub fn redact(msg: &str) -> String {
    let mut out = String::with_capacity(msg.len());
    let chars: Vec<char> = msg.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let drive = i + 2 < chars.len() && chars[i].is_ascii_alphabetic() && chars[i + 1] == ':' && matches!(chars[i + 2], '\\' | '/');
        let unc = i + 1 < chars.len() && chars[i] == '\\' && chars[i + 1] == '\\';
        if drive || unc {
            out.push_str("<đường-dẫn>");
            // tới hết đường dẫn: dừng ở dấu nháy / xuống dòng / " (os error"
            while i < chars.len() && !matches!(chars[i], '"' | '\'' | '\n' | ')' | '(' | ',') {
                i += 1;
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Ghi nhận lỗi nền (AI, quét, cập nhật…) — giữ 50 lỗi gần nhất trong RAM.
pub fn note(context: &str, msg: &str) {
    let mut q = RECENT.lock().unwrap();
    q.push_back((db::now(), format!("{context}: {}", redact(msg))));
    while q.len() > 50 {
        q.pop_front();
    }
}

fn count(conn: &rusqlite::Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap_or(-1)
}

fn file_len(p: &std::path::Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn windows_version() -> String {
    if cfg!(target_os = "macos") {
        let v = std::process::Command::new("sw_vers").arg("-productVersion").output();
        return v.map(|o| format!("macOS {}", String::from_utf8_lossy(&o.stdout).trim())).unwrap_or_default();
    }
    let mut c = std::process::Command::new("cmd");
    crate::media::hidden(&mut c);
    c.args(["/C", "ver"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

pub fn build(app: &AppHandle) -> Value {
    let state = app.state::<AppState>();
    let db_path = state.data_dir.join("nink-vault.db");
    let backups = std::fs::read_dir(state.data_dir.join("backups")).map(|d| d.flatten().count()).unwrap_or(0);
    let corrupt = std::fs::read_dir(&state.data_dir)
        .map(|d| d.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("corrupt-")).count())
        .unwrap_or(0);
    let conn = state.db.lock().unwrap();
    let setting = |k: &str| db::get_setting(&conn, k);
    let quick_check: String = conn.query_row("PRAGMA quick_check(1)", [], |r| r.get(0)).unwrap_or_else(|e| e.to_string());
    let presence = crate::presence::plugin_presence(&conn);
    let scans: Vec<Value> = conn
        .prepare("SELECT started_at, finished_at, found, error FROM scan_history ORDER BY id DESC LIMIT 5")
        .and_then(|mut s| {
            s.query_map([], |r| {
                Ok(json!({ "started_at": r.get::<_, i64>(0)?, "finished_at": r.get::<_, Option<i64>>(1)?,
                           "found": r.get::<_, Option<i64>>(2)?, "error": r.get::<_, Option<String>>(3)?.map(|e| redact(&e)) }))
            })
            .map(|rows| rows.flatten().collect())
        })
        .unwrap_or_default();
    let recent: Vec<Value> = RECENT.lock().unwrap().iter().map(|(t, m)| json!({ "at": t, "message": m })).collect();
    json!({
        "format": "mrm-diagnostics",
        "schema": 1,
        "created_at": db::now(),
        "app": { "version": env!("CARGO_PKG_VERSION"), "os": windows_version(), "arch": std::env::consts::ARCH },
        "database": {
            "schema_version": count(&conn, "PRAGMA user_version"),
            "quick_check": quick_check,
            "size_bytes": file_len(&db_path),
            "wal_bytes": file_len(&db_path.with_file_name("nink-vault.db-wal")),
            "backups": backups,
            "corrupt_folders": corrupt,
        },
        "library": {
            "libraries": count(&conn, "SELECT count(*) FROM libraries"),
            "resources": count(&conn, "SELECT count(*) FROM resources"),
            "sources": count(&conn, "SELECT count(*) FROM resource_sources"),
            "sources_unavailable": count(&conn, "SELECT count(*) FROM resource_sources WHERE available = 0"),
            "archives": count(&conn, "SELECT count(*) FROM resource_sources WHERE kind = 'archive'"),
            "tags": count(&conn, "SELECT count(*) FROM tags"),
            "collections": count(&conn, "SELECT count(*) FROM collections"),
            "scan_excludes": crate::exclude::current_rules(&conn).len(),
        },
        "media": {
            "audio": count(&conn, "SELECT count(*) FROM media_assets WHERE media_type = 'audio' AND available = 1"),
            "image": count(&conn, "SELECT count(*) FROM media_assets WHERE media_type = 'image' AND available = 1"),
            "video": count(&conn, "SELECT count(*) FROM media_assets WHERE media_type = 'video' AND available = 1"),
            "unavailable": count(&conn, "SELECT count(*) FROM media_assets WHERE available = 0"),
            "metadata_pending": count(&conn, "SELECT count(*) FROM media_assets WHERE meta_state = 0"),
            "metadata_failed": count(&conn, "SELECT count(*) FROM media_assets WHERE meta_state = 2"),
        },
        "ai": {
            "enabled": setting("ai_enabled").as_deref() == Some("1"),
            "embed_model": setting("ai_embed_model"),
            "chat_model": setting("ai_chat_model"),
            "resource_embeddings": count(&conn, "SELECT count(*) FROM resource_embeddings"),
            "asset_embeddings": count(&conn, "SELECT count(*) FROM asset_embeddings"),
            "asset_categorized": count(&conn, "SELECT count(*) FROM media_assets WHERE ai_category IS NOT NULL"),
            "vision_enabled": setting("ai_vision_enabled").as_deref() == Some("1"),
            "vision_captions": count(&conn, "SELECT count(*) FROM asset_captions WHERE state = 1"),
            "vision_failed": count(&conn, "SELECT count(*) FROM asset_captions WHERE state = 2"),
        },
        "plugin": { "active": presence.plugin_active, "version": presence.version },
        "updates": { "auto": setting("update_auto").as_deref() != Some("0"), "checked_at": setting("update_checked_at") },
        "recent_scans": scans,
        "recent_errors": recent,
    })
}

/// Xuất file chẩn đoán (người dùng chọn nơi lưu).
#[tauri::command]
pub async fn export_diagnostics(app: AppHandle, path: String) -> Res<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let v = build(&app);
        std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap_or_default()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_paths() {
        assert_eq!(redact(r"không mở được D:\Setup\Edition\a b.wav (os error 5)"), "không mở được <đường-dẫn>(os error 5)");
        assert_eq!(redact("file \"C:/Users/x/y.png\" hỏng"), "file \"<đường-dẫn>\" hỏng");
        assert_eq!(redact(r"\\nas\share\x lỗi"), "<đường-dẫn>");
        assert_eq!(redact("GitHub 403"), "GitHub 403");
    }
}
