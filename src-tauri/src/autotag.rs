//! Gắn tag tự động có ghi nguồn gốc — để người dùng biết tag nào do máy gắn và rà soát lại sau.
//!
//! `resource_tags.source`:
//!   NULL      = người dùng gắn tay hoặc đã xác nhận
//!   "content" = từ loại file bên trong      "path"    = từ tên thư mục chứa
//!   "rule"    = từ Rule                      "learned" = học từ cách bạn gắn tag
//!   "ai"      = AI (phân tích / gần nghĩa)   "scan"    = trạng thái do máy quét gắn (Downloaded/Extracted)
//! Tag người dùng đã gỡ được ghi vào `tag_rejections` và không bao giờ tự gắn lại.

use crate::db;
use rusqlite::{params, Connection, OptionalExtension};

/// Điều kiện SQL: resource có tag tự động chưa được xác nhận (bỏ qua trạng thái do máy quét).
pub const NEEDS_REVIEW_SQL: &str = "EXISTS (SELECT 1 FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id
    WHERE rt.resource_id = r.id AND rt.source IS NOT NULL AND rt.source NOT IN ('scan', 'folder') AND t.kind <> 'status')";

pub fn is_rejected(conn: &Connection, resource_id: i64, tag_id: i64) -> rusqlite::Result<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM tag_rejections WHERE resource_id = ?1 AND tag_id = ?2", params![resource_id, tag_id], |_| Ok(()))
        .optional()?
        .is_some())
}

/// Gắn tag tự động. Không ghi đè tag đã có (tag tay giữ nguyên là tag tay), bỏ qua tag người dùng đã gỡ.
/// Trả về true nếu thực sự gắn thêm.
pub fn insert_auto(conn: &Connection, resource_id: i64, tag_id: i64, source: &str, reason: &str) -> rusqlite::Result<bool> {
    if is_rejected(conn, resource_id, tag_id)? {
        return Ok(false);
    }
    let n = conn.execute(
        "INSERT OR IGNORE INTO resource_tags (resource_id, tag_id, source, reason, applied_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![resource_id, tag_id, source, reason, db::now()],
    )?;
    Ok(n > 0)
}

/// Gắn tay / xác nhận: tag trở thành tag của người dùng, xóa dấu "đã gỡ".
pub fn insert_manual(conn: &Connection, resource_id: i64, tag_id: i64) -> rusqlite::Result<()> {
    conn.execute("INSERT OR IGNORE INTO resource_tags (resource_id, tag_id) VALUES (?1, ?2)", params![resource_id, tag_id])?;
    conn.execute(
        "UPDATE resource_tags SET source = NULL, reason = NULL, applied_at = NULL WHERE resource_id = ?1 AND tag_id = ?2",
        params![resource_id, tag_id],
    )?;
    conn.execute("DELETE FROM tag_rejections WHERE resource_id = ?1 AND tag_id = ?2", params![resource_id, tag_id])?;
    Ok(())
}

/// Gỡ tag và ghi nhớ để lần sau không tự gắn lại.
pub fn remove_and_reject(conn: &Connection, resource_id: i64, tag_id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM resource_tags WHERE resource_id = ?1 AND tag_id = ?2", params![resource_id, tag_id])?;
    conn.execute(
        "INSERT OR IGNORE INTO tag_rejections (resource_id, tag_id, created_at) VALUES (?1, ?2, ?3)",
        params![resource_id, tag_id, db::now()],
    )?;
    Ok(())
}

/// Tìm hoặc tạo tag app/type theo tên.
pub fn ensure_tag(conn: &Connection, kind: &str, name: &str) -> rusqlite::Result<i64> {
    if let Some(id) = db::tag_id(conn, kind, name)? {
        return Ok(id);
    }
    conn.execute("INSERT INTO tags (kind, name) VALUES (?1, ?2)", params![kind, name])?;
    Ok(conn.last_insert_rowid())
}

#[derive(serde::Deserialize, Default)]
struct StoredSuggestions {
    #[serde(default)]
    types: Vec<String>,
    #[serde(default)]
    apps: Vec<String>,
    #[serde(default)]
    path_types: Vec<String>,
    #[serde(default)]
    path_apps: Vec<String>,
}

/// Chính sách mặc định khi quét resource MỚI: chỉ nguồn chắc chắn (nội dung file, tên thư mục, rule),
/// chỉ App + Type, mỗi loại tối đa 2 tag. Tag "học" và AI để người dùng chủ động áp dụng.
pub fn apply_default(conn: &Connection, resource_id: i64) -> rusqlite::Result<usize> {
    let json: Option<String> =
        conn.query_row("SELECT suggestions FROM resources WHERE id = ?1", [resource_id], |r| r.get(0)).optional()?.flatten();
    let s: StoredSuggestions = json.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
    let mut n = 0;
    let mut plan: Vec<(&str, String, &str, &str)> = Vec::new();
    for t in s.types.iter().take(2) {
        plan.push(("type", t.clone(), "content", "Từ các file bên trong"));
    }
    for a in s.apps.iter().take(2) {
        plan.push(("app", a.clone(), "content", "Từ các file bên trong"));
    }
    for t in s.path_types.iter().take(1) {
        plan.push(("type", t.clone(), "path", "Từ tên thư mục chứa"));
    }
    for a in s.path_apps.iter().take(2) {
        plan.push(("app", a.clone(), "path", "Từ tên thư mục chứa"));
    }
    for (kind, name, source, reason) in plan {
        let tid = ensure_tag(conn, kind, &name)?;
        if insert_auto(conn, resource_id, tid, source, reason)? {
            n += 1;
        }
    }
    for (tid, rule) in crate::rules::suggestions(conn, resource_id)? {
        if insert_auto(conn, resource_id, tid, "rule", &format!("Rule: {rule}"))? {
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_and_rejection() {
        let dir = std::env::temp_dir().join(format!("mrm_autotag_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let conn = db::open(&dir.join("t.db")).unwrap();
        conn.execute(
            "INSERT INTO resources (name, suggestions, created_at, updated_at) VALUES ('Kodak', ?1, 0, 0)",
            [r#"{"types":["LUT"],"apps":[],"path_types":["LUT"],"path_apps":["DaVinci Resolve"]}"#],
        )
        .unwrap();
        assert_eq!(apply_default(&conn, 1).unwrap(), 2); // LUT (content) + DaVinci Resolve (path)
        let review: i64 = conn.query_row(&format!("SELECT count(*) FROM resources r WHERE {NEEDS_REVIEW_SQL}"), [], |r| r.get(0)).unwrap();
        assert_eq!(review, 1);
        let lut = db::tag_id(&conn, "type", "LUT").unwrap().unwrap();
        let dr = db::tag_id(&conn, "app", "DaVinci Resolve").unwrap().unwrap();
        // xác nhận LUT, gỡ DaVinci Resolve
        insert_manual(&conn, 1, lut).unwrap();
        remove_and_reject(&conn, 1, dr).unwrap();
        let review: i64 = conn.query_row(&format!("SELECT count(*) FROM resources r WHERE {NEEDS_REVIEW_SQL}"), [], |r| r.get(0)).unwrap();
        assert_eq!(review, 0);
        // quét lại: không tự gắn lại tag đã gỡ, không biến tag tay thành tag tự động
        assert_eq!(apply_default(&conn, 1).unwrap(), 0);
        let src: Option<String> =
            conn.query_row("SELECT source FROM resource_tags WHERE tag_id = ?1", [lut], |r| r.get(0)).unwrap();
        assert!(src.is_none());
    }
}
