//! Quét Library: liệt kê ứng viên, đọc metadata (không sửa file), ghi vào DB và chạy matcher.
//! Nguyên tắc: KHÔNG move / rename / delete bất kỳ file thật nào.

use crate::classify;
use crate::db;
use crate::matcher::{self, Decision, MatchItem};
use crate::normalize::{self, is_archive_ext};
use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

const SKIP_NAMES: &[&str] = &["$recycle.bin", "system volume information", "desktop.ini", "thumbs.db", ".ds_store"];

#[derive(Debug, Clone, Serialize)]
pub struct ScanProgress {
    pub library_id: i64,
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub current: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ScanResult {
    pub library_id: i64,
    pub online: bool,
    pub found: usize,
    pub added: usize,
    pub missing: usize,
    /// nguồn cũ vẫn còn trên đĩa nhưng đã được nhóm lại (tách/gộp) — metadata được chuyển sang resource mới
    pub regrouped: usize,
    pub auto_merged: usize,
    pub suggested: usize,
}

#[derive(Debug, Clone)]
struct Candidate {
    abs: PathBuf,
    rel: String,
    name: String,
    kind: &'static str,
    modified: Option<i64>,
    size: u64,
    file_count: u64,
    ext_counts: HashMap<String, u64>,
    content_sig: Option<String>,
    content_size: u64,
}

// ---------------------------------------------------------------- volume / relocation

#[cfg(windows)]
pub fn volume_serial(path: &Path) -> Option<u32> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;
    let root = volume_root(path)?;
    let wide: Vec<u16> = std::ffi::OsStr::new(&root).encode_wide().chain(Some(0)).collect();
    let mut serial: u32 = 0;
    let ok = unsafe {
        GetVolumeInformationW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            0,
            &mut serial,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    if ok != 0 { Some(serial) } else { None }
}

#[cfg(not(windows))]
pub fn volume_serial(_path: &Path) -> Option<u32> {
    None
}

fn volume_root(path: &Path) -> Option<String> {
    let s = path.to_string_lossy();
    let mut chars = s.chars();
    let letter = chars.next()?;
    if letter.is_ascii_alphabetic() && chars.next() == Some(':') {
        Some(format!("{}:\\", letter.to_ascii_uppercase()))
    } else {
        None
    }
}

/// Library có đang truy cập được không. Nếu ổ ngoài đổi ký tự, tìm lại bằng volume serial
/// và cập nhật `libraries.path` (chỉ sửa metadata, không đụng file).
pub fn check_library(conn: &Connection, lib_id: i64) -> rusqlite::Result<bool> {
    let (path, serial): (String, Option<i64>) =
        conn.query_row("SELECT path, volume_serial FROM libraries WHERE id = ?1", [lib_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let p = Path::new(&path);
    if p.is_dir() {
        return Ok(true);
    }
    let Some(serial) = serial else { return Ok(false) };
    if volume_root(p).is_none() {
        return Ok(false);
    }
    for letter in b'A'..=b'Z' {
        let candidate = format!("{}{}", letter as char, &path[1..]);
        let cp = Path::new(&candidate);
        if volume_serial(cp).map(|s| s as i64) == Some(serial) && cp.is_dir() {
            let taken: Option<i64> = conn
                .query_row("SELECT id FROM libraries WHERE path = ?1", [&candidate], |r| r.get(0))
                .optional()?;
            if taken.is_none() {
                conn.execute("UPDATE libraries SET path = ?1 WHERE id = ?2", params![candidate, lib_id])?;
                return Ok(true);
            }
        }
    }
    Ok(false)
}

// ---------------------------------------------------------------- filesystem

fn is_skipped(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with('.') || lower.starts_with('~') || SKIP_NAMES.contains(&lower.as_str())
}

fn modified_secs(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

fn ext_of(name: &str) -> Option<String> {
    let pos = name.rfind('.')?;
    let ext = name[pos + 1..].to_lowercase();
    if ext.is_empty() || ext.len() > 8 { None } else { Some(ext) }
}

/// Liệt kê ứng viên resource. Ở độ sâu < scan_depth, folder được coi là "thư mục chứa"
/// (đi tiếp vào trong); file nén/file lẻ ở mọi độ sâu đều là ứng viên.
fn list_candidates(root: &Path, depth: usize) -> Vec<Candidate> {
    let mut out = Vec::new();
    fn walk(root: &Path, dir: &Path, level: usize, depth: usize, out: &mut Vec<Candidate>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if is_skipped(&name) || crate::exclude::hit(&entry.path()) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let abs = entry.path();
            let rel = abs.strip_prefix(root).unwrap_or(&abs).to_string_lossy().to_string();
            if meta.is_dir() {
                if level < depth {
                    walk(root, &abs, level + 1, depth, out);
                } else {
                    out.push(Candidate { abs, rel, name, kind: "folder", modified: modified_secs(&meta), size: 0, file_count: 0, ext_counts: HashMap::new(), content_sig: None, content_size: 0 });
                }
            } else if meta.is_file() {
                let kind = if ext_of(&name).map(|e| is_archive_ext(&e)).unwrap_or(false) { "archive" } else { "file" };
                out.push(Candidate { abs, rel, name, kind, modified: modified_secs(&meta), size: meta.len(), file_count: 0, ext_counts: HashMap::new(), content_sig: None, content_size: 0 });
            }
        }
    }
    walk(root, root, 1, depth.max(1), &mut out);
    out
}

/// Chữ ký nội dung: hash danh sách (đường dẫn tương đối, size) đã sắp xếp.
/// Bỏ thư mục gốc chung (ZIP thường bọc thêm "Tên gói/") để ZIP và folder giải nén cho cùng chữ ký.
pub fn content_signature(mut entries: Vec<(String, u64)>) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    for e in &mut entries {
        e.0 = e.0.replace('\\', "/").to_lowercase();
    }
    loop {
        let Some(top) = entries[0].0.split_once('/').map(|(a, _)| a.to_string()) else { break };
        if entries.iter().all(|(p, _)| p.split_once('/').map(|(a, _)| a == top).unwrap_or(false)) {
            for e in &mut entries {
                e.0 = e.0[top.len() + 1..].to_string();
            }
        } else {
            break;
        }
    }
    entries.sort();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for (p, sz) in &entries {
        std::hash::Hash::hash(p, &mut h);
        std::hash::Hash::hash(sz, &mut h);
    }
    Some(format!("{:016x}-{}", std::hash::Hasher::finish(&h), entries.len()))
}

/// Hash nhanh cho file lẻ / RAR / 7Z: size + 1MB đầu + 1MB cuối.
fn partial_hash(path: &Path, size: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    const CHUNK: u64 = 1024 * 1024;
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&size, &mut h);
    let mut buf = vec![0u8; CHUNK.min(size) as usize];
    f.read_exact(&mut buf).ok()?;
    std::hash::Hash::hash(&buf, &mut h);
    if size > CHUNK * 2 {
        f.seek(SeekFrom::End(-(CHUNK as i64))).ok()?;
        f.read_exact(&mut buf).ok()?;
        std::hash::Hash::hash(&buf, &mut h);
    }
    Some(format!("p{:016x}", std::hash::Hasher::finish(&h)))
}

fn inspect(c: &mut Candidate) {
    match c.kind {
        "folder" => {
            let (mut size, mut count) = (0u64, 0u64);
            let mut entries = Vec::new();
            let walk = walkdir::WalkDir::new(&c.abs).follow_links(false).into_iter();
            for e in walk.filter_entry(|e| e.depth() == 0 || !crate::exclude::hit(e.path())).flatten() {
                if e.file_type().is_file() {
                    count += 1;
                    let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                    size += len;
                    let name = e.file_name().to_string_lossy();
                    if let Some(ext) = ext_of(&name) {
                        *c.ext_counts.entry(ext).or_default() += 1;
                    }
                    if !is_skipped(&name) {
                        let rel = e.path().strip_prefix(&c.abs).unwrap_or(e.path()).to_string_lossy().to_string();
                        entries.push((rel, len));
                    }
                }
            }
            c.size = size;
            c.file_count = count;
            c.content_size = entries.iter().map(|x| x.1).sum();
            c.content_sig = content_signature(entries);
        }
        "archive" => {
            // Chỉ đọc central directory của ZIP — không giải nén.
            if c.name.to_lowercase().ends_with(".zip") {
                if let Ok(f) = std::fs::File::open(&c.abs) {
                    if let Ok(mut z) = zip::ZipArchive::new(std::io::BufReader::new(f)) {
                        let mut count = 0;
                        let mut entries = Vec::new();
                        for i in 0..z.len() {
                            if let Ok(entry) = z.by_index_raw(i) {
                                if entry.is_dir() {
                                    continue;
                                }
                                count += 1;
                                let name = entry.name().to_string();
                                if let Some(ext) = ext_of(&name) {
                                    *c.ext_counts.entry(ext).or_default() += 1;
                                }
                                let base = name.rsplit('/').next().unwrap_or(&name);
                                if !is_skipped(base) && !name.starts_with("__MACOSX/") {
                                    entries.push((name, entry.size()));
                                }
                            }
                        }
                        c.file_count = count;
                        c.content_size = entries.iter().map(|x| x.1).sum();
                        c.content_sig = content_signature(entries);
                    }
                }
            } else {
                c.content_sig = partial_hash(&c.abs, c.size);
                c.content_size = c.size;
            }
        }
        _ => {
            c.file_count = 1;
            if let Some(ext) = ext_of(&c.name) {
                c.ext_counts.insert(ext, 1);
            }
            c.content_sig = partial_hash(&c.abs, c.size);
            c.content_size = c.size;
        }
    }
}

fn ext_summary_json(counts: &HashMap<String, u64>) -> String {
    let mut v: Vec<(&String, &u64)> = counts.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let top: serde_json::Map<String, serde_json::Value> =
        v.into_iter().take(15).map(|(k, c)| (k.clone(), serde_json::json!(c))).collect();
    serde_json::Value::Object(top).to_string()
}

fn parse_ext_summary(s: Option<String>) -> HashMap<String, u64> {
    s.and_then(|s| serde_json::from_str::<HashMap<String, u64>>(&s).ok()).unwrap_or_default()
}

// ---------------------------------------------------------------- scan

struct Existing {
    modified: Option<i64>,
    size: u64,
    file_count: u64,
    ext_summary: Option<String>,
    content_sig: Option<String>,
    content_size: Option<i64>,
}

/// `force`: các ứng viên (rel_path) chắc chắn đã thay đổi (từ file watcher) — đọc lại dù mtime không đổi.
pub fn scan_library(
    db: &Mutex<Connection>,
    lib_id: i64,
    full: bool,
    force: Option<&HashSet<String>>,
    emit: &(dyn Fn(ScanProgress) + Sync),
) -> Result<ScanResult, String> {
    let err = |e: rusqlite::Error| e.to_string();
    let progress = |phase: &str, done: usize, total: usize, current: &str| {
        emit(ScanProgress { library_id: lib_id, phase: phase.into(), done, total, current: current.into() })
    };
    let started = db::now();
    let mut result = ScanResult { library_id: lib_id, ..Default::default() };

    // 1. Library info + availability
    let (root, depth, existing, overrides) = {
        let conn = db.lock().unwrap();
        crate::exclude::reload(&conn);
        let online = check_library(&conn, lib_id).map_err(err)?;
        let (path, depth, serial): (String, i64, Option<i64>) = conn
            .query_row("SELECT path, scan_depth, volume_serial FROM libraries WHERE id = ?1", [lib_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(err)?;
        if !online {
            conn.execute("UPDATE resource_sources SET available = 0 WHERE library_id = ?1", [lib_id]).map_err(err)?;
            conn.execute(
                "INSERT INTO scan_history (library_id, started_at, finished_at, error) VALUES (?1, ?2, ?2, 'Library offline')",
                params![lib_id, started],
            )
            .map_err(err)?;
            progress("done", 0, 0, "");
            return Ok(result);
        }
        if serial.is_none() {
            if let Some(s) = volume_serial(Path::new(&path)) {
                conn.execute("UPDATE libraries SET volume_serial = ?1 WHERE id = ?2", params![s as i64, lib_id]).map_err(err)?;
            }
        }
        let mut stmt = conn
            .prepare("SELECT rel_path, modified_at, size, file_count, ext_summary, content_sig, content_size FROM resource_sources WHERE library_id = ?1")
            .map_err(err)?;
        let existing: HashMap<String, Existing> = stmt
            .query_map([lib_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    Existing {
                        modified: r.get(1)?,
                        size: r.get::<_, i64>(2)? as u64,
                        file_count: r.get::<_, i64>(3)? as u64,
                        ext_summary: r.get(4)?,
                        content_sig: r.get(5)?,
                        content_size: r.get(6)?,
                    },
                ))
            })
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;
        let overrides: crate::autodetect::Overrides = {
            let mut s = conn.prepare("SELECT rel_path, mode FROM scan_overrides WHERE library_id = ?1").map_err(err)?;
            let v = s.query_map([lib_id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(err)?.collect::<Result<_, _>>().map_err(err)?;
            v
        };
        (PathBuf::from(path), depth.max(0) as usize, existing, overrides)
    };
    result.online = true;

    // 2. Liệt kê + đọc metadata (không giữ lock DB)
    progress("listing", 0, 0, &root.to_string_lossy());
    // scan_depth = 0: chế độ Tự động (nhận diện thư mục chứa / gói)
    let mut candidates = if depth == 0 { auto_candidates(&root, &overrides) } else { list_candidates(&root, depth) };
    let total = candidates.len();
    result.found = total;
    let done = AtomicUsize::new(0);
    candidates.par_iter_mut().for_each(|c| {
        let forced = force.map(|f| f.contains(&c.rel)).unwrap_or(false);
        let reuse = !full
            && !forced
            && existing
                .get(&c.rel)
                .map(|e| e.modified == c.modified && e.modified.is_some() && (e.content_sig.is_some() || e.file_count == 0))
                .unwrap_or(false);
        if reuse {
            let e = &existing[&c.rel];
            if c.kind == "folder" {
                c.size = e.size;
            }
            c.file_count = e.file_count;
            c.ext_counts = parse_ext_summary(e.ext_summary.clone());
            c.content_sig = e.content_sig.clone();
            c.content_size = e.content_size.unwrap_or(0) as u64;
        } else {
            inspect(c);
        }
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        if n % 25 == 0 || n == total {
            progress("inspecting", n, total, &c.name);
        }
    });

    // 3. Ghi DB trong một transaction
    progress("saving", total, total, "");
    let mut conn = db.lock().unwrap();
    let tx = conn.transaction().map_err(err)?;
    let now = db::now();
    tx.execute("INSERT INTO scan_history (library_id, started_at, found) VALUES (?1, ?2, ?3)", params![lib_id, started, total as i64])
        .map_err(err)?;
    let scan_id = tx.last_insert_rowid();
    let downloaded = db::tag_id(&tx, "status", "Downloaded").map_err(err)?;
    let extracted = db::tag_id(&tx, "status", "Extracted").map_err(err)?;

    let mut touched: HashSet<i64> = HashSet::new();
    let mut added_rids: HashSet<i64> = HashSet::new();
    let mut seen_sources: HashSet<i64> = HashSet::new();
    for c in &candidates {
        let summary = ext_summary_json(&c.ext_counts);
        let found: Option<(i64, i64)> = tx
            .query_row(
                "SELECT id, resource_id FROM resource_sources WHERE library_id = ?1 AND rel_path = ?2",
                params![lib_id, c.rel],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(err)?;
        match found {
            Some((sid, rid)) => {
                tx.execute(
                    "UPDATE resource_sources SET name = ?1, kind = ?2, size = ?3, file_count = ?4, modified_at = ?5,
                     ext_summary = ?6, available = 1, last_seen = ?7, content_sig = ?8, content_size = ?9 WHERE id = ?10",
                    params![c.name, c.kind, c.size as i64, c.file_count as i64, c.modified, summary, now, c.content_sig, c.content_size as i64, sid],
                )
                .map_err(err)?;
                seen_sources.insert(sid);
                let was = existing.get(&c.rel);
                let forced = force.map(|f| f.contains(&c.rel)).unwrap_or(false);
                if full || forced || was.map(|e| e.modified != c.modified).unwrap_or(true) {
                    touched.insert(rid);
                }
            }
            None => {
                let is_file = c.kind != "folder";
                let norm = normalize::normalize(&c.name, is_file);
                tx.execute(
                    "INSERT INTO resources (name, version, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
                    params![normalize::pretty_name(&c.name, is_file), norm.version, now],
                )
                .map_err(err)?;
                let rid = tx.last_insert_rowid();
                tx.execute(
                    "INSERT INTO resource_sources (resource_id, library_id, rel_path, name, kind, size, file_count, modified_at,
                     ext_summary, available, first_seen, last_seen, content_sig, content_size)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,1,?10,?10,?11,?12)",
                    params![rid, lib_id, c.rel, c.name, c.kind, c.size as i64, c.file_count as i64, c.modified, summary, now,
                            c.content_sig, c.content_size as i64],
                )
                .map_err(err)?;
                seen_sources.insert(tx.last_insert_rowid());
                let status = match c.kind {
                    "archive" => downloaded,
                    "folder" => extracted,
                    _ => None,
                };
                if let Some(t) = status {
                    crate::autotag::insert_auto(&tx, rid, t, "scan", "Trạng thái do máy quét").map_err(err)?;
                }
                touched.insert(rid);
                added_rids.insert(rid);
                result.added += 1;
            }
        }
    }

    // Nguồn không còn là ứng viên:
    //  - đường dẫn không còn trên đĩa -> đánh dấu unavailable (giữ nguyên metadata, không xóa)
    //  - vẫn còn trên đĩa -> đã được nhóm lại (tách/gộp) -> chuyển metadata sang resource mới
    let all_sources: Vec<(i64, i64, bool, String)> = {
        let mut s = tx
            .prepare("SELECT id, resource_id, available, rel_path FROM resource_sources WHERE library_id = ?1")
            .map_err(err)?;
        let v = s
            .query_map([lib_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;
        v
    };
    let cand_rid: HashMap<String, i64> = {
        let mut s = tx.prepare("SELECT rel_path, resource_id FROM resource_sources WHERE library_id = ?1").map_err(err)?;
        let v = s.query_map([lib_id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(err)?.collect::<Result<_, _>>().map_err(err)?;
        v
    };
    let cand_rels: Vec<&String> = candidates.iter().map(|c| &c.rel).collect();
    let sep = std::path::MAIN_SEPARATOR.to_string();
    let mut missing_rids = Vec::new();
    for (sid, rid, available, rel) in all_sources {
        if seen_sources.contains(&sid) {
            continue;
        }
        if root.join(&rel).exists() {
            regroup_source(&tx, sid, rid, &rel, &sep, &cand_rels, &cand_rid, &mut touched).map_err(err)?;
            result.regrouped += 1;
        } else if available {
            tx.execute("UPDATE resource_sources SET available = 0 WHERE id = ?1", [sid]).map_err(err)?;
            result.missing += 1;
            missing_rids.push(rid);
        }
    }
    touched.extend(missing_rids);
    touched.retain(|rid| {
        tx.query_row("SELECT 1 FROM resources WHERE id = ?1", [rid], |_| Ok(())).optional().ok().flatten().is_some()
    });

    // 4. Matcher trên các resource chỉ có 1 nguồn, chưa bị người dùng tách ra
    let items: Vec<MatchItem> = {
        let mut s = tx
            .prepare(
                "SELECT s.id, s.kind, s.rel_path, s.name, s.content_sig FROM resource_sources s
                 WHERE s.library_id = ?1 AND s.available = 1 AND s.match_state = 'single' AND s.kind IN ('folder','archive')
                   AND (SELECT count(*) FROM resource_sources x WHERE x.resource_id = s.resource_id) = 1",
            )
            .map_err(err)?;
        let v = s
            .query_map([lib_id], |r| {
                let kind: String = r.get(1)?;
                let rel: String = r.get(2)?;
                let name: String = r.get(3)?;
                let parent = Path::new(&rel).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                Ok(MatchItem {
                    source_id: r.get(0)?,
                    is_archive: kind == "archive",
                    is_folder: kind == "folder",
                    parent,
                    norm: normalize::normalize(&name, kind == "archive"),
                    sig: r.get(4)?,
                })
            })
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;
        v
    };
    let skip: HashSet<(i64, i64)> = {
        let mut s = tx.prepare("SELECT source_a, source_b FROM match_suggestions").map_err(err)?;
        let v = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(err)?.collect::<Result<_, _>>().map_err(err)?;
        v
    };
    for pair in matcher::find_matches(&items, &skip) {
        match pair.decision {
            Decision::Auto => {
                let ra: i64 = tx.query_row("SELECT resource_id FROM resource_sources WHERE id = ?1", [pair.a], |r| r.get(0)).map_err(err)?;
                let rb: i64 = tx.query_row("SELECT resource_id FROM resource_sources WHERE id = ?1", [pair.b], |r| r.get(0)).map_err(err)?;
                if ra != rb {
                    // Hòa điểm -> giữ resource của folder (tên folder thường "sạch" hơn tên file nén)
                    let b_is_folder = items.iter().any(|i| i.source_id == pair.b && i.is_folder);
                    let (pref, other) = if b_is_folder { (rb, ra) } else { (ra, rb) };
                    let (keep, drop) = choose_keep(&tx, pref, other).map_err(err)?;
                    db::merge_into(&tx, keep, drop).map_err(err)?;
                    touched.remove(&drop);
                    touched.insert(keep);
                    result.auto_merged += 1;
                }
                tx.execute(
                    "UPDATE resource_sources SET match_state = 'auto', match_score = ?1 WHERE id IN (?2, ?3)",
                    params![pair.score, pair.a, pair.b],
                )
                .map_err(err)?;
            }
            Decision::Possible => {
                result.suggested += tx
                    .execute(
                        "INSERT OR IGNORE INTO match_suggestions (source_a, source_b, score, created_at) VALUES (?1, ?2, ?3, ?4)",
                        params![pair.a, pair.b, pair.score, now],
                    )
                    .map_err(err)?;
            }
        }
    }

    // 5. Cập nhật gợi ý phân loại + FTS
    let rules = crate::rules::load_enabled(&tx).map_err(err)?;
    // Resource mới: tự gắn tag chắc chắn (nội dung file, tên thư mục, rule) — đánh dấu "tự động" để rà soát sau
    let auto_tag_new = db::get_setting(&tx, "auto_tag_new").as_deref() != Some("0");
    for rid in &touched {
        refresh_suggestions(&tx, *rid).map_err(err)?;
        if auto_tag_new && added_rids.contains(rid) {
            crate::autotag::apply_default(&tx, *rid).map_err(err)?;
        }
        tx.execute("UPDATE resources SET cover = NULL WHERE id = ?1 AND cover_user = 0", [rid]).map_err(err)?;
        crate::rules::apply_auto(&tx, &rules, *rid).map_err(err)?;
        db::reindex(&tx, *rid).map_err(err)?;
    }

    tx.execute("UPDATE libraries SET last_scan_at = ?1 WHERE id = ?2", params![now, lib_id]).map_err(err)?;
    tx.execute(
        "UPDATE scan_history SET finished_at = ?1, added = ?2, missing = ?3, auto_merged = ?4, suggested = ?5 WHERE id = ?6",
        params![db::now(), result.added as i64, result.missing as i64, result.auto_merged as i64, result.suggested as i64, scan_id],
    )
    .map_err(err)?;
    tx.commit().map_err(err)?;
    progress("done", total, total, "");
    Ok(result)
}

/// Giữ resource được chăm sóc nhiều hơn; hòa thì giữ `preferred`.
fn choose_keep(conn: &Connection, preferred: i64, other: i64) -> rusqlite::Result<(i64, i64)> {
    let sp = db::curation_score(conn, preferred)?;
    let so = db::curation_score(conn, other)?;
    Ok(if so > sp { (other, preferred) } else { (preferred, other) })
}

/// Ứng viên cho chế độ Tự động.
fn auto_candidates(root: &Path, overrides: &crate::autodetect::Overrides) -> Vec<Candidate> {
    crate::autodetect::candidates(root, overrides)
        .into_iter()
        .map(|c| {
            let meta = std::fs::metadata(&c.abs).ok();
            Candidate {
                modified: meta.as_ref().and_then(modified_secs),
                size: if c.kind == "folder" { 0 } else { meta.as_ref().map(|m| m.len()).unwrap_or(0) },
                abs: c.abs,
                rel: c.rel,
                name: c.name,
                kind: c.kind,
                file_count: 0,
                ext_counts: HashMap::new(),
                content_sig: None,
                content_size: 0,
            }
        })
        .collect()
}

/// Nguồn `rel` vẫn còn trên đĩa nhưng không còn là ứng viên (người dùng tách/gộp, hoặc đổi chế độ quét):
/// - Tách: các ứng viên mới nằm bên trong `rel` nhận lại tag + notes của resource cũ.
/// - Gộp: resource cũ được gộp vào ứng viên mới chứa `rel`.
#[allow(clippy::too_many_arguments)]
fn regroup_source(
    tx: &Connection,
    sid: i64,
    rid: i64,
    rel: &str,
    sep: &str,
    cand_rels: &[&String],
    cand_rid: &HashMap<String, i64>,
    touched: &mut HashSet<i64>,
) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM match_suggestions WHERE source_a = ?1 OR source_b = ?1", [sid])?;
    tx.execute("DELETE FROM resource_sources WHERE id = ?1", [sid])?;
    let inner = format!("{rel}{sep}");
    let children: Vec<i64> = cand_rels
        .iter()
        .filter(|c| c.starts_with(&inner))
        .filter_map(|c| cand_rid.get(c.as_str()).copied())
        .filter(|c| *c != rid)
        .collect();
    let parent = cand_rels
        .iter()
        .filter(|c| rel.starts_with(&format!("{c}{sep}")))
        .max_by_key(|c| c.len())
        .and_then(|c| cand_rid.get(c.as_str()).copied());
    let exists = tx.query_row("SELECT 1 FROM resources WHERE id = ?1", [rid], |_| Ok(())).optional()?.is_some();
    if exists {
        if !children.is_empty() {
            let (notes,): (String,) = tx.query_row("SELECT notes FROM resources WHERE id = ?1", [rid], |r| Ok((r.get(0)?,)))?;
            for c in &children {
                tx.execute(
"INSERT OR IGNORE INTO resource_tags (resource_id, tag_id, source, reason, applied_at) SELECT ?1, tag_id, source, reason, applied_at FROM resource_tags WHERE resource_id = ?2",
                    params![c, rid],
                )?;
                if !notes.trim().is_empty() {
                    tx.execute(
                        "UPDATE resources SET notes = CASE WHEN trim(notes) = '' THEN ?1 ELSE notes || char(10) || char(10) || ?1 END
                         WHERE id = ?2 AND instr(notes, ?1) = 0",
                        params![notes, c],
                    )?;
                }
                touched.insert(*c);
            }
        } else if let Some(p) = parent {
            if p != rid {
                db::merge_into(tx, p, rid)?;
                touched.insert(p);
            }
        }
    }
    // Resource không còn nguồn nào -> xóa (metadata đã được chuyển đi ở trên)
    let left: i64 = tx.query_row("SELECT count(*) FROM resource_sources WHERE resource_id = ?1", [rid], |r| r.get(0)).unwrap_or(0);
    if left == 0 {
        tx.execute("DELETE FROM resources WHERE id = ?1", [rid])?;
        tx.execute("DELETE FROM resource_fts WHERE rowid = ?1", [rid])?;
        touched.remove(&rid);
    } else {
        touched.insert(rid);
    }
    Ok(())
}

/// Tính lại gợi ý type/app từ extension của tất cả nguồn + tên thư mục chứa.
pub fn refresh_suggestions(conn: &Connection, resource_id: i64) -> rusqlite::Result<()> {
    let mut total: HashMap<String, u64> = HashMap::new();
    let mut rels: Vec<String> = Vec::new();
    let mut s = conn.prepare("SELECT ext_summary, rel_path FROM resource_sources WHERE resource_id = ?1")?;
    for row in s.query_map([resource_id], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?)))? {
        let (ext, rel) = row?;
        for (k, v) in parse_ext_summary(ext) {
            *total.entry(k).or_default() += v;
        }
        rels.push(rel);
    }
    let mut sug = classify::suggest(&total);
    for rel in &rels {
        let (types, apps) = crate::autodetect::hints_from_path(rel);
        for t in types {
            if !sug.path_types.iter().any(|x| x == t) {
                sug.path_types.push(t.to_string());
            }
        }
        for a in apps {
            if !sug.path_apps.iter().any(|x| x == a) {
                sug.path_apps.push(a.to_string());
            }
        }
    }
    conn.execute(
        "UPDATE resources SET suggestions = ?1 WHERE id = ?2",
        params![serde_json::to_string(&sug).unwrap_or_default(), resource_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nink_test_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_zip(path: &Path, entries: &[&str]) {
        let f = fs::File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        for e in entries {
            z.start_file(*e, zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();
            std::io::Write::write_all(&mut z, b"x").unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn end_to_end_scan() {
        let root = tmpdir("e2e");
        fs::create_dir_all(root.join("FilmConvert Nitrate 3.52")).unwrap();
        fs::write(root.join("FilmConvert Nitrate 3.52").join("FilmConvert.ofx"), b"abc").unwrap();
        write_zip(&root.join("FilmConvert_Nitrate_v3.52.zip"), &["FilmConvert.ofx", "readme.txt"]);
        fs::create_dir_all(root.join("Whoosh SFX")).unwrap();
        fs::write(root.join("Whoosh SFX").join("a.wav"), b"a").unwrap();
        fs::write(root.join("Whoosh SFX").join("b.wav"), b"b").unwrap();
        write_zip(&root.join("Glitch Transitions Pack.zip"), &["a.mov"]);
        fs::create_dir_all(root.join("Glitch Transition Pack Vol 2")).unwrap();

        let conn = db::open(&tmpdir("e2e_db").join("t.db")).unwrap();
        conn.execute("INSERT INTO libraries (name, path, scan_depth, created_at) VALUES ('t', ?1, 1, 0)", [root.to_string_lossy()]).unwrap();
        let lib = conn.last_insert_rowid();
        let m = Mutex::new(conn);
        let r = scan_library(&m, lib, false, None, &|_| {}).unwrap();
        assert_eq!(r.found, 5);
        assert_eq!(r.added, 5);
        assert_eq!(r.auto_merged, 1);
        assert_eq!(r.suggested, 1);

        let conn = m.lock().unwrap();
        let resources: i64 = conn.query_row("SELECT count(*) FROM resources", [], |r| r.get(0)).unwrap();
        assert_eq!(resources, 4);
        let merged: i64 = conn
            .query_row("SELECT count(*) FROM resources WHERE name = 'FilmConvert Nitrate 3.52'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(merged, 1, "resource ghép phải mang tên folder");
        let sug: String = conn
            .query_row("SELECT suggestions FROM resources WHERE name = 'Whoosh SFX'", [], |r| r.get(0))
            .unwrap();
        assert!(sug.contains("SFX"));
        let hit: i64 = conn
            .query_row("SELECT count(*) FROM resource_fts WHERE resource_fts MATCH '\"film\"* \"convert\"*'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hit, 1);
        drop(conn);

        // Xóa folder -> source đánh dấu unavailable, metadata còn nguyên; file thật khác không bị đụng.
        fs::remove_dir_all(root.join("Whoosh SFX")).unwrap();
        let r2 = scan_library(&m, lib, false, None, &|_| {}).unwrap();
        assert_eq!(r2.missing, 1);
        assert_eq!(r2.added, 0);
        assert!(root.join("FilmConvert_Nitrate_v3.52.zip").exists());
        let conn = m.lock().unwrap();
        let resources: i64 = conn.query_row("SELECT count(*) FROM resources", [], |r| r.get(0)).unwrap();
        assert_eq!(resources, 4);
    }

    #[test]
    fn auto_mode_split_and_merge_keep_metadata() {
        let root = tmpdir("regroup");
        for f in ["Bundle/Pack A/a.cube", "Bundle/Pack A/b.cube", "Bundle/Pack B/c.cube", "Bundle/Pack B/d.cube", "Bundle/preview.jpg"] {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, b"x").unwrap();
        }
        let conn = db::open(&tmpdir("regroup_db").join("t.db")).unwrap();
        conn.execute("INSERT INTO libraries (name, path, scan_depth, created_at) VALUES ('t', ?1, 0, 0)", [root.to_string_lossy()]).unwrap();
        let m = Mutex::new(conn);
        scan_library(&m, 1, false, None, &|_| {}).unwrap();
        let names = |m: &Mutex<Connection>| -> Vec<String> {
            let c = m.lock().unwrap();
            let mut s = c.prepare("SELECT name FROM resources ORDER BY name").unwrap();
            let v = s.query_map([], |r| r.get(0)).unwrap().flatten().collect();
            v
        };
        // "Bundle" có ảnh preview + ≤4 thư mục con -> nhận là một gói
        assert_eq!(names(&m), vec!["Bundle"]);
        {
            let c = m.lock().unwrap();
            c.execute("UPDATE resources SET notes = 'Mua ở Envato'", []).unwrap();
            // bản quét đầu đã tự gắn LUT (nguồn: nội dung file); người dùng xác nhận -> thành tag tay
            c.execute("INSERT OR IGNORE INTO resource_tags (resource_id, tag_id) SELECT r.id, t.id FROM resources r, tags t WHERE t.name = 'LUT'", []).unwrap();
            let auto: Option<String> = c.query_row("SELECT source FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id WHERE t.name = 'LUT'", [], |r| r.get(0)).unwrap();
            assert_eq!(auto.as_deref(), Some("content"));
            c.execute("UPDATE resource_tags SET source = NULL", []).unwrap();
            c.execute("INSERT INTO scan_overrides VALUES (1, 'Bundle', 'container')", []).unwrap();
        }
        // Tách nhỏ -> 2 gói con + file preview, cùng nhận tag & notes của Bundle
        let r = scan_library(&m, 1, false, None, &|_| {}).unwrap();
        assert_eq!(r.regrouped, 1);
        assert_eq!(names(&m), vec!["Pack A", "Pack B", "preview"]);
        {
            let c = m.lock().unwrap();
            let tagged: i64 = c
                .query_row("SELECT count(*) FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id WHERE t.name = 'LUT'", [], |r| r.get(0))
                .unwrap();
            assert_eq!(tagged, 3);
            let notes: String = c.query_row("SELECT notes FROM resources WHERE name = 'Pack A'", [], |r| r.get(0)).unwrap();
            assert_eq!(notes, "Mua ở Envato");
            c.execute("UPDATE resources SET favorite = 1 WHERE name = 'Pack B'", []).unwrap();
            // Gộp lại: ép cả Bundle là một gói
            c.execute("UPDATE scan_overrides SET mode = 'pack'", []).unwrap();
        }
        let r = scan_library(&m, 1, false, None, &|_| {}).unwrap();
        assert!(r.regrouped >= 3);
        assert_eq!(names(&m), vec!["Bundle"]);
        let c = m.lock().unwrap();
        let fav: i64 = c.query_row("SELECT favorite FROM resources", [], |r| r.get(0)).unwrap();
        assert_eq!(fav, 1, "favorite của phần con được giữ khi gộp");
        assert!(root.join("Bundle/Pack A/a.cube").exists(), "không đụng file thật");
    }

    /// Ghi liên tục vào DB (giả lập MRM) để thử đồng thời với plugin: MRM_CONC_DB=<file> MRM_CONC_SECS=20
    #[test]
    #[ignore]
    fn concurrency_writer() {
        let Ok(p) = std::env::var("MRM_CONC_DB") else { return };
        let secs: u64 = std::env::var("MRM_CONC_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(20);
        let mut conn = db::open(Path::new(&p)).unwrap();
        if let Ok(src) = std::env::var("MRM_CONC_RESTORE") {
            // giả lập restore_backup: backup API ghi đè DB đang mở (WAL) rồi migrate
            conn.restore(rusqlite::MAIN_DB, Path::new(&src), None::<fn(rusqlite::backup::Progress)>).unwrap();
            db::migrate(&conn).unwrap();
            println!("restored from {src}");
        }
        conn.execute("INSERT OR IGNORE INTO libraries (id, name, path, created_at) VALUES (1, 'c', 'C:/x', 0)", []).unwrap();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        let mut i = 0i64;
        while std::time::Instant::now() < end {
            let tx = conn.unchecked_transaction().unwrap();
            for _ in 0..40 {
                i += 1;
                tx.execute(
                    "INSERT INTO media_assets (uid, library_id, media_type, rel_path, filename, ext, size, modified_ms, added_at, last_seen)
                     VALUES (?1, 1, 'audio', ?2, 'f.wav', 'wav', ?3, 0, 0, 0)",
                    rusqlite::params![format!("u{i}"), format!("dir/file_{i:08}_padding_padding_padding.wav"), i],
                )
                .unwrap();
            }
            tx.execute("INSERT INTO app_presence (app, last_seen) VALUES ('mrm', ?1) ON CONFLICT(app) DO UPDATE SET last_seen = excluded.last_seen", [i]).unwrap();
            tx.commit().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        let n: i64 = conn.query_row("SELECT count(*) FROM media_assets", [], |r| r.get(0)).unwrap();
        println!("rust writer done: rows={n}");
    }

    /// Tạo DB mẫu cho test tích hợp plugin: MRM_FIXTURE_DB=<file> MRM_FIXTURE_LIB=<dir> cargo test fixture_db -- --ignored
    #[test]
    #[ignore]
    fn fixture_db() {
        let (Ok(dbp), Ok(lib)) = (std::env::var("MRM_FIXTURE_DB"), std::env::var("MRM_FIXTURE_LIB")) else { return };
        let _ = fs::remove_file(&dbp);
        let conn = db::open(Path::new(&dbp)).unwrap();
        conn.execute("INSERT INTO libraries (name, path, scan_depth, created_at) VALUES ('fixture', ?1, 0, 0)", [&lib]).unwrap();
        let m = Mutex::new(conn);
        scan_library(&m, 1, false, None, &|_| {}).unwrap();
        let r = crate::media_index::index_library(&m, 1, None).unwrap();
        crate::media_index::probe_batch(&m, 500);
        println!("fixture: {r:?}");
    }

    /// Chạy thử trên thư viện thật (chỉ đọc), DB tạm: MRM_TEST_LIB=D:\path cargo test real_library -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_library() {
        let Ok(path) = std::env::var("MRM_TEST_LIB") else { return };
        let db_path = tmpdir("real_db").join("t.db");
        let conn = db::open(&db_path).unwrap();
        conn.execute("INSERT INTO libraries (name, path, scan_depth, created_at) VALUES ('real', ?1, 0, 0)", [&path]).unwrap();
        let m = Mutex::new(conn);
        let t = std::time::Instant::now();
        let r = scan_library(&m, 1, false, None, &|_| {}).unwrap();
        println!("scan: {:?} in {:.1}s", r, t.elapsed().as_secs_f64());
        let conn = m.lock().unwrap();
        let n: i64 = conn.query_row("SELECT count(*) FROM resources", [], |r| r.get(0)).unwrap();
        let multi: i64 = conn
            .query_row("SELECT count(*) FROM (SELECT resource_id FROM resource_sources GROUP BY resource_id HAVING count(*) > 1)", [], |r| r.get(0))
            .unwrap();
        println!("resources: {n}, zip+folder merged: {multi}");
        let with_app: i64 = conn
            .query_row("SELECT count(DISTINCT rt.resource_id) FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id WHERE t.kind = 'app'", [], |r| r.get(0))
            .unwrap();
        let with_type: i64 = conn
            .query_row("SELECT count(DISTINCT rt.resource_id) FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id WHERE t.kind = 'type'", [], |r| r.get(0))
            .unwrap();
        let apps: Vec<(String, i64)> = conn
            .prepare("SELECT t.name, count(*) FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id WHERE t.kind = 'app' GROUP BY t.id ORDER BY 2 DESC")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .flatten()
            .collect();
        println!("auto-tagged: app {with_app}/{n}, type {with_type}/{n}; apps: {apps:?}");
        drop(conn);
        let t = std::time::Instant::now();
        let mr = crate::media_index::index_library(&m, 1, None).unwrap();
        println!("media index: {:?} in {:.1}s", mr, t.elapsed().as_secs_f64());
        let t = std::time::Instant::now();
        let r2 = crate::media_index::index_library(&m, 1, None).unwrap();
        println!("media re-index: added {} updated {} in {:.1}s", r2.added, r2.updated, t.elapsed().as_secs_f64());
        let conn = m.lock().unwrap();
        let by: Vec<(String, i64)> = conn.prepare("SELECT media_type, count(*) FROM media_assets GROUP BY 1").unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().flatten().collect();
        let seqs: Vec<(String, i64)> = conn.prepare("SELECT rel_path, seq_count FROM media_assets WHERE seq_count IS NOT NULL ORDER BY seq_count DESC LIMIT 5").unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().flatten().collect();
        let orphan: i64 = conn.query_row("SELECT count(*) FROM media_assets WHERE resource_id IS NULL", [], |r| r.get(0)).unwrap();
        println!("by type {by:?}; assets without parent resource: {orphan}; top sequences {seqs:?}");
        let mut s = conn
            .prepare(
                "SELECT r.name, group_concat(s.kind || ':' || s.rel_path, ' + '), r.suggestions FROM resources r
                 JOIN resource_sources s ON s.resource_id = r.id GROUP BY r.id ORDER BY min(s.rel_path)",
            )
            .unwrap();
        let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))).unwrap();
        let out: Vec<String> = rows.flatten().map(|(n, src, sug)| format!("{n}  <=  {src}  ||  {}", sug.unwrap_or_default())).collect();
        std::fs::write(std::env::temp_dir().join("mrm_real_library.txt"), out.join("
")).unwrap();
    }
}
