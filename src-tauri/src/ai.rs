//! Phase 5 — AI chạy hoàn toàn local qua Ollama do app tự quản lý.
//! - Runtime Ollama được tải về thư mục lưu trữ (không nằm trong installer), chạy ngầm ở cổng riêng.
//! - Embedding (bge-m3, đa ngôn ngữ) cho semantic search + gợi ý tag.
//! - LLM (qwen2.5:7b) đọc cấu trúc folder/README để đề xuất mô tả & tag — luôn chờ người dùng xác nhận.

use crate::commands::AppState;
use crate::db;
use crate::media::{ext_of, hidden};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

type Res<T> = Result<T, String>;
fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

pub const PORT: u16 = 11435;
pub const RUNTIME_URL: &str = "https://github.com/ollama/ollama/releases/latest/download/ollama-windows-amd64.zip";
pub const DEFAULT_EMBED_MODEL: &str = "bge-m3";
pub const DEFAULT_CHAT_MODEL: &str = "qwen2.5:7b";

pub(crate) fn base_url() -> String {
    format!("http://127.0.0.1:{PORT}")
}

pub(crate) fn agent(timeout_secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(timeout_secs))).build().into()
}

// ================================================================ state

#[derive(Serialize, Clone, Default)]
pub struct AiProgress {
    /// idle | downloading | extracting | starting | pulling | indexing | analyzing | error
    pub phase: String,
    pub message: String,
    pub done: u64,
    pub total: u64,
}

#[derive(Default)]
pub struct AiManager {
    child: Mutex<Option<std::process::Child>>,
    progress: Mutex<AiProgress>,
    busy: AtomicBool,
    /// embedding của resource trong RAM: id -> vector (đã chuẩn hóa)
    vectors: Mutex<Option<HashMap<i64, Vec<f32>>>>,
    /// embedding của nhãn tag: (khóa danh sách tag, [(tag_id, vector)])
    tag_vectors: Mutex<Option<(String, Vec<(i64, Vec<f32>)>)>>,
    analyzing: AtomicBool,
    indexer_started: AtomicBool,
}

pub(crate) fn set_progress(app: &AppHandle, phase: &str, message: &str, done: u64, total: u64) {
    let p = AiProgress { phase: phase.into(), message: message.into(), done, total };
    *app.state::<AiManager>().progress.lock().unwrap() = p.clone();
    let _ = app.emit("ai-progress", p);
}

fn ai_dir(state: &AppState) -> PathBuf {
    state.storage_dir().join("ai")
}
fn runtime_exe(state: &AppState) -> PathBuf {
    ai_dir(state).join("ollama").join("ollama.exe")
}
fn models_dir(state: &AppState) -> PathBuf {
    ai_dir(state).join("models")
}

fn setting(app: &AppHandle, key: &str) -> Option<String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    db::get_setting(&conn, key)
}
pub fn embed_model(app: &AppHandle) -> String {
    setting(app, "ai_embed_model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.into())
}
pub fn chat_model(app: &AppHandle) -> String {
    setting(app, "ai_chat_model").unwrap_or_else(|| DEFAULT_CHAT_MODEL.into())
}
pub fn ai_enabled(app: &AppHandle) -> bool {
    setting(app, "ai_enabled").as_deref() == Some("1")
}

// ================================================================ runtime

pub fn server_up() -> bool {
    agent(2).get(format!("{}/api/version", base_url())).call().is_ok()
}

pub(crate) fn installed_models() -> Vec<String> {
    #[derive(Deserialize)]
    struct Tags {
        models: Vec<M>,
    }
    #[derive(Deserialize)]
    struct M {
        name: String,
    }
    agent(5)
        .get(format!("{}/api/tags", base_url()))
        .call()
        .ok()
        .and_then(|mut r| r.body_mut().read_json::<Tags>().ok())
        .map(|t| t.models.into_iter().map(|m| m.name).collect())
        .unwrap_or_default()
}

pub(crate) fn has_model(list: &[String], model: &str) -> bool {
    let want = if model.contains(':') { model.to_string() } else { format!("{model}:latest") };
    list.iter().any(|m| *m == want || *m == model)
}

pub(crate) fn start_server(app: &AppHandle) -> Res<()> {
    if server_up() {
        return Ok(());
    }
    let state = app.state::<AppState>();
    let exe = runtime_exe(&state);
    if !exe.exists() {
        return Err("Chưa cài AI runtime".into());
    }
    std::fs::create_dir_all(models_dir(&state)).map_err(e)?;
    set_progress(app, "starting", "Đang khởi động AI runtime…", 0, 0);
    let mut cmd = std::process::Command::new(&exe);
    hidden(&mut cmd);
    cmd.arg("serve")
        .env("OLLAMA_HOST", format!("127.0.0.1:{PORT}"))
        .env("OLLAMA_MODELS", models_dir(&state))
        .env("OLLAMA_KEEP_ALIVE", "10m")
        .env("OLLAMA_NUM_PARALLEL", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let child = cmd.spawn().map_err(e)?;
    bind_to_app_lifetime(&child);
    *app.state::<AiManager>().child.lock().unwrap() = Some(child);
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(30) {
        if server_up() {
            set_progress(app, "idle", "", 0, 0);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    set_progress(app, "error", "AI runtime không phản hồi", 0, 0);
    Err("AI runtime không khởi động được".into())
}

/// Gắn tiến trình Ollama vào một Job Object "kill on close": nếu app thoát bất thường (crash, bị kill)
/// thì Windows tự tắt luôn Ollama, không để lại tiến trình chiếm VRAM.
#[cfg(windows)]
fn bind_to_app_lifetime(child: &std::process::Child) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    static JOB: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let job = *JOB.get_or_init(|| unsafe {
        let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if h.is_null() {
            return 0;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            h,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        h as usize // handle giữ mở suốt đời app; đóng khi process app kết thúc
    });
    if job != 0 {
        unsafe {
            AssignProcessToJobObject(job as _, child.as_raw_handle() as _);
        }
    }
}
#[cfg(not(windows))]
fn bind_to_app_lifetime(_child: &std::process::Child) {}

/// Nhả mọi model đang nạp khỏi VRAM (runtime vẫn chạy, lần dùng sau tự nạp lại).
pub fn unload_models() {
    #[derive(Deserialize)]
    struct Ps {
        models: Vec<M>,
    }
    #[derive(Deserialize)]
    struct M {
        name: String,
    }
    let Ok(mut r) = agent(3).get(format!("{}/api/ps", base_url())).call() else { return };
    let Ok(ps) = r.body_mut().read_json::<Ps>() else { return };
    for m in ps.models {
        let _ = agent(10).post(format!("{}/api/generate", base_url())).send_json(json!({ "model": m.name, "keep_alive": 0 }));
    }
}

pub fn shutdown(app: &AppHandle) {
    if let Some(mut c) = app.state::<AiManager>().child.lock().unwrap().take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

fn download(app: &AppHandle, url: &str, dest: &Path) -> Res<()> {
    let mut resp = agent(3600).get(url).call().map_err(e)?;
    let total: u64 = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0);
    let tmp = dest.with_extension("part");
    let mut out = std::fs::File::create(&tmp).map_err(e)?;
    let mut reader = resp.body_mut().with_config().limit(u64::MAX).reader();
    let mut buf = vec![0u8; 1 << 20];
    let mut done = 0u64;
    let mut last = Instant::now();
    loop {
        let n = reader.read(&mut buf).map_err(e)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(e)?;
        done += n as u64;
        if last.elapsed() > Duration::from_millis(300) {
            set_progress(app, "downloading", "Đang tải AI runtime (Ollama)…", done, total);
            last = Instant::now();
        }
    }
    out.flush().map_err(e)?;
    drop(out);
    std::fs::rename(&tmp, dest).map_err(e)
}

pub(crate) fn pull_model(app: &AppHandle, model: &str) -> Res<()> {
    #[derive(Deserialize)]
    struct Line {
        status: Option<String>,
        total: Option<u64>,
        completed: Option<u64>,
        error: Option<String>,
    }
    let mut resp = agent(24 * 3600).post(format!("{}/api/pull", base_url())).send_json(json!({ "model": model, "stream": true })).map_err(e)?;
    let reader = std::io::BufReader::new(resp.body_mut().with_config().limit(u64::MAX).reader());
    for line in reader.lines() {
        let line = line.map_err(e)?;
        let Ok(l) = serde_json::from_str::<Line>(&line) else { continue };
        if let Some(err) = l.error {
            return Err(err);
        }
        let status = l.status.unwrap_or_default();
        set_progress(app, "pulling", &format!("Tải model {model}: {status}"), l.completed.unwrap_or(0), l.total.unwrap_or(0));
    }
    Ok(())
}

/// Cài đặt đầy đủ: tải runtime (nếu thiếu) → khởi động → tải 2 model → bật AI.
#[tauri::command]
pub async fn ai_install(app: AppHandle) -> Res<()> {
    let mgr = app.state::<AiManager>();
    if mgr.busy.swap(true, Ordering::SeqCst) {
        return Err("Đang cài đặt".into());
    }
    let app2 = app.clone();
    let res = tauri::async_runtime::spawn_blocking(move || -> Res<()> {
        let app = app2;
        let state = app.state::<AppState>();
        let exe = runtime_exe(&state);
        if !exe.exists() {
            let dir = ai_dir(&state);
            std::fs::create_dir_all(&dir).map_err(e)?;
            let zip_path = dir.join("ollama-windows-amd64.zip");
            if !zip_path.exists() {
                download(&app, RUNTIME_URL, &zip_path)?;
            }
            set_progress(&app, "extracting", "Đang giải nén AI runtime…", 0, 0);
            let f = std::fs::File::open(&zip_path).map_err(e)?;
            let mut z = zip::ZipArchive::new(std::io::BufReader::new(f)).map_err(e)?;
            z.extract(dir.join("ollama")).map_err(e)?;
            let _ = std::fs::remove_file(&zip_path); // chỉ xóa file tải tạm của app
        }
        start_server(&app)?;
        let have = installed_models();
        for m in [embed_model(&app), chat_model(&app)] {
            if !has_model(&have, &m) {
                pull_model(&app, &m)?;
            }
        }
        db::set_setting(&state.db.lock().unwrap(), "ai_enabled", "1").map_err(e)?;
        set_progress(&app, "idle", "AI đã sẵn sàng", 0, 0);
        start_indexer(&app);
        Ok(())
    })
    .await
    .map_err(e)?;
    mgr.busy.store(false, Ordering::SeqCst);
    if let Err(err) = &res {
        set_progress(&app, "error", err, 0, 0);
    }
    res
}

#[tauri::command]
pub fn ai_set_enabled(app: AppHandle, enabled: bool) -> Res<()> {
    {
        let state = app.state::<AppState>();
        db::set_setting(&state.db.lock().unwrap(), "ai_enabled", if enabled { "1" } else { "0" }).map_err(e)?;
    }
    if enabled {
        let a = app.clone();
        std::thread::spawn(move || {
            if start_server(&a).is_ok() {
                start_indexer(&a);
            }
        });
    } else {
        shutdown(&app);
    }
    Ok(())
}

#[derive(Serialize)]
pub struct AiStatus {
    enabled: bool,
    installed: bool,
    running: bool,
    embed_model: String,
    chat_model: String,
    embed_ready: bool,
    chat_ready: bool,
    progress: AiProgress,
    busy: bool,
    gpu: Option<String>,
    storage_dir: String,
    free_bytes: Option<u64>,
    indexed: i64,
    total: i64,
    analyzing: bool,
}

fn gpu_name() -> Option<String> {
    let mut c = std::process::Command::new("nvidia-smi");
    hidden(&mut c);
    let out = c.args(["--query-gpu=name,memory.total", "--format=csv,noheader"]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[tauri::command]
pub async fn ai_status(app: AppHandle) -> Res<AiStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mgr = app.state::<AiManager>();
        let running = server_up();
        let models = if running { installed_models() } else { vec![] };
        let (em, cm) = (embed_model(&app), chat_model(&app));
        let (indexed, total) = {
            let conn = state.db.lock().unwrap();
            let em2 = em.clone();
            (
                conn.query_row("SELECT count(*) FROM resource_embeddings WHERE model = ?1", [em2], |r| r.get(0)).unwrap_or(0),
                conn.query_row("SELECT count(*) FROM resources", [], |r| r.get(0)).unwrap_or(0),
            )
        };
        static GPU: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
        let progress = mgr.progress.lock().unwrap().clone();
        let busy = mgr.busy.load(Ordering::SeqCst);
        let analyzing = mgr.analyzing.load(Ordering::SeqCst);
        Ok(AiStatus {
            enabled: ai_enabled(&app),
            installed: runtime_exe(&state).exists(),
            running,
            embed_ready: has_model(&models, &em),
            chat_ready: has_model(&models, &cm),
            embed_model: em,
            chat_model: cm,
            progress,
            busy,
            gpu: GPU.get_or_init(gpu_name).clone(),
            storage_dir: ai_dir(&state).to_string_lossy().to_string(),
            free_bytes: crate::commands::free_space(&state.storage_dir()),
            indexed,
            total,
            analyzing,
        })
    })
    .await
    .map_err(e)?
}

/// Khởi động AI nền khi mở app (nếu đã bật trước đó).
pub fn autostart(app: &AppHandle) {
    if !ai_enabled(app) {
        return;
    }
    let a = app.clone();
    std::thread::spawn(move || {
        if start_server(&a).is_ok() {
            start_indexer(&a);
        }
    });
}

// ================================================================ embeddings

fn normalize_vec(mut v: Vec<f32>) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in &mut v {
            *x /= n;
        }
    }
    v
}

pub fn embed(model: &str, input: &[String]) -> Res<Vec<Vec<f32>>> {
    #[derive(Deserialize)]
    struct R {
        embeddings: Vec<Vec<f32>>,
    }
    let mut resp = agent(300)
        .post(format!("{}/api/embed", base_url()))
        .send_json(json!({ "model": model, "input": input, "truncate": true }))
        .map_err(e)?;
    let r: R = resp.body_mut().with_config().limit(64 << 20).read_json().map_err(e)?;
    Ok(r.embeddings.into_iter().map(normalize_vec).collect())
}

pub(crate) fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
pub(crate) fn from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

pub(crate) fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Danh sách tên file bên trong (tối đa `limit`) — giúp AI hiểu resource chứa gì.
pub fn inner_file_names(conn: &Connection, resource_id: i64, limit: usize) -> Vec<String> {
    let ids: Vec<i64> = conn
        .prepare("SELECT id FROM resource_sources WHERE resource_id = ?1 AND available = 1")
        .and_then(|mut s| s.query_map([resource_id], |r| r.get(0)).map(|rows| rows.flatten().collect()))
        .unwrap_or_default();
    let mut out = Vec::new();
    for sid in ids {
        let Ok(src) = crate::media::resolve_source(conn, sid) else { continue };
        match src.kind.as_str() {
            "folder" => {
                for en in walkdir::WalkDir::new(&src.abs).max_depth(4).into_iter().flatten() {
                    if en.depth() == 0 {
                        continue;
                    }
                    let rel = en.path().strip_prefix(&src.abs).unwrap_or(en.path()).to_string_lossy().replace('\\', "/");
                    out.push(if en.file_type().is_dir() { format!("{rel}/") } else { rel });
                    if out.len() >= limit {
                        return out;
                    }
                }
            }
            "archive" if ext_of(&src.abs.to_string_lossy()) == "zip" => {
                if let Ok(f) = std::fs::File::open(&src.abs) {
                    if let Ok(mut z) = zip::ZipArchive::new(std::io::BufReader::new(f)) {
                        for i in 0..z.len() {
                            if let Ok(f) = z.by_index_raw(i) {
                                out.push(f.name().to_string());
                                if out.len() >= limit {
                                    return out;
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        if !out.is_empty() {
            break; // một nguồn là đủ (ZIP và folder thường giống nhau)
        }
    }
    out
}

fn resource_text(conn: &Connection, id: i64) -> Res<String> {
    let (name, notes): (String, String) = conn.query_row("SELECT name, notes FROM resources WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(e)?;
    let tags: Vec<String> = conn
        .prepare("SELECT t.name FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id WHERE rt.resource_id = ?1 AND t.kind <> 'status'")
        .and_then(|mut s| s.query_map([id], |r| r.get(0)).map(|rows| rows.flatten().collect()))
        .map_err(e)?;
    let files = inner_file_names(conn, id, 60);
    let norm = crate::normalize::normalize(&name, false).display;
    let notes: String = notes.chars().take(1500).collect();
    Ok(format!("{name} ({norm})\nTags: {}\nNotes: {notes}\nFiles: {}", tags.join(", "), files.join(", ")))
}

pub(crate) fn text_hash(s: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn load_vectors(app: &AppHandle) {
    let state = app.state::<AppState>();
    let model = embed_model(app);
    let map: HashMap<i64, Vec<f32>> = {
        let conn = state.db.lock().unwrap();
        conn.prepare("SELECT resource_id, vector FROM resource_embeddings WHERE model = ?1")
            .and_then(|mut s| {
                s.query_map([model], |r| Ok((r.get::<_, i64>(0)?, from_blob(&r.get::<_, Vec<u8>>(1)?))))
                    .map(|rows| rows.flatten().collect())
            })
            .unwrap_or_default()
    };
    *app.state::<AiManager>().vectors.lock().unwrap() = Some(map);
}

/// Một vòng lập chỉ mục: embed các resource mới/đã thay đổi. Trả về số resource đã xử lý.
fn index_round(app: &AppHandle) -> Res<usize> {
    let state = app.state::<AppState>();
    let model = embed_model(app);
    let (stale, total_stale): (Vec<i64>, i64) = {
        let conn = state.db.lock().unwrap();
        let sql = "FROM resources r LEFT JOIN resource_embeddings x ON x.resource_id = r.id AND x.model = ?1
                   WHERE x.resource_id IS NULL OR x.embedded_at < r.updated_at";
        let ids = conn
            .prepare(&format!("SELECT r.id {sql} ORDER BY r.id LIMIT 32"))
            .and_then(|mut s| s.query_map([&model], |r| r.get(0)).map(|rows| rows.flatten().collect()))
            .map_err(e)?;
        let n = conn.query_row(&format!("SELECT count(*) {sql}"), [&model], |r| r.get(0)).map_err(e)?;
        (ids, n)
    };
    if stale.is_empty() {
        return Ok(0);
    }
    let total = {
        let conn = state.db.lock().unwrap();
        conn.query_row("SELECT count(*) FROM resources", [], |r| r.get::<_, i64>(0)).unwrap_or(0)
    };
    set_progress(app, "indexing", "Đang lập chỉ mục AI…", (total - total_stale).max(0) as u64, total as u64);

    // Dựng text (đọc filesystem) — lock ngắn cho từng resource
    let mut items: Vec<(i64, String, String)> = Vec::new();
    let mut unchanged: Vec<i64> = Vec::new();
    for id in &stale {
        let conn = state.db.lock().unwrap();
        let Ok(text) = resource_text(&conn, *id) else { continue };
        let h = text_hash(&text);
        let old: Option<String> = conn
            .query_row("SELECT text_hash FROM resource_embeddings WHERE resource_id = ?1 AND model = ?2", params![id, model], |r| r.get(0))
            .optional()
            .map_err(e)?;
        if old.as_deref() == Some(h.as_str()) {
            unchanged.push(*id);
        } else {
            items.push((*id, text, h));
        }
    }
    let vectors = if items.is_empty() { vec![] } else { embed(&model, &items.iter().map(|x| x.1.clone()).collect::<Vec<_>>())? };
    let now = db::now();
    {
        let conn = state.db.lock().unwrap();
        for id in &unchanged {
            conn.execute("UPDATE resource_embeddings SET embedded_at = ?1 WHERE resource_id = ?2", params![now, id]).map_err(e)?;
        }
        for ((id, _, h), v) in items.iter().zip(&vectors) {
            conn.execute(
                "INSERT INTO resource_embeddings (resource_id, model, text_hash, vector, embedded_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(resource_id) DO UPDATE SET model = excluded.model, text_hash = excluded.text_hash,
                 vector = excluded.vector, embedded_at = excluded.embedded_at",
                params![id, model, h, to_blob(v), now],
            )
            .map_err(e)?;
        }
    }
    if let Some(map) = app.state::<AiManager>().vectors.lock().unwrap().as_mut() {
        for ((id, _, _), v) in items.iter().zip(vectors) {
            map.insert(*id, v);
        }
    }
    Ok(stale.len())
}

fn tag_label(kind: &str, name: &str) -> String {
    match kind {
        "app" => format!("Phần mềm / software: {name}"),
        "type" => format!("Loại tài nguyên dựng video / resource type: {name}"),
        _ => format!("Chức năng / function: {name}"),
    }
}

fn refresh_tag_vectors(app: &AppHandle) -> Res<()> {
    let state = app.state::<AppState>();
    let tags: Vec<(i64, String, String)> = {
        let conn = state.db.lock().unwrap();
        conn.prepare("SELECT id, kind, name FROM tags WHERE kind IN ('app','type','function') ORDER BY id")
            .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|rows| rows.flatten().collect()))
            .map_err(e)?
    };
    let key = text_hash(&format!("{:?}{}", tags, embed_model(app)));
    let mgr = app.state::<AiManager>();
    if mgr.tag_vectors.lock().unwrap().as_ref().map(|(k, _)| *k == key).unwrap_or(false) {
        return Ok(());
    }
    let labels: Vec<String> = tags.iter().map(|(_, k, n)| tag_label(k, n)).collect();
    let vecs = embed(&embed_model(app), &labels)?;
    *mgr.tag_vectors.lock().unwrap() = Some((key, tags.iter().map(|t| t.0).zip(vecs).collect()));
    Ok(())
}

/// Luồng nền: khi AI chạy, liên tục cập nhật chỉ mục embedding cho resource mới/đã sửa.
pub fn start_indexer(app: &AppHandle) {
    let mgr = app.state::<AiManager>();
    if mgr.indexer_started.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        load_vectors(&app);
        loop {
            if !ai_enabled(&app) || !server_up() {
                std::thread::sleep(Duration::from_secs(10));
                continue;
            }
            // DaVinci đang dùng plugin -> nhường GPU
            if crate::presence::plugin_active(&app.state::<AppState>().db.lock().unwrap()) {
                std::thread::sleep(Duration::from_secs(10));
                continue;
            }
            let _ = refresh_tag_vectors(&app);
            match index_round(&app) {
                Ok(0) => {
                    // resource đã xong -> lập chỉ mục file media (Media Browser)
                    match crate::asset_ai::index_round(&app) {
                        Ok(n) if n > 0 => continue,
                        Err(err) => {
                            eprintln!("asset ai index error: {err}");
                            std::thread::sleep(Duration::from_secs(15));
                            continue;
                        }
                        _ => {}
                    }
                    let p = app.state::<AiManager>().progress.lock().unwrap().phase.clone();
                    if p == "indexing" {
                        set_progress(&app, "idle", "Chỉ mục AI đã cập nhật", 0, 0);
                        let _ = app.emit("ai-index-done", ());
                    }
                    std::thread::sleep(Duration::from_secs(5));
                }
                Ok(_) => {}
                Err(err) => {
                    eprintln!("ai index error: {err}");
                    std::thread::sleep(Duration::from_secs(15));
                }
            }
        }
    });
}

/// Semantic search: (resource_id, cosine) — rỗng nếu AI chưa sẵn sàng.
pub fn semantic_hits(app: &AppHandle, query: &str) -> Vec<(i64, f32)> {
    if !ai_enabled(app) || query.trim().is_empty() {
        return vec![];
    }
    let mgr = app.state::<AiManager>();
    let ready = mgr.vectors.lock().unwrap().as_ref().map(|m| !m.is_empty()).unwrap_or(false);
    if !ready || !server_up() {
        return vec![];
    }
    let Ok(mut q) = embed(&embed_model(app), &[query.to_string()]) else { return vec![] };
    let Some(qv) = q.pop() else { return vec![] };
    let guard = mgr.vectors.lock().unwrap();
    let Some(map) = guard.as_ref() else { return vec![] };
    let mut scored: Vec<(i64, f32)> = map.iter().map(|(id, v)| (*id, dot(&qv, v))).collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let best = scored.first().map(|x| x.1).unwrap_or(0.0);
    scored.into_iter().take(40).filter(|(_, s)| *s >= 0.38 && *s >= best - 0.15).collect()
}

/// Gợi ý tag bằng embedding:
/// 1. kNN: các resource giống nhất đã được gắn tag "bỏ phiếu" cho tag của chúng.
/// 2. So với nhãn tag (khi thư viện còn ít resource đã gắn tag).
pub fn tag_suggestions(state: &AppState, conn: &Connection, id: i64) -> Res<Vec<(i64, f32)>> {
    let _ = state;
    let Some(app) = APP.get() else { return Ok(vec![]) };
    // Lưu ý: caller đang giữ lock DB -> đọc setting qua `conn`, KHÔNG gọi ai_enabled() (sẽ deadlock)
    if db::get_setting(conn, "ai_enabled").as_deref() != Some("1") {
        return Ok(vec![]);
    }
    let mgr = app.state::<AiManager>();
    let guard = mgr.vectors.lock().unwrap();
    let Some(map) = guard.as_ref() else { return Ok(vec![]) };
    let Some(me) = map.get(&id) else { return Ok(vec![]) };

    let mut tags_of: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut s = conn
        .prepare("SELECT rt.resource_id, rt.tag_id FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id WHERE t.kind IN ('app','type','function','personal')")
        .map_err(e)?;
    for row in s.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).map_err(e)?.flatten() {
        tags_of.entry(row.0).or_default().push(row.1);
    }
    let mut neigh: Vec<(i64, f32)> =
        map.iter().filter(|(rid, _)| **rid != id && tags_of.contains_key(rid)).map(|(rid, v)| (*rid, dot(me, v))).collect();
    neigh.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    neigh.truncate(8);
    let mut votes: HashMap<i64, (f32, f32)> = HashMap::new(); // tag -> (tổng trọng số, sim lớn nhất)
    let mut weight_sum = 0.0;
    for (rid, sim) in &neigh {
        if *sim < 0.6 {
            continue;
        }
        weight_sum += sim;
        for t in &tags_of[rid] {
            let v = votes.entry(*t).or_default();
            v.0 += sim;
            v.1 = v.1.max(*sim);
        }
    }
    let mut out: Vec<(i64, f32)> = votes
        .into_iter()
        .filter(|(_, (w, _))| weight_sum > 0.0 && *w / weight_sum >= 0.5)
        .map(|(t, (_, s))| (t, s))
        .collect();

    if out.len() < 2 {
        if let Some((_, tv)) = mgr.tag_vectors.lock().unwrap().as_ref() {
            let mut sims: Vec<(i64, f32)> = tv.iter().map(|(t, v)| (*t, dot(me, v))).collect();
            sims.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            for (t, s) in sims.into_iter().take(3) {
                if s >= 0.5 && !out.iter().any(|x| x.0 == t) {
                    out.push((t, s));
                }
            }
        }
    }
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(5);
    Ok(out)
}

pub static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

// ================================================================ LLM analysis

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AiAnalysis {
    pub description: String,
    pub apps: Vec<String>,
    pub types: Vec<String>,
    pub function_tags: Vec<String>,
    pub install_notes: String,
    #[serde(default)]
    pub model: String,
}

fn readme_text(conn: &Connection, id: i64) -> String {
    let names = inner_file_names(conn, id, 400);
    let pick = names.iter().find(|n| {
        let l = n.to_lowercase();
        let base = l.rsplit('/').next().unwrap_or(&l);
        (base.starts_with("readme") || base.contains("install") || base.contains("huong dan") || base.contains("instruction"))
            && (base.ends_with(".txt") || base.ends_with(".md") || !base.contains('.'))
    });
    let Some(inner) = pick else { return String::new() };
    let sources: Vec<i64> = conn
        .prepare("SELECT id FROM resource_sources WHERE resource_id = ?1 AND available = 1")
        .and_then(|mut s| s.query_map([id], |r| r.get(0)).map(|rows| rows.flatten().collect()))
        .unwrap_or_default();
    for sid in sources {
        let Ok(src) = crate::media::resolve_source(conn, sid) else { continue };
        let mut buf = Vec::new();
        let ok = match src.kind.as_str() {
            "folder" => std::fs::File::open(src.abs.join(inner)).and_then(|f| f.take(4000).read_to_end(&mut buf)).is_ok(),
            "archive" => std::fs::File::open(&src.abs)
                .ok()
                .and_then(|f| zip::ZipArchive::new(std::io::BufReader::new(f)).ok())
                .and_then(|mut z| z.by_name(inner).ok().and_then(|f| f.take(4000).read_to_end(&mut buf).ok()))
                .is_some(),
            _ => false,
        };
        if ok && !buf.is_empty() {
            return String::from_utf8_lossy(&buf).to_string();
        }
    }
    String::new()
}

fn analyze_one(app: &AppHandle, id: i64) -> Res<AiAnalysis> {
    let state = app.state::<AppState>();
    let (name, notes, files, readme, apps, types, functions, sources) = {
        let conn = state.db.lock().unwrap();
        let (name, notes): (String, String) = conn.query_row("SELECT name, notes FROM resources WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(e)?;
        let list = |kind: &str| -> Vec<String> {
            conn.prepare("SELECT name FROM tags WHERE kind = ?1 ORDER BY name")
                .and_then(|mut s| s.query_map([kind], |r| r.get(0)).map(|rows| rows.flatten().collect()))
                .unwrap_or_default()
        };
        let sources: Vec<String> = conn
            .prepare("SELECT kind || ': ' || name FROM resource_sources WHERE resource_id = ?1")
            .and_then(|mut s| s.query_map([id], |r| r.get(0)).map(|rows| rows.flatten().collect()))
            .unwrap_or_default();
        (name, notes, inner_file_names(&conn, id, 150), readme_text(&conn, id), list("app"), list("type"), list("function"), sources)
    };
    let schema = json!({
        "type": "object",
        "properties": {
            "description": { "type": "string" },
            "apps": { "type": "array", "items": { "type": "string", "enum": apps } },
            "types": { "type": "array", "items": { "type": "string", "enum": types } },
            "function_tags": { "type": "array", "items": { "type": "string" } },
            "install_notes": { "type": "string" }
        },
        "required": ["description", "apps", "types", "function_tags", "install_notes"]
    });
    let system = "Bạn là trợ lý phân loại tài nguyên dựng video (plugin, LUT, SFX, template, preset, footage...). \
        Dựa vào tên, cấu trúc thư mục và README, hãy trả lời bằng JSON đúng schema. \
        description: 1-3 câu tiếng Việt, nói rõ tài nguyên là gì và dùng để làm gì. \
        apps/types: chỉ chọn trong danh sách cho phép, để trống nếu không chắc. \
        function_tags: 1-5 tag ngắn bằng tiếng Anh mô tả chức năng (vd: Film Emulation, Tracking, Whoosh, Light Leak); ưu tiên dùng lại tag có sẵn. \
        install_notes: cách cài ngắn gọn bằng tiếng Việt nếu suy ra được (vd: copy .dctl vào thư mục LUT của Resolve), nếu không thì để chuỗi rỗng. \
        Không bịa thông tin không có căn cứ.";
    let user = format!(
        "Tên: {name}\nNguồn: {}\nGhi chú hiện có: {}\nFunction tag đang có trong thư viện: {}\n\nCấu trúc file (tối đa 150):\n{}\n\nREADME/hướng dẫn (trích):\n{}",
        sources.join("; "),
        notes.chars().take(800).collect::<String>(),
        functions.join(", "),
        files.join("\n"),
        readme.chars().take(3000).collect::<String>()
    );
    #[derive(Deserialize)]
    struct ChatResp {
        message: Msg,
    }
    #[derive(Deserialize)]
    struct Msg {
        content: String,
    }
    let model = chat_model(app);
    let mut resp = agent(600)
        .post(format!("{}/api/chat", base_url()))
        .send_json(json!({
            "model": model,
            "stream": false,
            "format": schema,
            "keep_alive": "2m",
            "options": { "temperature": 0.2, "num_ctx": 8192, "num_predict": 700 },
            "messages": [ { "role": "system", "content": system }, { "role": "user", "content": user } ]
        }))
        .map_err(e)?;
    let r: ChatResp = resp.body_mut().read_json().map_err(e)?;
    let mut a: AiAnalysis = serde_json::from_str(&r.message.content).map_err(|err| format!("AI trả về không đúng định dạng: {err}"))?;
    a.model = model;
    // chặn giá trị ngoài danh sách (phòng khi model bỏ qua enum)
    a.apps.retain(|x| apps.contains(x));
    a.types.retain(|x| types.contains(x));
    a.function_tags = a.function_tags.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty() && t.len() <= 40).take(5).collect();
    Ok(a)
}

/// Phân tích bằng LLM cho danh sách resource (chạy nền). Kết quả lưu vào ai_suggestions chờ xác nhận.
#[tauri::command]
pub async fn ai_analyze(app: AppHandle, ids: Vec<i64>) -> Res<()> {
    let mgr = app.state::<AiManager>();
    if mgr.analyzing.swap(true, Ordering::SeqCst) {
        return Err("AI đang phân tích một nhóm khác".into());
    }
    let a = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let app = a;
        let run = || -> Res<()> {
            start_server(&app)?;
            let total = ids.len() as u64;
            for (i, id) in ids.iter().enumerate() {
                set_progress(&app, "analyzing", "AI đang phân tích…", i as u64, total);
                match analyze_one(&app, *id) {
                    Ok(res) => {
                        let state = app.state::<AppState>();
                        {
                            let conn = state.db.lock().unwrap();
                            conn.execute(
                                "INSERT INTO ai_suggestions (resource_id, data, created_at) VALUES (?1, ?2, ?3)
                                 ON CONFLICT(resource_id) DO UPDATE SET data = excluded.data, created_at = excluded.created_at",
                                params![id, serde_json::to_string(&res).unwrap_or_default(), db::now()],
                            )
                            .map_err(e)?;
                            // Tự áp dụng ngay (mặc định bật) — người dùng chỉnh lại sau nếu chưa chuẩn
                            if db::get_setting(&conn, "ai_auto_apply").as_deref() != Some("0") {
                                apply_one(&conn, *id).map_err(e)?;
                            }
                        }
                        *state.learn.lock().unwrap() = None;
                        let _ = app.emit("ai-analyzed", *id);
                    }
                    Err(err) => eprintln!("ai analyze {id}: {err}"),
                }
            }
            set_progress(&app, "idle", "Phân tích xong", total, total);
            Ok(())
        };
        let r = run();
        app.state::<AiManager>().analyzing.store(false, Ordering::SeqCst);
        if let Err(err) = &r {
            set_progress(&app, "error", err, 0, 0);
        }
        r
    })
    .await
    .map_err(e)?
}

/// Áp dụng kết quả phân tích AI đang chờ của một resource: gắn tag + ghi mô tả/cách cài vào Notes.
/// Trả về false nếu resource không có kết quả chờ.
pub fn apply_one(conn: &Connection, id: i64) -> rusqlite::Result<bool> {
    let data: Option<String> =
        conn.query_row("SELECT data FROM ai_suggestions WHERE resource_id = ?1", [id], |r| r.get(0)).optional()?;
    let Some(a) = data.and_then(|d| serde_json::from_str::<AiAnalysis>(&d).ok()) else { return Ok(false) };
    let mut tag_ids: Vec<i64> = Vec::new();
    for (kind, names) in [("app", &a.apps), ("type", &a.types)] {
        for n in names {
            if let Some(t) = db::tag_id(conn, kind, n)? {
                tag_ids.push(t);
            }
        }
    }
    for n in &a.function_tags {
        // dùng lại tag Function/Personal cùng tên nếu có, không thì tạo tag Function mới
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM tags WHERE kind IN ('function','personal') AND name = ?1 COLLATE NOCASE ORDER BY kind LIMIT 1",
                [n],
                |r| r.get(0),
            )
            .optional()?;
        let t = match existing {
            Some(t) => t,
            None => {
                conn.execute("INSERT INTO tags (kind, name) VALUES ('function', ?1)", [n])?;
                conn.last_insert_rowid()
            }
        };
        tag_ids.push(t);
    }
    for t in tag_ids {
        crate::autotag::insert_auto(conn, id, t, "ai", &format!("Phân tích AI ({})", a.model))?;
    }
    let mut add = a.description.trim().to_string();
    if !a.install_notes.trim().is_empty() {
        if !add.is_empty() {
            add.push('\n');
        }
        add.push_str(&format!("Cài đặt: {}", a.install_notes.trim()));
    }
    if !add.is_empty() {
        conn.execute(
            "UPDATE resources SET notes = CASE WHEN trim(notes) = '' THEN ?1 ELSE notes || char(10) || char(10) || ?1 END
             WHERE id = ?2 AND instr(notes, ?3) = 0",
            params![add, id, a.description.trim()],
        )?;
    }
    conn.execute("UPDATE resources SET ai_applied_at = ?1 WHERE id = ?2", params![db::now(), id])?;
    conn.execute("DELETE FROM ai_suggestions WHERE resource_id = ?1", [id])?;
    db::touch(conn, id)?;
    db::reindex(conn, id)?;
    Ok(true)
}

/// Áp dụng hàng loạt: `ids` = None -> mọi kết quả AI đang chờ.
#[tauri::command]
pub fn ai_apply(state: State<AppState>, ids: Option<Vec<i64>>) -> Res<usize> {
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    let targets: Vec<i64> = match ids {
        Some(v) => v,
        None => {
            let mut s = tx.prepare("SELECT resource_id FROM ai_suggestions").map_err(e)?;
            let v = s.query_map([], |r| r.get(0)).map_err(e)?.collect::<Result<_, _>>().map_err(e)?;
            v
        }
    };
    let mut n = 0;
    for id in targets {
        if apply_one(&tx, id).map_err(e)? {
            n += 1;
        }
    }
    tx.commit().map_err(e)?;
    *state.learn.lock().unwrap() = None;
    Ok(n)
}

#[tauri::command]
pub fn ai_get_auto_apply(state: State<AppState>) -> bool {
    db::get_setting(&state.db.lock().unwrap(), "ai_auto_apply").as_deref() != Some("0")
}

#[tauri::command]
pub fn ai_set_auto_apply(state: State<AppState>, enabled: bool) -> Res<()> {
    db::set_setting(&state.db.lock().unwrap(), "ai_auto_apply", if enabled { "1" } else { "0" }).map_err(e)
}

#[tauri::command]
pub fn ai_dismiss(state: State<AppState>, id: i64) -> Res<()> {
    state.db.lock().unwrap().execute("DELETE FROM ai_suggestions WHERE resource_id = ?1", [id]).map_err(e)?;
    Ok(())
}

#[tauri::command]
pub fn ai_pending_count(state: State<AppState>) -> Res<i64> {
    state.db.lock().unwrap().query_row("SELECT count(*) FROM ai_suggestions", [], |r| r.get(0)).map_err(e)
}

#[tauri::command]
pub fn ai_set_models(state: State<AppState>, embed: String, chat: String) -> Res<()> {
    let conn = state.db.lock().unwrap();
    if !embed.trim().is_empty() {
        db::set_setting(&conn, "ai_embed_model", embed.trim()).map_err(e)?;
    }
    if !chat.trim().is_empty() {
        db::set_setting(&conn, "ai_chat_model", chat.trim()).map_err(e)?;
    }
    Ok(())
}

/// Tìm resource tương tự (theo embedding).
#[tauri::command]
pub fn ai_similar(app: AppHandle, id: i64) -> Res<Vec<(i64, f32)>> {
    let mgr = app.state::<AiManager>();
    let guard = mgr.vectors.lock().unwrap();
    let Some(map) = guard.as_ref() else { return Ok(vec![]) };
    let Some(me) = map.get(&id) else { return Ok(vec![]) };
    let mut v: Vec<(i64, f32)> = map.iter().filter(|(r, _)| **r != id).map(|(r, x)| (*r, dot(me, x))).collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    Ok(v.into_iter().take(8).filter(|x| x.1 >= 0.55).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_roundtrip_and_norm() {
        let v = normalize_vec(vec![3.0, 4.0]);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert_eq!(from_blob(&to_blob(&v)), v);
        assert!((dot(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn apply_analysis_adds_tags_and_notes() {
        let dir = std::env::temp_dir().join(format!("mrm_ai_apply_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let conn = db::open(&dir.join("t.db")).unwrap();
        conn.execute("INSERT INTO resources (name, notes, created_at, updated_at) VALUES ('Kodak 2383', 'Ghi chú cũ', 0, 0)", []).unwrap();
        let a = AiAnalysis {
            description: "Bộ LUT giả lập phim Kodak 2383.".into(),
            apps: vec!["DaVinci Resolve".into()],
            types: vec!["LUT".into()],
            function_tags: vec!["Film Emulation".into()],
            install_notes: "Copy .cube vào thư mục LUT".into(),
            model: "test".into(),
        };
        conn.execute("INSERT INTO ai_suggestions VALUES (1, ?1, 0)", [serde_json::to_string(&a).unwrap()]).unwrap();
        assert!(apply_one(&conn, 1).unwrap());
        let tags: Vec<String> = conn
            .prepare("SELECT t.name FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id ORDER BY t.name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(tags, vec!["DaVinci Resolve", "Film Emulation", "LUT"]);
        let (notes, applied): (String, Option<i64>) =
            conn.query_row("SELECT notes, ai_applied_at FROM resources WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert!(notes.starts_with("Ghi chú cũ

Bộ LUT"));
        assert!(notes.contains("Cài đặt: Copy .cube"));
        assert!(applied.is_some());
        // chạy lại không nhân đôi (kết quả chờ đã bị xóa)
        assert!(!apply_one(&conn, 1).unwrap());
    }

    #[test]
    fn model_name_match() {
        let list = vec!["bge-m3:latest".to_string(), "qwen2.5:7b".to_string()];
        assert!(has_model(&list, "bge-m3"));
        assert!(has_model(&list, "qwen2.5:7b"));
        assert!(!has_model(&list, "llama3"));
    }
}
