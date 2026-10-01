//! Lập chỉ mục media cấp file (âm thanh / hình ảnh / video) cho Media Browser và plugin DaVinci.
//!
//! - Chỉ MRM quét; plugin đọc qua view `api_assets` trong cùng database.
//! - Định danh asset = id/uid ổn định; đường dẫn chỉ là địa chỉ. File đổi tên/di chuyển trong library
//!   được nhận lại (relink) nhờ size + thời điểm sửa, nên tag/favorite không mất.
//! - Chuỗi khung hình (frame_0001.png … frame_0240.png) được gom thành một asset.
//! - Ghi theo lô nhỏ (transaction ngắn) để plugin không bị chặn khi đang dùng.

use crate::db;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const AUDIO: &[&str] = &["wav", "mp3", "flac", "aiff", "aif", "m4a", "aac", "ogg"];
pub const IMAGE: &[&str] = &["jpg", "jpeg", "png", "bmp", "tif", "tiff", "webp", "gif"];
pub const VIDEO: &[&str] = &["mp4", "mov", "mxf", "mkv", "avi", "m4v", "webm"];
/// Số khung tối thiểu để coi là chuỗi ảnh.
const SEQ_MIN: usize = 10;
const BATCH: usize = 400;

pub fn kind_of(ext: &str) -> Option<&'static str> {
    let e = ext.to_ascii_lowercase();
    if AUDIO.contains(&e.as_str()) {
        Some("audio")
    } else if IMAGE.contains(&e.as_str()) {
        Some("image")
    } else if VIDEO.contains(&e.as_str()) {
        Some("video")
    } else {
        None
    }
}

#[derive(Debug, Clone)]
struct Found {
    rel: String,
    filename: String,
    ext: String,
    kind: &'static str,
    size: u64,
    mtime_ms: i64,
    seq: Option<(String, i64, i64, i64)>, // pattern, start, end, count
}

fn skip_name(name: &str) -> bool {
    let l = name.to_lowercase();
    l.starts_with('.') || l.starts_with("._") || l == "__macosx" || l == "$recycle.bin" || l == "system volume information"
}

fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Tách "frame_0012" -> ("frame_", "0012")
fn split_trailing_digits(stem: &str) -> Option<(&str, &str)> {
    let digits = stem.chars().rev().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits == stem.len() && digits < 3 {
        return None;
    }
    let cut = stem.len() - digits;
    Some((&stem[..cut], &stem[cut..]))
}

/// Duyệt library, trả về danh sách media (đã gom chuỗi khung hình).
fn walk(root: &Path, allowed: &HashSet<&str>, skip_dir: Option<&Path>) -> Vec<Found> {
    let mut files: Vec<Found> = Vec::new();
    let it = walkdir::WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| {
        if e.depth() == 0 {
            return true;
        }
        if skip_name(&e.file_name().to_string_lossy()) {
            return false;
        }
        !matches!(skip_dir, Some(s) if e.path().starts_with(s)) && !crate::exclude::hit(e.path())
    });
    for en in it.flatten() {
        if !en.file_type().is_file() {
            continue;
        }
        let filename = en.file_name().to_string_lossy().to_string();
        let Some((_, ext)) = filename.rsplit_once('.') else { continue };
        let ext = ext.to_ascii_lowercase();
        let Some(kind) = kind_of(&ext) else { continue };
        if !allowed.contains(kind) {
            continue;
        }
        let Ok(meta) = en.metadata() else { continue };
        let rel = en.path().strip_prefix(root).unwrap_or(en.path()).to_string_lossy().to_string();
        files.push(Found { rel, filename, ext, kind, size: meta.len(), mtime_ms: mtime_ms(&meta), seq: None });
    }
    group_sequences(files)
}

/// Gom các ảnh cùng thư mục, cùng tiền tố, cùng số chữ số và đuôi thành một asset "chuỗi khung hình".
fn group_sequences(files: Vec<Found>) -> Vec<Found> {
    let mut groups: HashMap<(String, String, String, usize), Vec<(i64, usize)>> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        if f.kind != "image" {
            continue;
        }
        let stem = &f.filename[..f.filename.len() - f.ext.len() - 1];
        if let Some((prefix, digits)) = split_trailing_digits(stem) {
            let dir = Path::new(&f.rel).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
            if let Ok(n) = digits.parse::<i64>() {
                groups.entry((dir, prefix.to_string(), f.ext.clone(), digits.len())).or_default().push((n, i));
            }
        }
    }
    let mut absorbed: HashSet<usize> = HashSet::new();
    let mut out: Vec<Found> = Vec::new();
    for ((_, prefix, ext, width), mut members) in groups {
        if members.len() < SEQ_MIN {
            continue;
        }
        members.sort();
        let (start, end) = (members[0].0, members[members.len() - 1].0);
        // cho phép thiếu vài khung, nhưng không gom các file đánh số rời rạc
        if (end - start + 1) as f64 > members.len() as f64 * 1.5 {
            continue;
        }
        let first = &files[members[0].1];
        let size: u64 = members.iter().map(|(_, i)| files[*i].size).sum();
        let mtime = members.iter().map(|(_, i)| files[*i].mtime_ms).max().unwrap_or(0);
        let pattern = format!("{prefix}%0{width}d.{ext}");
        out.push(Found { size, mtime_ms: mtime, seq: Some((pattern, start, end, members.len() as i64)), ..first.clone() });
        absorbed.extend(members.iter().map(|(_, i)| *i));
    }
    for (i, f) in files.into_iter().enumerate() {
        if !absorbed.contains(&i) {
            out.push(f);
        }
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

fn new_uid(seed: &str) -> String {
    use std::hash::{Hash, Hasher};
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut a = std::collections::hash_map::DefaultHasher::new();
    (seed, nanos, n, std::process::id()).hash(&mut a);
    let mut b = std::collections::hash_map::DefaultHasher::new();
    (n, nanos, seed, "mrm").hash(&mut b);
    format!("{:016x}{:016x}", a.finish(), b.finish())
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct MediaIndexResult {
    pub library_id: i64,
    pub found: usize,
    pub added: usize,
    pub updated: usize,
    pub relinked: usize,
    pub missing: usize,
    pub sequences: usize,
}

struct Existing {
    id: i64,
    size: i64,
    mtime: i64,
    available: bool,
    ext: String,
    filename: String,
}

/// Map asset -> resource cha: nguồn folder/file của resource chứa đường dẫn asset.
fn resource_map(conn: &Connection, lib_id: i64) -> rusqlite::Result<HashMap<String, i64>> {
    let mut s = conn.prepare("SELECT lower(rel_path), resource_id FROM resource_sources WHERE library_id = ?1 AND kind IN ('folder','file')")?;
    let v = s.query_map([lib_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?.collect();
    v
}

fn parent_resource(map: &HashMap<String, i64>, rel: &str) -> Option<i64> {
    let lower = rel.to_lowercase();
    let mut cur: &str = &lower;
    loop {
        if let Some(r) = map.get(cur) {
            return Some(*r);
        }
        match cur.rfind(['\\', '/']) {
            Some(i) => cur = &cur[..i],
            None => return None,
        }
    }
}

/// Lập chỉ mục media cho một library. An toàn khi gọi lặp lại (không nhân bản asset).
pub fn index_library(db: &Mutex<Connection>, lib_id: i64, skip_dir: Option<&Path>) -> Result<MediaIndexResult, String> {
    let err = |e: rusqlite::Error| e.to_string();
    let mut res = MediaIndexResult { library_id: lib_id, ..Default::default() };
    let (root, types, existing, rmap) = {
        let conn = db.lock().unwrap();
        crate::exclude::reload(&conn);
        let (path, types): (String, Option<String>) = conn
            .query_row("SELECT path, media_types FROM libraries WHERE id = ?1", [lib_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(err)?;
        let mut s = conn
            .prepare("SELECT id, rel_path, size, modified_ms, available, ext, filename FROM media_assets WHERE library_id = ?1")
            .map_err(err)?;
        let existing: HashMap<String, Existing> = s
            .query_map([lib_id], |r| {
                Ok((
                    r.get::<_, String>(1)?,
                    Existing { id: r.get(0)?, size: r.get(2)?, mtime: r.get(3)?, available: r.get::<_, i64>(4)? != 0, ext: r.get(5)?, filename: r.get(6)? },
                ))
            })
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;
        (PathBuf::from(path), types, existing, resource_map(&conn, lib_id).map_err(err)?)
    };
    if !root.is_dir() {
        // ổ offline: giữ nguyên metadata, chỉ đánh dấu unavailable
        let conn = db.lock().unwrap();
        res.missing = conn.execute("UPDATE media_assets SET available = 0 WHERE library_id = ?1 AND available = 1", [lib_id]).map_err(err)?;
        if res.missing > 0 {
            db::bump_media_rev(&conn).map_err(err)?;
        }
        return Ok(res);
    }
    let allowed: HashSet<&str> = match types.and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok()) {
        Some(v) => ["audio", "image", "video"].into_iter().filter(|k| v.iter().any(|x| x == k)).collect(),
        None => ["audio", "image", "video"].into_iter().collect(),
    };
    let found = walk(&root, &allowed, skip_dir);
    res.found = found.len();
    res.sequences = found.iter().filter(|f| f.seq.is_some()).count();

    let seen: HashSet<&str> = found.iter().map(|f| f.rel.as_str()).collect();
    // Ứng viên relink: asset cũ không còn thấy ở đường dẫn cũ, khóa theo (size, mtime, ext)
    let mut orphans: HashMap<(i64, i64, String), Vec<(String, i64, String)>> = HashMap::new();
    for (rel, e) in &existing {
        if !seen.contains(rel.as_str()) {
            orphans.entry((e.size, e.mtime, e.ext.clone())).or_default().push((rel.clone(), e.id, e.filename.clone()));
        }
    }
    let now = db::now();
    let mut relinked_ids: HashSet<i64> = HashSet::new();
    // File chuyển từ library khác sang (vd gom thư mục): nối lại để giữ tag / yêu thích
    let mut foreign = if found.iter().any(|f| f.size > 0 && !existing.contains_key(&f.rel)) {
        foreign_candidates(&db.lock().unwrap(), lib_id).map_err(err)?
    } else {
        HashMap::new()
    };
    for chunk in found.chunks(BATCH) {
        let mut conn = db.lock().unwrap();
        let tx = conn.transaction().map_err(err)?;
        for f in chunk {
            let rid = parent_resource(&rmap, &f.rel);
            let (pat, st, en, cnt) = match &f.seq {
                Some((p, s, e, c)) => (Some(p.clone()), Some(*s), Some(*e), Some(*c)),
                None => (None, None, None, None),
            };
            match existing.get(&f.rel) {
                Some(e) if e.size == f.size as i64 && e.mtime == f.mtime_ms => {
                    tx.execute(
                        "UPDATE media_assets SET last_seen = ?1, available = 1, resource_id = ?2, seq_count = coalesce(?3, seq_count) WHERE id = ?4",
                        params![now, rid, cnt, e.id],
                    )
                    .map_err(err)?;
                    if !e.available {
                        res.updated += 1;
                    }
                }
                Some(e) => {
                    tx.execute(
                        "UPDATE media_assets SET size = ?1, modified_ms = ?2, last_seen = ?3, available = 1, resource_id = ?4,
                         seq_pattern = ?5, seq_start = ?6, seq_end = ?7, seq_count = ?8, meta_state = 0 WHERE id = ?9",
                        params![f.size as i64, f.mtime_ms, now, rid, pat, st, en, cnt, e.id],
                    )
                    .map_err(err)?;
                    res.updated += 1;
                }
                None => {
                    // Đổi tên / di chuyển? Ưu tiên cùng tên file, sau đó cùng size + thời điểm sửa
                    let key = (f.size as i64, f.mtime_ms, f.ext.clone());
                    let cand = orphans.get_mut(&key).and_then(|v| {
                        let pos = v.iter().position(|x| x.2.eq_ignore_ascii_case(&f.filename)).or(if v.len() == 1 { Some(0) } else { None })?;
                        Some(v.remove(pos))
                    });
                    if let Some((_, id, _)) = cand.filter(|_| f.size > 0) {
                        tx.execute(
                            "UPDATE media_assets SET rel_path = ?1, filename = ?2, last_seen = ?3, available = 1, resource_id = ?4,
                             seq_pattern = ?5, seq_start = ?6, seq_end = ?7, seq_count = ?8 WHERE id = ?9",
                            params![f.rel, f.filename, now, rid, pat, st, en, cnt, id],
                        )
                        .map_err(err)?;
                        tx.execute("DELETE FROM asset_embeddings WHERE asset_id = ?1", [id]).map_err(err)?;
                        relinked_ids.insert(id);
                        res.relinked += 1;
                    } else if let Some(id) = (f.size > 0).then(|| take_foreign(&mut foreign, &key, &f.filename)).flatten() {
                        tx.execute(
                            "UPDATE media_assets SET library_id = ?1, rel_path = ?2, filename = ?3, last_seen = ?4, available = 1, resource_id = ?5,
                             seq_pattern = ?6, seq_start = ?7, seq_end = ?8, seq_count = ?9 WHERE id = ?10",
                            params![lib_id, f.rel, f.filename, now, rid, pat, st, en, cnt, id],
                        )
                        .map_err(err)?;
                        tx.execute("DELETE FROM asset_embeddings WHERE asset_id = ?1", [id]).map_err(err)?;
                        res.relinked += 1;
                    } else {
                        tx.execute(
                            "INSERT INTO media_assets (uid, library_id, resource_id, media_type, rel_path, filename, ext, size, modified_ms,
                             available, added_at, last_seen, seq_pattern, seq_start, seq_end, seq_count)
                             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,1,?10,?10,?11,?12,?13,?14)",
                            params![new_uid(&f.rel), lib_id, rid, f.kind, f.rel, f.filename, f.ext, f.size as i64, f.mtime_ms, now, pat, st, en, cnt],
                        )
                        .map_err(err)?;
                        res.added += 1;
                    }
                }
            }
        }
        tx.commit().map_err(err)?;
    }
    // Không còn thấy -> unavailable (giữ metadata, tag, favorite)
    {
        let conn = db.lock().unwrap();
        for (rel, e) in &existing {
            if !seen.contains(rel.as_str()) && e.available && !relinked_ids.contains(&e.id) {
                conn.execute("UPDATE media_assets SET available = 0 WHERE id = ?1", [e.id]).map_err(err)?;
                res.missing += 1;
            }
        }
        if res.added + res.updated + res.relinked + res.missing > 0 {
            db::bump_media_rev(&conn).map_err(err)?;
        }
    }
    Ok(res)
}

/// Asset ở library khác, khóa theo (size, mtime, ext): (id, gốc library, rel_path, tên file).
type Foreign = HashMap<(i64, i64, String), Vec<(i64, String, String, String)>>;

fn foreign_candidates(conn: &Connection, lib_id: i64) -> rusqlite::Result<Foreign> {
    let mut s = conn.prepare(
        "SELECT a.id, l.path, a.rel_path, a.filename, a.size, a.modified_ms, a.ext
         FROM media_assets a JOIN libraries l ON l.id = a.library_id
         WHERE a.library_id <> ?1 AND a.size > 0",
    )?;
    let mut map: Foreign = HashMap::new();
    let rows = s.query_map([lib_id], |r| {
        Ok(((r.get::<_, i64>(4)?, r.get::<_, i64>(5)?, r.get::<_, String>(6)?), (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
    })?;
    for row in rows {
        let (k, v) = row?;
        map.entry(k).or_default().push(v);
    }
    Ok(map)
}

/// Chọn asset ở library khác mà file mới này là bản đã di chuyển tới.
/// An toàn: chỉ nhận khi ổ của library cũ đang kết nối và file cũ thật sự không còn
/// (bản sao còn nguyên, hoặc ổ ngoài đang rút -> không lấy metadata).
fn take_foreign(map: &mut Foreign, key: &(i64, i64, String), filename: &str) -> Option<i64> {
    let v = map.get_mut(key)?;
    let moved = |(_, root, rel, _): &(i64, String, String, String)| {
        let root = Path::new(root);
        root.is_dir() && !root.join(rel).exists()
    };
    let same_name: Vec<usize> = (0..v.len()).filter(|&i| v[i].3.eq_ignore_ascii_case(filename) && moved(&v[i])).collect();
    let pos = match same_name.first() {
        Some(&i) => i,
        // khác tên: chỉ nhận khi không mơ hồ (đúng 1 ứng viên)
        None if v.len() == 1 && moved(&v[0]) => 0,
        None => return None,
    };
    Some(v.remove(pos).0)
}

// ================================================================ metadata (chạy nền)

/// Đọc header WAV: (thời lượng giây, sample rate, số kênh) — không cần FFmpeg.
pub fn wav_info(path: &Path) -> Option<(f64, u32, u16)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let mut hdr = [0u8; 12];
    f.read_exact(&mut hdr).ok()?;
    if &hdr[0..4] != b"RIFF" || &hdr[8..12] != b"WAVE" {
        return None;
    }
    let (mut rate, mut chans, mut byte_rate) = (0u32, 0u16, 0u32);
    loop {
        let mut ch = [0u8; 8];
        if f.read_exact(&mut ch).is_err() {
            return None;
        }
        let size = u32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]]);
        match &ch[0..4] {
            b"fmt " => {
                let mut b = vec![0u8; size as usize];
                f.read_exact(&mut b).ok()?;
                if b.len() < 16 {
                    return None;
                }
                chans = u16::from_le_bytes([b[2], b[3]]);
                rate = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
                byte_rate = u32::from_le_bytes([b[8], b[9], b[10], b[11]]);
                if size % 2 == 1 {
                    f.seek(SeekFrom::Current(1)).ok()?;
                }
            }
            b"data" => {
                if byte_rate == 0 {
                    return None;
                }
                return Some((size as f64 / byte_rate as f64, rate, chans));
            }
            _ => {
                f.seek(SeekFrom::Current(size as i64 + (size % 2) as i64)).ok()?;
            }
        }
    }
}

struct Pending {
    id: i64,
    path: PathBuf,
    kind: String,
    ext: String,
    seq_count: Option<i64>,
}

/// Đọc metadata cho tối đa `limit` asset đang chờ. Trả về số asset đã xử lý.
pub fn probe_batch(db: &Mutex<Connection>, limit: usize) -> usize {
    let pending: Vec<Pending> = {
        let conn = db.lock().unwrap();
        let Ok(mut s) = conn.prepare(
            "SELECT id, path, media_type, ext, seq_count FROM api_assets
             WHERE id IN (SELECT id FROM media_assets WHERE meta_state = 0 AND available = 1 LIMIT ?1)",
        ) else {
            return 0;
        };
        s.query_map([limit as i64], |r| {
            Ok(Pending { id: r.get(0)?, path: PathBuf::from(r.get::<_, String>(1)?), kind: r.get(2)?, ext: r.get(3)?, seq_count: r.get(4)? })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    };
    probe_pending(db, pending)
}

/// Đọc metadata ngay cho một asset (khi người dùng mở nó trong Media Browser).
pub fn probe_one(db: &Mutex<Connection>, id: i64) -> usize {
    let pending: Vec<Pending> = {
        let conn = db.lock().unwrap();
        conn.query_row(
            "SELECT id, path, media_type, ext, seq_count FROM api_assets WHERE id = ?1",
            [id],
            |r| Ok(Pending { id: r.get(0)?, path: PathBuf::from(r.get::<_, String>(1)?), kind: r.get(2)?, ext: r.get(3)?, seq_count: r.get(4)? }),
        )
        .map(|p| vec![p])
        .unwrap_or_default()
    };
    probe_pending(db, pending)
}

fn probe_pending(db: &Mutex<Connection>, pending: Vec<Pending>) -> usize {
    if pending.is_empty() {
        return 0;
    }
    let mut results: Vec<(i64, i64, Option<f64>, Option<u32>, Option<u32>, Option<f64>, Option<String>, Option<u32>, Option<u16>)> = Vec::new();
    let mut audio_only: Vec<i64> = Vec::new();
    for p in &pending {
        let mut r = (p.id, 2i64, None, None, None, None, None, None, None);
        match p.kind.as_str() {
            "image" => {
                if let Ok((w, h)) = image::image_dimensions(&p.path) {
                    r.1 = 1;
                    r.3 = Some(w);
                    r.4 = Some(h);
                    if let Some(c) = p.seq_count {
                        r.2 = Some(c as f64 / 25.0); // chuỗi ảnh: ước tính ở 25 fps
                    }
                }
            }
            "audio" if p.ext == "wav" => {
                if let Some((d, rate, ch)) = wav_info(&p.path) {
                    r = (p.id, 1, Some(d), None, None, None, Some("pcm".into()), Some(rate), Some(ch));
                }
            }
            _ => {}
        }
        if r.1 != 1 && p.kind != "image" {
            let m = crate::media::probe(&p.path);
            // .mov/.mp4 chỉ có tiếng (vd: SFX trong MediaFiles của DaVinci) -> thực chất là âm thanh
            if p.kind == "video" && m.video_codec.is_none() && m.audio_codec.is_some() && m.duration > 0.0 {
                audio_only.push(p.id);
            }
            if m.duration > 0.0 || m.width > 0 {
                r = (
                    p.id,
                    1,
                    Some(m.duration).filter(|d| *d > 0.0),
                    Some(m.width).filter(|w| *w > 0),
                    Some(m.height).filter(|h| *h > 0),
                    m.fps,
                    m.video_codec.or(m.audio_codec),
                    None,
                    None,
                );
            }
        }
        results.push(r);
    }
    let mut conn = db.lock().unwrap();
    if let Ok(tx) = conn.transaction() {
        for (id, state, dur, w, h, fps, codec, rate, ch) in &results {
            let _ = tx.execute(
                "UPDATE media_assets SET meta_state = ?1, duration = coalesce(?2, duration), width = coalesce(?3, width),
                 height = coalesce(?4, height), fps = coalesce(?5, fps), codec = coalesce(?6, codec),
                 sample_rate = coalesce(?7, sample_rate), channels = coalesce(?8, channels) WHERE id = ?9",
                params![state, dur, w, h, fps, codec, rate, ch, id],
            );
        }
        for id in &audio_only {
            let _ = tx.execute("UPDATE media_assets SET media_type = 'audio' WHERE id = ?1", [id]);
        }
        if results.iter().any(|r| r.1 == 1) {
            let _ = db::bump_media_rev(&tx);
        }
        let _ = tx.commit();
    }
    results.len()
}

/// Số asset theo loại (cho sidebar / dashboard).
#[allow(dead_code)]
pub fn counts(conn: &Connection) -> rusqlite::Result<HashMap<String, i64>> {
    let mut s = conn.prepare("SELECT media_type, count(*) FROM media_assets WHERE available = 1 GROUP BY media_type")?;
    let v = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?.collect();
    v
}

#[allow(dead_code)]
pub fn asset_id_by_uid(conn: &Connection, uid: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row("SELECT id FROM media_assets WHERE uid = ?1", [uid], |r| r.get(0)).optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mrm_media_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }
    fn write(p: &Path, bytes: &[u8]) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }
    fn wav(secs: u32) -> Vec<u8> {
        let (rate, ch, bits) = (8000u32, 1u16, 16u16);
        let data = rate * secs * 2;
        let mut v = Vec::new();
        v.extend(b"RIFF");
        v.extend((36 + data).to_le_bytes());
        v.extend(b"WAVEfmt ");
        v.extend(16u32.to_le_bytes());
        v.extend(1u16.to_le_bytes());
        v.extend(ch.to_le_bytes());
        v.extend(rate.to_le_bytes());
        v.extend((rate * 2).to_le_bytes());
        v.extend(2u16.to_le_bytes());
        v.extend(bits.to_le_bytes());
        v.extend(b"data");
        v.extend(data.to_le_bytes());
        v.extend(vec![0u8; data as usize]);
        v
    }

    #[test]
    fn relinks_files_moved_between_libraries() {
        let base = tmp("xlib");
        let (a, b) = (base.join("A"), base.join("B"));
        write(&a.join("SFX/boom.wav"), &wav(1));
        write(&a.join("SFX/copy.wav"), &wav(3));
        fs::create_dir_all(&b).unwrap();
        let conn = db::open(&tmp("xdb").join("t.db")).unwrap();
        for (n, p) in [("A", &a), ("B", &b)] {
            conn.execute("INSERT INTO libraries (name, path, created_at) VALUES (?1, ?2, 0)", params![n, p.to_string_lossy()]).unwrap();
        }
        let m = Mutex::new(conn);
        index_library(&m, 1, None).unwrap();
        m.lock().unwrap().execute("INSERT INTO asset_favorites SELECT id, 0 FROM media_assets", []).unwrap();
        let ids = |c: &Connection, f: &str| -> (i64, i64) { c.query_row("SELECT id, library_id FROM media_assets WHERE filename = ?1", [f], |r| Ok((r.get(0)?, r.get(1)?))).unwrap() };
        let (boom, _) = ids(&m.lock().unwrap(), "boom.wav");

        // di chuyển boom.wav sang B (giữ thời gian sửa như Explorer); copy.wav chỉ SAO CHÉP sang B
        write(&b.join("Gom/boom.wav"), &fs::read(a.join("SFX/boom.wav")).unwrap());
        write(&b.join("Gom/copy.wav"), &fs::read(a.join("SFX/copy.wav")).unwrap());
        for f in ["boom.wav", "copy.wav"] {
            let mt = fs::metadata(a.join("SFX").join(f)).unwrap().modified().unwrap();
            fs::File::options().write(true).open(b.join("Gom").join(f)).unwrap().set_modified(mt).unwrap();
        }
        fs::remove_file(a.join("SFX/boom.wav")).unwrap();

        let r = index_library(&m, 2, None).unwrap();
        assert_eq!((r.relinked, r.added), (1, 1), "{r:?}");
        let c = m.lock().unwrap();
        let (id, lib) = ids(&c, "boom.wav");
        assert_eq!((id, lib), (boom, 2), "cùng asset, đã sang library B");
        let fav: bool = c.query_row("SELECT favorite FROM api_assets WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert!(fav, "yêu thích đi theo file");
        let copies: i64 = c.query_row("SELECT count(*) FROM media_assets WHERE filename = 'copy.wav'", [], |r| r.get(0)).unwrap();
        assert_eq!(copies, 2, "bản sao: file gốc vẫn còn -> không lấy metadata");
    }

    #[test]
    fn index_sequences_relink_and_offline() {
        let root = tmp("lib");
        write(&root.join("SFX Pack/Whoosh/whoosh_01.wav"), &wav(2));
        write(&root.join("SFX Pack/Impact/heavy.wav"), &wav(1));
        for i in 1..=24 {
            write(&root.join(format!("GFX/Smoke/smoke_{i:04}.png")), b"png");
        }
        write(&root.join("GFX/cover.jpg"), b"jpg");
        write(&root.join("GFX/__MACOSX/._x.png"), b"junk");
        write(&root.join("Video/overlay.mp4"), b"mp4");
        write(&root.join("notes.txt"), b"x");
        // cache của app (luật loại trừ mặc định "User Data/Cache") -> không lập chỉ mục
        write(&root.join("CapCut/User Data/Cache/frame/abc.png"), b"png");
        write(&root.join("pack/node_modules/icon.png"), b"png");

        let conn = db::open(&tmp("db").join("t.db")).unwrap();
        conn.execute("INSERT INTO libraries (name, path, created_at) VALUES ('t', ?1, 0)", [root.to_string_lossy()]).unwrap();
        // resource cha "SFX Pack"
        conn.execute("INSERT INTO resources (name, created_at, updated_at) VALUES ('SFX Pack', 0, 0)", []).unwrap();
        conn.execute(
            "INSERT INTO resource_sources (resource_id, library_id, rel_path, name, kind, first_seen, last_seen) VALUES (1, 1, 'SFX Pack', 'SFX Pack', 'folder', 0, 0)",
            [],
        )
        .unwrap();
        let m = Mutex::new(conn);

        let r = index_library(&m, 1, None).unwrap();
        assert_eq!(r.found, 5, "{r:?}"); // 2 wav + 1 chuỗi smoke + cover.jpg + overlay.mp4
        assert_eq!(r.sequences, 1);
        assert_eq!(r.added, 5);
        {
            let c = m.lock().unwrap();
            let (pat, cnt): (String, i64) =
                c.query_row("SELECT seq_pattern, seq_count FROM media_assets WHERE seq_pattern IS NOT NULL", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            assert_eq!((pat.as_str(), cnt), ("smoke_%04d.png", 24));
            let parent: Option<String> = c
                .query_row("SELECT resource_name FROM api_assets WHERE filename = 'whoosh_01.wav'", [], |r| r.get(0))
                .unwrap();
            assert_eq!(parent.as_deref(), Some("SFX Pack"));
            // favorite + tag trên whoosh
            c.execute("INSERT INTO asset_favorites SELECT id, 0 FROM media_assets WHERE filename = 'whoosh_01.wav'", []).unwrap();
        }

        // chạy lại: không nhân bản
        let r = index_library(&m, 1, None).unwrap();
        assert_eq!((r.added, r.updated, r.missing), (0, 0, 0));

        // đổi tên file -> giữ nguyên asset (favorite còn)
        fs::rename(root.join("SFX Pack/Whoosh/whoosh_01.wav"), root.join("SFX Pack/Whoosh/whoosh_fast.wav")).unwrap();
        let r = index_library(&m, 1, None).unwrap();
        assert_eq!(r.relinked, 1);
        assert_eq!(r.added, 0);
        {
            let c = m.lock().unwrap();
            let fav: bool = c.query_row("SELECT favorite FROM api_assets WHERE filename = 'whoosh_fast.wav'", [], |r| r.get(0)).unwrap();
            assert!(fav, "favorite giữ nguyên sau khi đổi tên");
        }

        // metadata nền: đọc WAV header
        assert!(probe_batch(&m, 50) > 0);
        {
            let c = m.lock().unwrap();
            let (d, rate): (f64, i64) =
                c.query_row("SELECT duration, sample_rate FROM media_assets WHERE filename = 'heavy.wav'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            assert!((d - 1.0).abs() < 0.01);
            assert_eq!(rate, 8000);
        }

        // ổ offline -> unavailable, metadata còn
        fs::remove_dir_all(&root).unwrap();
        let r = index_library(&m, 1, None).unwrap();
        assert_eq!(r.missing, 5);
        let c = m.lock().unwrap();
        let n: i64 = c.query_row("SELECT count(*) FROM media_assets", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 5);
    }
}
