//! SQLite schema, seed data và các helper dùng chung.

use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub const SCHEMA_VERSION: i64 = 12;
/// Phiên bản "hợp đồng" dữ liệu giữa MRM và plugin DaVinci (các view api_* và bảng asset_*).
/// Tăng khi thay đổi làm plugin cũ đọc/ghi sai.
pub const PLUGIN_CONTRACT: i64 = 1;

pub const SEED_APPS: &[&str] = &[
    "DaVinci Resolve", "After Effects", "Premiere Pro", "CapCut", "Photoshop", "Blender", "OBS", "Standalone",
];

pub const SEED_TYPES: &[&str] = &[
    "Plugin", "Effect", "Transition", "LUT", "PowerGrade", "DCTL", "Macro", "Title", "Fusion", "SFX", "Music",
    "Font", "Overlay", "Texture", "Stock Video", "Stock Image", "Template", "Preset", "Script", "Brush",
    "3D Model", "Installer", "Project File", "Other",
];

pub const SEED_STATUSES: &[&str] = &["Downloaded", "Extracted", "Installed", "Tested", "Archived"];

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;",
    )?;
    migrate(&conn)?;
    Ok(conn)
}

/// Kết quả kiểm tra sức khỏe DB: None = ổn, Some(lý do) = hỏng.
fn health(path: &Path) -> Option<String> {
    use rusqlite::{ErrorCode, OpenFlags};
    let is_corrupt = |e: &rusqlite::Error| {
        matches!(e.sqlite_error_code(), Some(ErrorCode::DatabaseCorrupt) | Some(ErrorCode::NotADatabase))
    };
    let conn = match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(c) => c,
        Err(e) if is_corrupt(&e) => return Some(e.to_string()),
        Err(_) => return None, // lỗi tạm thời (bị khóa…) — không coi là hỏng
    };
    let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
    match conn.query_row("PRAGMA quick_check(1)", [], |r| r.get::<_, String>(0)) {
        Ok(s) if s == "ok" => conn
            .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
            .err()
            .filter(is_corrupt)
            .map(|e| e.to_string()),
        Ok(s) => Some(s),
        Err(e) if is_corrupt(&e) => Some(e.to_string()),
        Err(_) => None,
    }
}

/// Mở DB, nếu phát hiện hỏng thì: chuyển bộ file hỏng sang `corrupt-<thời gian>` (giữ nguyên, không xóa),
/// chép bản backup mới nhất còn tốt vào chỗ cũ. Trả về thông báo để giao diện hiển thị.
pub fn open_safe(path: &Path, backup_dir: &Path) -> rusqlite::Result<(Connection, Option<String>)> {
    let mut notice = None;
    if path.exists() {
        if let Some(reason) = health(path) {
            let dir = path.parent().unwrap_or(Path::new(".")).join(format!("corrupt-{}", now()));
            let _ = std::fs::create_dir_all(&dir);
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if let Err(err) = std::fs::rename(path, dir.join(&name)) {
                // file đang bị chương trình khác giữ (vd: plugin trong DaVinci) -> không đụng vào
                let _ = std::fs::remove_dir(&dir);
                return Err(rusqlite::Error::InvalidPath(std::path::PathBuf::from(format!(
                    "Database bị hỏng ({reason}) nhưng đang bị chương trình khác mở ({err}). Hãy đóng DaVinci Resolve rồi mở lại MRM để tự khôi phục."
                ))));
            }
            for suffix in ["-wal", "-shm"] {
                let f = path.with_file_name(format!("{name}{suffix}"));
                if f.exists() {
                    let _ = std::fs::rename(&f, dir.join(format!("{name}{suffix}")));
                }
            }
            let mut backups: Vec<(std::time::SystemTime, std::path::PathBuf)> = std::fs::read_dir(backup_dir)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| e.path().extension().map(|x| x == "db").unwrap_or(false))
                        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
                        .collect()
                })
                .unwrap_or_default();
            backups.sort_by(|a, b| b.0.cmp(&a.0));
            let good = backups.into_iter().map(|(_, p)| p).find(|p| {
                health(p).is_none()
                    && Connection::open_with_flags(p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                        .and_then(|c| c.query_row("SELECT count(*) FROM resources", [], |r| r.get::<_, i64>(0)))
                        .is_ok()
            });
            let kept = dir.to_string_lossy().to_string();
            notice = Some(match good.and_then(|b| std::fs::copy(&b, path).ok().map(|_| b)) {
                Some(b) => format!(
                    "Database bị hỏng ({reason}). MRM đã tự khôi phục từ bản sao lưu {}. Bộ file hỏng được giữ tại {kept}. Hãy quét lại thư viện để cập nhật những thay đổi sau thời điểm sao lưu.",
                    b.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                ),
                None => format!("Database bị hỏng ({reason}) và không có bản sao lưu dùng được — MRM đã tạo database mới. Bộ file hỏng được giữ tại {kept}."),
            });
        }
    }
    let conn = open(path)?;
    Ok((conn, notice))
}

/// Gộp WAL vào file chính (không chờ người đọc khác) — giữ WAL nhỏ, giảm rủi ro khi mất điện/crash.
pub fn checkpoint(conn: &Connection, truncate: bool) {
    let sql = if truncate { "PRAGMA wal_checkpoint(TRUNCATE)" } else { "PRAGMA wal_checkpoint(PASSIVE)" };
    let _ = conn.query_row(sql, [], |_| Ok(()));
}

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(
            r#"
            BEGIN;
            CREATE TABLE libraries (
                id            INTEGER PRIMARY KEY,
                name          TEXT NOT NULL,
                path          TEXT NOT NULL UNIQUE,
                volume_serial INTEGER,
                scan_depth    INTEGER NOT NULL DEFAULT 1,
                created_at    INTEGER NOT NULL,
                last_scan_at  INTEGER
            );

            CREATE TABLE resources (
                id          INTEGER PRIMARY KEY,
                name        TEXT NOT NULL,
                notes       TEXT NOT NULL DEFAULT '',
                favorite    INTEGER NOT NULL DEFAULT 0,
                version     TEXT,
                -- gợi ý rule-based (JSON {types:[],apps:[]}), chỉ để hiển thị
                suggestions TEXT,
                created_at  INTEGER NOT NULL,
                updated_at  INTEGER NOT NULL
            );

            -- Nguồn vật lý của resource. Đường dẫn lưu tương đối so với library
            -- để khi ổ đổi ký tự chỉ cần cập nhật libraries.path.
            CREATE TABLE resource_sources (
                id           INTEGER PRIMARY KEY,
                resource_id  INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
                library_id   INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
                rel_path     TEXT NOT NULL,
                name         TEXT NOT NULL,
                kind         TEXT NOT NULL CHECK (kind IN ('folder','archive','file')),
                size         INTEGER NOT NULL DEFAULT 0,
                file_count   INTEGER NOT NULL DEFAULT 0,
                modified_at  INTEGER,
                ext_summary  TEXT,
                available    INTEGER NOT NULL DEFAULT 1,
                -- auto | confirmed | single | split
                match_state  TEXT NOT NULL DEFAULT 'single',
                match_score  REAL,
                first_seen   INTEGER NOT NULL,
                last_seen    INTEGER NOT NULL,
                UNIQUE (library_id, rel_path)
            );
            CREATE INDEX idx_sources_resource ON resource_sources(resource_id);

            -- kind: app | type | function | personal | status
            CREATE TABLE tags (
                id    INTEGER PRIMARY KEY,
                kind  TEXT NOT NULL,
                name  TEXT NOT NULL,
                color TEXT,
                UNIQUE (kind, name COLLATE NOCASE)
            );

            CREATE TABLE resource_tags (
                resource_id INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
                tag_id      INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                PRIMARY KEY (resource_id, tag_id)
            );
            CREATE INDEX idx_resource_tags_tag ON resource_tags(tag_id);

            CREATE TABLE collections (
                id         INTEGER PRIMARY KEY,
                name       TEXT NOT NULL UNIQUE,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE collection_resources (
                collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
                resource_id   INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
                PRIMARY KEY (collection_id, resource_id)
            );

            -- Cặp nguồn có thể là cùng một tài nguyên (75–94%), chờ người dùng xác nhận.
            CREATE TABLE match_suggestions (
                id         INTEGER PRIMARY KEY,
                source_a   INTEGER NOT NULL REFERENCES resource_sources(id) ON DELETE CASCADE,
                source_b   INTEGER NOT NULL REFERENCES resource_sources(id) ON DELETE CASCADE,
                score      REAL NOT NULL,
                state      TEXT NOT NULL DEFAULT 'pending', -- pending | accepted | rejected
                created_at INTEGER NOT NULL,
                UNIQUE (source_a, source_b)
            );

            CREATE TABLE scan_history (
                id          INTEGER PRIMARY KEY,
                library_id  INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
                started_at  INTEGER NOT NULL,
                finished_at INTEGER,
                found       INTEGER NOT NULL DEFAULT 0,
                added       INTEGER NOT NULL DEFAULT 0,
                missing     INTEGER NOT NULL DEFAULT 0,
                auto_merged INTEGER NOT NULL DEFAULT 0,
                suggested   INTEGER NOT NULL DEFAULT 0,
                error       TEXT
            );

            CREATE VIRTUAL TABLE resource_fts USING fts5(
                name, sources, tags, notes,
                tokenize = 'unicode61 remove_diacritics 2'
            );
            COMMIT;
            "#,
        )?;
        seed(conn)?;
        conn.execute_batch("PRAGMA user_version = 1;")?;
    }
    if version < 2 {
        conn.execute_batch(
            r#"
            BEGIN;
            CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);

            -- Ảnh bìa: JSON {source_id, path} | {"none":true}; cover_user = 1 nếu người dùng tự chọn
            ALTER TABLE resources ADD COLUMN cover TEXT;
            ALTER TABLE resources ADD COLUMN cover_user INTEGER NOT NULL DEFAULT 0;

            -- Chữ ký nội dung: hash danh sách (đường dẫn tương đối, size) — nhận ra ZIP/folder cùng nội dung
            ALTER TABLE resource_sources ADD COLUMN content_sig TEXT;
            ALTER TABLE resource_sources ADD COLUMN content_size INTEGER;
            CREATE INDEX idx_sources_sig ON resource_sources(content_sig);

            -- kind: addon_of | requires | alternative | related  (a -> b)
            CREATE TABLE resource_relations (
                a          INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
                b          INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
                kind       TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (a, b, kind)
            );

            -- Luật phân loại do người dùng định nghĩa
            CREATE TABLE rules (
                id         INTEGER PRIMARY KEY,
                name       TEXT NOT NULL,
                enabled    INTEGER NOT NULL DEFAULT 1,
                auto_apply INTEGER NOT NULL DEFAULT 0,
                match_all  INTEGER NOT NULL DEFAULT 1,
                conditions TEXT NOT NULL,   -- JSON [{field, op, value}]
                tag_ids    TEXT NOT NULL,   -- JSON [tag_id]
                created_at INTEGER NOT NULL
            );

            -- Cặp resource người dùng xác nhận "không phải trùng"
            CREATE TABLE duplicate_dismissed (
                a INTEGER NOT NULL, b INTEGER NOT NULL, PRIMARY KEY (a, b)
            );

            CREATE TABLE resource_embeddings (
                resource_id INTEGER PRIMARY KEY REFERENCES resources(id) ON DELETE CASCADE,
                model       TEXT NOT NULL,
                text_hash   TEXT NOT NULL,
                vector      BLOB NOT NULL,
                embedded_at INTEGER NOT NULL DEFAULT 0
            );

            -- Kết quả phân tích AI chờ người dùng xác nhận
            CREATE TABLE ai_suggestions (
                resource_id INTEGER PRIMARY KEY REFERENCES resources(id) ON DELETE CASCADE,
                data        TEXT NOT NULL,
                created_at  INTEGER NOT NULL
            );
            COMMIT;
            "#,
        )?;
        conn.execute("INSERT OR IGNORE INTO tags (kind, name) VALUES ('status', 'Update Available')", [])?;
        conn.execute_batch("PRAGMA user_version = 2;")?;
    }
    if version < 3 {
        conn.execute_batch(
            r#"
            BEGIN;
            -- Chế độ quét Tự động: người dùng ép một thư mục là 'pack' (một resource) hoặc 'container' (tách con)
            CREATE TABLE scan_overrides (
                library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
                rel_path   TEXT NOT NULL,
                mode       TEXT NOT NULL CHECK (mode IN ('pack','container')),
                PRIMARY KEY (library_id, rel_path)
            );
            INSERT OR IGNORE INTO tags (kind, name) VALUES ('app', 'Lightroom');
            COMMIT;
            "#,
        )?;
        conn.execute_batch("PRAGMA user_version = 3;")?;
    }
    if version < 4 {
        conn.execute_batch(
            r#"
            BEGIN;
            -- Ảnh minh họa trong Notes (file JPEG trong thư mục dữ liệu của app)
            CREATE TABLE note_images (
                id          INTEGER PRIMARY KEY,
                resource_id INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
                file        TEXT NOT NULL,
                position    INTEGER NOT NULL DEFAULT 0,
                created_at  INTEGER NOT NULL
            );
            CREATE INDEX idx_note_images_resource ON note_images(resource_id);
            COMMIT;
            "#,
        )?;
        conn.execute_batch("PRAGMA user_version = 4;")?;
    }
    if version < 5 {
        // thời điểm kết quả AI được áp dụng — để lọc lại (is:ai) và rà soát sau
        conn.execute_batch("ALTER TABLE resources ADD COLUMN ai_applied_at INTEGER;")?;
        conn.execute_batch("PRAGMA user_version = 5;")?;
    }
    if version < 6 {
        conn.execute_batch(
            r#"
            BEGIN;
            -- Nguồn gốc tag: NULL = người dùng gắn/đã xác nhận; còn lại = tự động (content/path/rule/learned/ai/scan)
            ALTER TABLE resource_tags ADD COLUMN source TEXT;
            ALTER TABLE resource_tags ADD COLUMN reason TEXT;
            ALTER TABLE resource_tags ADD COLUMN applied_at INTEGER;
            -- Tag người dùng đã gỡ -> không tự gắn lại
            CREATE TABLE tag_rejections (
                resource_id INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
                tag_id      INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                created_at  INTEGER NOT NULL,
                PRIMARY KEY (resource_id, tag_id)
            );
            -- Tag trạng thái máy quét đã gắn trước đây
            UPDATE resource_tags SET source = 'scan'
             WHERE tag_id IN (SELECT id FROM tags WHERE kind = 'status' AND name IN ('Downloaded','Extracted'));
            -- Tag do AI điền trước đây (resource có ai_applied_at) chưa rõ tag nào -> để nguyên là tag tay
            COMMIT;
            "#,
        )?;
        conn.execute_batch("PRAGMA user_version = 6;")?;
    }
    if version < 7 {
        conn.execute_batch(MEDIA_SCHEMA)?;
        conn.execute_batch("PRAGMA user_version = 7;")?;
    }
    if version < 8 {
        // video chưa có kích thước hình có thể là file chỉ có tiếng -> đọc lại metadata để phân loại lại
        conn.execute_batch(
            "UPDATE media_assets SET meta_state = 0 WHERE media_type = 'video' AND width IS NULL;
             PRAGMA user_version = 8;",
        )?;
    }
    if version < 9 {
        // Phase 5: AI cho file media — embedding + danh mục AI (gợi ý, không phải tag)
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE asset_embeddings (
                 asset_id    INTEGER PRIMARY KEY REFERENCES media_assets(id) ON DELETE CASCADE,
                 model       TEXT NOT NULL,
                 text_hash   TEXT NOT NULL,
                 vector      BLOB NOT NULL,
                 embedded_at INTEGER NOT NULL
             );
             ALTER TABLE media_assets ADD COLUMN ai_category TEXT;
             ALTER TABLE media_assets ADD COLUMN ai_score REAL;
             CREATE INDEX idx_assets_ai_category ON media_assets(ai_category);
             PRAGMA user_version = 9;
             COMMIT;",
        )?;
    }
    if version < 10 {
        // AI nhìn ảnh/video (tùy chọn): câu mô tả nội dung hình; state 1 = xong, 2 = không đọc được file
        conn.execute_batch(
            "CREATE TABLE asset_captions (
                 asset_id   INTEGER PRIMARY KEY REFERENCES media_assets(id) ON DELETE CASCADE,
                 model      TEXT NOT NULL,
                 caption    TEXT,
                 state      INTEGER NOT NULL,
                 created_at INTEGER NOT NULL
             );
             PRAGMA user_version = 10;",
        )?;
    }
    if version < 11 {
        // Xem được bên trong RAR/7Z -> tìm lại ảnh bìa tự động cho resource trước đây "không có ảnh bìa"
        conn.execute_batch(
            "UPDATE resources SET cover = NULL WHERE cover_user = 0 AND cover LIKE '%\"none\"%';
             PRAGMA user_version = 11;",
        )?;
    }
    if version < 12 {
        // Thư mục yêu thích / ghim trong Media Browser
        conn.execute_batch(
            "CREATE TABLE favorite_folders (
                 id         INTEGER PRIMARY KEY,
                 library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
                 path       TEXT NOT NULL,
                 name       TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 UNIQUE (library_id, path)
             );
             PRAGMA user_version = 12;",
        )?;
    }
    set_setting(conn, "plugin_contract", &PLUGIN_CONTRACT.to_string())?;
    Ok(())
}

/// Shared Core cho Media Browser (MRM) + HNH SFX Finder (plugin DaVinci).
/// Nguyên tắc: một database duy nhất; plugin đọc qua view `api_*`, ghi vào bảng asset_* bằng transaction ngắn.
const MEDIA_SCHEMA: &str = r#"
BEGIN;
-- Loại media library cho phép lập chỉ mục (JSON ["audio","image","video"]; NULL = cả ba)
ALTER TABLE libraries ADD COLUMN media_types TEXT;

-- Asset cấp file (âm thanh / hình ảnh / video). Đường dẫn chỉ là "địa chỉ"; định danh là id/uid.
CREATE TABLE media_assets (
    id           INTEGER PRIMARY KEY,
    uid          TEXT NOT NULL UNIQUE,
    library_id   INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    resource_id  INTEGER REFERENCES resources(id) ON DELETE SET NULL,
    media_type   TEXT NOT NULL CHECK (media_type IN ('audio','image','video')),
    rel_path     TEXT NOT NULL,
    filename     TEXT NOT NULL,
    ext          TEXT NOT NULL,
    size         INTEGER NOT NULL,
    modified_ms  INTEGER NOT NULL,
    available    INTEGER NOT NULL DEFAULT 1,
    added_at     INTEGER NOT NULL,
    last_seen    INTEGER NOT NULL,
    -- chuỗi khung hình (frame_0001.png ... frame_0240.png) gom thành một asset
    seq_pattern  TEXT,
    seq_start    INTEGER,
    seq_end      INTEGER,
    seq_count    INTEGER,
    -- metadata đọc sau (nền): 0 = chờ, 1 = xong, 2 = lỗi
    meta_state   INTEGER NOT NULL DEFAULT 0,
    duration     REAL,
    width        INTEGER,
    height       INTEGER,
    fps          REAL,
    codec        TEXT,
    sample_rate  INTEGER,
    channels     INTEGER,
    UNIQUE (library_id, rel_path)
);
CREATE INDEX idx_assets_type ON media_assets(media_type, available);
CREATE INDEX idx_assets_resource ON media_assets(resource_id);
CREATE INDEX idx_assets_identity ON media_assets(size, modified_ms);

CREATE TABLE asset_tags (
    asset_id   INTEGER NOT NULL REFERENCES media_assets(id) ON DELETE CASCADE,
    tag_id     INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    source     TEXT,
    reason     TEXT,
    applied_at INTEGER,
    PRIMARY KEY (asset_id, tag_id)
);
CREATE INDEX idx_asset_tags_tag ON asset_tags(tag_id);

CREATE TABLE asset_favorites (
    asset_id   INTEGER PRIMARY KEY REFERENCES media_assets(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL
);

-- Recent + số lần dùng (preview/import/insert)
CREATE TABLE asset_usage (
    asset_id     INTEGER PRIMARY KEY REFERENCES media_assets(id) ON DELETE CASCADE,
    last_used_at INTEGER NOT NULL,
    use_count    INTEGER NOT NULL DEFAULT 0
);

-- Smart collection = bộ lọc đã lưu (định dạng filters giữ nguyên của plugin)
CREATE TABLE asset_smart_collections (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    filters    TEXT NOT NULL,
    position   INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);

-- Collection thường (MRM) có thể chứa cả asset
CREATE TABLE collection_assets (
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    asset_id      INTEGER NOT NULL REFERENCES media_assets(id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, asset_id)
);

-- Nhịp tim ứng dụng: plugin DaVinci ghi mỗi vài giây khi đang mở
CREATE TABLE app_presence (
    app       TEXT PRIMARY KEY,
    pid       INTEGER,
    version   TEXT,
    detail    TEXT,
    last_seen INTEGER NOT NULL
);

-- Yêu cầu từ plugin gửi MRM (vd: quét lại thư viện)
CREATE TABLE app_requests (
    id         INTEGER PRIMARY KEY,
    app        TEXT NOT NULL,
    kind       TEXT NOT NULL,
    payload    TEXT,
    created_at INTEGER NOT NULL,
    handled_at INTEGER,
    result     TEXT
);

-- ================= Hợp đồng đọc cho plugin (không đọc thẳng bảng gốc) =================
CREATE VIEW api_libraries AS
SELECT l.id, l.name, l.path, coalesce(l.media_types, '["audio","image","video"]') AS media_types
FROM libraries l;

CREATE VIEW api_assets AS
SELECT a.id, a.uid, a.library_id, l.path AS root, a.rel_path,
       CASE WHEN substr(l.path, -1) IN ('\', '/') THEN l.path || a.rel_path ELSE l.path || '\' || a.rel_path END AS path,
       a.filename, a.ext, a.media_type, a.size, a.modified_ms, a.available, a.added_at,
       a.seq_pattern, a.seq_start, a.seq_end, a.seq_count,
       a.duration, a.width, a.height, a.fps, a.codec, a.sample_rate, a.channels,
       a.resource_id, r.name AS resource_name,
       (SELECT 1 FROM asset_favorites f WHERE f.asset_id = a.id) IS NOT NULL AS favorite,
       u.last_used_at, coalesce(u.use_count, 0) AS use_count
FROM media_assets a
JOIN libraries l ON l.id = a.library_id
LEFT JOIN resources r ON r.id = a.resource_id
LEFT JOIN asset_usage u ON u.asset_id = a.id;

CREATE VIEW api_asset_tags AS
SELECT at.asset_id, t.id AS tag_id, t.name, t.kind, at.source
FROM asset_tags at JOIN tags t ON t.id = at.tag_id;
COMMIT;
"#;

/// Tăng bộ đếm nội dung media — plugin DaVinci chỉ tải lại index khi số này đổi.
pub fn bump_media_rev(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('media_rev', '1')
         ON CONFLICT(key) DO UPDATE SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)",
        [],
    )?;
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional().ok().flatten()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

fn seed(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare("INSERT OR IGNORE INTO tags (kind, name) VALUES (?1, ?2)")?;
    for a in SEED_APPS {
        stmt.execute(params!["app", a])?;
    }
    for t in SEED_TYPES {
        stmt.execute(params!["type", t])?;
    }
    for s in SEED_STATUSES {
        stmt.execute(params!["status", s])?;
    }
    Ok(())
}

pub fn tag_id(conn: &Connection, kind: &str, name: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM tags WHERE kind = ?1 AND name = ?2 COLLATE NOCASE",
        params![kind, name],
        |r| r.get(0),
    )
    .optional()
}

/// Cập nhật dòng full-text search cho một resource (rowid = resource id).
pub fn reindex(conn: &Connection, resource_id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM resource_fts WHERE rowid = ?1", [resource_id])?;
    let row: Option<(String, String)> = conn
        .query_row("SELECT name, notes FROM resources WHERE id = ?1", [resource_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    let Some((name, notes)) = row else { return Ok(()) };

    let mut sources = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT s.name, s.rel_path, l.name FROM resource_sources s JOIN libraries l ON l.id = s.library_id
         WHERE s.resource_id = ?1",
    )?;
    let rows = stmt.query_map([resource_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
    for row in rows {
        let (sname, rel, lib) = row?;
        // thêm dạng đã tách camelCase để "motion bro" tìm được "MotionBro"
        let norm = crate::normalize::normalize(&sname, false).display;
        sources.push(format!("{sname} {norm} {rel} {lib}"));
    }
    let name_norm = crate::normalize::normalize(&name, false).display;

    let mut tags = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT t.name FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id WHERE rt.resource_id = ?1",
    )?;
    for t in stmt.query_map([resource_id], |r| r.get::<_, String>(0))? {
        tags.push(t?);
    }

    conn.execute(
        "INSERT INTO resource_fts (rowid, name, sources, tags, notes) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![resource_id, format!("{name} {name_norm}"), sources.join("\n"), tags.join(" "), notes],
    )?;
    Ok(())
}

pub fn touch(conn: &Connection, resource_id: i64) -> rusqlite::Result<()> {
    conn.execute("UPDATE resources SET updated_at = ?1 WHERE id = ?2", params![now(), resource_id])?;
    Ok(())
}

/// Ghép `from` vào `into`: chuyển source, hợp tag/collection, nối notes, xóa `from`.
pub fn merge_into(conn: &Connection, into: i64, from: i64) -> rusqlite::Result<()> {
    if into == from {
        return Ok(());
    }
    conn.execute("UPDATE resource_sources SET resource_id = ?1 WHERE resource_id = ?2", params![into, from])?;
    conn.execute(
        "INSERT OR IGNORE INTO resource_tags (resource_id, tag_id, source, reason, applied_at)
         SELECT ?1, tag_id, source, reason, applied_at FROM resource_tags WHERE resource_id = ?2",
        params![into, from],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO collection_resources (collection_id, resource_id)
         SELECT collection_id, ?1 FROM collection_resources WHERE resource_id = ?2",
        params![into, from],
    )?;
    let (from_notes, from_fav): (String, i64) =
        conn.query_row("SELECT notes, favorite FROM resources WHERE id = ?1", [from], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if !from_notes.trim().is_empty() {
        conn.execute(
            "UPDATE resources SET notes = CASE WHEN trim(notes) = '' THEN ?1 ELSE notes || char(10) || char(10) || ?1 END
             WHERE id = ?2",
            params![from_notes, into],
        )?;
    }
    if from_fav != 0 {
        conn.execute("UPDATE resources SET favorite = 1 WHERE id = ?1", [into])?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO resource_relations (a, b, kind, created_at) SELECT ?1, b, kind, created_at FROM resource_relations WHERE a = ?2 AND b <> ?1",
        params![into, from],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO resource_relations (a, b, kind, created_at) SELECT a, ?1, kind, created_at FROM resource_relations WHERE b = ?2 AND a <> ?1",
        params![into, from],
    )?;
    conn.execute("UPDATE resources SET cover = NULL WHERE id = ?1 AND cover_user = 0", [into])?;
    conn.execute(
        "UPDATE note_images SET resource_id = ?1, position = position + 100 WHERE resource_id = ?2",
        params![into, from],
    )?;
    conn.execute("DELETE FROM resources WHERE id = ?1", [from])?;
    conn.execute("DELETE FROM resource_fts WHERE rowid = ?1", [from])?;
    touch(conn, into)?;
    Ok(())
}

/// "Mức độ đã được người dùng chăm sóc" — dùng để chọn resource giữ lại khi ghép.
pub fn curation_score(conn: &Connection, resource_id: i64) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT (SELECT count(*) FROM resource_tags WHERE resource_id = r.id)
              + (CASE WHEN trim(r.notes) <> '' THEN 5 ELSE 0 END)
              + r.favorite
         FROM resources r WHERE r.id = ?1",
        [resource_id],
        |r| r.get(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_db_is_kept_and_restored_from_backup() {
        let dir = std::env::temp_dir().join(format!("mrm_corrupt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let bdir = dir.join("backups");
        std::fs::create_dir_all(&bdir).unwrap();
        let path = dir.join("nink-vault.db");
        {
            let c = open(&path).unwrap();
            c.execute("INSERT INTO resources (name, created_at, updated_at) VALUES ('giữ lại', 0, 0)", []).unwrap();
            c.execute("VACUUM INTO ?1", [bdir.join("manual-1.db").to_string_lossy()]).unwrap();
            for i in 0..300 {
                c.execute("INSERT INTO resources (name, created_at, updated_at) VALUES (?1, 0, 0)", [format!("r{i}")]).unwrap();
            }
            checkpoint(&c, true);
        }
        // DB lành: không đụng gì
        let (c, n) = open_safe(&path, &bdir).unwrap();
        assert!(n.is_none());
        drop(c);
        // phá hỏng các trang giữa file
        let mut bytes = std::fs::read(&path).unwrap();
        let len = bytes.len();
        for b in &mut bytes[4096..len.min(4096 * 6)] {
            *b = 0xA5;
        }
        std::fs::write(&path, bytes).unwrap();
        let (c, n) = open_safe(&path, &bdir).unwrap();
        assert!(n.unwrap().contains("manual-1.db"));
        let cnt: i64 = c.query_row("SELECT count(*) FROM resources", [], |r| r.get(0)).unwrap();
        assert_eq!(cnt, 1);
        let kept = std::fs::read_dir(&dir).unwrap().flatten().find(|e| e.file_name().to_string_lossy().starts_with("corrupt-")).unwrap();
        assert!(kept.path().join("nink-vault.db").exists(), "file hỏng được giữ lại");
        drop(c);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_old_backup_then_migrate() {
        let dir = std::env::temp_dir().join(format!("mrm_restore_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // backup "cũ": schema v4 (chưa có ai_applied_at)
        let old = open(&dir.join("old.db")).unwrap();
        old.execute_batch(
            "ALTER TABLE resources DROP COLUMN ai_applied_at;
             ALTER TABLE resource_tags DROP COLUMN source;
             ALTER TABLE resource_tags DROP COLUMN reason;
             ALTER TABLE resource_tags DROP COLUMN applied_at;
             DROP TABLE tag_rejections;
             DROP VIEW api_assets; DROP VIEW api_asset_tags; DROP VIEW api_libraries;
             DROP TABLE asset_tags; DROP TABLE asset_favorites; DROP TABLE asset_usage;
             DROP TABLE asset_embeddings; DROP TABLE asset_captions; DROP TABLE favorite_folders;
             DROP TABLE asset_smart_collections; DROP TABLE collection_assets; DROP TABLE media_assets;
             DROP TABLE app_presence; DROP TABLE app_requests;
             ALTER TABLE libraries DROP COLUMN media_types;
             PRAGMA user_version = 4;",
        )
        .unwrap();
        old.execute("INSERT INTO resources (name, created_at, updated_at) VALUES ('x', 0, 0)", []).unwrap();
        drop(old);
        let mut cur = open(&dir.join("cur.db")).unwrap();
        cur.restore(rusqlite::MAIN_DB, dir.join("old.db"), None::<fn(rusqlite::backup::Progress)>).unwrap();
        migrate(&cur).unwrap();
        let v: Option<i64> = cur.query_row("SELECT ai_applied_at FROM resources", [], |r| r.get(0)).unwrap();
        assert!(v.is_none());
        let ver: i64 = cur.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(ver, SCHEMA_VERSION);
        let _ = &mut cur;
    }

    #[test]
    fn schema_and_fts5_work() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let n: i64 = conn.query_row("SELECT count(*) FROM tags WHERE kind = 'type'", [], |r| r.get(0)).unwrap();
        assert_eq!(n as usize, SEED_TYPES.len());
        conn.execute("INSERT INTO resources (name, created_at, updated_at) VALUES ('MotionBro', 0, 0)", []).unwrap();
        reindex(&conn, 1).unwrap();
        let hit: i64 = conn
            .query_row("SELECT rowid FROM resource_fts WHERE resource_fts MATCH '\"motion\"* \"bro\"*'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hit, 1);
    }
}
