//! AI nhìn ảnh/video (tùy chọn, mặc định TẮT; chỉ trong MRM — plugin DaVinci không dùng).
//! Model thị giác (gemma3:4b qua Ollama) viết một câu mô tả cho mỗi ảnh / khung giữa của video.
//! Câu mô tả được đưa vào văn bản embedding của file -> tìm theo nội dung hình ("cô gái tóc dài", "nền trời xanh")
//! và giúp danh mục AI chính xác hơn. Chạy nền từng file, tự dừng khi plugin DaVinci mở, có thể tạm dừng.
use crate::ai;
use crate::commands::AppState;
use crate::db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

type Res<T> = Result<T, String>;
fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

pub const DEFAULT_VISION_MODEL: &str = "gemma3:4b";
const FRAME_MAX: u32 = 768;

static PAUSED: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);
static BUSY: AtomicBool = AtomicBool::new(false);
static WORKER: AtomicBool = AtomicBool::new(false);

fn setting(app: &AppHandle, key: &str) -> Option<String> {
    db::get_setting(&app.state::<AppState>().db.lock().unwrap(), key)
}
pub fn vision_model(app: &AppHandle) -> String {
    setting(app, "ai_vision_model").unwrap_or_else(|| DEFAULT_VISION_MODEL.into())
}
fn vision_enabled(app: &AppHandle) -> bool {
    setting(app, "ai_vision_enabled").as_deref() == Some("1")
}

/// Chỉ xem ảnh/video đủ lớn (bỏ icon, ảnh cache < 64px); chuỗi PNG đã gom nên chỉ xem khung đầu.
const ELIGIBLE: &str = "FROM media_assets a LEFT JOIN asset_captions c ON c.asset_id = a.id
     WHERE a.media_type IN ('image','video') AND a.available = 1 AND a.meta_state = 1
       AND coalesce(a.width, 0) >= 64 AND coalesce(a.height, 0) >= 64";

fn counts(app: &AppHandle) -> (i64, i64) {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    let total = conn.query_row(&format!("SELECT count(*) {ELIGIBLE}"), [], |r| r.get(0)).unwrap_or(0);
    let done = conn.query_row(&format!("SELECT count(*) {ELIGIBLE} AND c.asset_id IS NOT NULL"), [], |r| r.get(0)).unwrap_or(0);
    (done, total)
}

// ---------------------------------------------------------------- base64 (ảnh gửi cho Ollama)

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

// ---------------------------------------------------------------- một lượt

fn prompt(kind: &str) -> String {
    format!(
        "You are tagging assets for a video editor's stock library. Describe this {kind} in ONE short sentence (max 20 words): \
         the main subject, colors and style. If it is an overlay, texture, light leak, particles or effect on a black or \
         transparent background, say that. No introduction."
    )
}

/// Ảnh JPEG nhỏ để AI xem: ảnh thu nhỏ, hoặc khung ở GIỮA video (khung đầu của overlay thường là màn đen).
fn frame_for(path: &Path, is_video: bool, duration: Option<f64>, out: &Path) -> Res<()> {
    if is_video {
        let at = duration.filter(|d| *d > 0.2).map(|d| d * 0.5).unwrap_or(0.0);
        return crate::media::video_frame(path, at, out, FRAME_MAX);
    }
    crate::media::image_thumb(path, out, FRAME_MAX).or_else(|_| crate::media::video_frame(path, 0.0, out, FRAME_MAX))
}

fn describe(model: &str, jpg: &[u8], kind: &str) -> Res<String> {
    #[derive(Deserialize)]
    struct R {
        response: String,
    }
    let mut resp = ai::agent(180)
        .post(format!("{}/api/generate", ai::base_url()))
        .send_json(json!({
            "model": model,
            "prompt": prompt(kind),
            "images": [base64(jpg)],
            "stream": false,
            "keep_alive": "10m",
            "options": { "num_predict": 60, "temperature": 0.2 }
        }))
        .map_err(e)?;
    let r: R = resp.body_mut().read_json().map_err(e)?;
    let text = r.response.trim().trim_matches('"').replace('\n', " ");
    if text.is_empty() {
        return Err("AI không trả lời".into());
    }
    Ok(text.chars().take(300).collect())
}

/// Xem một file chưa có mô tả. Ok(false) = đã xem hết.
fn caption_next(app: &AppHandle) -> Res<bool> {
    let state = app.state::<AppState>();
    let next: Option<(i64, String, String, Option<f64>)> = {
        let conn = state.db.lock().unwrap();
        conn.query_row(
            &format!(
                "SELECT a.id, a.media_type, v.path, a.duration {ELIGIBLE} AND c.asset_id IS NULL
                 ORDER BY a.media_type = 'video', a.id LIMIT 1"
            )
            .replace("FROM media_assets a", "FROM media_assets a JOIN api_assets v ON v.id = a.id"),
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(e)?
    };
    let Some((id, mt, path, duration)) = next else { return Ok(false) };
    let model = vision_model(app);
    let dir = crate::media::cache_dir(&state).join("vision");
    std::fs::create_dir_all(&dir).map_err(e)?;
    let jpg: PathBuf = dir.join(format!("{id}.jpg"));
    let is_video = mt == "video";
    let result = frame_for(Path::new(&path), is_video, duration, &jpg)
        .and_then(|_| std::fs::read(&jpg).map_err(e))
        .and_then(|bytes| describe(&model, &bytes, if is_video { "video frame" } else { "image" }));
    let _ = std::fs::remove_file(&jpg);
    let (caption, st) = match &result {
        Ok(c) => (Some(c.clone()), 1),
        // runtime không trả lời -> để lần sau; file hỏng / không đọc được -> đánh dấu lỗi, không thử lại mãi
        Err(err) if !ai::server_up() => return Err(err.clone()),
        Err(_) => (None, 2),
    };
    let conn = state.db.lock().unwrap();
    conn.execute(
        "INSERT INTO asset_captions (asset_id, model, caption, state, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(asset_id) DO UPDATE SET model = excluded.model, caption = excluded.caption, state = excluded.state,
         created_at = excluded.created_at",
        params![id, model, caption, st, db::now()],
    )
    .map_err(e)?;
    // mô tả mới -> tính lại embedding (tìm kiếm + danh mục AI)
    crate::asset_ai::invalidate(&conn, &[id]);
    Ok(true)
}

fn unload(model: &str) {
    let _ = ai::agent(10)
        .post(format!("{}/api/generate", ai::base_url()))
        .send_json(json!({ "model": model, "keep_alive": 0 }));
}

fn model_installed(model: &str) -> bool {
    ai::has_model(&ai::installed_models(), model)
}

/// Luồng nền: chỉ chạy khi AI bật + AI nhìn ảnh bật + không tạm dừng + plugin DaVinci không mở.
pub fn start_worker(app: &AppHandle) {
    if WORKER.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let mut installed: Option<(String, bool, Instant)> = None;
        let mut was_running = false;
        loop {
            let model = vision_model(&app);
            let mut ok = ai::ai_enabled(&app) && vision_enabled(&app) && !PAUSED.load(Ordering::SeqCst) && ai::server_up();
            if ok && crate::presence::plugin_active(&app.state::<AppState>().db.lock().unwrap()) {
                ok = false; // nhường GPU cho DaVinci
            }
            if ok {
                let fresh = installed.as_ref().map(|(m, _, t)| *m == model && t.elapsed() < Duration::from_secs(60)).unwrap_or(false);
                if !fresh {
                    installed = Some((model.clone(), model_installed(&model), Instant::now()));
                }
                ok = installed.as_ref().map(|x| x.1).unwrap_or(false);
            }
            if !ok {
                if was_running {
                    unload(&model);
                    was_running = false;
                    RUNNING.store(false, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
            match caption_next(&app) {
                Ok(true) => {
                    was_running = true;
                    RUNNING.store(true, Ordering::SeqCst);
                    let (done, total) = counts(&app);
                    ai::set_progress(&app, "captioning", "AI đang xem ảnh/video…", done as u64, total as u64);
                }
                Ok(false) => {
                    if was_running {
                        ai::set_progress(&app, "idle", "AI đã xem xong ảnh/video", 0, 0);
                        unload(&model);
                        was_running = false;
                    }
                    RUNNING.store(false, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_secs(20));
                }
                Err(err) => {
                    eprintln!("vision error: {err}");
                    std::thread::sleep(Duration::from_secs(15));
                }
            }
        }
    });
}

// ---------------------------------------------------------------- commands

#[derive(Serialize)]
pub struct VisionStatus {
    enabled: bool,
    installed: bool,
    model: String,
    done: i64,
    total: i64,
    paused: bool,
    running: bool,
    installing: bool,
}

#[tauri::command]
pub async fn vision_status(app: AppHandle) -> Res<VisionStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        let model = vision_model(&app);
        let (done, total) = counts(&app);
        Ok(VisionStatus {
            enabled: vision_enabled(&app),
            installed: ai::server_up() && model_installed(&model),
            model,
            done,
            total,
            paused: PAUSED.load(Ordering::SeqCst),
            running: RUNNING.load(Ordering::SeqCst),
            installing: BUSY.load(Ordering::SeqCst),
        })
    })
    .await
    .map_err(e)?
}

/// Bật: cần AI đã bật; tải model thị giác nếu chưa có (~3.3 GB). Tắt: dừng ngay và nhả VRAM.
#[tauri::command]
pub async fn vision_set_enabled(app: AppHandle, enabled: bool) -> Res<()> {
    if !enabled {
        db::set_setting(&app.state::<AppState>().db.lock().unwrap(), "ai_vision_enabled", "0").map_err(e)?;
        let model = vision_model(&app);
        tauri::async_runtime::spawn_blocking(move || unload(&model)).await.map_err(e)?;
        return Ok(());
    }
    if !ai::ai_enabled(&app) {
        return Err("Hãy bật AI offline trước (Libraries & Settings → AI).".into());
    }
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err("Đang tải model".into());
    }
    let a = app.clone();
    let res = tauri::async_runtime::spawn_blocking(move || -> Res<()> {
        ai::start_server(&a)?;
        let model = vision_model(&a);
        if !model_installed(&model) {
            ai::pull_model(&a, &model)?;
        }
        db::set_setting(&a.state::<AppState>().db.lock().unwrap(), "ai_vision_enabled", "1").map_err(e)?;
        ai::set_progress(&a, "idle", "AI nhìn ảnh/video đã bật", 0, 0);
        Ok(())
    })
    .await
    .map_err(e)?;
    BUSY.store(false, Ordering::SeqCst);
    if let Err(err) = &res {
        ai::set_progress(&app, "error", err, 0, 0);
    }
    start_worker(&app);
    res
}

#[tauri::command]
pub fn vision_set_paused(paused: bool) {
    PAUSED.store(paused, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
