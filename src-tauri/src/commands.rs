//! Tauri commands — API giữa UI và backend.

use crate::db;
use crate::normalize;
use crate::scanner::{self, ScanProgress, ScanResult};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub data_dir: PathBuf,
    pub scanning: Mutex<HashSet<i64>>,
    /// Thư mục chứa dữ liệu lớn (cache preview, AI runtime & models) — có thể đặt ở ổ khác.
    pub storage: Mutex<PathBuf>,
    /// Giới hạn số tác vụ FFmpeg/decode chạy song song.
    pub media_sem: std::sync::Arc<tokio::sync::Semaphore>,
    /// Mô hình gợi ý học từ tag người dùng đã gắn (cache, tự làm mới sau 30 giây).
    pub learn: Mutex<Option<std::sync::Arc<crate::organize::LearnModel>>>,
}

impl AppState {
    pub fn storage_dir(&self) -> PathBuf {
        self.storage.lock().unwrap().clone()
    }
}

/// Cho phép UI đọc file trong các library và thư mục cache qua asset protocol (chỉ đọc).
pub fn allow_asset_dirs(app: &AppHandle) {
    let state = app.state::<AppState>();
    let scope = app.asset_protocol_scope();
    let _ = scope.allow_directory(state.storage_dir().join("cache"), true);
    let _ = scope.allow_directory(crate::noteimg::dir(&state), true);
    let libs = {
        let conn = state.db.lock().unwrap();
        collect(&conn, "SELECT path FROM libraries", [], |r| r.get::<_, String>(0)).unwrap_or_default()
    };
    for l in libs {
        let _ = scope.allow_directory(&l, true);
    }
}

#[derive(Serialize)]
pub struct StorageInfo {
    dir: String,
    free_bytes: Option<u64>,
    default_dir: String,
}

#[cfg(windows)]
pub fn free_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let mut p = path.to_path_buf();
    while !p.exists() {
        p = p.parent()?.to_path_buf();
    }
    let wide: Vec<u16> = p.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut free: u64 = 0;
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    if ok != 0 { Some(free) } else { None }
}
#[cfg(not(windows))]
pub fn free_space(_path: &Path) -> Option<u64> {
    None
}

#[tauri::command]
pub fn get_storage_info(state: State<AppState>) -> StorageInfo {
    let dir = state.storage_dir();
    StorageInfo { free_bytes: free_space(&dir), dir: dir.to_string_lossy().to_string(), default_dir: state.data_dir.to_string_lossy().to_string() }
}

/// Đổi thư mục lưu dữ liệu lớn. Không di chuyển dữ liệu cũ (cache sẽ tạo lại; AI runtime/model cần tải lại
/// hoặc người dùng tự copy thư mục `ai` sang).
#[tauri::command]
pub fn set_storage_dir(app: AppHandle, state: State<AppState>, path: String) -> Res<()> {
    let p = PathBuf::from(&path);
    // "G:foo" là đường dẫn tương đối theo ổ -> phải từ chối
    if !p.is_absolute() {
        return Err("Cần đường dẫn tuyệt đối, ví dụ G:\\MRMData".into());
    }
    std::fs::create_dir_all(&p).map_err(e)?;
    db::set_setting(&state.db.lock().unwrap(), "storage_dir", &path).map_err(e)?;
    *state.storage.lock().unwrap() = p;
    allow_asset_dirs(&app);
    Ok(())
}

type Res<T> = Result<T, String>;

fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

fn collect<T, F>(conn: &Connection, sql: &str, p: impl rusqlite::Params, f: F) -> Res<Vec<T>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut stmt = conn.prepare(sql).map_err(e)?;
    let rows = stmt.query_map(p, f).map_err(e)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(e)
}

// ================================================================ Libraries

#[derive(Serialize)]
pub struct LibraryInfo {
    id: i64,
    name: String,
    path: String,
    scan_depth: i64,
    last_scan_at: Option<i64>,
    online: bool,
    source_count: i64,
    size: i64,
}

fn library_info(conn: &Connection, id: i64) -> Res<LibraryInfo> {
    let online = scanner::check_library(conn, id).map_err(e)?;
    conn.query_row(
        "SELECT l.id, l.name, l.path, l.scan_depth, l.last_scan_at,
                (SELECT count(*) FROM resource_sources WHERE library_id = l.id),
                (SELECT coalesce(sum(size),0) FROM resource_sources WHERE library_id = l.id)
         FROM libraries l WHERE l.id = ?1",
        [id],
        |r| {
            Ok(LibraryInfo {
                id: r.get(0)?,
                name: r.get(1)?,
                path: r.get(2)?,
                scan_depth: r.get(3)?,
                last_scan_at: r.get(4)?,
                online,
                source_count: r.get(5)?,
                size: r.get(6)?,
            })
        },
    )
    .map_err(e)
}

#[tauri::command]
pub fn list_libraries(app: AppHandle, state: State<AppState>) -> Res<Vec<LibraryInfo>> {
    let list: Vec<LibraryInfo> = {
        let conn = state.db.lock().unwrap();
        let ids = collect(&conn, "SELECT id FROM libraries ORDER BY name COLLATE NOCASE", [], |r| r.get::<_, i64>(0))?;
        ids.into_iter().map(|id| library_info(&conn, id)).collect::<Res<_>>()?
    };
    // library có thể vừa được tìm lại ở ký tự ổ khác
    let scope = app.asset_protocol_scope();
    for l in &list {
        let _ = scope.allow_directory(&l.path, true);
    }
    Ok(list)
}

#[tauri::command]
pub fn add_library(app: AppHandle, state: State<AppState>, path: String, name: Option<String>, scan_depth: Option<i64>) -> Res<LibraryInfo> {
    let p = Path::new(&path);
    if !p.is_dir() {
        return Err("Thư mục không tồn tại".into());
    }
    let path = p.to_string_lossy().trim_end_matches(['\\', '/']).to_string();
    let path = if path.ends_with(':') { format!("{path}\\") } else { path };
    let name = name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.clone()));
    let conn = state.db.lock().unwrap();
    let dup: Option<i64> = conn.query_row("SELECT id FROM libraries WHERE path = ?1 COLLATE NOCASE", [&path], |r| r.get(0)).optional().map_err(e)?;
    if dup.is_some() {
        return Err("Library này đã được thêm".into());
    }
    conn.execute(
        "INSERT INTO libraries (name, path, volume_serial, scan_depth, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![name, path, scanner::volume_serial(p).map(|s| s as i64), scan_depth.unwrap_or(0).clamp(0, 5), db::now()],
    )
    .map_err(e)?;
    let _ = app.asset_protocol_scope().allow_directory(&path, true);
    let info = library_info(&conn, conn.last_insert_rowid());
    drop(conn);
    crate::watcher::refresh(&app);
    info
}

#[tauri::command]
pub fn update_library(state: State<AppState>, id: i64, name: String, scan_depth: i64) -> Res<()> {
    let conn = state.db.lock().unwrap();
    conn.execute("UPDATE libraries SET name = ?1, scan_depth = ?2 WHERE id = ?3", params![name, scan_depth.clamp(0, 5), id]).map_err(e)?;
    Ok(())
}

/// Chỉ xóa metadata trong MRM — file thật trên ổ đĩa giữ nguyên.
#[tauri::command]
pub fn remove_library(app: AppHandle, state: State<AppState>, id: i64) -> Res<()> {
    let mut conn = state.db.lock().unwrap();
    // lưới an toàn: backup trước khi xóa metadata của cả library
    backup_to(&conn, &backup_dir(&state), "before-remove")?;
    let tx = conn.transaction().map_err(e)?;
    let rids = collect(&tx, "SELECT DISTINCT resource_id FROM resource_sources WHERE library_id = ?1", [id], |r| r.get::<_, i64>(0))?;
    tx.execute("DELETE FROM resource_sources WHERE library_id = ?1", [id]).map_err(e)?;
    tx.execute("DELETE FROM libraries WHERE id = ?1", [id]).map_err(e)?;
    for rid in rids {
        let left: i64 = tx.query_row("SELECT count(*) FROM resource_sources WHERE resource_id = ?1", [rid], |r| r.get(0)).map_err(e)?;
        if left == 0 {
            tx.execute("DELETE FROM resources WHERE id = ?1", [rid]).map_err(e)?;
            tx.execute("DELETE FROM resource_fts WHERE rowid = ?1", [rid]).map_err(e)?;
        } else {
            db::reindex(&tx, rid).map_err(e)?;
        }
    }
    tx.commit().map_err(e)?;
    drop(conn);
    crate::watcher::refresh(&app);
    Ok(())
}

fn run_scan(app: &AppHandle, id: i64, full: bool) -> Res<ScanResult> {
    let state = app.state::<AppState>();
    if !state.scanning.lock().unwrap().insert(id) {
        return Err("Library này đang được quét".into());
    }
    let emitter = app.clone();
    let emit = move |p: ScanProgress| {
        let _ = emitter.emit("scan-progress", p);
    };
    let result = scanner::scan_library(&state.db, id, full, None, &emit);
    state.scanning.lock().unwrap().remove(&id);
    *state.learn.lock().unwrap() = None;
    if result.is_ok() {
        // Media Browser: lập chỉ mục asset cấp file (lô nhỏ, không chặn plugin)
        if let Err(err) = crate::media_index::index_library(&state.db, id, Some(&state.storage_dir())) {
            eprintln!("media index {id}: {err}");
        }
    }
    result
}

#[tauri::command]
pub async fn scan_library(app: AppHandle, id: i64, full: bool) -> Res<ScanResult> {
    tauri::async_runtime::spawn_blocking(move || run_scan(&app, id, full)).await.map_err(e)?
}

#[tauri::command]
pub async fn scan_all(app: AppHandle) -> Res<Vec<ScanResult>> {
    tauri::async_runtime::spawn_blocking(move || {
        let ids = {
            let state = app.state::<AppState>();
            let conn = state.db.lock().unwrap();
            collect(&conn, "SELECT id FROM libraries", [], |r| r.get::<_, i64>(0))?
        };
        ids.into_iter().map(|id| run_scan(&app, id, false)).collect()
    })
    .await
    .map_err(e)?
}

// ================================================================ Resources

#[derive(Deserialize)]
pub struct ResourceQuery {
    /// all | unclassified | favorites | missing | tag | library
    view: String,
    tag_id: Option<i64>,
    library_id: Option<i64>,
    search: String,
    /// name | size | updated | created | modified
    sort: String,
    desc: bool,
    /// Dùng semantic search (AI) khi tìm kiếm — mặc định bật nếu AI sẵn sàng.
    semantic: Option<bool>,
}

#[derive(Serialize)]
pub struct ResourceSummary {
    id: i64,
    name: String,
    version: Option<String>,
    favorite: bool,
    size: i64,
    kinds: Vec<String>,
    source_count: i64,
    unavailable_count: i64,
    tag_ids: Vec<i64>,
    /// tag gắn tự động, chưa xác nhận
    auto_tag_ids: Vec<i64>,
    has_notes: bool,
    created_at: i64,
    updated_at: i64,
    modified_at: Option<i64>,
}

/// Tách query: `app:resolve type:"stock video" tag:tracking status:installed is:fav is:missing` + từ khóa tự do.
fn parse_search(input: &str) -> (Vec<(String, String)>, Vec<String>) {
    let mut filters = Vec::new();
    let mut words = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let mut tok = String::new();
        let mut in_quote = false;
        while i < chars.len() && (in_quote || !chars[i].is_whitespace()) {
            if chars[i] == '"' {
                in_quote = !in_quote;
            } else {
                tok.push(chars[i]);
            }
            i += 1;
        }
        if let Some((k, v)) = tok.split_once(':') {
            let k = k.to_lowercase();
            if matches!(k.as_str(), "app" | "type" | "tag" | "status" | "is" | "lib") && !v.is_empty() {
                filters.push((k, v.to_string()));
                continue;
            }
        }
        if !tok.is_empty() {
            words.push(tok);
        }
    }
    (filters, words)
}

fn fts_query(words: &[String]) -> Option<String> {
    let parts: Vec<String> = words
        .iter()
        .map(|w| w.replace('"', ""))
        .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
        .map(|w| format!("\"{w}\"*"))
        .collect();
    if parts.is_empty() { None } else { Some(parts.join(" ")) }
}

#[tauri::command]
pub async fn query_resources(app: AppHandle, q: ResourceQuery) -> Res<Vec<ResourceSummary>> {
    tauri::async_runtime::spawn_blocking(move || query_resources_blocking(&app, q)).await.map_err(e)?
}

/// Trộn thứ hạng FTS và semantic bằng Reciprocal Rank Fusion.
fn rrf(fts: &[i64], sem: &[(i64, f32)]) -> Vec<i64> {
    let mut score: HashMap<i64, f64> = HashMap::new();
    for (i, id) in fts.iter().enumerate() {
        *score.entry(*id).or_default() += 1.0 / (60.0 + i as f64);
    }
    for (i, (id, _)) in sem.iter().enumerate() {
        *score.entry(*id).or_default() += 1.0 / (60.0 + i as f64);
    }
    let mut v: Vec<(i64, f64)> = score.into_iter().collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    v.into_iter().map(|x| x.0).collect()
}

fn query_resources_blocking(app: &AppHandle, q: ResourceQuery) -> Res<Vec<ResourceSummary>> {
    let state = app.state::<AppState>();
    // Semantic search chạy trước, không giữ lock DB (gọi AI runtime)
    let (_, words_pre) = parse_search(&q.search);
    let semantic = if q.semantic.unwrap_or(true) && !words_pre.is_empty() { crate::ai::semantic_hits(app, &words_pre.join(" ")) } else { vec![] };
    let conn = state.db.lock().unwrap();
    let mut conds: Vec<String> = Vec::new();
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    use rusqlite::types::Value as V;

    match q.view.as_str() {
        "unclassified" => conds.push(
            "NOT EXISTS (SELECT 1 FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id
                         WHERE rt.resource_id = r.id AND t.kind IN ('app','type'))".into(),
        ),
        "favorites" => conds.push("r.favorite = 1".into()),
        "review" => conds.push(crate::autotag::NEEDS_REVIEW_SQL.into()),
        "missing" => conds.push("EXISTS (SELECT 1 FROM resource_sources s WHERE s.resource_id = r.id AND s.available = 0)".into()),
        "tag" => {
            conds.push("EXISTS (SELECT 1 FROM resource_tags rt WHERE rt.resource_id = r.id AND rt.tag_id = ?)".into());
            args.push(V::Integer(q.tag_id.unwrap_or(-1)));
        }
        "library" => {
            conds.push("EXISTS (SELECT 1 FROM resource_sources s WHERE s.resource_id = r.id AND s.library_id = ?)".into());
            args.push(V::Integer(q.library_id.unwrap_or(-1)));
        }
        _ => {}
    }

    let (filters, words) = parse_search(&q.search);
    for (k, v) in filters {
        match k.as_str() {
            "is" => match v.to_lowercase().as_str() {
                "fav" | "favorite" => conds.push("r.favorite = 1".into()),
                "missing" | "offline" => conds.push("EXISTS (SELECT 1 FROM resource_sources s WHERE s.resource_id = r.id AND s.available = 0)".into()),
                "unclassified" => conds.push(
                    "NOT EXISTS (SELECT 1 FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id WHERE rt.resource_id = r.id AND t.kind IN ('app','type'))".into(),
                ),
                "matched" => conds.push("(SELECT count(*) FROM resource_sources s WHERE s.resource_id = r.id) > 1".into()),
                // đã được AI điền tag/mô tả — để rà soát lại
                "ai" => conds.push("r.ai_applied_at IS NOT NULL".into()),
                // có tag tự động chưa rà soát
                "auto" | "review" => conds.push(crate::autotag::NEEDS_REVIEW_SQL.into()),
                // có kết quả AI đang chờ áp dụng
                "aipending" | "ai-pending" => conds.push("EXISTS (SELECT 1 FROM ai_suggestions a WHERE a.resource_id = r.id)".into()),
                _ => {}
            },
            "lib" => {
                conds.push("EXISTS (SELECT 1 FROM resource_sources s JOIN libraries l ON l.id = s.library_id WHERE s.resource_id = r.id AND l.name LIKE ?)".into());
                args.push(V::Text(format!("%{v}%")));
            }
            _ => {
                let kind = if k == "tag" { None } else { Some(k.clone()) };
                let sql = match kind {
                    Some(_) => "EXISTS (SELECT 1 FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id WHERE rt.resource_id = r.id AND t.kind = ? AND t.name LIKE ?)",
                    None => "EXISTS (SELECT 1 FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id WHERE rt.resource_id = r.id AND t.kind IN ('function','personal') AND t.name LIKE ?)",
                };
                conds.push(sql.into());
                if let Some(kind) = kind {
                    args.push(V::Text(kind));
                }
                args.push(V::Text(format!("%{v}%")));
            }
        }
    }
    let fts = fts_query(&words);
    let mut ranked: Option<Vec<i64>> = None;
    if let Some(m) = &fts {
        let fts_ids = collect(&conn, "SELECT rowid FROM resource_fts WHERE resource_fts MATCH ?1 ORDER BY rank", [m], |r| r.get::<_, i64>(0))?;
        let merged = rrf(&fts_ids, &semantic);
        conds.push("r.id IN (SELECT value FROM json_each(?))".into());
        args.push(V::Text(serde_json::to_string(&merged).map_err(e)?));
        ranked = Some(merged);
    } else if !words.is_empty() {
        conds.push("0".into());
    }

    let order = match q.sort.as_str() {
        "relevance" => "r.name COLLATE NOCASE",
        "size" => "size",
        "updated" => "r.updated_at",
        "created" => "r.created_at",
        "modified" => "modified",
        _ => "r.name COLLATE NOCASE",
    };
    let dir = if q.desc { "DESC" } else { "ASC" };
    let where_sql = if conds.is_empty() { String::new() } else { format!("WHERE {}", conds.join(" AND ")) };
    let sql = format!(
        "SELECT r.id, r.name, r.version, r.favorite, r.notes <> '', r.created_at, r.updated_at,
                coalesce((SELECT sum(size) FROM resource_sources s WHERE s.resource_id = r.id), 0) AS size,
                (SELECT group_concat(DISTINCT kind) FROM resource_sources s WHERE s.resource_id = r.id),
                (SELECT count(*) FROM resource_sources s WHERE s.resource_id = r.id),
                (SELECT count(*) FROM resource_sources s WHERE s.resource_id = r.id AND s.available = 0),
                (SELECT max(modified_at) FROM resource_sources s WHERE s.resource_id = r.id) AS modified
         FROM resources r {where_sql} ORDER BY {order} {dir}, r.id"
    );
    let mut list = collect(&conn, &sql, params_from_iter(args), |r| {
        let kinds: Option<String> = r.get(8)?;
        Ok(ResourceSummary {
            id: r.get(0)?,
            name: r.get(1)?,
            version: r.get(2)?,
            favorite: r.get::<_, i64>(3)? != 0,
            has_notes: r.get(4)?,
            created_at: r.get(5)?,
            updated_at: r.get(6)?,
            size: r.get(7)?,
            kinds: kinds.map(|k| k.split(',').map(String::from).collect()).unwrap_or_default(),
            source_count: r.get(9)?,
            unavailable_count: r.get(10)?,
            modified_at: r.get(11)?,
            tag_ids: Vec::new(),
            auto_tag_ids: Vec::new(),
        })
    })?;

    let mut tag_map: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut auto_map: HashMap<i64, Vec<i64>> = HashMap::new();
    for (rid, tid, auto) in collect(
        &conn,
        "SELECT resource_id, tag_id, source IS NOT NULL AND source <> 'scan' FROM resource_tags",
        [],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, bool>(2)?)),
    )? {
        tag_map.entry(rid).or_default().push(tid);
        if auto {
            auto_map.entry(rid).or_default().push(tid);
        }
    }
    for item in &mut list {
        item.tag_ids = tag_map.remove(&item.id).unwrap_or_default();
        item.auto_tag_ids = auto_map.remove(&item.id).unwrap_or_default();
    }
    // Library offline -> coi như nguồn unavailable
    let offline: Vec<i64> = {
        let libs = collect(&conn, "SELECT id, path FROM libraries", [], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        libs.into_iter().filter(|(_, p)| !Path::new(p).is_dir()).map(|(id, _)| id).collect()
    };
    if !offline.is_empty() {
        let ph = offline.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let counts = collect(
            &conn,
            &format!("SELECT resource_id, count(*) FROM resource_sources WHERE library_id IN ({ph}) AND available = 1 GROUP BY resource_id"),
            params_from_iter(offline.iter()),
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        let m: HashMap<i64, i64> = counts.into_iter().collect();
        for item in &mut list {
            if let Some(c) = m.get(&item.id) {
                item.unavailable_count += c;
            }
        }
    }
    if let (Some(order), "relevance") = (&ranked, q.sort.as_str()) {
        let pos: HashMap<i64, usize> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        list.sort_by_key(|r| pos.get(&r.id).copied().unwrap_or(usize::MAX));
    }
    Ok(list)
}

#[derive(Serialize)]
pub struct SourceInfo {
    id: i64,
    library_id: i64,
    library_name: String,
    abs_path: String,
    rel_path: String,
    name: String,
    kind: String,
    size: i64,
    file_count: i64,
    modified_at: Option<i64>,
    extensions: Vec<(String, u64)>,
    available: bool,
    match_state: String,
    match_score: Option<f64>,
    /// library đang dùng chế độ quét Tự động (cho phép tách/gộp thư mục)
    library_auto: bool,
    /// ghi đè đang áp dụng cho chính đường dẫn này
    override_mode: Option<String>,
}

#[derive(Serialize)]
pub struct ScanOverride {
    rel_path: String,
    mode: String,
}

#[tauri::command]
pub fn list_scan_overrides(state: State<AppState>, library_id: i64) -> Res<Vec<ScanOverride>> {
    let conn = state.db.lock().unwrap();
    collect(&conn, "SELECT rel_path, mode FROM scan_overrides WHERE library_id = ?1 ORDER BY rel_path", [library_id], |r| {
        Ok(ScanOverride { rel_path: r.get(0)?, mode: r.get(1)? })
    })
}

/// mode: "pack" (gộp thành một resource) | "container" (tách thành resource con) | None (bỏ ghi đè)
#[tauri::command]
pub fn set_scan_override(state: State<AppState>, library_id: i64, rel_path: String, mode: Option<String>) -> Res<()> {
    let conn = state.db.lock().unwrap();
    match mode.as_deref() {
        Some(m @ ("pack" | "container")) => {
            conn.execute(
                "INSERT INTO scan_overrides (library_id, rel_path, mode) VALUES (?1, ?2, ?3)
                 ON CONFLICT(library_id, rel_path) DO UPDATE SET mode = excluded.mode",
                params![library_id, rel_path, m],
            )
            .map_err(e)?;
            // ghi đè bên trong thư mục được gộp không còn ý nghĩa
            if m == "pack" {
                let sep = std::path::MAIN_SEPARATOR;
                conn.execute(
                    "DELETE FROM scan_overrides WHERE library_id = ?1 AND substr(rel_path, 1, length(?2) + 1) = ?2 || ?3",
                    params![library_id, rel_path, sep.to_string()],
                )
                .map_err(e)?;
            }
        }
        _ => {
            conn.execute("DELETE FROM scan_overrides WHERE library_id = ?1 AND rel_path = ?2", params![library_id, rel_path]).map_err(e)?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub struct MatchSuggestionInfo {
    id: i64,
    score: f64,
    a: SourceBrief,
    b: SourceBrief,
}

#[derive(Serialize)]
pub struct SourceBrief {
    source_id: i64,
    resource_id: i64,
    resource_name: String,
    name: String,
    kind: String,
    rel_path: String,
    library_name: String,
    size: i64,
}

#[derive(Serialize)]
pub struct ResourceDetail {
    id: i64,
    name: String,
    version: Option<String>,
    notes: String,
    favorite: bool,
    created_at: i64,
    updated_at: i64,
    tag_ids: Vec<i64>,
    suggested: Vec<SuggestedTag>,
    sources: Vec<SourceInfo>,
    matches: Vec<MatchSuggestionInfo>,
    versions: Vec<crate::organize::VersionItem>,
    relations: Vec<crate::organize::Relation>,
    cover_user: bool,
    ai: Option<serde_json::Value>,
    note_images: Vec<crate::noteimg::NoteImage>,
    ai_applied_at: Option<i64>,
    auto_tags: Vec<AutoTag>,
}

#[derive(Serialize)]
pub struct AutoTag {
    tag_id: i64,
    source: String,
    reason: Option<String>,
    applied_at: Option<i64>,
}

#[derive(Deserialize)]
pub struct SuggestApplyOptions {
    /// content | path | rule | learned | ai
    sources: Vec<String>,
    /// app | type | function | personal
    kinds: Vec<String>,
    min_score: f64,
    dry_run: bool,
}

#[derive(Serialize, Default)]
pub struct SuggestApplyResult {
    resources: usize,
    tags: usize,
    by_kind: HashMap<String, usize>,
}

/// Áp dụng gợi ý hàng loạt. Tag được đánh dấu tự động (nguồn + lý do) để rà soát lại sau.
#[tauri::command]
pub fn apply_suggestions(state: State<AppState>, ids: Vec<i64>, opts: SuggestApplyOptions) -> Res<SuggestApplyResult> {
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    let mut res = SuggestApplyResult::default();
    for id in ids {
        let (have, sug): (Vec<i64>, Option<String>) = (
            collect(&tx, "SELECT tag_id FROM resource_tags WHERE resource_id = ?1", [id], |r| r.get(0))?,
            tx.query_row("SELECT suggestions FROM resources WHERE id = ?1", [id], |r| r.get(0)).optional().map_err(e)?.flatten(),
        );
        let mut added = 0;
        let mut per_kind: HashMap<String, usize> = HashMap::new();
        for s in collect_suggestions(&state, &tx, id, &have, sug)? {
            if !opts.sources.contains(&s.source) || !opts.kinds.contains(&s.kind) || s.score < opts.min_score {
                continue;
            }
            // mỗi loại tối đa 2 tag / resource để tránh gắn tràn lan
            let c = per_kind.entry(s.kind.clone()).or_default();
            if *c >= 2 {
                continue;
            }
            *c += 1;
            added += 1;
            *res.by_kind.entry(s.kind.clone()).or_default() += 1;
            if !opts.dry_run {
                let tid = match s.tag_id {
                    Some(t) => t,
                    None => crate::autotag::ensure_tag(&tx, &s.kind, &s.name).map_err(e)?,
                };
                crate::autotag::insert_auto(&tx, id, tid, &s.source, &s.reason).map_err(e)?;
            }
        }
        if added > 0 {
            res.resources += 1;
            res.tags += added;
            if !opts.dry_run {
                db::touch(&tx, id).map_err(e)?;
                db::reindex(&tx, id).map_err(e)?;
            }
        }
    }
    if opts.dry_run {
        drop(tx); // rollback
    } else {
        tx.commit().map_err(e)?;
        *state.learn.lock().unwrap() = None;
    }
    Ok(res)
}

const PREF_KEYS: &[&str] = &["auto_tag_new", "show_onboarding"];

/// Tùy chọn đơn giản dạng bật/tắt (mặc định bật).
#[tauri::command]
pub fn get_pref(state: State<AppState>, key: String) -> Res<bool> {
    if !PREF_KEYS.contains(&key.as_str()) {
        return Err("Tùy chọn không hợp lệ".into());
    }
    Ok(db::get_setting(&state.db.lock().unwrap(), &key).as_deref() != Some("0"))
}

/// Thông báo một lần (vd: đã tự khôi phục database hỏng) — đọc xong thì xóa.
#[tauri::command]
pub fn take_db_notice(state: State<AppState>) -> Res<Option<String>> {
    let conn = state.db.lock().unwrap();
    let n = db::get_setting(&conn, "db_notice").filter(|s| !s.is_empty());
    if n.is_some() {
        conn.execute("DELETE FROM settings WHERE key = 'db_notice'", []).map_err(e)?;
    }
    Ok(n)
}

#[tauri::command]
pub fn set_pref(state: State<AppState>, key: String, on: bool) -> Res<()> {
    if !PREF_KEYS.contains(&key.as_str()) {
        return Err("Tùy chọn không hợp lệ".into());
    }
    db::set_setting(&state.db.lock().unwrap(), &key, if on { "1" } else { "0" }).map_err(e)
}

/// Xác nhận tag tự động (tag_id = None: mọi tag tự động của các resource này).
#[tauri::command]
pub fn confirm_auto_tags(state: State<AppState>, ids: Vec<i64>, tag_id: Option<i64>) -> Res<usize> {
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    let mut n = 0;
    for id in ids {
        n += match tag_id {
            Some(t) => tx.execute(
                "UPDATE resource_tags SET source = NULL, reason = NULL, applied_at = NULL WHERE resource_id = ?1 AND tag_id = ?2 AND source IS NOT NULL",
                params![id, t],
            ),
            None => tx.execute(
                "UPDATE resource_tags SET source = NULL, reason = NULL, applied_at = NULL WHERE resource_id = ?1 AND source IS NOT NULL AND source <> 'scan'",
                [id],
            ),
        }
        .map_err(e)?;
    }
    tx.commit().map_err(e)?;
    Ok(n)
}

#[derive(Serialize, Clone)]
pub struct SuggestedTag {
    kind: String,
    name: String,
    tag_id: Option<i64>,
    /// content | rule | learned | ai
    source: String,
    reason: String,
    score: f64,
}

/// Gộp gợi ý từ: extension bên trong (content), rule, mô hình học từ tag đã gắn, và AI (embedding).
fn collect_suggestions(state: &AppState, conn: &Connection, id: i64, have: &[i64], sug_json: Option<String>) -> Res<Vec<SuggestedTag>> {
    let tags: HashMap<i64, (String, String)> = collect(conn, "SELECT id, kind, name FROM tags", [], |r| {
        Ok((r.get::<_, i64>(0)?, (r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
    })?
    .into_iter()
    .collect();
    let mut out: Vec<SuggestedTag> = Vec::new();
    let rejected: Vec<i64> = collect(conn, "SELECT tag_id FROM tag_rejections WHERE resource_id = ?1", [id], |r| r.get(0))?;
    let push = |out: &mut Vec<SuggestedTag>, kind: &str, name: &str, tag_id: Option<i64>, source: &str, reason: String, score: f64| {
        if let Some(t) = tag_id {
            if have.contains(&t) || rejected.contains(&t) {
                return;
            }
        }
        let dup = out.iter().any(|x| x.kind == kind && x.name.eq_ignore_ascii_case(name));
        if !dup {
            out.push(SuggestedTag { kind: kind.into(), name: name.into(), tag_id, source: source.into(), reason, score });
        }
    };
    if let Some(s) = sug_json.and_then(|s| serde_json::from_str::<SuggestionsDe>(&s).ok()) {
        for t in s.types {
            let tid = db::tag_id(conn, "type", &t).map_err(e)?;
            push(&mut out, "type", &t, tid, "content", "Từ các file bên trong".into(), 0.8);
        }
        for a in s.apps {
            let tid = db::tag_id(conn, "app", &a).map_err(e)?;
            push(&mut out, "app", &a, tid, "content", "Từ các file bên trong".into(), 0.8);
        }
        for t in s.path_types {
            let tid = db::tag_id(conn, "type", &t).map_err(e)?;
            push(&mut out, "type", &t, tid, "path", "Từ tên thư mục chứa".into(), 0.7);
        }
        for a in s.path_apps {
            let tid = db::tag_id(conn, "app", &a).map_err(e)?;
            push(&mut out, "app", &a, tid, "path", "Từ tên thư mục chứa".into(), 0.7);
        }
    }
    for (tid, rule) in crate::rules::suggestions(conn, id).map_err(e)? {
        if let Some((k, n)) = tags.get(&tid) {
            push(&mut out, k, n, Some(tid), "rule", format!("Rule: {rule}"), 0.9);
        }
    }
    let model = crate::organize::learn_model(state, conn)?;
    let toks = crate::organize::tokens_of_resource(conn, id)?;
    for (tid, p, tok) in crate::organize::learned_suggestions(&model, &toks) {
        if let Some((k, n)) = tags.get(&tid) {
            let tok = tok.trim_start_matches("ext:");
            push(&mut out, k, n, Some(tid), "learned", format!("Resource có “{tok}” thường được gắn tag này ({:.0}%)", p * 100.0), p);
        }
    }
    for (tid, sim) in crate::ai::tag_suggestions(state, conn, id).unwrap_or_default() {
        if let Some((k, n)) = tags.get(&tid) {
            push(&mut out, k, n, Some(tid), "ai", format!("AI: gần nghĩa ({:.0}%)", sim * 100.0), sim as f64);
        }
    }
    Ok(out)
}

fn join_path(lib: &str, rel: &str) -> String {
    Path::new(lib).join(rel).to_string_lossy().to_string()
}

#[tauri::command]
pub fn get_resource(state: State<AppState>, id: i64) -> Res<Option<ResourceDetail>> {
    let conn = state.db.lock().unwrap();
    let base = conn
        .query_row(
            "SELECT id, name, version, notes, favorite, created_at, updated_at, suggestions, cover_user, ai_applied_at FROM resources WHERE id = ?1",
            [id],
            |r| {
                Ok((r.get::<_, Option<String>>(7)?, ResourceDetail {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    version: r.get(2)?,
                    notes: r.get(3)?,
                    favorite: r.get::<_, i64>(4)? != 0,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                    tag_ids: vec![],
                    suggested: vec![],
                    sources: vec![],
                    matches: vec![],
                    versions: vec![],
                    relations: vec![],
                    cover_user: r.get::<_, i64>(8)? != 0,
                    ai: None,
                    note_images: vec![],
                    ai_applied_at: r.get(9)?,
                    auto_tags: vec![],
                }))
            },
        )
        .optional()
        .map_err(e)?;
    let Some((sug_json, mut d)) = base else { return Ok(None) };
    d.tag_ids = collect(&conn, "SELECT tag_id FROM resource_tags WHERE resource_id = ?1", [id], |r| r.get(0))?;
    d.suggested = collect_suggestions(&state, &conn, id, &d.tag_ids, sug_json)?;
    d.versions = crate::organize::family_of(&conn, id)?;
    d.relations = crate::organize::relations_of(&conn, id)?;
    d.note_images = crate::noteimg::list(&conn, id).map_err(e)?;
    d.auto_tags = collect(
        &conn,
        "SELECT tag_id, source, reason, applied_at FROM resource_tags WHERE resource_id = ?1 AND source IS NOT NULL AND source <> 'scan'",
        [id],
        |r| Ok(AutoTag { tag_id: r.get(0)?, source: r.get(1)?, reason: r.get(2)?, applied_at: r.get(3)? }),
    )?;
    d.ai = conn
        .query_row("SELECT data FROM ai_suggestions WHERE resource_id = ?1", [id], |r| r.get::<_, String>(0))
        .optional()
        .map_err(e)?
        .and_then(|s| serde_json::from_str(&s).ok());
    d.sources = collect(
        &conn,
        "SELECT s.id, s.library_id, l.name, l.path, s.rel_path, s.name, s.kind, s.size, s.file_count, s.modified_at,
                s.ext_summary, s.available, s.match_state, s.match_score, l.scan_depth = 0,
                (SELECT mode FROM scan_overrides o WHERE o.library_id = s.library_id AND o.rel_path = s.rel_path)
         FROM resource_sources s JOIN libraries l ON l.id = s.library_id WHERE s.resource_id = ?1
         ORDER BY CASE s.kind WHEN 'folder' THEN 0 WHEN 'archive' THEN 1 ELSE 2 END, s.name",
        [id],
        |r| {
            let lib_path: String = r.get(3)?;
            let rel: String = r.get(4)?;
            let ext: Option<String> = r.get(10)?;
            let mut extensions: Vec<(String, u64)> = ext
                .and_then(|s| serde_json::from_str::<HashMap<String, u64>>(&s).ok())
                .unwrap_or_default()
                .into_iter()
                .collect();
            extensions.sort_by(|a, b| b.1.cmp(&a.1));
            let abs = join_path(&lib_path, &rel);
            Ok(SourceInfo {
                id: r.get(0)?,
                library_id: r.get(1)?,
                library_name: r.get(2)?,
                available: r.get::<_, i64>(11)? != 0 && Path::new(&lib_path).is_dir(),
                abs_path: abs,
                rel_path: rel,
                name: r.get(5)?,
                kind: r.get(6)?,
                size: r.get(7)?,
                file_count: r.get(8)?,
                modified_at: r.get(9)?,
                extensions,
                match_state: r.get(12)?,
                match_score: r.get(13)?,
                library_auto: r.get(14)?,
                override_mode: r.get(15)?,
            })
        },
    )?;
    d.matches = match_suggestions_where(&conn, "AND (sa.resource_id = ?1 OR sb.resource_id = ?1)", [id])?;
    Ok(Some(d))
}

#[derive(Deserialize)]
struct SuggestionsDe {
    types: Vec<String>,
    apps: Vec<String>,
    #[serde(default)]
    path_types: Vec<String>,
    #[serde(default)]
    path_apps: Vec<String>,
}

#[derive(Deserialize)]
pub struct ResourcePatch {
    name: Option<String>,
    notes: Option<String>,
    favorite: Option<bool>,
}

#[tauri::command]
pub fn update_resource(state: State<AppState>, id: i64, patch: ResourcePatch) -> Res<()> {
    let conn = state.db.lock().unwrap();
    if let Some(name) = patch.name.filter(|n| !n.trim().is_empty()) {
        conn.execute("UPDATE resources SET name = ?1 WHERE id = ?2", params![name.trim(), id]).map_err(e)?;
    }
    if let Some(notes) = patch.notes {
        conn.execute("UPDATE resources SET notes = ?1 WHERE id = ?2", params![notes, id]).map_err(e)?;
    }
    if let Some(f) = patch.favorite {
        conn.execute("UPDATE resources SET favorite = ?1 WHERE id = ?2", params![f as i64, id]).map_err(e)?;
    }
    db::touch(&conn, id).map_err(e)?;
    db::reindex(&conn, id).map_err(e)
}

#[tauri::command]
pub fn set_favorite(state: State<AppState>, ids: Vec<i64>, favorite: bool) -> Res<()> {
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    for id in ids {
        tx.execute("UPDATE resources SET favorite = ?1 WHERE id = ?2", params![favorite as i64, id]).map_err(e)?;
    }
    tx.commit().map_err(e)
}

#[tauri::command]
pub fn set_tag(state: State<AppState>, resource_ids: Vec<i64>, tag_id: i64, on: bool) -> Res<()> {
    *state.learn.lock().unwrap() = None;
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    for rid in resource_ids {
        if on {
            crate::autotag::insert_manual(&tx, rid, tag_id).map_err(e)?;
        } else {
            crate::autotag::remove_and_reject(&tx, rid, tag_id).map_err(e)?;
        }
        db::touch(&tx, rid).map_err(e)?;
        db::reindex(&tx, rid).map_err(e)?;
    }
    tx.commit().map_err(e)
}

/// Gộp nhiều resource thành một (giữ resource được chăm sóc nhiều nhất).
#[tauri::command]
pub fn merge_resources(state: State<AppState>, ids: Vec<i64>) -> Res<i64> {
    if ids.len() < 2 {
        return Err("Cần chọn ít nhất 2 resource".into());
    }
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    let mut keep = ids[0];
    let mut best = db::curation_score(&tx, keep).map_err(e)?;
    for &id in &ids[1..] {
        let s = db::curation_score(&tx, id).map_err(e)?;
        if s > best {
            best = s;
            keep = id;
        }
    }
    for &id in &ids {
        if id != keep {
            db::merge_into(&tx, keep, id).map_err(e)?;
        }
    }
    tx.execute("UPDATE resource_sources SET match_state = 'confirmed' WHERE resource_id = ?1", [keep]).map_err(e)?;
    scanner::refresh_suggestions(&tx, keep).map_err(e)?;
    db::reindex(&tx, keep).map_err(e)?;
    tx.commit().map_err(e)?;
    Ok(keep)
}

/// Unmerge: tách một nguồn ra thành resource riêng. Matcher sẽ không tự ghép lại nguồn này.
#[tauri::command]
pub fn split_source(state: State<AppState>, source_id: i64) -> Res<i64> {
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    let (old_rid, name, kind): (i64, String, String) = tx
        .query_row("SELECT resource_id, name, kind FROM resource_sources WHERE id = ?1", [source_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(e)?;
    let count: i64 = tx.query_row("SELECT count(*) FROM resource_sources WHERE resource_id = ?1", [old_rid], |r| r.get(0)).map_err(e)?;
    if count < 2 {
        return Err("Resource chỉ có một nguồn".into());
    }
    let is_file = kind != "folder";
    let now = db::now();
    tx.execute(
        "INSERT INTO resources (name, version, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        params![normalize::pretty_name(&name, is_file), normalize::normalize(&name, is_file).version, now],
    )
    .map_err(e)?;
    let new_rid = tx.last_insert_rowid();
    tx.execute(
        "UPDATE resource_sources SET resource_id = ?1, match_state = 'split', match_score = NULL WHERE id = ?2",
        params![new_rid, source_id],
    )
    .map_err(e)?;
    // Giữ lại các tag phân loại cho resource mới (người dùng có thể bỏ sau)
    tx.execute(
"INSERT OR IGNORE INTO resource_tags (resource_id, tag_id, source, reason, applied_at)
         SELECT ?1, tag_id, source, reason, applied_at FROM resource_tags WHERE resource_id = ?2",
        params![new_rid, old_rid],
    )
    .map_err(e)?;
    for rid in [old_rid, new_rid] {
        scanner::refresh_suggestions(&tx, rid).map_err(e)?;
        db::touch(&tx, rid).map_err(e)?;
        db::reindex(&tx, rid).map_err(e)?;
    }
    tx.commit().map_err(e)?;
    Ok(new_rid)
}

#[tauri::command]
pub fn reveal_source(state: State<AppState>, source_id: i64) -> Res<()> {
    let conn = state.db.lock().unwrap();
    let (lib, rel): (String, String) = conn
        .query_row(
            "SELECT l.path, s.rel_path FROM resource_sources s JOIN libraries l ON l.id = s.library_id WHERE s.id = ?1",
            [source_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(e)?;
    let abs = join_path(&lib, &rel).replace('/', "\\");
    if !Path::new(&abs).exists() {
        return Err("Nguồn hiện không truy cập được".into());
    }
    // SHOpenFolderAndSelectItems — xử lý đúng đường dẫn có dấu cách / Unicode
    // (trước đây `explorer /select,<path>` bị bọc nháy sai nên Explorer mở thư mục mặc định)
    tauri_plugin_opener::reveal_item_in_dir(&abs).map_err(e)
}

// ================================================================ Tags

#[derive(Serialize)]
pub struct TagInfo {
    id: i64,
    kind: String,
    name: String,
    color: Option<String>,
    count: i64,
}

#[tauri::command]
pub fn list_tags(state: State<AppState>) -> Res<Vec<TagInfo>> {
    let conn = state.db.lock().unwrap();
    collect(
        &conn,
        "SELECT t.id, t.kind, t.name, t.color, (SELECT count(*) FROM resource_tags WHERE tag_id = t.id)
         FROM tags t ORDER BY t.kind, t.name COLLATE NOCASE",
        [],
        |r| Ok(TagInfo { id: r.get(0)?, kind: r.get(1)?, name: r.get(2)?, color: r.get(3)?, count: r.get(4)? }),
    )
}

const TAG_KINDS: &[&str] = &["app", "type", "function", "personal", "status"];

/// Tạo tag (hoặc trả về tag đã có cùng tên).
#[tauri::command]
pub fn create_tag(state: State<AppState>, kind: String, name: String) -> Res<i64> {
    let name = name.trim();
    if name.is_empty() || !TAG_KINDS.contains(&kind.as_str()) {
        return Err("Tag không hợp lệ".into());
    }
    let conn = state.db.lock().unwrap();
    if let Some(id) = db::tag_id(&conn, &kind, name).map_err(e)? {
        return Ok(id);
    }
    conn.execute("INSERT INTO tags (kind, name) VALUES (?1, ?2)", params![kind, name]).map_err(e)?;
    Ok(conn.last_insert_rowid())
}

fn reindex_tag_users(conn: &Connection, tag_id: i64) -> Res<()> {
    for rid in collect(conn, "SELECT resource_id FROM resource_tags WHERE tag_id = ?1", [tag_id], |r| r.get::<_, i64>(0))? {
        db::reindex(conn, rid).map_err(e)?;
    }
    Ok(())
}

#[tauri::command]
pub fn update_tag(state: State<AppState>, id: i64, name: String, color: Option<String>) -> Res<()> {
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    tx.execute("UPDATE tags SET name = ?1, color = ?2 WHERE id = ?3", params![name.trim(), color, id]).map_err(e)?;
    reindex_tag_users(&tx, id)?;
    tx.commit().map_err(e)
}

#[tauri::command]
pub fn delete_tag(state: State<AppState>, id: i64) -> Res<()> {
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    let users = collect(&tx, "SELECT resource_id FROM resource_tags WHERE tag_id = ?1", [id], |r| r.get::<_, i64>(0))?;
    tx.execute("DELETE FROM tags WHERE id = ?1", [id]).map_err(e)?;
    for rid in users {
        db::reindex(&tx, rid).map_err(e)?;
    }
    tx.commit().map_err(e)
}

// ================================================================ Match suggestions

fn match_suggestions_where<P: rusqlite::Params>(conn: &Connection, extra: &str, p: P) -> Res<Vec<MatchSuggestionInfo>> {
    let sql = format!(
        "SELECT m.id, m.score,
                sa.id, sa.resource_id, ra.name, sa.name, sa.kind, sa.rel_path, la.name, sa.size,
                sb.id, sb.resource_id, rb.name, sb.name, sb.kind, sb.rel_path, lb.name, sb.size
         FROM match_suggestions m
         JOIN resource_sources sa ON sa.id = m.source_a JOIN resources ra ON ra.id = sa.resource_id JOIN libraries la ON la.id = sa.library_id
         JOIN resource_sources sb ON sb.id = m.source_b JOIN resources rb ON rb.id = sb.resource_id JOIN libraries lb ON lb.id = sb.library_id
         WHERE m.state = 'pending' AND sa.resource_id <> sb.resource_id {extra}
         ORDER BY m.score DESC"
    );
    collect(conn, &sql, p, |r| {
        let brief = |o: usize| -> rusqlite::Result<SourceBrief> {
            Ok(SourceBrief {
                source_id: r.get(o)?,
                resource_id: r.get(o + 1)?,
                resource_name: r.get(o + 2)?,
                name: r.get(o + 3)?,
                kind: r.get(o + 4)?,
                rel_path: r.get(o + 5)?,
                library_name: r.get(o + 6)?,
                size: r.get(o + 7)?,
            })
        };
        Ok(MatchSuggestionInfo { id: r.get(0)?, score: r.get(1)?, a: brief(2)?, b: brief(10)? })
    })
}

#[tauri::command]
pub fn list_match_suggestions(state: State<AppState>) -> Res<Vec<MatchSuggestionInfo>> {
    let conn = state.db.lock().unwrap();
    match_suggestions_where(&conn, "", [])
}

#[tauri::command]
pub fn resolve_match(state: State<AppState>, id: i64, accept: bool) -> Res<Option<i64>> {
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    let (a, b): (i64, i64) = tx.query_row("SELECT source_a, source_b FROM match_suggestions WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(e)?;
    let mut kept = None;
    if accept {
        let ra: i64 = tx.query_row("SELECT resource_id FROM resource_sources WHERE id = ?1", [a], |r| r.get(0)).map_err(e)?;
        let rb: i64 = tx.query_row("SELECT resource_id FROM resource_sources WHERE id = ?1", [b], |r| r.get(0)).map_err(e)?;
        let (keep, drop) = if db::curation_score(&tx, rb).map_err(e)? > db::curation_score(&tx, ra).map_err(e)? { (rb, ra) } else { (ra, rb) };
        db::merge_into(&tx, keep, drop).map_err(e)?;
        let score: f64 = tx.query_row("SELECT score FROM match_suggestions WHERE id = ?1", [id], |r| r.get(0)).map_err(e)?;
        tx.execute("UPDATE resource_sources SET match_state = 'confirmed', match_score = ?1 WHERE id IN (?2, ?3)", params![score, a, b]).map_err(e)?;
        scanner::refresh_suggestions(&tx, keep).map_err(e)?;
        db::reindex(&tx, keep).map_err(e)?;
        kept = Some(keep);
    }
    tx.execute("UPDATE match_suggestions SET state = ?1 WHERE id = ?2", params![if accept { "accepted" } else { "rejected" }, id]).map_err(e)?;
    tx.commit().map_err(e)?;
    Ok(kept)
}

// ================================================================ Dashboard

#[derive(Serialize)]
pub struct Bucket {
    id: i64,
    name: String,
    count: i64,
    size: i64,
}

#[derive(Serialize)]
pub struct Dashboard {
    resources: i64,
    total_size: i64,
    archives: i64,
    folders: i64,
    matched: i64,
    unclassified: i64,
    favorites: i64,
    missing: i64,
    pending_matches: i64,
    review: i64,
    by_app: Vec<Bucket>,
    by_type: Vec<Bucket>,
    by_status: Vec<Bucket>,
    by_library: Vec<Bucket>,
}

#[tauri::command]
pub fn get_dashboard(state: State<AppState>) -> Res<Dashboard> {
    let conn = state.db.lock().unwrap();
    let one = |sql: &str| -> Res<i64> { conn.query_row(sql, [], |r| r.get(0)).map_err(e) };
    let bucket = |kind: &str| -> Res<Vec<Bucket>> {
        collect(
            &conn,
            "SELECT t.id, t.name, count(rt.resource_id),
                    coalesce(sum((SELECT sum(size) FROM resource_sources s WHERE s.resource_id = rt.resource_id)), 0)
             FROM tags t JOIN resource_tags rt ON rt.tag_id = t.id WHERE t.kind = ?1
             GROUP BY t.id ORDER BY 3 DESC",
            [kind],
            |r| Ok(Bucket { id: r.get(0)?, name: r.get(1)?, count: r.get(2)?, size: r.get(3)? }),
        )
    };
    Ok(Dashboard {
        resources: one("SELECT count(*) FROM resources")?,
        total_size: one("SELECT coalesce(sum(size),0) FROM resource_sources")?,
        archives: one("SELECT count(*) FROM resource_sources WHERE kind = 'archive'")?,
        folders: one("SELECT count(*) FROM resource_sources WHERE kind = 'folder'")?,
        matched: one("SELECT count(*) FROM (SELECT resource_id FROM resource_sources GROUP BY resource_id HAVING count(*) > 1)")?,
        unclassified: one(
            "SELECT count(*) FROM resources r WHERE NOT EXISTS (SELECT 1 FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id
             WHERE rt.resource_id = r.id AND t.kind IN ('app','type'))",
        )?,
        favorites: one("SELECT count(*) FROM resources WHERE favorite = 1")?,
        missing: one("SELECT count(DISTINCT resource_id) FROM resource_sources WHERE available = 0")?,
        pending_matches: one(
            "SELECT count(*) FROM match_suggestions m JOIN resource_sources a ON a.id = m.source_a JOIN resource_sources b ON b.id = m.source_b
             WHERE m.state = 'pending' AND a.resource_id <> b.resource_id",
        )?,
        review: one(&format!("SELECT count(*) FROM resources r WHERE {}", crate::autotag::NEEDS_REVIEW_SQL))?,
        by_app: bucket("app")?,
        by_type: bucket("type")?,
        by_status: bucket("status")?,
        by_library: collect(
            &conn,
            "SELECT l.id, l.name, count(DISTINCT s.resource_id), coalesce(sum(s.size),0)
             FROM libraries l LEFT JOIN resource_sources s ON s.library_id = l.id GROUP BY l.id ORDER BY 4 DESC",
            [],
            |r| Ok(Bucket { id: r.get(0)?, name: r.get(1)?, count: r.get(2)?, size: r.get(3)? }),
        )?,
    })
}

// ================================================================ Backup / export

fn backup_dir(state: &AppState) -> PathBuf {
    state.data_dir.join("backups")
}

pub fn backup_to(conn: &Connection, dir: &Path, prefix: &str) -> Res<PathBuf> {
    std::fs::create_dir_all(dir).map_err(e)?;
    let file = dir.join(format!("{prefix}-{}.db", db::now()));
    conn.execute("VACUUM INTO ?1", [file.to_string_lossy()]).map_err(e)?;
    Ok(file)
}

/// Backup tự động mỗi ngày khi mở app, giữ 10 bản gần nhất.
pub fn auto_backup(state: &AppState) -> Res<()> {
    let dir = backup_dir(state);
    let mut autos: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.file_name().map(|n| n.to_string_lossy().starts_with("auto-")).unwrap_or(false)).collect())
        .unwrap_or_default();
    autos.sort();
    let latest = autos.last().and_then(|p| p.metadata().ok()).and_then(|m| m.modified().ok());
    let due = latest.map(|t| t.elapsed().map(|d| d.as_secs() > 86_400).unwrap_or(true)).unwrap_or(true);
    if due {
        let conn = state.db.lock().unwrap();
        let has_data: i64 = conn.query_row("SELECT count(*) FROM libraries", [], |r| r.get(0)).map_err(e)?;
        if has_data > 0 {
            autos.push(backup_to(&conn, &dir, "auto")?);
        }
    }
    while autos.len() > 10 {
        let old = autos.remove(0);
        let _ = std::fs::remove_file(old); // chỉ xóa file backup của chính app
    }
    Ok(())
}

#[derive(Serialize)]
pub struct BackupInfo {
    path: String,
    name: String,
    size: u64,
    modified: i64,
}

#[tauri::command]
pub fn backup_now(state: State<AppState>) -> Res<String> {
    let conn = state.db.lock().unwrap();
    backup_to(&conn, &backup_dir(&state), "manual").map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub fn list_backups(state: State<AppState>) -> Res<Vec<BackupInfo>> {
    let mut v: Vec<BackupInfo> = std::fs::read_dir(backup_dir(&state))
        .map(|rd| {
            rd.flatten()
                .filter_map(|en| {
                    let m = en.metadata().ok()?;
                    Some(BackupInfo {
                        path: en.path().to_string_lossy().to_string(),
                        name: en.file_name().to_string_lossy().to_string(),
                        size: m.len(),
                        modified: m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(v)
}

/// Khôi phục database từ file backup (tự backup bản hiện tại trước khi ghi đè).
#[tauri::command]
pub fn restore_backup(state: State<AppState>, path: String) -> Res<()> {
    let src = PathBuf::from(&path);
    // kiểm tra file là database MRM hợp lệ
    {
        let test = Connection::open_with_flags(&src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(e)?;
        test.query_row("SELECT count(*) FROM resources", [], |r| r.get::<_, i64>(0))
            .map_err(|_| "File không phải database MRM".to_string())?;
    }
    let mut conn = state.db.lock().unwrap();
    backup_to(&conn, &backup_dir(&state), "before-restore")?;
    conn.restore(rusqlite::MAIN_DB, &src, None::<fn(rusqlite::backup::Progress)>).map_err(e)?;
    // backup từ phiên bản cũ -> nâng schema ngay, không đợi khởi động lại
    db::migrate(&conn).map_err(e)?;
    *state.learn.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
pub fn export_json(state: State<AppState>, path: String) -> Res<()> {
    let conn = state.db.lock().unwrap();
    let libraries = collect(&conn, "SELECT id, name, path, scan_depth FROM libraries", [], |r| {
        Ok(serde_json::json!({"id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?, "path": r.get::<_, String>(2)?, "scan_depth": r.get::<_, i64>(3)?}))
    })?;
    let tags = collect(&conn, "SELECT id, kind, name, color FROM tags", [], |r| {
        Ok(serde_json::json!({"id": r.get::<_, i64>(0)?, "kind": r.get::<_, String>(1)?, "name": r.get::<_, String>(2)?, "color": r.get::<_, Option<String>>(3)?}))
    })?;
    let mut resources = collect(&conn, "SELECT id, name, version, notes, favorite, created_at, updated_at FROM resources ORDER BY id", [], |r| {
        Ok(serde_json::json!({
            "id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?, "version": r.get::<_, Option<String>>(2)?,
            "notes": r.get::<_, String>(3)?, "favorite": r.get::<_, i64>(4)? != 0,
            "created_at": r.get::<_, i64>(5)?, "updated_at": r.get::<_, i64>(6)?,
        }))
    })?;
    let mut sources: HashMap<i64, Vec<serde_json::Value>> = HashMap::new();
    for (rid, v) in collect(&conn, "SELECT resource_id, library_id, rel_path, kind, size FROM resource_sources", [], |r| {
        Ok((r.get::<_, i64>(0)?, serde_json::json!({"library_id": r.get::<_, i64>(1)?, "rel_path": r.get::<_, String>(2)?, "kind": r.get::<_, String>(3)?, "size": r.get::<_, i64>(4)?})))
    })? {
        sources.entry(rid).or_default().push(v);
    }
    let mut rtags: HashMap<i64, Vec<i64>> = HashMap::new();
    for (rid, tid) in collect(&conn, "SELECT resource_id, tag_id FROM resource_tags", [], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
        rtags.entry(rid).or_default().push(tid);
    }
    for r in &mut resources {
        let id = r["id"].as_i64().unwrap_or(0);
        r["sources"] = serde_json::json!(sources.remove(&id).unwrap_or_default());
        r["tag_ids"] = serde_json::json!(rtags.remove(&id).unwrap_or_default());
    }
    let doc = serde_json::json!({
        "app": "MRM", "schema_version": db::SCHEMA_VERSION, "exported_at": db::now(),
        "libraries": libraries, "tags": tags, "resources": resources,
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&doc).map_err(e)?).map_err(e)
}

#[tauri::command]
pub fn open_data_dir(state: State<AppState>) -> Res<()> {
    tauri_plugin_opener::open_path(&state.data_dir, None::<&str>).map_err(e)
}

#[tauri::command]
pub fn get_data_dir(state: State<AppState>) -> String {
    state.data_dir.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search() {
        let (f, w) = parse_search(r#"skin resolve app:resolve type:"stock video" tag:tracking"#);
        assert_eq!(w, vec!["skin", "resolve"]);
        assert_eq!(f[1], ("type".to_string(), "stock video".to_string()));
        assert_eq!(fts_query(&w).unwrap(), "\"skin\"* \"resolve\"*");
        assert!(fts_query(&["-".into()]).is_none());
    }
}
