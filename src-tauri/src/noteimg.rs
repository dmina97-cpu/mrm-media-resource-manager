//! Ảnh minh họa trong Notes (tối đa 3 ảnh / resource).
//! Ảnh được chuẩn hóa (JPEG, cạnh dài ≤ 1600px) và lưu trong thư mục dữ liệu của app —
//! không bao giờ ghi vào thư mục tài nguyên. Ảnh đầu tiên dùng làm ảnh bìa (trừ khi người dùng tự chọn bìa).

use crate::commands::AppState;
use crate::db;
use crate::media;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

type Res<T> = Result<T, String>;
fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

pub const MAX_IMAGES: i64 = 3;
const MAX_SIDE: u32 = 1600;

pub fn dir(state: &AppState) -> PathBuf {
    state.data_dir.join("note-images")
}

#[derive(Serialize, Clone)]
pub struct NoteImage {
    pub id: i64,
    pub file: String,
    pub position: i64,
}

pub fn list(conn: &Connection, resource_id: i64) -> rusqlite::Result<Vec<NoteImage>> {
    let mut s = conn.prepare("SELECT id, file, position FROM note_images WHERE resource_id = ?1 ORDER BY position, id")?;
    let v = s
        .query_map([resource_id], |r| Ok(NoteImage { id: r.get(0)?, file: r.get(1)?, position: r.get(2)? }))?
        .collect();
    v
}

/// Nguồn ảnh: file trên máy (chọn/kéo thả), dữ liệu dán từ clipboard, hoặc file bên trong gói.
#[derive(Deserialize)]
pub struct ImageInput {
    path: Option<String>,
    data_b64: Option<String>,
    ext: Option<String>,
    source_id: Option<i64>,
    inner: Option<String>,
}

fn b64_decode(s: &str) -> Res<Vec<u8>> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut map = [255u8; 256];
    for (i, c) in T.iter().enumerate() {
        map[*c as usize] = i as u8;
    }
    let s = s.split_once(',').map(|x| x.1).unwrap_or(s); // bỏ tiền tố data:...;base64,
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for &c in s.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        let v = map[c as usize];
        if v == 255 {
            return Err("Dữ liệu ảnh không hợp lệ".into());
        }
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

fn unique_name(resource_id: i64) -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("r{resource_id}-{nanos:x}.jpg")
}

/// Chuyển mọi đầu vào thành một file JPEG chuẩn trong thư mục note-images.
fn produce(state: &AppState, resource_id: i64, input: &ImageInput) -> Res<PathBuf> {
    let out_dir = dir(state);
    std::fs::create_dir_all(&out_dir).map_err(e)?;
    let out = out_dir.join(unique_name(resource_id));
    let tmp_dir = std::env::temp_dir().join("mrm-note-import");
    std::fs::create_dir_all(&tmp_dir).map_err(e)?;

    if let Some(b64) = &input.data_b64 {
        let bytes = b64_decode(b64)?;
        if bytes.len() > 60 * 1024 * 1024 {
            return Err("Ảnh quá lớn".into());
        }
        let ext = input.ext.clone().unwrap_or_else(|| "png".into()).to_lowercase();
        let tmp = tmp_dir.join(format!("paste-{}.{ext}", unique_name(resource_id)));
        std::fs::write(&tmp, bytes).map_err(e)?;
        let r = media::image_thumb(&tmp, &out, MAX_SIDE);
        let _ = std::fs::remove_file(&tmp);
        r?;
        return Ok(out);
    }
    if let Some(p) = &input.path {
        let p = Path::new(p);
        let kind = media::file_kind(&p.to_string_lossy());
        match kind {
            "image" => media::image_thumb(p, &out, MAX_SIDE)?,
            "video" => media::video_poster(p, &out, MAX_SIDE)?,
            _ => return Err("Chỉ hỗ trợ file ảnh hoặc video".into()),
        }
        return Ok(out);
    }
    if let (Some(sid), Some(inner)) = (input.source_id, &input.inner) {
        let src = media::resolve_source(&state.db.lock().unwrap(), sid)?;
        let thumb = media::thumb_for(state, &src, inner, MAX_SIDE)?;
        std::fs::copy(&thumb, &out).map_err(e)?;
        return Ok(out);
    }
    Err("Thiếu nguồn ảnh".into())
}

#[tauri::command]
pub async fn add_note_image(app: AppHandle, resource_id: i64, input: ImageInput) -> Res<NoteImage> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let count: i64 = state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM note_images WHERE resource_id = ?1", [resource_id], |r| r.get(0))
            .map_err(e)?;
        if count >= MAX_IMAGES {
            return Err(format!("Tối đa {MAX_IMAGES} ảnh cho mỗi resource"));
        }
        let file = produce(&state, resource_id, &input)?;
        let conn = state.db.lock().unwrap();
        let pos: i64 = conn
            .query_row("SELECT coalesce(max(position), -1) + 1 FROM note_images WHERE resource_id = ?1", [resource_id], |r| r.get(0))
            .map_err(e)?;
        conn.execute(
            "INSERT INTO note_images (resource_id, file, position, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![resource_id, file.to_string_lossy(), pos, db::now()],
        )
        .map_err(e)?;
        let id = conn.last_insert_rowid();
        db::touch(&conn, resource_id).map_err(e)?;
        Ok(NoteImage { id, file: file.to_string_lossy().to_string(), position: pos })
    })
    .await
    .map_err(e)?
}

#[tauri::command]
pub fn remove_note_image(state: State<AppState>, id: i64) -> Res<()> {
    let conn = state.db.lock().unwrap();
    let (rid, file): (i64, String) =
        conn.query_row("SELECT resource_id, file FROM note_images WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(e)?;
    conn.execute("DELETE FROM note_images WHERE id = ?1", [id]).map_err(e)?;
    // chỉ xóa file nằm trong thư mục note-images của app
    if Path::new(&file).starts_with(dir(&state)) {
        let _ = std::fs::remove_file(&file);
    }
    db::touch(&conn, rid).map_err(e)
}

/// Đưa ảnh lên đầu (thành ảnh bìa) và bỏ ảnh bìa tự chọn trước đó.
/// Đặt ảnh bìa từ ảnh dán (Ctrl+V), file ảnh/video hoặc khung hình: lưu thành ảnh Notes ĐẦU TIÊN
/// (ảnh bìa = ảnh Notes đầu tiên), bỏ ảnh bìa tự chọn trước đó.
#[tauri::command]
pub async fn set_cover_image(app: AppHandle, resource_id: i64, input: ImageInput) -> Res<NoteImage> {
    let img = add_note_image(app.clone(), resource_id, input).await?;
    let state = app.state::<AppState>();
    to_front(&state.db.lock().unwrap(), img.id)?;
    Ok(NoteImage { position: 0, ..img })
}

fn to_front(conn: &Connection, id: i64) -> Res<()> {
    let rid: i64 = conn.query_row("SELECT resource_id FROM note_images WHERE id = ?1", [id], |r| r.get(0)).map_err(e)?;
    conn.execute("UPDATE note_images SET position = position + 1 WHERE resource_id = ?1", [rid]).map_err(e)?;
    conn.execute("UPDATE note_images SET position = 0 WHERE id = ?1", [id]).map_err(e)?;
    conn.execute("UPDATE resources SET cover = NULL, cover_user = 0 WHERE id = ?1", [rid]).map_err(e)?;
    db::touch(conn, rid).map_err(e)
}

#[tauri::command]
pub fn note_image_to_front(state: State<AppState>, id: i64) -> Res<()> {
    to_front(&state.db.lock().unwrap(), id)
}

/// Xóa file ảnh không còn thuộc resource nào (resource bị xóa/gộp).
pub fn cleanup_orphans(state: &AppState) {
    let known: std::collections::HashSet<String> = {
        let conn = state.db.lock().unwrap();
        conn.prepare("SELECT file FROM note_images")
            .and_then(|mut s| s.query_map([], |r| r.get::<_, String>(0)).map(|rows| rows.flatten().collect()))
            .unwrap_or_default()
    };
    let Ok(rd) = std::fs::read_dir(dir(state)) else { return };
    for en in rd.flatten() {
        let p = en.path().to_string_lossy().to_string();
        if !known.contains(&p) {
            let _ = std::fs::remove_file(en.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produce_from_file_and_clipboard() {
        let base = std::env::temp_dir().join(format!("mrm_note_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let conn = crate::db::open(&base.join("t.db")).unwrap();
        let state = AppState {
            db: std::sync::Mutex::new(conn),
            data_dir: base.clone(),
            scanning: Default::default(),
            storage: std::sync::Mutex::new(base.clone()),
            media_sem: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
            learn: Default::default(),
        };
        // ảnh 3000x1000 -> thu nhỏ về cạnh dài 1600
        let src = base.join("big.png");
        image::RgbImage::from_pixel(3000, 1000, image::Rgb([200, 30, 30])).save(&src).unwrap();
        let out = produce(&state, 7, &ImageInput { path: Some(src.to_string_lossy().into()), data_b64: None, ext: None, source_id: None, inner: None }).unwrap();
        assert!(out.starts_with(dir(&state)));
        let img = image::open(&out).unwrap();
        assert_eq!((img.width(), img.height()), (1600, 533));
        // dán từ clipboard (PNG base64)
        let mut png = Vec::new();
        image::RgbImage::from_pixel(10, 10, image::Rgb([0, 0, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut b64 = String::new();
        for ch in png.chunks(3) {
            let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
            for i in 0..4 {
                if i <= ch.len() {
                    b64.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    b64.push('=');
                }
            }
        }
        let out2 = produce(&state, 7, &ImageInput { path: None, data_b64: Some(format!("data:image/png;base64,{b64}")), ext: Some("png".into()), source_id: None, inner: None }).unwrap();
        assert!(image::open(&out2).is_ok());
        assert_ne!(out, out2);
    }

    #[test]
    fn base64() {
        assert_eq!(b64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(b64_decode("data:image/png;base64,aGk=").unwrap(), b"hi");
        assert!(b64_decode("!!").is_err());
    }
}
