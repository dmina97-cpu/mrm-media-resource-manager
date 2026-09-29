//! Collections: nhóm tay do người dùng tạo ("SFX vlog", "LUT điện ảnh"…), chứa cả resource lẫn file media.
use crate::commands::AppState;
use crate::db;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;

type Res<T> = Result<T, String>;
fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

#[derive(Serialize)]
pub struct Collection {
    id: i64,
    name: String,
    resources: i64,
    assets: i64,
}

pub fn list(conn: &Connection) -> Res<Vec<Collection>> {
    let mut s = conn
        .prepare(
            "SELECT c.id, c.name,
                    (SELECT count(*) FROM collection_resources x WHERE x.collection_id = c.id),
                    (SELECT count(*) FROM collection_assets y JOIN media_assets a ON a.id = y.asset_id
                      WHERE y.collection_id = c.id AND a.available = 1)
             FROM collections c ORDER BY c.name COLLATE NOCASE",
        )
        .map_err(e)?;
    let v = s
        .query_map([], |r| Ok(Collection { id: r.get(0)?, name: r.get(1)?, resources: r.get(2)?, assets: r.get(3)? }))
        .map_err(e)?
        .flatten()
        .collect();
    Ok(v)
}

#[tauri::command]
pub fn list_collections(state: State<AppState>) -> Res<Vec<Collection>> {
    list(&state.db.lock().unwrap())
}

fn clean_name(name: &str) -> Res<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("Tên collection trống".into());
    }
    Ok(n.chars().take(80).collect())
}

pub fn create(conn: &Connection, name: &str) -> Res<i64> {
    let n = clean_name(name)?;
    if let Some(id) = conn
        .query_row("SELECT id FROM collections WHERE name = ?1 COLLATE NOCASE", [&n], |r| r.get(0))
        .optional()
        .map_err(e)?
    {
        return Ok(id);
    }
    conn.execute("INSERT INTO collections (name, created_at) VALUES (?1, ?2)", params![n, db::now()]).map_err(e)?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn create_collection(state: State<AppState>, name: String) -> Res<i64> {
    create(&state.db.lock().unwrap(), &name)
}

#[tauri::command]
pub fn rename_collection(state: State<AppState>, id: i64, name: String) -> Res<()> {
    let n = clean_name(&name)?;
    state
        .db
        .lock()
        .unwrap()
        .execute("UPDATE collections SET name = ?1 WHERE id = ?2", params![n, id])
        .map_err(|err| if err.to_string().contains("UNIQUE") { "Đã có collection trùng tên".to_string() } else { e(err) })?;
    Ok(())
}

/// Xóa collection: chỉ bỏ nhóm, resource/file bên trong giữ nguyên.
#[tauri::command]
pub fn delete_collection(state: State<AppState>, id: i64) -> Res<()> {
    state.db.lock().unwrap().execute("DELETE FROM collections WHERE id = ?1", [id]).map_err(e)?;
    Ok(())
}

pub fn set_members(conn: &mut Connection, table: &str, collection_id: i64, ids: &[i64], on: bool) -> Res<()> {
    let col = match table {
        "collection_resources" => "resource_id",
        "collection_assets" => "asset_id",
        _ => return Err("bảng không hợp lệ".into()),
    };
    let tx = conn.transaction().map_err(e)?;
    for id in ids {
        if on {
            tx.execute(&format!("INSERT OR IGNORE INTO {table} (collection_id, {col}) VALUES (?1, ?2)"), params![collection_id, id]).map_err(e)?;
        } else {
            tx.execute(&format!("DELETE FROM {table} WHERE collection_id = ?1 AND {col} = ?2"), params![collection_id, id]).map_err(e)?;
        }
    }
    tx.commit().map_err(e)
}

#[tauri::command]
pub fn set_collection_resources(state: State<AppState>, collection_id: i64, ids: Vec<i64>, on: bool) -> Res<()> {
    set_members(&mut state.db.lock().unwrap(), "collection_resources", collection_id, &ids, on)
}

#[tauri::command]
pub fn set_collection_assets(state: State<AppState>, collection_id: i64, ids: Vec<i64>, on: bool) -> Res<()> {
    set_members(&mut state.db.lock().unwrap(), "collection_assets", collection_id, &ids, on)
}

/// Collection đang chứa resource (hoặc asset) này.
#[tauri::command]
pub fn collections_of(state: State<AppState>, resource_id: Option<i64>, asset_id: Option<i64>) -> Res<Vec<i64>> {
    let conn = state.db.lock().unwrap();
    let (sql, id) = match (resource_id, asset_id) {
        (Some(r), _) => ("SELECT collection_id FROM collection_resources WHERE resource_id = ?1", r),
        (_, Some(a)) => ("SELECT collection_id FROM collection_assets WHERE asset_id = ?1", a),
        _ => return Ok(vec![]),
    };
    let mut s = conn.prepare(sql).map_err(e)?;
    let v = s.query_map([id], |r| r.get(0)).map_err(e)?.flatten().collect();
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_members_and_delete_keeps_items() {
        let dir = std::env::temp_dir().join(format!("mrm_coll_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut conn = db::open(&dir.join("t.db")).unwrap();
        conn.execute("INSERT INTO resources (id, name, created_at, updated_at) VALUES (1, 'A', 0, 0), (2, 'B', 0, 0)", []).unwrap();
        let c = create(&conn, "  SFX vlog ").unwrap();
        assert_eq!(create(&conn, "sfx VLOG").unwrap(), c, "trùng tên (không phân biệt hoa thường) -> dùng lại");
        set_members(&mut conn, "collection_resources", c, &[1, 2, 2], true).unwrap();
        assert_eq!(list(&conn).unwrap()[0].resources, 2);
        set_members(&mut conn, "collection_resources", c, &[2], false).unwrap();
        assert_eq!(list(&conn).unwrap()[0].resources, 1);
        assert!(set_members(&mut conn, "resources; DROP TABLE x", c, &[1], true).is_err());
        conn.execute("DELETE FROM collections WHERE id = ?1", [c]).unwrap();
        let n: i64 = conn.query_row("SELECT count(*) FROM resources", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2, "xóa collection không xóa resource");
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
