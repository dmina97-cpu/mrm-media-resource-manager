//! Chuẩn hóa tên file/folder và tính độ tương đồng để ghép ZIP ↔ folder.

/// Các token "rác" thường gặp trong tên gói tải về, không mang nghĩa nhận diện.
const NOISE: &[&str] = &[
    "full", "crack", "cracked", "repack", "portable", "x64", "x86", "win", "win64", "win32",
    "windows", "setup", "installer", "keygen", "patch", "patched", "final", "copy", "www",
    "com", "net", "free", "download", "retail", "multilingual", "incl", "rar", "zip",
];

const ARCHIVE_EXT: &[&str] = &["zip", "rar", "7z"];

#[derive(Debug, Clone, PartialEq)]
pub struct Normalized {
    /// Token có nghĩa (đã bỏ noise, bỏ version).
    pub tokens: Vec<String>,
    /// Token nối liền không khoảng trắng — xử lý "MotionBro" vs "Motion Bro".
    pub compact: String,
    pub version: Option<String>,
    /// Chuỗi hiển thị đã chuẩn hóa, gồm cả version.
    pub display: String,
}

pub fn is_archive_ext(ext: &str) -> bool {
    ARCHIVE_EXT.contains(&ext.to_ascii_lowercase().as_str())
}

/// Bỏ phần mở rộng (chỉ dùng cho file). Xử lý cả `.part1.rar`.
pub fn strip_extension(name: &str) -> &str {
    let mut s = name;
    if let Some(pos) = s.rfind('.') {
        let ext = &s[pos + 1..];
        if !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && !ext.chars().all(|c| c.is_ascii_digit())
        {
            s = &s[..pos];
        }
    }
    // "Pack.part1" -> "Pack"
    if let Some(pos) = s.rfind('.') {
        let tail = s[pos + 1..].to_ascii_lowercase();
        if tail.starts_with("part") && tail[4..].chars().all(|c| c.is_ascii_digit()) && tail.len() > 4 {
            s = &s[..pos];
        }
    }
    s
}

/// Bỏ hậu tố Google Drive khi tải thư mục lớn: "Pack-20260923T035558Z-1-001" -> "Pack".
pub fn strip_drive_suffix(s: &str) -> &str {
    let b = s.as_bytes();
    // tìm "-YYYYMMDDTHHMMSSZ"
    for i in 0..b.len() {
        if b[i] != b'-' || i + 17 > b.len() {
            continue;
        }
        let d = &b[i + 1..i + 17];
        let ok = d[..8].iter().all(u8::is_ascii_digit)
            && d[8] == b'T'
            && d[9..15].iter().all(u8::is_ascii_digit)
            && d[15] == b'Z';
        if ok {
            return s[..i].trim_end();
        }
    }
    s
}

/// Tên hiển thị đẹp cho resource mới: bỏ extension, `_` -> khoảng trắng.
pub fn pretty_name(name: &str, is_file: bool) -> String {
    let base = if is_file { strip_extension(name) } else { name };
    let base = strip_drive_suffix(base);
    let s: String = base.chars().map(|c| if c == '_' { ' ' } else { c }).collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() { name.to_string() } else { s }
}

/// Tách camelCase và ranh giới chữ/số: "FilmConvertV3" -> "Film Convert V 3".
fn split_boundaries(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 8);
    for i in 0..chars.len() {
        let c = chars[i];
        if i > 0 {
            let p = chars[i - 1];
            let next_lower = chars.get(i + 1).map(|n| n.is_lowercase()).unwrap_or(false);
            let boundary = (p.is_lowercase() && c.is_uppercase())
                || (p.is_uppercase() && c.is_uppercase() && next_lower)
                || (p.is_alphabetic() && c.is_ascii_digit())
                || (p.is_ascii_digit() && c.is_alphabetic());
            if boundary {
                out.push(' ');
            }
        }
        out.push(c);
    }
    out
}

fn is_version_token(t: &str) -> bool {
    let parts: Vec<&str> = t.split('.').collect();
    parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

pub fn normalize(name: &str, is_file: bool) -> Normalized {
    let base = if is_file { strip_extension(name) } else { name };
    let base = strip_drive_suffix(base);
    let split = split_boundaries(base);

    // Giữ chữ/số; '.' chỉ giữ khi nằm giữa 2 chữ số (version), còn lại là dấu phân cách.
    let chars: Vec<char> = split.chars().collect();
    let mut cleaned = String::with_capacity(chars.len());
    for (i, &c) in chars.iter().enumerate() {
        if c.is_alphanumeric() {
            cleaned.extend(c.to_lowercase());
        } else if c == '.'
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && chars.get(i + 1).map(|n| n.is_ascii_digit()).unwrap_or(false)
        {
            cleaned.push('.');
        } else {
            cleaned.push(' ');
        }
    }

    let raw: Vec<&str> = cleaned.split_whitespace().collect();
    let mut tokens = Vec::new();
    let mut version: Option<String> = None;
    let mut display = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let t = raw[i];
        let next = raw.get(i + 1).copied();
        // "v 3.52", "ver 2", "version 4.5"
        if matches!(t, "v" | "ver" | "version")
            && next.map(|n| n.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)).unwrap_or(false)
        {
            let n = next.unwrap();
            if version.is_none() {
                version = Some(n.to_string());
            }
            display.push(n.to_string());
            i += 2;
            continue;
        }
        // "x64" / "Win64" bị tách thành "x 64" / "win 64" ở bước trên
        if matches!(t, "x" | "win") && matches!(next, Some("64") | Some("86") | Some("32")) {
            i += 2;
            continue;
        }
        if is_version_token(t) {
            if version.is_none() {
                version = Some(t.to_string());
            }
            display.push(t.to_string());
            i += 1;
            continue;
        }
        if NOISE.contains(&t) || (t.starts_with("part") && t.len() > 4 && t[4..].chars().all(|c| c.is_ascii_digit())) {
            i += 1;
            continue;
        }
        tokens.push(t.to_string());
        display.push(t.to_string());
        i += 1;
    }

    let compact = tokens.concat();
    Normalized { tokens, compact, version, display: display.join(" ") }
}

fn token_sort_ratio(a: &[String], b: &[String]) -> f64 {
    let mut x = a.to_vec();
    let mut y = b.to_vec();
    x.sort();
    y.sort();
    strsim::normalized_levenshtein(&x.join(" "), &y.join(" "))
}

/// Điểm tương đồng 0–100.
pub fn similarity(a: &Normalized, b: &Normalized) -> f64 {
    if a.compact.is_empty() || b.compact.is_empty() {
        return 0.0;
    }
    let compact = strsim::normalized_levenshtein(&a.compact, &b.compact);
    let tokens = token_sort_ratio(&a.tokens, &b.tokens);
    let mut score = compact.max(tokens) * 100.0;

    match (&a.version, &b.version) {
        (Some(x), Some(y)) if x != y => score = score.min(80.0), // khác version -> không tự ghép
        (Some(_), None) | (None, Some(_)) => score *= 0.97,
        _ => {}
    }
    (score * 10.0).round() / 10.0
}

pub const AUTO_MATCH: f64 = 95.0;
pub const POSSIBLE_MATCH: f64 = 75.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn sim(a: &str, a_file: bool, b: &str, b_file: bool) -> f64 {
        similarity(&normalize(a, a_file), &normalize(b, b_file))
    }

    #[test]
    fn normalizes_doc_examples() {
        let a = normalize("FilmConvert_Nitrate_v3.52.zip", true);
        assert_eq!(a.display, "film convert nitrate 3.52");
        assert_eq!(a.version.as_deref(), Some("3.52"));
        let b = normalize("FilmConvert Nitrate 3.52", false);
        assert_eq!(a, b);
    }

    #[test]
    fn folder_name_with_dots_is_not_stripped() {
        let n = normalize("Pack 2.0", false);
        assert_eq!(n.version.as_deref(), Some("2.0"));
        assert_eq!(n.tokens, vec!["pack"]);
    }

    #[test]
    fn motion_bro_variants_auto_match() {
        assert!(sim("Motion Bro 4.5 FULL.zip", true, "MotionBro_v4.5", false) >= AUTO_MATCH);
    }

    #[test]
    fn identical_names_score_100() {
        assert_eq!(sim("Cinematic Editing Bundle.zip", true, "Cinematic Editing Bundle", false), 100.0);
    }

    #[test]
    fn multipart_and_noise() {
        assert_eq!(sim("Magic Bullet Suite.part1.rar", true, "Magic_Bullet_Suite (x64) [Repack]", false), 100.0);
    }

    #[test]
    fn different_versions_are_only_possible() {
        let s = sim("Twitch v1.0.zip", true, "Twitch v2.0", false);
        assert!(s < AUTO_MATCH && s >= POSSIBLE_MATCH, "{s}");
    }

    #[test]
    fn missing_version_on_one_side_still_matches() {
        assert!(sim("FilmConvert_Nitrate_v3.52.zip", true, "FilmConvert Nitrate", false) >= AUTO_MATCH);
    }

    #[test]
    fn unrelated_names_are_separate() {
        assert!(sim("Light Leaks Pack.zip", true, "Whoosh SFX", false) < POSSIBLE_MATCH);
    }

    #[test]
    fn small_typo_is_possible_match() {
        let s = sim("Glitch Transitions Pack.zip", true, "Glitch Transition Pack Vol 2", false);
        assert!(s >= POSSIBLE_MATCH && s < AUTO_MATCH, "{s}");
    }

    #[test]
    fn google_drive_parts_group_together() {
        let a = normalize("MotionVFX Package-20260923T035558Z-1-001.zip", true);
        let b = normalize("MotionVFX Package-20260923T035558Z-1-005.zip", true);
        assert_eq!(a, b);
        assert_eq!(a.display, "motion vfx package");
        assert_eq!(pretty_name("VanBeek Films-20260923T040018Z-1-002.zip", true), "VanBeek Films");
    }

    #[test]
    fn pretty_names() {
        assert_eq!(pretty_name("FilmConvert_Nitrate_v3.52.zip", true), "FilmConvert Nitrate v3.52");
        assert_eq!(pretty_name("Pack 2.0", false), "Pack 2.0");
    }
}
