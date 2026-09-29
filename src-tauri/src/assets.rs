//! Media Browser: truy vấn / gắn tag / yêu thích từng file media (bảng media_assets do media_index lập).
//! Chỉ đọc file thật; mọi thay đổi chỉ nằm trong database.
use crate::commands::AppState;
use crate::db;
use rusqlite::{params, params_from_iter, types::Value, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{AppHandle, Manager, State};

type Res<T> = Result<T, String>;
fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

/// Loại tag dùng chung với plugin DaVinci (HNH SFX Finder ghi tag kind = 'function').
const ASSET_TAG_KIND: &str = "function";

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct AssetQuery {
    pub media_type: Option<String>,
    pub text: Option<String>,
    pub library_id: Option<i64>,
    pub resource_id: Option<i64>,
    pub tag_id: Option<i64>,
    pub ext: Option<String>,
    pub ai_category: Option<String>,
    pub favorites: bool,
    pub recent: bool,
    /// tìm theo ý nghĩa (AI) ngoài khớp từ khóa
    pub semantic: bool,
    pub include_missing: bool,
    /// name | added | size | duration | recent | relevance
    pub sort: Option<String>,
    pub desc: bool,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Serialize)]
pub struct AssetItem {
    id: i64,
    media_type: String,
    filename: String,
    ext: String,
    rel_path: String,
    library_id: i64,
    size: i64,
    modified_ms: i64,
    added_at: i64,
    available: bool,
    duration: Option<f64>,
    width: Option<i64>,
    height: Option<i64>,
    seq_count: Option<i64>,
    favorite: bool,
    resource_id: Option<i64>,
    resource_name: Option<String>,
    ai_category: Option<String>,
}

#[derive(Serialize)]
pub struct AssetPage {
    total: i64,
    items: Vec<AssetItem>,
}

/// Điều kiện từ khóa: mỗi từ phải khớp đường dẫn, tên resource hoặc tag.
fn text_cond(q: &AssetQuery) -> Option<(String, Vec<Value>)> {
    let mut w: Vec<String> = Vec::new();
    let mut p: Vec<Value> = Vec::new();
    for tok in q.text.as_deref().unwrap_or("").split_whitespace().take(8) {
        let like = format!("%{}%", tok.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        w.push(
            "(a.rel_path LIKE ? ESCAPE '\\' OR r.name LIKE ? ESCAPE '\\'
              OR EXISTS (SELECT 1 FROM asset_tags x JOIN tags t ON t.id = x.tag_id WHERE x.asset_id = a.id AND t.name LIKE ? ESCAPE '\\')
              OR EXISTS (SELECT 1 FROM asset_captions k WHERE k.asset_id = a.id AND k.caption LIKE ? ESCAPE '\\'))"
                .into(),
        );
        for _ in 0..4 {
            p.push(Value::Text(like.clone()));
        }
    }
    (!w.is_empty()).then(|| (w.join(" AND "), p))
}

/// `sem`: kết quả tìm theo ý nghĩa (asset_id, độ giống) — file khớp từ khóa HOẶC khớp ý nghĩa đều được hiện.
fn build_filter(q: &AssetQuery, sem: &[(i64, f32)]) -> (String, Vec<Value>) {
    let mut w: Vec<String> = Vec::new();
    let mut p: Vec<Value> = Vec::new();
    if !q.include_missing {
        w.push("a.available = 1".into());
    }
    if let Some(t) = q.media_type.as_deref().filter(|t| matches!(*t, "audio" | "image" | "video")) {
        w.push("a.media_type = ?".into());
        p.push(Value::Text(t.into()));
    }
    if let Some(id) = q.library_id {
        w.push("a.library_id = ?".into());
        p.push(Value::Integer(id));
    }
    if let Some(id) = q.resource_id {
        w.push("a.resource_id = ?".into());
        p.push(Value::Integer(id));
    }
    if let Some(id) = q.tag_id {
        w.push("EXISTS (SELECT 1 FROM asset_tags x WHERE x.asset_id = a.id AND x.tag_id = ?)".into());
        p.push(Value::Integer(id));
    }
    if let Some(c) = q.ai_category.as_deref().filter(|c| !c.is_empty()) {
        w.push("a.ai_category = ?".into());
        p.push(Value::Text(c.into()));
    }
    if let Some(ext) = q.ext.as_deref().filter(|x| !x.is_empty()) {
        w.push("a.ext = ?".into());
        p.push(Value::Text(ext.to_lowercase()));
    }
    if q.favorites {
        w.push("f.asset_id IS NOT NULL".into());
    }
    if q.recent {
        w.push("u.last_used_at > 0".into());
    }
    if let Some((kw, kp)) = text_cond(q) {
        if sem.is_empty() {
            w.push(kw);
        } else {
            let ids = sem.iter().map(|(id, _)| id.to_string()).collect::<Vec<_>>().join(",");
            w.push(format!("(({kw}) OR a.id IN ({ids}))"));
        }
        p.extend(kp);
    }
    let sql = if w.is_empty() { String::new() } else { format!("WHERE {}", w.join(" AND ")) };
    (sql, p)
}

const FROM: &str = "FROM media_assets a
     LEFT JOIN resources r ON r.id = a.resource_id
     LEFT JOIN asset_favorites f ON f.asset_id = a.id
     LEFT JOIN asset_usage u ON u.asset_id = a.id";

const ITEM_COLS: &str = "a.id, a.media_type, a.filename, a.ext, a.rel_path, a.library_id, a.size, a.modified_ms, a.added_at, a.available,
                a.duration, a.width, a.height, a.seq_count, f.asset_id IS NOT NULL, a.resource_id, r.name, a.ai_category";

fn item_row(r: &rusqlite::Row) -> rusqlite::Result<AssetItem> {
    Ok(AssetItem {
        id: r.get(0)?,
        media_type: r.get(1)?,
        filename: r.get(2)?,
        ext: r.get(3)?,
        rel_path: r.get(4)?,
        library_id: r.get(5)?,
        size: r.get(6)?,
        modified_ms: r.get(7)?,
        added_at: r.get(8)?,
        available: r.get::<_, i64>(9)? != 0,
        duration: r.get(10)?,
        width: r.get(11)?,
        height: r.get(12)?,
        seq_count: r.get(13)?,
        favorite: r.get(14)?,
        resource_id: r.get(15)?,
        resource_name: r.get(16)?,
        ai_category: r.get(17)?,
    })
}

pub fn run_query(conn: &Connection, q: &AssetQuery, sem: &[(i64, f32)]) -> Res<AssetPage> {
    let (filter, mut params) = build_filter(q, sem);
    let total: i64 = conn
        .query_row(&format!("SELECT count(*) {FROM} {filter}"), params_from_iter(params.iter()), |r| r.get(0))
        .map_err(e)?;
    let dir = if q.desc { "DESC" } else { "ASC" };
    let by_name = format!("a.filename COLLATE NOCASE {dir}, a.rel_path COLLATE NOCASE");
    let order = match q.sort.as_deref().unwrap_or("name") {
        "added" => format!("a.added_at {dir}, a.id {dir}"),
        "size" => format!("a.size {dir}"),
        "duration" => format!("a.duration IS NULL, a.duration {dir}"),
        "recent" => format!("u.last_used_at IS NULL, u.last_used_at {dir}"),
        // Liên quan: khớp từ khóa được cộng điểm, rồi theo độ giống ý nghĩa
        "relevance" => match text_cond(q) {
            Some((kw, kp)) => {
                let cases = sem.iter().map(|(id, sc)| format!("WHEN {id} THEN {sc}")).collect::<Vec<_>>().join(" ");
                let semx = if sem.is_empty() { "0".to_string() } else { format!("CASE a.id {cases} ELSE 0 END") };
                params.extend(kp);
                format!("(coalesce(({kw}), 0) * 0.35 + {semx}) DESC, {by_name}")
            }
            None => by_name,
        },
        _ => by_name,
    };
    let limit = q.limit.clamp(1, 500);
    let sql = format!("SELECT {ITEM_COLS} {FROM} {filter} ORDER BY {order} LIMIT {limit} OFFSET {}", q.offset.max(0));
    let mut s = conn.prepare(&sql).map_err(e)?;
    let items = s
        .query_map(params_from_iter(params.iter()), item_row)
        .map_err(e)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(e)?;
    Ok(AssetPage { total, items })
}

/// Tìm theo ý nghĩa cho truy vấn (nếu bật và AI sẵn sàng).
fn semantic_for(app: &AppHandle, q: &AssetQuery) -> Vec<(i64, f32)> {
    match q.text.as_deref().filter(|t| q.semantic && !t.trim().is_empty()) {
        Some(t) => crate::asset_ai::semantic(app, t),
        None => vec![],
    }
}

#[tauri::command]
pub async fn query_assets(app: AppHandle, query: AssetQuery) -> Res<AssetPage> {
    tauri::async_runtime::spawn_blocking(move || {
        let sem = semantic_for(&app, &query);
        let state = app.state::<AppState>();
        let conn = state.db.lock().unwrap();
        run_query(&conn, &query, &sem)
    })
    .await
    .map_err(e)?
}

fn items_by_ids(conn: &Connection, ids: &[i64]) -> Res<Vec<AssetItem>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let list = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    let mut s = conn.prepare(&format!("SELECT {ITEM_COLS} {FROM} WHERE a.id IN ({list}) AND a.available = 1")).map_err(e)?;
    let mut found: std::collections::HashMap<i64, AssetItem> =
        s.query_map([], item_row).map_err(e)?.flatten().map(|i| (i.id, i)).collect();
    Ok(ids.iter().filter_map(|id| found.remove(id)).collect())
}

/// File giống nhất (AI) — cùng loại âm thanh / hình.
#[tauri::command]
pub fn similar_assets(state: State<AppState>, id: i64) -> Res<Vec<AssetItem>> {
    let ids: Vec<i64> = crate::asset_ai::similar(id, 12).into_iter().map(|x| x.0).collect();
    items_by_ids(&state.db.lock().unwrap(), &ids)
}

/// Gắn một tag cho MỌI file khớp bộ lọc hiện tại (vd: toàn bộ danh mục AI "Whoosh").
#[tauri::command]
pub async fn tag_assets_by_query(app: AppHandle, query: AssetQuery, name: String) -> Res<usize> {
    tauri::async_runtime::spawn_blocking(move || {
        let sem = semantic_for(&app, &query);
        let state = app.state::<AppState>();
        let ids: Vec<i64> = {
            let conn = state.db.lock().unwrap();
            let (filter, params) = build_filter(&query, &sem);
            let mut s = conn.prepare(&format!("SELECT a.id {FROM} {filter}")).map_err(e)?;
            let v = s.query_map(params_from_iter(params.iter()), |r| r.get(0)).map_err(e)?.flatten().collect();
            v
        };
        let n = ids.len();
        set_tag_on(&mut state.db.lock().unwrap(), &ids, &name, true)?;
        Ok(n)
    })
    .await
    .map_err(e)?
}

#[derive(Serialize, Default)]
pub struct AssetCounts {
    audio: i64,
    image: i64,
    video: i64,
    favorites: i64,
    recent: i64,
    pending_meta: i64,
}

#[tauri::command]
pub fn asset_counts(state: State<AppState>) -> Res<AssetCounts> {
    let conn = state.db.lock().unwrap();
    let mut c = AssetCounts::default();
    let mut s = conn.prepare("SELECT media_type, count(*), sum(meta_state = 0) FROM media_assets WHERE available = 1 GROUP BY media_type").map_err(e)?;
    let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))).map_err(e)?;
    for (t, n, pending) in rows.flatten() {
        c.pending_meta += pending;
        match t.as_str() {
            "audio" => c.audio = n,
            "image" => c.image = n,
            "video" => c.video = n,
            _ => {}
        }
    }
    c.favorites = conn.query_row("SELECT count(*) FROM asset_favorites", [], |r| r.get(0)).map_err(e)?;
    c.recent = conn.query_row("SELECT count(*) FROM asset_usage WHERE last_used_at > 0", [], |r| r.get(0)).map_err(e)?;
    Ok(c)
}

#[derive(Serialize, Default)]
pub struct ResourceMedia {
    audio: i64,
    image: i64,
    video: i64,
}

/// Số file media (đã lập chỉ mục) nằm trong một resource — cho nút "Xem trong Media Browser".
#[tauri::command]
pub fn resource_asset_counts(state: State<AppState>, resource_id: i64) -> Res<ResourceMedia> {
    let conn = state.db.lock().unwrap();
    let mut out = ResourceMedia::default();
    let mut s = conn
        .prepare("SELECT media_type, count(*) FROM media_assets WHERE resource_id = ?1 AND available = 1 GROUP BY media_type")
        .map_err(e)?;
    for (t, n) in s.query_map([resource_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))).map_err(e)?.flatten() {
        match t.as_str() {
            "audio" => out.audio = n,
            "image" => out.image = n,
            "video" => out.video = n,
            _ => {}
        }
    }
    Ok(out)
}

#[derive(Serialize)]
pub struct Facet {
    key: String,
    id: Option<i64>,
    count: i64,
}

#[derive(Serialize)]
pub struct AssetFacets {
    exts: Vec<Facet>,
    tags: Vec<Facet>,
    ai: Vec<Facet>,
}

/// Định dạng + tag có trong loại media đang xem (cho bộ lọc).
#[tauri::command]
pub fn asset_facets(state: State<AppState>, media_type: Option<String>) -> Res<AssetFacets> {
    let conn = state.db.lock().unwrap();
    let t = media_type.unwrap_or_default();
    let facet = |sql: &str| -> Res<Vec<Facet>> {
        let mut s = conn.prepare(sql).map_err(e)?;
        let v = s
            .query_map([&t], |r| Ok(Facet { key: r.get(0)?, id: r.get(1)?, count: r.get(2)? }))
            .map_err(e)?
            .flatten()
            .collect();
        Ok(v)
    };
    Ok(AssetFacets {
        exts: facet("SELECT ext, NULL, count(*) FROM media_assets WHERE available = 1 AND (?1 = '' OR media_type = ?1) GROUP BY ext ORDER BY 3 DESC")?,
        tags: facet(
            "SELECT t.name, t.id, count(*) FROM asset_tags x JOIN tags t ON t.id = x.tag_id JOIN media_assets a ON a.id = x.asset_id
             WHERE a.available = 1 AND (?1 = '' OR a.media_type = ?1) GROUP BY t.id ORDER BY 3 DESC, t.name COLLATE NOCASE",
        )?,
        ai: facet(
            "SELECT ai_category, NULL, count(*) FROM media_assets WHERE available = 1 AND ai_category IS NOT NULL
             AND (?1 = '' OR media_type = ?1) GROUP BY ai_category ORDER BY 3 DESC",
        )?,
    })
}

#[derive(Serialize)]
pub struct AssetTag {
    id: i64,
    name: String,
}

#[derive(Serialize)]
pub struct AssetDetail {
    id: i64,
    uid: String,
    media_type: String,
    filename: String,
    ext: String,
    rel_path: String,
    path: String,
    library_id: i64,
    library_name: String,
    size: i64,
    modified_ms: i64,
    added_at: i64,
    available: bool,
    meta_state: i64,
    duration: Option<f64>,
    width: Option<i64>,
    height: Option<i64>,
    fps: Option<f64>,
    codec: Option<String>,
    sample_rate: Option<i64>,
    channels: Option<i64>,
    seq_pattern: Option<String>,
    seq_start: Option<i64>,
    seq_end: Option<i64>,
    seq_count: Option<i64>,
    favorite: bool,
    use_count: i64,
    last_used_at: Option<i64>,
    resource_id: Option<i64>,
    resource_name: Option<String>,
    ai_category: Option<String>,
    ai_score: Option<f64>,
    ai_caption: Option<String>,
    tags: Vec<AssetTag>,
}

pub fn detail(conn: &Connection, id: i64) -> Res<AssetDetail> {
    let mut d = conn
        .query_row(
            "SELECT a.id, a.uid, a.media_type, a.filename, a.ext, a.rel_path, v.path, a.library_id, l.name, a.size, a.modified_ms,
                    a.added_at, a.available, a.meta_state, a.duration, a.width, a.height, a.fps, a.codec, a.sample_rate, a.channels,
                    a.seq_pattern, a.seq_start, a.seq_end, a.seq_count, f.asset_id IS NOT NULL, coalesce(u.use_count, 0), u.last_used_at,
                    a.resource_id, r.name, a.ai_category, a.ai_score,
                    (SELECT caption FROM asset_captions k WHERE k.asset_id = a.id)
             FROM media_assets a JOIN api_assets v ON v.id = a.id JOIN libraries l ON l.id = a.library_id
             LEFT JOIN resources r ON r.id = a.resource_id LEFT JOIN asset_favorites f ON f.asset_id = a.id
             LEFT JOIN asset_usage u ON u.asset_id = a.id WHERE a.id = ?1",
            [id],
            |r| {
                Ok(AssetDetail {
                    id: r.get(0)?,
                    uid: r.get(1)?,
                    media_type: r.get(2)?,
                    filename: r.get(3)?,
                    ext: r.get(4)?,
                    rel_path: r.get(5)?,
                    path: r.get(6)?,
                    library_id: r.get(7)?,
                    library_name: r.get(8)?,
                    size: r.get(9)?,
                    modified_ms: r.get(10)?,
                    added_at: r.get(11)?,
                    available: r.get::<_, i64>(12)? != 0,
                    meta_state: r.get(13)?,
                    duration: r.get(14)?,
                    width: r.get(15)?,
                    height: r.get(16)?,
                    fps: r.get(17)?,
                    codec: r.get(18)?,
                    sample_rate: r.get(19)?,
                    channels: r.get(20)?,
                    seq_pattern: r.get(21)?,
                    seq_start: r.get(22)?,
                    seq_end: r.get(23)?,
                    seq_count: r.get(24)?,
                    favorite: r.get(25)?,
                    use_count: r.get(26)?,
                    last_used_at: r.get(27)?,
                    resource_id: r.get(28)?,
                    resource_name: r.get(29)?,
                    ai_category: r.get(30)?,
                    ai_score: r.get(31)?,
                    ai_caption: r.get(32)?,
                    tags: vec![],
                })
            },
        )
        .map_err(e)?;
    let mut s = conn
        .prepare("SELECT t.id, t.name FROM asset_tags x JOIN tags t ON t.id = x.tag_id WHERE x.asset_id = ?1 ORDER BY t.name COLLATE NOCASE")
        .map_err(e)?;
    d.tags = s.query_map([id], |r| Ok(AssetTag { id: r.get(0)?, name: r.get(1)? })).map_err(e)?.flatten().collect();
    Ok(d)
}

/// Chi tiết asset; metadata chưa đọc thì đọc ngay (ngoài khóa DB).
#[tauri::command]
pub async fn get_asset(app: AppHandle, id: i64) -> Res<AssetDetail> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let pending = {
            let conn = state.db.lock().unwrap();
            conn.query_row("SELECT meta_state = 0 AND available = 1 FROM media_assets WHERE id = ?1", [id], |r| r.get::<_, bool>(0))
                .optional()
                .map_err(e)?
                .ok_or("Không tìm thấy file media")?
        };
        if pending {
            crate::media_index::probe_one(&state.db, id);
        }
        let conn = state.db.lock().unwrap();
        detail(&conn, id)
    })
    .await
    .map_err(e)?
}

#[tauri::command]
pub fn set_asset_favorite(state: State<AppState>, ids: Vec<i64>, on: bool) -> Res<()> {
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    for id in ids {
        if on {
            tx.execute("INSERT OR IGNORE INTO asset_favorites (asset_id, created_at) VALUES (?1, ?2)", params![id, db::now()]).map_err(e)?;
        } else {
            tx.execute("DELETE FROM asset_favorites WHERE asset_id = ?1", [id]).map_err(e)?;
        }
    }
    tx.commit().map_err(e)
}

fn tag_id(conn: &Connection, name: &str) -> Res<i64> {
    if let Some(id) = conn
        .query_row("SELECT id FROM tags WHERE kind = ?1 AND name = ?2 COLLATE NOCASE", params![ASSET_TAG_KIND, name], |r| r.get(0))
        .optional()
        .map_err(e)?
    {
        return Ok(id);
    }
    conn.execute("INSERT INTO tags (kind, name) VALUES (?1, ?2)", params![ASSET_TAG_KIND, name]).map_err(e)?;
    Ok(conn.last_insert_rowid())
}

/// Thêm (on = true) hoặc gỡ một tag trên nhiều asset.
#[tauri::command]
pub fn set_asset_tag(state: State<AppState>, ids: Vec<i64>, name: String, on: bool) -> Res<()> {
    set_tag_on(&mut state.db.lock().unwrap(), &ids, &name, on)
}

fn set_tag_on(conn: &mut Connection, ids: &[i64], name: &str, on: bool) -> Res<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Tên tag trống".into());
    }
    let tx = conn.transaction().map_err(e)?;
    let tid = tag_id(&tx, name)?;
    crate::asset_ai::invalidate(&tx, ids);
    for &id in ids {
        if on {
            tx.execute("INSERT OR IGNORE INTO asset_tags (asset_id, tag_id, applied_at) VALUES (?1, ?2, ?3)", params![id, tid, db::now()])
                .map_err(e)?;
        } else {
            tx.execute("DELETE FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2", params![id, tid]).map_err(e)?;
        }
    }
    tx.commit().map_err(e)
}

/// Tên các tag đã dùng cho asset (gợi ý khi gõ).
#[tauri::command]
pub fn list_asset_tag_names(state: State<AppState>) -> Res<Vec<String>> {
    let conn = state.db.lock().unwrap();
    let mut s = conn
        .prepare("SELECT name FROM tags WHERE kind = ?1 ORDER BY name COLLATE NOCASE")
        .map_err(e)?;
    let v = s.query_map([ASSET_TAG_KIND], |r| r.get(0)).map_err(e)?.flatten().collect();
    Ok(v)
}

fn paths_of(conn: &Connection, ids: &[i64]) -> Res<Vec<String>> {
    let mut s = conn.prepare("SELECT path FROM api_assets WHERE id = ?1 AND available = 1").map_err(e)?;
    let mut out = Vec::new();
    for id in ids {
        if let Some(p) = s.query_row([id], |r| r.get::<_, String>(0)).optional().map_err(e)? {
            out.push(p);
        }
    }
    Ok(out)
}

/// Đường dẫn file thật (để kéo thả / sao chép sang app dựng khác). Ghi nhận lượt dùng.
#[tauri::command]
pub fn use_assets(state: State<AppState>, ids: Vec<i64>) -> Res<Vec<String>> {
    let conn = state.db.lock().unwrap();
    let paths = paths_of(&conn, &ids)?;
    for id in &ids {
        let _ = conn.execute(
            "INSERT INTO asset_usage (asset_id, last_used_at, use_count) VALUES (?1, ?2, 1)
             ON CONFLICT(asset_id) DO UPDATE SET last_used_at = excluded.last_used_at, use_count = use_count + 1",
            params![id, db::now()],
        );
    }
    Ok(paths)
}

#[tauri::command]
pub fn reveal_asset(state: State<AppState>, id: i64) -> Res<()> {
    let path = paths_of(&state.db.lock().unwrap(), &[id])?.pop().ok_or("File hiện không truy cập được")?;
    if !Path::new(&path).exists() {
        return Err("File hiện không truy cập được".into());
    }
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(e)
}

/// Ảnh nhỏ hiển thị dưới con trỏ khi kéo file ra app khác.
#[tauri::command]
pub fn drag_icon(state: State<AppState>) -> Res<String> {
    let p = crate::media::cache_dir(&state).join("drag-icon.png");
    if !p.exists() {
        std::fs::create_dir_all(p.parent().unwrap()).map_err(e)?;
        std::fs::write(&p, include_bytes!("../icons/64x64.png")).map_err(e)?;
    }
    Ok(p.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_filters_sort_and_tags() {
        let dir = std::env::temp_dir().join(format!("mrm_assets_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let conn = db::open(&dir.join("t.db")).unwrap();
        conn.execute("INSERT INTO libraries (id, name, path, created_at) VALUES (1, 'L', 'D:\\Lib', 0)", []).unwrap();
        conn.execute("INSERT INTO resources (id, name, created_at, updated_at) VALUES (7, 'Whoosh Pack', 0, 0)", []).unwrap();
        let add = |id: i64, t: &str, rel: &str, size: i64, res: Option<i64>| {
            let name = rel.rsplit('\\').next().unwrap();
            let ext = name.rsplit('.').next().unwrap();
            conn.execute(
                "INSERT INTO media_assets (id, uid, library_id, resource_id, media_type, rel_path, filename, ext, size, modified_ms, added_at, last_seen)
                 VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?1, 0)",
                params![id, format!("u{id}"), res, t, rel, name, ext, size],
            )
            .unwrap();
        };
        add(1, "audio", "SFX\\whoosh_fast.wav", 300, Some(7));
        add(2, "audio", "SFX\\impact 100%.mp3", 100, None);
        add(3, "video", "Clips\\sky.mov", 900, None);
        let q = |f: AssetQuery| run_query(&conn, &AssetQuery { limit: 100, ..f }, &[]).unwrap();

        let audio = q(AssetQuery { media_type: Some("audio".into()), ..Default::default() });
        assert_eq!(audio.total, 2);
        assert_eq!(audio.items[0].filename, "impact 100%.mp3", "sắp theo tên");
        assert_eq!(q(AssetQuery { text: Some("100%".into()), ..Default::default() }).total, 1, "ký tự % được escape");
        assert_eq!(q(AssetQuery { text: Some("pack".into()), ..Default::default() }).items[0].id, 1, "tìm theo tên resource");
        assert_eq!(q(AssetQuery { sort: Some("size".into()), desc: true, ..Default::default() }).items[0].id, 3);

        // tag + yêu thích
        let tid = tag_id(&conn, "Gun").unwrap();
        conn.execute("INSERT INTO asset_tags (asset_id, tag_id) VALUES (2, ?1)", [tid]).unwrap();
        assert_eq!(tag_id(&conn, "gun").unwrap(), tid, "tag không phân biệt hoa thường");
        assert_eq!(q(AssetQuery { text: Some("gun".into()), ..Default::default() }).items[0].id, 2, "tìm theo tag");
        assert_eq!(q(AssetQuery { tag_id: Some(tid), ..Default::default() }).total, 1);
        conn.execute("INSERT INTO asset_favorites (asset_id, created_at) VALUES (3, 0)", []).unwrap();
        let fav = q(AssetQuery { favorites: true, ..Default::default() });
        assert!(fav.total == 1 && fav.items[0].favorite);

        // file mất -> ẩn mặc định
        conn.execute("UPDATE media_assets SET available = 0 WHERE id = 3", []).unwrap();
        assert_eq!(q(AssetQuery::default()).total, 2);
        assert_eq!(q(AssetQuery { include_missing: true, ..Default::default() }).total, 3);

        // tìm theo ý nghĩa: file không khớp từ khóa nhưng AI thấy giống vẫn hiện, xếp theo độ liên quan
        let sem = [(3i64, 0.7f32), (2, 0.55), (1, 0.5)];
        let rel = run_query(
            &conn,
            &AssetQuery { text: Some("impact".into()), sort: Some("relevance".into()), include_missing: true, limit: 10, ..Default::default() },
            &sem,
        )
        .unwrap();
        assert_eq!(rel.items.iter().map(|i| i.id).collect::<Vec<_>>(), vec![2, 3, 1], "khớp từ khóa trước, rồi theo AI");
        conn.execute("UPDATE media_assets SET ai_category = 'Whoosh' WHERE id = 1", []).unwrap();
        assert_eq!(q(AssetQuery { ai_category: Some("Whoosh".into()), ..Default::default() }).items[0].id, 1);

        let d = detail(&conn, 2).unwrap();
        assert_eq!(d.path, "D:\\Lib\\SFX\\impact 100%.mp3");
        assert_eq!(d.tags.len(), 1);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
