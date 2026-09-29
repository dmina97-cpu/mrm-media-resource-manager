//! Đề xuất Resource Type / Application dựa trên phần mở rộng file bên trong resource.
//! Chỉ là gợi ý — người dùng xác nhận trước khi áp dụng.

use std::collections::HashMap;

/// (extension, type, app)
const RULES: &[(&str, Option<&str>, Option<&str>)] = &[
    // Color
    ("cube", Some("LUT"), None),
    ("3dl", Some("LUT"), None),
    ("look", Some("LUT"), None),
    ("dctl", Some("DCTL"), Some("DaVinci Resolve")),
    ("drx", Some("PowerGrade"), Some("DaVinci Resolve")),
    // DaVinci
    ("drfx", Some("Fusion"), Some("DaVinci Resolve")),
    ("setting", Some("Macro"), Some("DaVinci Resolve")),
    ("drp", Some("Project File"), Some("DaVinci Resolve")),
    ("drt", Some("Template"), Some("DaVinci Resolve")),
    // Adobe
    ("aep", Some("Template"), Some("After Effects")),
    ("aet", Some("Template"), Some("After Effects")),
    ("ffx", Some("Preset"), Some("After Effects")),
    ("aex", Some("Plugin"), Some("After Effects")),
    ("jsx", Some("Script"), Some("After Effects")),
    ("jsxbin", Some("Script"), Some("After Effects")),
    ("zxp", Some("Plugin"), Some("After Effects")),
    ("mogrt", Some("Template"), Some("Premiere Pro")),
    ("prproj", Some("Project File"), Some("Premiere Pro")),
    ("prfpset", Some("Preset"), Some("Premiere Pro")),
    ("abr", Some("Brush"), Some("Photoshop")),
    ("atn", Some("Preset"), Some("Photoshop")),
    ("psd", Some("Template"), Some("Photoshop")),
    // Plugins / installers
    ("ofx", Some("Plugin"), None),
    ("plugin", Some("Plugin"), None),
    ("exe", Some("Installer"), None),
    ("msi", Some("Installer"), None),
    // 3D
    ("blend", Some("3D Model"), Some("Blender")),
    ("fbx", Some("3D Model"), None),
    ("obj", Some("3D Model"), None),
    ("c4d", Some("3D Model"), None),
    // Media
    ("wav", Some("SFX"), None),
    ("mp3", Some("SFX"), None),
    ("aif", Some("SFX"), None),
    ("aiff", Some("SFX"), None),
    ("flac", Some("Music"), None),
    ("mov", Some("Stock Video"), None),
    ("mp4", Some("Stock Video"), None),
    ("mxf", Some("Stock Video"), None),
    ("png", Some("Stock Image"), None),
    ("jpg", Some("Stock Image"), None),
    ("jpeg", Some("Stock Image"), None),
    ("exr", Some("Texture"), None),
    ("ttf", Some("Font"), None),
    ("otf", Some("Font"), None),
];

/// Extension ít quan trọng — không dùng để đề xuất (ảnh preview, tài liệu...).
const IGNORED: &[&str] = &["txt", "pdf", "url", "html", "htm", "md", "nfo", "ini", "db", "ds_store", "json", "xml"];

#[derive(Debug, Default, serde::Serialize, serde::Deserialize, Clone)]
pub struct Suggestions {
    pub types: Vec<String>,
    pub apps: Vec<String>,
    /// Gợi ý từ tên thư mục chứa (vd: nằm trong "Luts" -> LUT, "DR plugin" -> DaVinci Resolve)
    #[serde(default)]
    pub path_types: Vec<String>,
    #[serde(default)]
    pub path_apps: Vec<String>,
}

/// `ext_counts`: extension (lowercase) -> số file.
pub fn suggest(ext_counts: &HashMap<String, u64>) -> Suggestions {
    let total: u64 = ext_counts
        .iter()
        .filter(|(e, _)| !IGNORED.contains(&e.as_str()))
        .map(|(_, c)| *c)
        .sum();
    if total == 0 {
        return Suggestions::default();
    }
    let mut type_score: HashMap<&str, u64> = HashMap::new();
    let mut app_score: HashMap<&str, u64> = HashMap::new();
    for (ext, count) in ext_counts {
        if let Some((_, t, a)) = RULES.iter().find(|(e, _, _)| e == ext) {
            // Vài file đặc trưng (.aex, .dctl, .exe) cũng đủ để gợi ý, nên cộng trọng số.
            let weight = match t {
                Some("Plugin") | Some("Installer") | Some("DCTL") | Some("Fusion") | Some("Macro") | Some("Template")
                | Some("Project File") | Some("Script") | Some("PowerGrade") | Some("LUT") | Some("Brush") => count * 20,
                _ => *count,
            };
            if let Some(t) = t {
                *type_score.entry(t).or_default() += weight;
            }
            if let Some(a) = a {
                *app_score.entry(a).or_default() += weight;
            }
        }
    }
    let pick = |m: HashMap<&str, u64>| -> Vec<String> {
        let mut v: Vec<(&str, u64)> = m.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let top = v.first().map(|x| x.1).unwrap_or(0);
        v.into_iter()
            .filter(|(_, s)| *s * 5 >= top) // giữ các nhóm >= 20% nhóm lớn nhất
            .take(3)
            .map(|(n, _)| n.to_string())
            .collect()
    };
    Suggestions { types: pick(type_score), apps: pick(app_score), ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(v: &[(&str, u64)]) -> HashMap<String, u64> {
        v.iter().map(|(e, c)| (e.to_string(), *c)).collect()
    }

    #[test]
    fn workflow_bundle() {
        let s = suggest(&counts(&[("wav", 40), ("mov", 30), ("png", 2), ("pdf", 1)]));
        assert_eq!(s.types, vec!["SFX", "Stock Video"]);
    }

    #[test]
    fn plugin_installer_wins_over_readme_images() {
        let s = suggest(&counts(&[("aex", 2), ("png", 10), ("txt", 3)]));
        assert_eq!(s.types[0], "Plugin");
        assert_eq!(s.apps, vec!["After Effects"]);
    }

    #[test]
    fn nothing_known() {
        assert!(suggest(&counts(&[("txt", 3)])).types.is_empty());
    }
}
