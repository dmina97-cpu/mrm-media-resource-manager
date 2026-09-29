//! Tự cập nhật MRM qua GitHub Releases của repo chính thức.
//! - Chỉ đọc repo cố định, chỉ tải file `MRM_x.y.z_x64-setup.exe` từ đúng repo đó (không theo URL lạ).
//! - Phiên bản lấy từ TÊN FILE (repo dùng chung với plugin; một release có thể chỉ có plugin).
//! - Kiểm tra kích thước + SHA-256 (GitHub cung cấp `digest`), rồi chạy bộ cài: bộ cài tự nhận bản cũ và cài đè đúng chỗ,
//!   dữ liệu (%APPDATA%) giữ nguyên.
use crate::commands::AppState;
use crate::db;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

type Res<T> = Result<T, String>;
fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

pub const REPO: &str = "dmina97-cpu/mrm-media-resource-manager";
const CHECK_EVERY_SECS: i64 = 24 * 3600;

pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches('v');
    let mut it = s.split('.');
    let v = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    it.next().is_none().then_some(v)
}

/// "MRM_0.9.0_x64-setup.exe" -> "0.9.0"
fn asset_version(name: &str) -> Option<String> {
    let v = name.strip_prefix("MRM_")?.strip_suffix("_x64-setup.exe")?;
    parse_version(v).map(|_| v.to_string())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Latest {
    pub version: String,
    pub tag: String,
    pub notes: String,
    pub published_at: String,
    pub asset_name: String,
    pub asset_url: String,
    pub size: u64,
    pub digest: Option<String>,
    pub page_url: String,
}

/// Chọn bản MRM mới nhất trong danh sách release (bỏ draft/prerelease, bỏ URL không thuộc repo).
pub fn pick_latest(releases: &Value) -> Option<Latest> {
    let prefix = format!("https://github.com/{REPO}/releases/download/");
    let mut best: Option<Latest> = None;
    for r in releases.as_array()? {
        if r["draft"].as_bool().unwrap_or(false) || r["prerelease"].as_bool().unwrap_or(false) {
            continue;
        }
        let tag = r["tag_name"].as_str().unwrap_or("").to_string();
        for a in r["assets"].as_array().into_iter().flatten() {
            let name = a["name"].as_str().unwrap_or("");
            let url = a["browser_download_url"].as_str().unwrap_or("");
            let size = a["size"].as_u64().unwrap_or(0);
            let (Some(version), true, true) = (asset_version(name), url.starts_with(&prefix), size > 0) else { continue };
            if a["state"].as_str().is_some_and(|s| s != "uploaded") {
                continue;
            }
            let newer = best.as_ref().map(|b| parse_version(&version) > parse_version(&b.version)).unwrap_or(true);
            if newer {
                best = Some(Latest {
                    version,
                    tag: tag.clone(),
                    notes: r["body"].as_str().unwrap_or("").chars().take(8000).collect(),
                    published_at: r["published_at"].as_str().unwrap_or("").to_string(),
                    asset_name: name.to_string(),
                    asset_url: url.to_string(),
                    size,
                    digest: a["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(|d| d.to_lowercase()),
                    page_url: format!("https://github.com/{REPO}/releases/tag/{}", urlencode(&tag)),
                });
            }
        }
    }
    best
}

fn urlencode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn agent(timeout: u64) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(timeout))).build().into()
}

fn fetch_releases() -> Res<Value> {
    let mut resp = agent(15)
        .get(format!("https://api.github.com/repos/{REPO}/releases?per_page=20"))
        .header("User-Agent", "MRM-Updater")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|err| match err {
            ureq::Error::StatusCode(403) | ureq::Error::StatusCode(429) => "GitHub đang giới hạn lượt kiểm tra — thử lại sau.".to_string(),
            ureq::Error::StatusCode(404) => "Chưa có bản phát hành công khai.".to_string(),
            other => format!("Không kết nối được GitHub: {other}"),
        })?;
    resp.body_mut().with_config().limit(4 << 20).read_json().map_err(e)
}

#[derive(Serialize)]
pub struct UpdateInfo {
    current: String,
    latest: Option<Latest>,
    available: bool,
    skipped: bool,
    auto: bool,
    checked_at: Option<i64>,
    error: Option<String>,
}

fn info(conn: &rusqlite::Connection, error: Option<String>) -> UpdateInfo {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let latest: Option<Latest> = db::get_setting(conn, "update_cache").and_then(|s| serde_json::from_str(&s).ok());
    let available = latest.as_ref().map(|l| parse_version(&l.version) > parse_version(&current)).unwrap_or(false);
    let skipped = available && db::get_setting(conn, "update_skip").as_deref() == latest.as_ref().map(|l| l.version.as_str());
    UpdateInfo {
        current,
        available,
        skipped,
        latest,
        auto: db::get_setting(conn, "update_auto").as_deref() != Some("0"),
        checked_at: db::get_setting(conn, "update_checked_at").and_then(|s| s.parse().ok()),
        error,
    }
}

/// manual = false: tự kiểm tra (tối đa 1 lần/ngày, tắt được); manual = true: kiểm tra ngay.
#[tauri::command]
pub async fn update_check(app: AppHandle, manual: bool) -> Res<UpdateInfo> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        {
            let conn = state.db.lock().unwrap();
            let auto = db::get_setting(&conn, "update_auto").as_deref() != Some("0");
            let last: i64 = db::get_setting(&conn, "update_checked_at").and_then(|s| s.parse().ok()).unwrap_or(0);
            if !manual && (!auto || db::now() - last < CHECK_EVERY_SECS) {
                return Ok(info(&conn, None));
            }
        }
        let fetched = fetch_releases();
        let conn = state.db.lock().unwrap();
        let _ = db::set_setting(&conn, "update_checked_at", &db::now().to_string());
        match fetched {
            Ok(v) => {
                let latest = pick_latest(&v);
                let _ = db::set_setting(&conn, "update_cache", &latest.as_ref().map(|l| serde_json::to_string(l).unwrap_or_default()).unwrap_or_default());
                Ok(info(&conn, None))
            }
            Err(err) => {
                crate::diag::note("Kiểm tra cập nhật", &err);
                Ok(info(&conn, Some(err)))
            }
        }
    })
    .await
    .map_err(e)?
}

#[tauri::command]
pub fn update_configure(state: tauri::State<AppState>, auto: Option<bool>, skip: Option<String>) -> Res<UpdateInfo> {
    let conn = state.db.lock().unwrap();
    if let Some(a) = auto {
        db::set_setting(&conn, "update_auto", if a { "1" } else { "0" }).map_err(e)?;
    }
    if let Some(v) = skip {
        db::set_setting(&conn, "update_skip", &v).map_err(e)?;
    }
    Ok(info(&conn, None))
}

#[derive(Serialize, Clone)]
struct Progress {
    done: u64,
    total: u64,
}

fn sha256_hex(path: &std::path::Path) -> Res<String> {
    let mut f = std::fs::File::open(path).map_err(e)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf).map_err(e)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Tải bộ cài vào `dir`, kiểm tra kích thước + SHA-256; trả về đường dẫn file đã kiểm tra.
pub fn download_verified(latest: &Latest, dir: &std::path::Path, emit: &dyn Fn(u64)) -> Res<std::path::PathBuf> {
    std::fs::create_dir_all(dir).map_err(e)?;
    let out = dir.join(&latest.asset_name);
    let part = dir.join(format!("{}.part", latest.asset_name));
    let resp = agent(3600).get(&latest.asset_url).header("User-Agent", "MRM-Updater").call().map_err(e)?;
    let mut reader = resp.into_body().into_with_config().limit(400 << 20).reader();
    let mut file = std::fs::File::create(&part).map_err(e)?;
    let (mut done, mut last) = (0u64, 0u64);
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = reader.read(&mut buf).map_err(e)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(e)?;
        done += n as u64;
        if done - last > 512 * 1024 {
            last = done;
            emit(done);
        }
    }
    file.flush().map_err(e)?;
    drop(file);
    let bad = |msg: &str| {
        let _ = std::fs::remove_file(&part);
        Err(msg.to_string())
    };
    if done != latest.size {
        return bad("File tải về không đủ dung lượng — hãy thử lại.");
    }
    if let Some(want) = &latest.digest {
        if &sha256_hex(&part)? != want {
            return bad("File tải về không khớp mã kiểm tra (SHA-256) — đã hủy để an toàn.");
        }
    }
    let _ = std::fs::remove_file(&out);
    std::fs::rename(&part, &out).map_err(e)?;
    emit(done);
    Ok(out)
}

/// Tải bản mới (có tiến độ), kiểm tra toàn vẹn, chạy bộ cài rồi thoát MRM để bộ cài ghi đè.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Res<()> {
    let a = app.clone();
    let installer = tauri::async_runtime::spawn_blocking(move || -> Res<std::path::PathBuf> {
        let state = a.state::<AppState>();
        let latest: Latest = {
            let conn = state.db.lock().unwrap();
            db::get_setting(&conn, "update_cache").and_then(|s| serde_json::from_str(&s).ok()).ok_or("Chưa có thông tin bản mới — hãy kiểm tra cập nhật.")?
        };
        if parse_version(&latest.version) <= parse_version(env!("CARGO_PKG_VERSION")) {
            return Err("Đang dùng bản mới nhất.".into());
        }
        if !latest.asset_url.starts_with(&format!("https://github.com/{REPO}/releases/download/")) || asset_version(&latest.asset_name).is_none() {
            return Err("Nguồn cập nhật không hợp lệ.".into());
        }
        let dir = crate::media::cache_dir(&state).join("updates");
        let emit = |done: u64| {
            let _ = a.emit("update-progress", Progress { done, total: latest.size });
        };
        let out = download_verified(&latest, &dir, &emit)?;
        Ok(out)
    })
    .await
    .map_err(e)??;
    // ShellExecute: bộ cài có thể cần quyền admin (UAC)
    tauri_plugin_opener::open_path(&installer, None::<&str>).map_err(e)?;
    let h = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(800));
        h.exit(0);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn asset(name: &str, size: u64) -> Value {
        json!({ "name": name, "size": size, "state": "uploaded",
                "browser_download_url": format!("https://github.com/{REPO}/releases/download/v/{name}"),
                "digest": "sha256:ABCDEF" })
    }

    #[test]
    fn picks_newest_mrm_asset_across_releases() {
        let releases = json!([
            { "tag_name": "plugin-v0.13.0", "draft": false, "prerelease": false, "body": "plugin only",
              "assets": [asset("HNH_SFX_Finder_MRM_v0.13.0.zip", 10)] },
            { "tag_name": "v0.10.0-beta", "draft": false, "prerelease": true, "assets": [asset("MRM_0.10.0_x64-setup.exe", 10)] },
            { "tag_name": "v0.9.0", "draft": false, "prerelease": false, "body": "Điểm mới", "published_at": "2026-10-01",
              "assets": [asset("MRM_0.9.0_x64-setup.exe", 123), asset("HNH_SFX_Finder_MRM_v0.12.1.zip", 10)] },
            { "tag_name": "v0.8.0", "draft": false, "prerelease": false, "assets": [asset("MRM_0.8.0_x64-setup.exe", 100)] },
            { "tag_name": "evil", "draft": false, "prerelease": false,
              "assets": [{ "name": "MRM_9.9.9_x64-setup.exe", "size": 1, "browser_download_url": "https://evil.test/MRM_9.9.9_x64-setup.exe" }] }
        ]);
        let l = pick_latest(&releases).unwrap();
        assert_eq!(l.version, "0.9.0", "bỏ prerelease, bỏ URL ngoài repo, bỏ release chỉ có plugin");
        assert_eq!(l.size, 123);
        assert_eq!(l.digest.as_deref(), Some("abcdef"));
        assert_eq!(l.page_url, format!("https://github.com/{REPO}/releases/tag/v0.9.0"));
        assert!(pick_latest(&json!([])).is_none());
    }

    /// Tải bộ cài thật từ GitHub + kiểm tra SHA-256 (không chạy bộ cài): MRM_UPDATE_LIVE=1 cargo test live_download -- --ignored
    #[test]
    #[ignore]
    fn live_download() {
        if std::env::var("MRM_UPDATE_LIVE").is_err() {
            return;
        }
        let latest = pick_latest(&fetch_releases().unwrap()).expect("có bản phát hành");
        assert!(latest.digest.is_some(), "GitHub cung cấp digest");
        let dir = std::env::temp_dir().join(format!("mrm_upd_{}", std::process::id()));
        let path = download_verified(&latest, &dir, &|_| {}).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), latest.size);
        // digest sai -> từ chối, không để lại file
        let mut wrong = latest.clone();
        wrong.digest = Some("00".repeat(32));
        wrong.asset_name = format!("x_{}", latest.asset_name);
        assert!(download_verified(&wrong, &dir, &|_| {}).unwrap_err().contains("SHA-256"));
        assert!(!dir.join(&wrong.asset_name).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn version_parsing() {
        assert_eq!(asset_version("MRM_0.10.2_x64-setup.exe").as_deref(), Some("0.10.2"));
        assert!(asset_version("MRM_0.10_x64-setup.exe").is_none());
        assert!(parse_version("v0.10.0") > parse_version("0.9.9"));
        assert!(parse_version("0.8.0-beta").is_none());
    }
}
