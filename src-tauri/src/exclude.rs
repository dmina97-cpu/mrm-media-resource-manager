//! Loại trừ thư mục khi quét (cache của app, node_modules, thùng rác…).
//! Mỗi luật là một đoạn đường dẫn, so khớp với các thư mục/file NẰM TRONG library (không tính phần đường dẫn của library):
//!   `node_modules`          -> bất kỳ thư mục/file tên node_modules
//!   `User Data/Cache`       -> hai cấp liên tiếp "User Data" rồi "Cache" (vd: cache của CapCut)
//!   `*.tmp`, `Cache*`       -> dấu * thay cho nhiều ký tự trong một tên
//! Không phân biệt hoa thường. File bị loại trừ chỉ không được lập chỉ mục — MRM không bao giờ xóa file.
use crate::commands::AppState;
use crate::db;
use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;
use std::sync::RwLock;
use tauri::State;

type Res<T> = Result<T, String>;

pub const DEFAULTS: &[&str] = &[
    "node_modules",
    "__MACOSX",
    "$RECYCLE.BIN",
    "System Volume Information",
    // cache của các app nền Chromium/Electron (CapCut, Adobe…) — hàng nghìn ảnh/frame rác
    "User Data/Cache",
    "Cache/Cache_Data",
    "GPUCache",
    "Code Cache",
];

#[derive(Default)]
struct Rules {
    /// mỗi luật: danh sách đoạn (đã viết thường)
    rules: Vec<Vec<String>>,
    /// gốc các library (viết thường, dấu '/') — phần này được bỏ qua khi so khớp
    roots: Vec<String>,
}

static RULES: RwLock<Option<Rules>> = RwLock::new(None);

fn norm(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

fn parse(rule: &str) -> Option<Vec<String>> {
    let segs: Vec<String> = rule.split(['/', '\\']).map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect();
    (!segs.is_empty()).then_some(segs)
}

/// So khớp một đoạn có dấu *.
fn glob(pat: &str, s: &str) -> bool {
    if !pat.contains('*') {
        return pat == s;
    }
    let parts: Vec<&str> = pat.split('*').collect();
    let mut rest = s;
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() {
            continue;
        }
        if i == 0 {
            match rest.strip_prefix(p) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(p);
        } else {
            match rest.find(p) {
                Some(pos) => rest = &rest[pos + p.len()..],
                None => return false,
            }
        }
    }
    true
}

fn matches(rules: &[Vec<String>], segs: &[&str]) -> bool {
    rules.iter().any(|r| r.len() <= segs.len() && segs.windows(r.len()).any(|w| w.iter().zip(r).all(|(s, p)| glob(p, s))))
}

/// Danh sách luật đang dùng (mặc định nếu người dùng chưa chỉnh).
pub fn current_rules(conn: &Connection) -> Vec<String> {
    db::get_setting(conn, "scan_excludes")
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_else(|| DEFAULTS.iter().map(|s| s.to_string()).collect())
}

/// Nạp lại luật + gốc library (gọi trước mỗi lần quét).
pub fn reload(conn: &Connection) {
    let rules = current_rules(conn).iter().filter_map(|r| parse(r)).collect();
    let mut roots: Vec<String> = conn
        .prepare("SELECT path FROM libraries")
        .and_then(|mut s| s.query_map([], |r| r.get::<_, String>(0)).map(|rows| rows.flatten().map(|p| norm(&p)).collect()))
        .unwrap_or_default();
    roots.sort_by_key(|r| std::cmp::Reverse(r.len())); // gốc dài nhất trước (library lồng nhau)
    *RULES.write().unwrap() = Some(Rules { rules, roots });
}

/// Đường dẫn (tuyệt đối) này có bị loại trừ không?
pub fn hit(path: &Path) -> bool {
    let guard = RULES.read().unwrap();
    guard.as_ref().map(|r| hit_in(r, path)).unwrap_or(false)
}

fn hit_in(r: &Rules, path: &Path) -> bool {
    if r.rules.is_empty() {
        return false;
    }
    let p = norm(&path.to_string_lossy());
    let inner = r
        .roots
        .iter()
        .find_map(|root| p.strip_prefix(root.as_str()).filter(|rest| rest.is_empty() || rest.starts_with('/')))
        .unwrap_or(&p);
    let segs: Vec<&str> = inner.split('/').filter(|s| !s.is_empty()).collect();
    matches(&r.rules, &segs)
}

#[derive(Serialize)]
pub struct ExcludeInfo {
    rules: Vec<String>,
    defaults: Vec<String>,
}

#[tauri::command]
pub fn get_scan_excludes(state: State<AppState>) -> ExcludeInfo {
    let conn = state.db.lock().unwrap();
    ExcludeInfo { rules: current_rules(&conn), defaults: DEFAULTS.iter().map(|s| s.to_string()).collect() }
}

#[tauri::command]
pub fn set_scan_excludes(state: State<AppState>, rules: Vec<String>) -> Res<()> {
    let mut clean: Vec<String> = Vec::new();
    for r in rules {
        let r = r.trim().trim_matches(['/', '\\']).to_string();
        if r.is_empty() || clean.iter().any(|c| c.eq_ignore_ascii_case(&r)) {
            continue;
        }
        if parse(&r).map(|s| s.iter().all(|x| x == "*")).unwrap_or(true) {
            return Err(format!("Luật “{r}” sẽ loại trừ mọi thứ — hãy ghi cụ thể hơn."));
        }
        clean.push(r);
    }
    let conn = state.db.lock().unwrap();
    db::set_setting(&conn, "scan_excludes", &serde_json::to_string(&clean).unwrap_or_default()).map_err(|e| e.to_string())?;
    reload(&conn);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(rules: &[&str], roots: &[&str]) -> Rules {
        Rules { rules: rules.iter().filter_map(|r| parse(r)).collect(), roots: roots.iter().map(|r| norm(r)).collect() }
    }

    #[test]
    fn rules_match_inside_library_only() {
        let r = rules(&["node_modules", "User Data/Cache", "*.tmp", "cache*"], &["D:\\Lib"]);
        let hit = |p: &str| hit_in(&r, Path::new(p));
        assert!(hit("D:\\Lib\\pack\\node_modules"));
        assert!(hit("D:\\Lib\\CapCut\\User Data\\Cache\\frame\\a.png"));
        assert!(!hit("D:\\Lib\\CapCut\\User Data\\Profiles\\a.png"), "chỉ 'User Data' không đủ");
        assert!(hit("D:\\Lib\\x\\render.TMP"), "không phân biệt hoa thường");
        assert!(hit("D:\\Lib\\x\\CacheStorage"), "glob đầu tên");
        assert!(!hit("D:\\Lib\\x\\mycache"));
        assert!(!hit("D:\\Lib"), "không loại trừ chính library");
        // phần đường dẫn của library không bị so khớp
        let r2 = rules(&["cache*"], &["D:\\CacheDrive\\SFX"]);
        let hit2 = |p: &str| hit_in(&r2, Path::new(p));
        assert!(!hit2("D:\\CacheDrive\\SFX\\whoosh.wav"));
        assert!(hit2("D:\\CacheDrive\\SFX\\cache_old\\a.wav"));
    }

    #[test]
    fn glob_basics() {
        assert!(glob("a*c", "abc"));
        assert!(glob("a*c", "ac"));
        assert!(!glob("a*c", "abd"));
        assert!(glob("*x*", "zzxzz"));
        assert!(glob("*", "anything"));
    }
}
