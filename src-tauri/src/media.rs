//! Phase 3 — Preview: duyệt file bên trong resource, thumbnail, ảnh bìa, waveform, proxy video.
//! Mọi thứ sinh ra đều nằm trong thư mục cache của app; file thật chỉ được đọc.

use crate::commands::AppState;
use crate::db;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Manager, State};

type Res<T> = Result<T, String>;
fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

pub const IMAGE_NATIVE: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"];
pub const IMAGE_FFMPEG: &[&str] = &["exr", "tga", "dpx", "psd", "heic", "avif", "jxl"];
pub const VIDEO: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm", "avi", "mxf", "wmv", "flv", "mts", "m2ts", "mpg", "mpeg", "3gp"];
pub const AUDIO: &[&str] = &["wav", "mp3", "aif", "aiff", "flac", "ogg", "m4a", "aac", "wma", "opus"];
pub const TEXT: &[&str] = &[
    "txt", "md", "nfo", "ini", "json", "xml", "cube", "3dl", "csv", "log", "setting", "jsx", "lua", "py", "dctl",
    "html", "htm", "url", "srt", "cfg", "yaml", "yml", "fuse",
];
const TEXT_LIMIT: usize = 256 * 1024;
const EXTRACT_LIMIT: u64 = 1024 * 1024 * 1024;

pub fn ext_of(name: &str) -> String {
    name.rsplit_once('.').map(|(_, x)| x.to_lowercase()).unwrap_or_default()
}

pub fn file_kind(name: &str) -> &'static str {
    let ext = ext_of(name);
    let x = ext.as_str();
    if IMAGE_NATIVE.contains(&x) || IMAGE_FFMPEG.contains(&x) {
        "image"
    } else if VIDEO.contains(&x) {
        "video"
    } else if AUDIO.contains(&x) {
        "audio"
    } else if x == "pdf" {
        "pdf"
    } else if TEXT.contains(&x) || name.to_lowercase().starts_with("readme") {
        "text"
    } else if x == "zip" {
        "archive"
    } else {
        "other"
    }
}

// ---------------------------------------------------------------- paths & cache

pub fn ffmpeg_path() -> PathBuf {
    let dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
    dir.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" })
}

pub fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

fn ffmpeg() -> Command {
    let mut c = Command::new(ffmpeg_path());
    hidden(&mut c);
    c.arg("-hide_banner").arg("-loglevel").arg("error").stdin(Stdio::null());
    c
}

pub fn cache_dir(state: &AppState) -> PathBuf {
    state.storage_dir().join("cache")
}

fn key_of(parts: &[&str]) -> String {
    let mut h = DefaultHasher::new();
    for p in parts {
        p.hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

/// Nguồn vật lý đã phân giải: đường dẫn tuyệt đối + loại + dấu thời gian (cho cache key).
pub struct SourceRef {
    pub abs: PathBuf,
    pub kind: String,
    pub stamp: String,
}

pub fn resolve_source(conn: &Connection, source_id: i64) -> Res<SourceRef> {
    let (lib, rel, kind, modified, size): (String, String, String, Option<i64>, i64) = conn
        .query_row(
            "SELECT l.path, s.rel_path, s.kind, s.modified_at, s.size FROM resource_sources s
             JOIN libraries l ON l.id = s.library_id WHERE s.id = ?1",
            [source_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(e)?;
    Ok(SourceRef { abs: Path::new(&lib).join(rel), kind, stamp: format!("{}:{}", modified.unwrap_or(0), size) })
}

/// Nguồn cho lệnh xem trước: một source của resource, hoặc một asset trong Media Browser (file đơn trên đĩa).
pub fn resolve_target(conn: &Connection, source_id: Option<i64>, asset_id: Option<i64>) -> Res<SourceRef> {
    if let Some(id) = source_id {
        return resolve_source(conn, id);
    }
    let id = asset_id.ok_or("Thiếu source hoặc asset")?;
    let (lib, rel, modified, size): (String, String, i64, i64) = conn
        .query_row(
            "SELECT l.path, a.rel_path, a.modified_ms, a.size FROM media_assets a JOIN libraries l ON l.id = a.library_id WHERE a.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(e)?;
    Ok(SourceRef { abs: Path::new(&lib).join(rel), kind: "file".into(), stamp: format!("{modified}:{size}") })
}

/// Tham chiếu đến một file cụ thể: file thật trên đĩa hoặc entry trong ZIP.
pub enum FileLoc {
    Disk(PathBuf),
    Zip { archive: PathBuf, entry: String },
}

fn safe_inner(inner: &str) -> Res<()> {
    if inner.split(['/', '\\']).any(|c| c == "..") {
        return Err("Đường dẫn không hợp lệ".into());
    }
    Ok(())
}

pub fn locate(src: &SourceRef, inner: &str) -> Res<FileLoc> {
    safe_inner(inner)?;
    match src.kind.as_str() {
        "folder" => Ok(FileLoc::Disk(if inner.is_empty() { src.abs.clone() } else { src.abs.join(inner) })),
        "archive" if !inner.is_empty() => Ok(FileLoc::Zip { archive: src.abs.clone(), entry: inner.to_string() }),
        _ => Ok(FileLoc::Disk(src.abs.clone())),
    }
}

fn open_zip(path: &Path) -> Res<zip::ZipArchive<std::io::BufReader<std::fs::File>>> {
    let f = std::fs::File::open(path).map_err(e)?;
    zip::ZipArchive::new(std::io::BufReader::new(f)).map_err(e)
}

/// Trả về đường dẫn file trên đĩa; entry trong ZIP được giải nén riêng vào cache (không đụng file gốc).
fn materialize(state: &AppState, src: &SourceRef, loc: &FileLoc) -> Res<PathBuf> {
    match loc {
        FileLoc::Disk(p) => Ok(p.clone()),
        FileLoc::Zip { archive, entry } => {
            let dir = cache_dir(state).join("extract");
            std::fs::create_dir_all(&dir).map_err(e)?;
            let out = dir.join(format!("{}.{}", key_of(&[&archive.to_string_lossy(), entry, &src.stamp]), ext_of(entry)));
            if out.exists() {
                return Ok(out);
            }
            let mut z = open_zip(archive)?;
            let mut f = z.by_name(entry).map_err(e)?;
            if f.size() > EXTRACT_LIMIT {
                return Err("File trong ZIP quá lớn để xem trước".into());
            }
            let tmp = out.with_extension("part");
            let mut w = std::fs::File::create(&tmp).map_err(e)?;
            std::io::copy(&mut f, &mut w).map_err(e)?;
            w.flush().map_err(e)?;
            std::fs::rename(&tmp, &out).map_err(e)?;
            Ok(out)
        }
    }
}

// ---------------------------------------------------------------- listing

#[derive(Serialize, Clone)]
pub struct FileEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    kind: String,
}

fn sort_entries(v: &mut [FileEntry]) {
    v.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
}

fn list_zip_dir(archive: &Path, dir: &str) -> Res<Vec<FileEntry>> {
    let mut z = open_zip(archive)?;
    let prefix = if dir.is_empty() { String::new() } else { format!("{}/", dir.trim_end_matches('/')) };
    let mut dirs = std::collections::BTreeMap::<String, ()>::new();
    let mut out = Vec::new();
    for i in 0..z.len() {
        let Ok(f) = z.by_index_raw(i) else { continue };
        let name = f.name().replace('\\', "/");
        let Some(rest) = name.strip_prefix(&prefix) else { continue };
        if rest.is_empty() {
            continue;
        }
        match rest.split_once('/') {
            Some((d, _)) => {
                dirs.insert(d.to_string(), ());
            }
            None if !f.is_dir() => out.push(FileEntry {
                name: rest.to_string(),
                path: name.clone(),
                is_dir: false,
                size: f.size(),
                kind: file_kind(rest).into(),
            }),
            None => {}
        }
    }
    for d in dirs.into_keys() {
        out.push(FileEntry { path: format!("{prefix}{d}"), name: d, is_dir: true, size: 0, kind: "dir".into() });
    }
    sort_entries(&mut out);
    Ok(out)
}

fn list_disk_dir(root: &Path, dir: &str) -> Res<Vec<FileEntry>> {
    let base = if dir.is_empty() { root.to_path_buf() } else { root.join(dir) };
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&base).map_err(e)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(meta) = entry.metadata() else { continue };
        let path = if dir.is_empty() { name.clone() } else { format!("{dir}/{name}") };
        out.push(FileEntry {
            kind: if meta.is_dir() { "dir".into() } else { file_kind(&name).into() },
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            name,
            path,
        });
    }
    sort_entries(&mut out);
    Ok(out)
}

#[tauri::command]
pub async fn list_source_files(app: AppHandle, source_id: i64, dir: String) -> Res<Vec<FileEntry>> {
    tauri::async_runtime::spawn_blocking(move || {
        safe_inner(&dir)?;
        let state = app.state::<AppState>();
        let src = resolve_source(&state.db.lock().unwrap(), source_id)?;
        match src.kind.as_str() {
            "folder" => list_disk_dir(&src.abs, &dir),
            "archive" => {
                if ext_of(&src.abs.to_string_lossy()) != "zip" {
                    return Err("Chưa hỗ trợ xem bên trong RAR/7Z".into());
                }
                list_zip_dir(&src.abs, &dir)
            }
            _ => {
                let name = src.abs.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let size = std::fs::metadata(&src.abs).map(|m| m.len()).unwrap_or(0);
                Ok(vec![FileEntry { kind: file_kind(&name).into(), name, path: String::new(), is_dir: false, size }])
            }
        }
    })
    .await
    .map_err(e)?
}

// ---------------------------------------------------------------- media info

#[derive(Serialize, Default, Clone)]
pub struct MediaInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub fps: Option<f64>,
}

pub fn probe(path: &Path) -> MediaInfo {
    let out = {
        let mut c = Command::new(ffmpeg_path());
        hidden(&mut c);
        c.arg("-hide_banner").arg("-i").arg(path).stdin(Stdio::null()).output()
    };
    let Ok(out) = out else { return MediaInfo::default() };
    parse_probe(&String::from_utf8_lossy(&out.stderr))
}

pub fn parse_probe(text: &str) -> MediaInfo {
    let mut info = MediaInfo::default();
    for line in text.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("Duration: ") {
            let d = rest.split(',').next().unwrap_or("");
            let parts: Vec<f64> = d.split(':').filter_map(|x| x.trim().parse().ok()).collect();
            if parts.len() == 3 {
                info.duration = parts[0] * 3600.0 + parts[1] * 60.0 + parts[2];
            }
        }
        if l.starts_with("Stream #") {
            if let Some(idx) = l.find("Video: ") {
                if info.video_codec.is_some() {
                    continue;
                }
                let rest = &l[idx + 7..];
                info.video_codec = rest.split([' ', ',']).next().map(str::to_string);
                for part in rest.split(',') {
                    let token = part.trim().split(' ').next().unwrap_or("");
                    if let Some((w, h)) = token.split_once('x') {
                        if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                            if info.width == 0 {
                                info.width = w;
                                info.height = h;
                            }
                        }
                    }
                    if part.trim().ends_with(" fps") {
                        info.fps = part.trim().trim_end_matches(" fps").parse().ok();
                    }
                }
            } else if let Some(idx) = l.find("Audio: ") {
                if info.audio_codec.is_none() {
                    info.audio_codec = l[idx + 7..].split([' ', ',']).next().map(str::to_string);
                }
            }
        }
    }
    info
}

/// WebView2 phát trực tiếp được không (không cần proxy).
pub fn browser_playable(name: &str, info: &MediaInfo) -> bool {
    let ext = ext_of(name);
    let container_ok = matches!(ext.as_str(), "mp4" | "m4v" | "mov" | "webm");
    let v_ok = info.video_codec.as_deref().map(|c| matches!(c, "h264" | "vp8" | "vp9" | "av1")).unwrap_or(true);
    let a_ok = info.audio_codec.as_deref().map(|c| matches!(c, "aac" | "mp3" | "opus" | "vorbis" | "flac")).unwrap_or(true);
    container_ok && v_ok && a_ok
}

// ---------------------------------------------------------------- thumbnails

pub fn image_thumb(input: &Path, out: &Path, max: u32) -> Res<()> {
    let ext = ext_of(&input.to_string_lossy());
    if IMAGE_NATIVE.contains(&ext.as_str()) {
        if let Ok(img) = image::open(input) {
            let t = img.thumbnail(max, max).to_rgb8();
            return t.save_with_format(out, image::ImageFormat::Jpeg).map_err(e);
        }
    }
    // định dạng khác (EXR, PSD, TGA...) hoặc ảnh lỗi -> FFmpeg
    let status = ffmpeg()
        .arg("-y")
        .arg("-i")
        .arg(input)
        .args(["-frames:v", "1", "-vf", &format!("scale={max}:{max}:force_original_aspect_ratio=decrease"), "-q:v", "4"])
        .arg(out)
        .status()
        .map_err(e)?;
    if status.success() && out.exists() { Ok(()) } else { Err("Không tạo được thumbnail".into()) }
}

pub(crate) fn video_frame(input: &Path, at: f64, out: &Path, max: u32) -> Res<()> {
    let status = ffmpeg()
        .arg("-y")
        .args(["-ss", &format!("{at:.2}")])
        .arg("-i")
        .arg(input)
        .args(["-frames:v", "1", "-vf", &format!("scale={max}:{max}:force_original_aspect_ratio=decrease"), "-q:v", "4"])
        .arg(out)
        .status()
        .map_err(e)?;
    if status.success() && out.exists() { Ok(()) } else { Err("Không đọc được frame video".into()) }
}

/// Frame đại diện của video (10% thời lượng, tối đa giây thứ 3).
pub fn video_poster(input: &Path, out: &Path, max: u32) -> Res<()> {
    let info = probe(input);
    let at = if info.duration > 0.0 { (info.duration * 0.1).min(3.0) } else { 0.0 };
    video_frame(input, at, out, max).or_else(|_| video_frame(input, 0.0, out, max))
}

pub fn thumb_for(state: &AppState, src: &SourceRef, inner: &str, max: u32) -> Res<PathBuf> {
    let loc = locate(src, inner)?;
    let name = match &loc {
        FileLoc::Disk(p) => p.to_string_lossy().to_string(),
        FileLoc::Zip { entry, .. } => entry.clone(),
    };
    let dir = cache_dir(state).join("thumbs");
    std::fs::create_dir_all(&dir).map_err(e)?;
    let out = dir.join(format!("{}.jpg", key_of(&[&src.abs.to_string_lossy(), inner, &src.stamp, &max.to_string()])));
    if out.exists() {
        return Ok(out);
    }
    let input = materialize(state, src, &loc)?;
    match file_kind(&name) {
        "image" => image_thumb(&input, &out, max)?,
        "video" => {
            let info = probe(&input);
            let at = if info.duration > 0.0 { (info.duration * 0.1).min(3.0) } else { 0.0 };
            video_frame(&input, at, &out, max).or_else(|_| video_frame(&input, 0.0, &out, max))?;
        }
        _ => return Err("Không có thumbnail cho loại file này".into()),
    }
    Ok(out)
}

#[tauri::command]
pub async fn get_thumbnail(app: AppHandle, source_id: Option<i64>, asset_id: Option<i64>, path: String, size: Option<u32>) -> Res<String> {
    let sem = app.state::<AppState>().media_sem.clone();
    let _permit = sem.acquire_owned().await.map_err(e)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let src = resolve_target(&state.db.lock().unwrap(), source_id, asset_id)?;
        thumb_for(&state, &src, &path, size.unwrap_or(480).clamp(64, 1600)).map(|p| p.to_string_lossy().to_string())
    })
    .await
    .map_err(e)?
}

/// Dải 10 frame (ghép ngang) để tua nhanh khi hover.
#[tauri::command]
pub async fn get_video_sprite(app: AppHandle, source_id: Option<i64>, asset_id: Option<i64>, path: String) -> Res<String> {
    let sem = app.state::<AppState>().media_sem.clone();
    let _permit = sem.acquire_owned().await.map_err(e)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let src = resolve_target(&state.db.lock().unwrap(), source_id, asset_id)?;
        let dir = cache_dir(&state).join("sprites");
        std::fs::create_dir_all(&dir).map_err(e)?;
        let out = dir.join(format!("{}.jpg", key_of(&[&src.abs.to_string_lossy(), &path, &src.stamp])));
        if out.exists() {
            return Ok(out.to_string_lossy().to_string());
        }
        let input = materialize(&state, &src, &locate(&src, &path)?)?;
        let info = probe(&input);
        if info.duration <= 0.0 {
            return Err("Không đọc được thời lượng video".into());
        }
        const N: u32 = 10;
        const W: u32 = 320;
        let tmp = dir.join(format!("tmp-{}", key_of(&[&out.to_string_lossy()])));
        std::fs::create_dir_all(&tmp).map_err(e)?;
        let mut frames = Vec::new();
        for i in 0..N {
            let at = info.duration * (i as f64 + 0.5) / N as f64;
            let f = tmp.join(format!("{i}.jpg"));
            if video_frame(&input, at, &f, W).is_ok() {
                if let Ok(img) = image::open(&f) {
                    frames.push(img.to_rgb8());
                }
            }
        }
        let _ = std::fs::remove_dir_all(&tmp);
        if frames.is_empty() {
            return Err("Không tạo được sprite".into());
        }
        let (fw, fh) = frames[0].dimensions();
        let mut sheet = image::RgbImage::new(fw * frames.len() as u32, fh);
        for (i, f) in frames.iter().enumerate() {
            let f = if f.dimensions() == (fw, fh) { f.clone() } else { image::imageops::resize(f, fw, fh, image::imageops::FilterType::Triangle) };
            image::imageops::replace(&mut sheet, &f, (i as u32 * fw) as i64, 0);
        }
        sheet.save_with_format(&out, image::ImageFormat::Jpeg).map_err(e)?;
        Ok(out.to_string_lossy().to_string())
    })
    .await
    .map_err(e)?
}

// ---------------------------------------------------------------- preview

#[derive(Serialize)]
pub struct PreviewInfo {
    kind: String,
    name: String,
    /// File trên đĩa để UI mở qua asset protocol (file gốc hoặc bản giải nén trong cache).
    file: Option<String>,
    text: Option<String>,
    truncated: bool,
    media: Option<MediaInfo>,
    playable: bool,
    size: u64,
}

#[tauri::command]
pub async fn preview_file(app: AppHandle, source_id: Option<i64>, asset_id: Option<i64>, path: String) -> Res<PreviewInfo> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let src = resolve_target(&state.db.lock().unwrap(), source_id, asset_id)?;
        let loc = locate(&src, &path)?;
        let name = match &loc {
            FileLoc::Disk(p) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            FileLoc::Zip { entry, .. } => entry.rsplit('/').next().unwrap_or(entry).to_string(),
        };
        let kind = file_kind(&name).to_string();
        let mut info = PreviewInfo { kind: kind.clone(), name: name.clone(), file: None, text: None, truncated: false, media: None, playable: false, size: 0 };
        match kind.as_str() {
            "text" => {
                let mut buf = Vec::new();
                match &loc {
                    FileLoc::Disk(p) => {
                        let f = std::fs::File::open(p).map_err(e)?;
                        info.size = f.metadata().map(|m| m.len()).unwrap_or(0);
                        f.take(TEXT_LIMIT as u64).read_to_end(&mut buf).map_err(e)?;
                    }
                    FileLoc::Zip { archive, entry } => {
                        let mut z = open_zip(archive)?;
                        let f = z.by_name(entry).map_err(e)?;
                        info.size = f.size();
                        f.take(TEXT_LIMIT as u64).read_to_end(&mut buf).map_err(e)?;
                    }
                }
                info.truncated = info.size as usize > buf.len();
                info.text = Some(String::from_utf8_lossy(&buf).to_string());
            }
            "image" | "video" | "audio" | "pdf" => {
                let p = materialize(&state, &src, &loc)?;
                info.size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                if kind == "video" || kind == "audio" {
                    let m = probe(&p);
                    info.playable = if kind == "video" {
                        browser_playable(&name, &m)
                    } else {
                        matches!(ext_of(&name).as_str(), "wav" | "mp3" | "flac" | "ogg" | "m4a" | "aac" | "opus")
                    };
                    info.media = Some(m);
                } else {
                    info.playable = kind != "image" || IMAGE_NATIVE.contains(&ext_of(&name).as_str());
                }
                info.file = Some(p.to_string_lossy().to_string());
            }
            _ => {}
        }
        Ok(info)
    })
    .await
    .map_err(e)?
}

/// Waveform (đỉnh biên độ, 0..1) — decode bằng FFmpeg, cache JSON.
#[tauri::command]
pub async fn get_waveform(app: AppHandle, source_id: Option<i64>, asset_id: Option<i64>, path: String) -> Res<Vec<f32>> {
    let sem = app.state::<AppState>().media_sem.clone();
    let _permit = sem.acquire_owned().await.map_err(e)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let src = resolve_target(&state.db.lock().unwrap(), source_id, asset_id)?;
        let dir = cache_dir(&state).join("waveforms");
        std::fs::create_dir_all(&dir).map_err(e)?;
        let out = dir.join(format!("{}.json", key_of(&[&src.abs.to_string_lossy(), &path, &src.stamp])));
        if let Ok(s) = std::fs::read_to_string(&out) {
            if let Ok(v) = serde_json::from_str::<Vec<f32>>(&s) {
                return Ok(v);
            }
        }
        let input = materialize(&state, &src, &locate(&src, &path)?)?;
        let output = ffmpeg()
            .arg("-i")
            .arg(&input)
            .args(["-t", "1800", "-ac", "1", "-ar", "4000", "-f", "f32le", "-"])
            .stdout(Stdio::piped())
            .output()
            .map_err(e)?;
        let samples: Vec<f32> = output.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        let peaks = peaks(&samples, 600);
        let _ = std::fs::write(&out, serde_json::to_string(&peaks).unwrap_or_default());
        Ok(peaks)
    })
    .await
    .map_err(e)?
}

pub fn peaks(samples: &[f32], bins: usize) -> Vec<f32> {
    if samples.is_empty() {
        return vec![];
    }
    let per = (samples.len() as f64 / bins as f64).max(1.0);
    let mut out = Vec::with_capacity(bins);
    let mut i = 0.0;
    while (i as usize) < samples.len() && out.len() < bins {
        let end = ((i + per) as usize).min(samples.len());
        let m = samples[i as usize..end].iter().fold(0f32, |a, &s| a.max(s.abs()));
        out.push(m.min(1.0));
        i += per;
    }
    let max = out.iter().cloned().fold(0f32, f32::max);
    if max > 0.0 {
        for v in &mut out {
            *v /= max;
        }
    }
    out
}

/// Bản xem thử (2 phút đầu, 720p) cho video WebView2 không phát được (ProRes, DNxHR, HEVC...).
/// Ưu tiên NVENC (GPU), fallback OpenH264 rồi VP9 trên CPU.
#[tauri::command]
pub async fn get_video_proxy(app: AppHandle, source_id: Option<i64>, asset_id: Option<i64>, path: String) -> Res<String> {
    let sem = app.state::<AppState>().media_sem.clone();
    let _permit = sem.acquire_owned().await.map_err(e)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let src = resolve_target(&state.db.lock().unwrap(), source_id, asset_id)?;
        let dir = cache_dir(&state).join("proxies");
        std::fs::create_dir_all(&dir).map_err(e)?;
        let key = key_of(&[&src.abs.to_string_lossy(), &path, &src.stamp]);
        for ext in ["mp4", "webm"] {
            let p = dir.join(format!("{key}.{ext}"));
            if p.exists() {
                return Ok(p.to_string_lossy().to_string());
            }
        }
        let input = materialize(&state, &src, &locate(&src, &path)?)?;
        let scale = "scale=w=1280:h=720:force_original_aspect_ratio=decrease:force_divisible_by=2";
        // GPU (NVENC) -> OpenH264 (CPU) -> VP9 (CPU)
        let attempts: [(&str, &[&str], &[&str]); 3] = [
            ("mp4", &["-c:v", "h264_nvenc", "-preset", "p4", "-cq", "28"], &["-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart"]),
            ("mp4", &["-c:v", "libopenh264", "-b:v", "3M"], &["-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart"]),
            ("webm", &["-c:v", "libvpx-vp9", "-deadline", "realtime", "-cpu-used", "8", "-b:v", "2M"], &["-c:a", "libopus", "-b:a", "128k"]),
        ];
        for (ext, venc, aenc) in attempts {
            let part = dir.join(format!("{key}.part.{ext}"));
            let ok = ffmpeg()
                .arg("-y")
                .arg("-i")
                .arg(&input)
                .args(["-t", "120", "-vf", scale, "-pix_fmt", "yuv420p"])
                .args(venc)
                .args(aenc)
                .arg(&part)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok && part.exists() {
                let fin = dir.join(format!("{key}.{ext}"));
                std::fs::rename(&part, &fin).map_err(e)?;
                return Ok(fin.to_string_lossy().to_string());
            }
            let _ = std::fs::remove_file(&part);
        }
        Err("Không tạo được bản xem thử cho video này".into())
    })
    .await
    .map_err(e)?
}

// ---------------------------------------------------------------- cover

#[derive(Serialize, serde::Deserialize, Clone)]
pub struct CoverRef {
    pub source_id: i64,
    pub path: String,
    #[serde(default)]
    pub kind: String,
}

fn cover_score(name: &str, depth: usize) -> Option<i32> {
    let kind = file_kind(name);
    let base = match kind {
        "image" => 50,
        "video" => 30,
        _ => return None,
    };
    let lower = name.to_lowercase();
    let bonus = ["preview", "cover", "thumb", "poster", "screenshot", "banner", "demo"]
        .iter()
        .any(|k| lower.contains(k)) as i32
        * 100;
    Some(base + bonus - depth as i32 * 5)
}

/// Chọn ảnh bìa tự động: ưu tiên file tên preview/cover/thumb, rồi ảnh, rồi video; nông trước sâu.
fn cover_sources(conn: &Connection, resource_id: i64) -> Res<Vec<(i64, SourceRef)>> {
    let ids: Vec<i64> = {
        let mut s = conn
            .prepare("SELECT id FROM resource_sources WHERE resource_id = ?1 AND available = 1 ORDER BY CASE kind WHEN 'folder' THEN 0 WHEN 'file' THEN 1 ELSE 2 END")
            .map_err(e)?;
        let v = s.query_map([resource_id], |r| r.get(0)).map_err(e)?.collect::<Result<_, _>>().map_err(e)?;
        v
    };
    ids.into_iter().map(|id| resolve_source(conn, id).map(|s| (id, s))).collect()
}

/// Không cần giữ lock DB: chỉ duyệt filesystem.
fn pick_cover(sources: Vec<(i64, SourceRef)>) -> Option<CoverRef> {
    let mut best: Option<(i32, CoverRef)> = None;
    fn consider(best: &mut Option<(i32, CoverRef)>, score: Option<i32>, source_id: i64, path: String, name: &str) {
        if let Some(sc) = score {
            if best.as_ref().map(|(b, _)| sc > *b).unwrap_or(true) {
                *best = Some((sc, CoverRef { source_id, path, kind: file_kind(name).into() }));
            }
        }
    }
    for (sid, src) in sources {
        match src.kind.as_str() {
            "folder" => {
                for en in walkdir::WalkDir::new(&src.abs).max_depth(4).into_iter().flatten().take(3000) {
                    if !en.file_type().is_file() {
                        continue;
                    }
                    let name = en.file_name().to_string_lossy().to_string();
                    let rel = en.path().strip_prefix(&src.abs).unwrap_or(en.path()).to_string_lossy().replace('\\', "/");
                    consider(&mut best, cover_score(&name, en.depth()), sid, rel, &name);
                }
            }
            "archive" if ext_of(&src.abs.to_string_lossy()) == "zip" => {
                if let Ok(mut z) = open_zip(&src.abs) {
                    for i in 0..z.len().min(3000) {
                        let Ok(f) = z.by_index_raw(i) else { continue };
                        if f.is_dir() || f.size() > 200 * 1024 * 1024 {
                            continue;
                        }
                        let full = f.name().to_string();
                        let name = full.rsplit('/').next().unwrap_or(&full).to_string();
                        let depth = full.matches('/').count() + 1;
                        // video trong ZIP phải giải nén mới xem được -> hạ điểm
                        let sc = cover_score(&name, depth).map(|s| if file_kind(&name) == "video" { s - 25 } else { s });
                        consider(&mut best, sc, sid, full, &name);
                    }
                }
            }
            "file" => {
                let name = src.abs.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                consider(&mut best, cover_score(&name, 0), sid, String::new(), &name);
            }
            _ => {}
        }
        if best.as_ref().map(|(s, _)| *s >= 140).unwrap_or(false) {
            break;
        }
    }
    best.map(|(_, c)| c)
}

#[derive(Serialize)]
pub struct CoverInfo {
    thumb: String,
    source_id: i64,
    path: String,
    kind: String,
}

#[tauri::command]
pub async fn get_cover(app: AppHandle, resource_id: i64) -> Res<Option<CoverInfo>> {
    let sem = app.state::<AppState>().media_sem.clone();
    let _permit = sem.acquire_owned().await.map_err(e)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        // Ảnh trong Notes là ảnh bìa, trừ khi người dùng đã tự chọn ảnh bìa từ file trong gói
        let (cover_user, first_note): (bool, Option<String>) = {
            let conn = state.db.lock().unwrap();
            let cu: i64 = conn.query_row("SELECT cover_user FROM resources WHERE id = ?1", [resource_id], |r| r.get(0)).optional().map_err(e)?.unwrap_or(0);
            let note: Option<String> = conn
                .query_row("SELECT file FROM note_images WHERE resource_id = ?1 ORDER BY position, id LIMIT 1", [resource_id], |r| r.get(0))
                .optional()
                .map_err(e)?;
            (cu != 0, note)
        };
        if !cover_user {
            if let Some(f) = first_note.filter(|f| Path::new(f).exists()) {
                return Ok(Some(CoverInfo { thumb: f, source_id: 0, path: String::new(), kind: "note".into() }));
            }
        }
        let cached: Option<String> = state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT cover FROM resources WHERE id = ?1", [resource_id], |r| r.get(0))
            .optional()
            .map_err(e)?
            .flatten();
        let cover: Option<CoverRef> = match cached {
            Some(s) if s.contains("\"none\"") => None,
            Some(s) => serde_json::from_str(&s).ok(),
            None => {
                // Quét tìm ảnh bìa không giữ lock DB lâu: chỉ lock khi cần đọc path source
                let sources = cover_sources(&state.db.lock().unwrap(), resource_id)?;
                let picked = pick_cover(sources);
                let json = match &picked {
                    Some(c) => serde_json::to_string(c).unwrap_or_default(),
                    None => "{\"none\":true}".into(),
                };
                state
                    .db
                    .lock()
                    .unwrap()
                    .execute("UPDATE resources SET cover = ?1 WHERE id = ?2 AND cover_user = 0", params![json, resource_id])
                    .map_err(e)?;
                picked
            }
        };
        let Some(c) = cover else { return Ok(None) };
        let src = resolve_source(&state.db.lock().unwrap(), c.source_id)?;
        match thumb_for(&state, &src, &c.path, 480) {
            Ok(t) => Ok(Some(CoverInfo { thumb: t.to_string_lossy().to_string(), source_id: c.source_id, path: c.path, kind: c.kind })),
            Err(_) => Ok(None),
        }
    })
    .await
    .map_err(e)?
}

#[tauri::command]
pub fn set_cover(state: State<AppState>, resource_id: i64, source_id: Option<i64>, path: Option<String>) -> Res<()> {
    let conn = state.db.lock().unwrap();
    match (source_id, path) {
        (Some(sid), Some(p)) => {
            let c = CoverRef { source_id: sid, kind: file_kind(&p).into(), path: p };
            conn.execute(
                "UPDATE resources SET cover = ?1, cover_user = 1 WHERE id = ?2",
                params![serde_json::to_string(&c).unwrap_or_default(), resource_id],
            )
            .map_err(e)?;
        }
        _ => {
            conn.execute("UPDATE resources SET cover = NULL, cover_user = 0 WHERE id = ?1", [resource_id]).map_err(e)?;
        }
    }
    db::touch(&conn, resource_id).map_err(e)
}

// ---------------------------------------------------------------- cache management

fn dir_size(p: &Path) -> (u64, Vec<(std::time::SystemTime, u64, PathBuf)>) {
    let mut total = 0;
    let mut files = Vec::new();
    for en in walkdir::WalkDir::new(p).into_iter().flatten() {
        if en.file_type().is_file() {
            if let Ok(m) = en.metadata() {
                total += m.len();
                files.push((m.accessed().or_else(|_| m.modified()).unwrap_or(std::time::UNIX_EPOCH), m.len(), en.path().to_path_buf()));
            }
        }
    }
    (total, files)
}

/// Giữ cache dưới giới hạn: xóa file ít dùng nhất (chỉ trong thư mục cache của app).
pub fn enforce_cache_limit(state: &AppState) {
    let limit_mb: u64 = {
        let conn = state.db.lock().unwrap();
        db::get_setting(&conn, "cache_limit_mb").and_then(|s| s.parse().ok()).unwrap_or(4096)
    };
    let dir = cache_dir(state);
    let (mut total, mut files) = dir_size(&dir);
    let limit = limit_mb * 1024 * 1024;
    if total <= limit {
        return;
    }
    files.sort_by_key(|f| f.0);
    for (_, size, path) in files {
        if total <= limit * 8 / 10 {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= size;
        }
    }
}

#[derive(Serialize)]
pub struct CacheStats {
    dir: String,
    size: u64,
    limit_mb: u64,
}

#[tauri::command]
pub fn cache_stats(state: State<AppState>) -> Res<CacheStats> {
    let dir = cache_dir(&state);
    let limit_mb = db::get_setting(&state.db.lock().unwrap(), "cache_limit_mb").and_then(|s| s.parse().ok()).unwrap_or(4096);
    Ok(CacheStats { size: dir_size(&dir).0, dir: dir.to_string_lossy().to_string(), limit_mb })
}

#[tauri::command]
pub fn clear_cache(state: State<AppState>) -> Res<()> {
    let dir = cache_dir(&state);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(e)?;
    }
    let conn = state.db.lock().unwrap();
    conn.execute("UPDATE resources SET cover = NULL WHERE cover_user = 0", []).map_err(e)?;
    Ok(())
}

#[tauri::command]
pub fn set_cache_limit(state: State<AppState>, mb: u64) -> Res<()> {
    db::set_setting(&state.db.lock().unwrap(), "cache_limit_mb", &mb.clamp(256, 200_000).to_string()).map_err(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ffmpeg_probe() {
        let t = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'a.mov':\n  Duration: 00:01:02.50, start: 0.000000, bitrate: 1 kb/s\n  Stream #0:0[0x1](und): Video: prores (HQ) (apch / 0x68637061), yuv422p10le, 1920x1080, 1 kb/s, 25 fps, 25 tbr\n  Stream #0:1[0x2](und): Audio: pcm_s16le (sowt / 0x74776F73), 48000 Hz, stereo";
        let m = parse_probe(t);
        assert_eq!(m.duration, 62.5);
        assert_eq!((m.width, m.height), (1920, 1080));
        assert_eq!(m.video_codec.as_deref(), Some("prores"));
        assert_eq!(m.audio_codec.as_deref(), Some("pcm_s16le"));
        assert_eq!(m.fps, Some(25.0));
        assert!(!browser_playable("a.mov", &m));
    }

    #[test]
    fn peaks_normalized() {
        let p = peaks(&[0.0, 0.5, -0.25, 0.1], 2);
        assert_eq!(p, vec![1.0, 0.5]);
    }

    #[test]
    fn cover_prefers_named_preview() {
        assert!(cover_score("preview.jpg", 2) > cover_score("a.png", 1));
        assert!(cover_score("a.png", 1) > cover_score("b.mov", 1));
        assert!(cover_score("a.wav", 0).is_none());
    }

    #[test]
    fn rejects_parent_traversal() {
        assert!(safe_inner("../x").is_err());
        assert!(safe_inner("a/b.png").is_ok());
    }
}
