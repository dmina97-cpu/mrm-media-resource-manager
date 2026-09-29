//! Chế độ quét "Tự động": tự quyết định mỗi thư mục là *thư mục chứa* (đi vào trong)
//! hay *gói tài nguyên* (dừng lại, cả thư mục là một resource).
//! Quy tắc được hiệu chỉnh trên thư viện thật (DaVinci/CapCut/Lightroom, tải từ Envato, Drive…).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

const ARCHIVES: &[&str] = &["zip", "rar", "7z"];
/// File "phụ" — không phải tài nguyên (link, ghi chú, preview, tài liệu).
const META: &[&str] = &[
    "url", "lnk", "txt", "nfo", "html", "htm", "md", "ini", "db", "log", "json", "xml", "pdf", "jpg", "jpeg", "png",
    "docx", "doc", "rtf", "webloc",
];
/// File bỏ qua hoàn toàn khi nằm ở tầng thư mục chứa (không tạo resource).
pub const CONTAINER_SKIP: &[&str] = &["url", "lnk", "txt", "nfo", "html", "htm", "md", "ini", "db", "log", "webloc"];
const JUNK_DIRS: &[&str] = &["__macosx", "$recycle.bin", "system volume information"];
const CODE_MARKERS: &[&str] = &[
    "package.json", "node_modules", ".git", "cargo.toml", "csxs", "manifest.xml", "pyproject.toml", "requirements.txt",
];
const GENERIC_EXACT: &[&str] = &[
    "help", "help documentation", "help file", "help files", "documentation", "docs", "doc", "tutorial", "tutorials", "fonts",
    "font", "macro", "macros", "macro file", "macro files", "drfx file", "drfx files", "project file", "project files", "preview",
    "previews", "assets", "extra", "extras", "extra files", "bonus", "old version", "new version", "win64", "x64", "x86", "windows",
    "win", "mac", "macos", "osx", "contents", "bin", "lib", "resources", "presets", "preset", "lut", "luts", "powergrade",
    "powergrades", "power grade", "power grades", "sfx", "sound effects", "audio", "music", "video", "videos", "overlays",
    "textures", "transitions", "titles", "elements", "graphics", "footage", "plugin", "plugins", "plug in", "plug ins", "install",
    "installer", "setup", "crack", "patch", "keygen", "readme", "source", "templates", "template", "grain", "mattes", "dr", "lr",
    "ps", "ae", "pr", "apps", "app", "user data", "userdata", "cache", "data", "config", "logs", "temp", "tmp", "locales",
    "davinci resolve", "premiere pro", "after effects", "photoshop", "lightroom", "capcut", "images", "image", "photos", "icons",
];
const GENERIC_WORDS: &[&str] = &[
    "overlays", "overlay", "luts", "lut", "presets", "transitions", "sfx", "fonts", "titles", "textures", "mattes", "grain",
    "elements", "assets", "tutorials", "tutorial", "help", "macros", "audio", "music", "footage", "documentation", "video",
    "videos", "fx", "matte", "字体",
];
const DOC_PREFIX: &[&str] = &[
    "help", "readme", "read me", "tutorial", "documentation", "how to", "huong dan", "hướng dẫn", "instruction", "guide",
];

pub fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(p) if p + 1 < name.len() => name[p + 1..].to_lowercase(),
        _ => String::new(),
    }
}

fn is_junk(name: &str) -> bool {
    let l = name.to_lowercase();
    JUNK_DIRS.contains(&l.as_str()) || l == "desktop.ini" || l == "thumbs.db" || l == ".ds_store"
}

/// Bỏ số thứ tự đầu tên và dấu câu: "02 GRAIN" -> "grain", "TC_-_COLOR" -> "tc color".
fn norm_words(name: &str) -> String {
    let lower = name.to_lowercase();
    let trimmed = lower.trim_start();
    let without_num = {
        let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 {
            trimmed[digits..].trim_start_matches(|c: char| c.is_whitespace() || "._-)".contains(c))
        } else {
            trimmed
        }
    };
    without_num
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn is_generic(name: &str) -> bool {
    let w = norm_words(name);
    if GENERIC_EXACT.contains(&w.as_str()) {
        return true;
    }
    let words: Vec<&str> = w.split(' ').filter(|x| !x.is_empty()).collect();
    // từ khóa phải là từ cuối: "Film Grain Overlays", "Conversion LUTs" — nhưng không phải "punch-hole-transitions-59247993"
    !words.is_empty() && words.len() <= 4 && words.last().map(|x| GENERIC_WORDS.contains(x)).unwrap_or(false)
}

/// "1. Abril", "02 GRAIN", "A1 DJI D LOG", "10 Cool Presets"
fn is_numbered(name: &str) -> bool {
    let s = name.trim_start();
    let mut chars = s.chars().peekable();
    if chars.peek().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
        chars.next();
    }
    let mut digits = 0;
    while chars.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        chars.next();
        digits += 1;
    }
    digits > 0 && chars.next().map(|c| c.is_whitespace() || "._-)".contains(c)).unwrap_or(false)
}

fn is_id_like(name: &str) -> bool {
    let l = name.to_lowercase();
    l.len() >= 6 && l.chars().all(|c| c.is_ascii_hexdigit() || "_-.".contains(c))
}

fn starts_with_doc(name: &str) -> bool {
    let w = norm_words(name);
    DOC_PREFIX.iter().any(|p| w.starts_with(p))
}

fn common_prefix_share(names: &[String]) -> f64 {
    if names.len() < 2 {
        return 0.0;
    }
    let ns: Vec<String> = names.iter().map(|n| norm_words(n)).collect();
    let mut best = 0;
    for n in &ns {
        let p: String = n.chars().take(12).collect();
        if p.chars().count() < 8 {
            continue;
        }
        best = best.max(ns.iter().filter(|x| x.starts_with(&p)).count());
    }
    best as f64 / ns.len() as f64
}

#[derive(Default)]
pub struct Listing {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
    pub markers: bool,
}

pub fn listing(path: &Path) -> Listing {
    let mut l = Listing::default();
    let Ok(rd) = std::fs::read_dir(path) else { return l };
    for en in rd.flatten() {
        let name = en.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        if CODE_MARKERS.contains(&lower.as_str()) {
            l.markers = true;
        }
        if name.starts_with('.') || name.starts_with('~') || is_junk(&name) || crate::exclude::hit(&en.path()) {
            continue;
        }
        match en.file_type() {
            Ok(t) if t.is_dir() => l.dirs.push(name),
            Ok(t) if t.is_file() => l.files.push(name),
            _ => {}
        }
    }
    l.files.sort();
    l.dirs.sort();
    l
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pack,
    Container,
    Empty,
}

/// Phân loại một thư mục. Trả về (loại, thư mục hiệu dụng sau khi bỏ lớp bọc X/X/X, lý do).
pub fn classify(path: &Path) -> (Kind, PathBuf, &'static str) {
    let mut eff = path.to_path_buf();
    let mut l = listing(&eff);
    // Lớp bọc do giải nén: thư mục chỉ chứa đúng 1 thư mục con (và vài file link/ghi chú)
    for _ in 0..6 {
        let real_files = l.files.iter().filter(|f| !CONTAINER_SKIP.contains(&ext_of(f).as_str())).count();
        if l.dirs.len() == 1 && real_files == 0 && !l.markers {
            eff = eff.join(&l.dirs[0]);
            l = listing(&eff);
        } else {
            break;
        }
    }
    if l.markers {
        return (Kind::Pack, eff, "code project");
    }
    if l.files.is_empty() && l.dirs.is_empty() {
        return (Kind::Empty, eff, "empty");
    }
    let res_files = l
        .files
        .iter()
        .filter(|f| {
            let x = ext_of(f);
            !META.contains(&x.as_str()) && !ARCHIVES.contains(&x.as_str())
        })
        .count();
    let arch_files = l.files.iter().filter(|f| ARCHIVES.contains(&ext_of(f).as_str())).count();
    let n_dirs = l.dirs.len();

    if l.files.iter().any(|f| matches!(ext_of(f).as_str(), "exe" | "dll" | "msi")) {
        return (Kind::Pack, eff, "software");
    }
    if res_files > 0 && (n_dirs == 0 || res_files >= n_dirs) {
        return (Kind::Pack, eff, "loose resource files");
    }
    if n_dirs > 0 {
        let idlike = l.dirs.iter().filter(|d| is_id_like(d)).count();
        if idlike * 2 >= n_dirs {
            return (Kind::Pack, eff, "id-like children");
        }
        let generic = l.dirs.iter().filter(|d| is_generic(d)).count();
        if generic * 2 >= n_dirs {
            return (Kind::Pack, eff, "structural children");
        }
        let numbered = l.dirs.iter().filter(|d| is_numbered(d)).count();
        if n_dirs >= 2 && numbered * 10 >= n_dirs * 6 {
            return (Kind::Pack, eff, "numbered children");
        }
        // thư mục con / file nén tên "Help…", "Tutorial…", "Hướng dẫn…" -> đây là một sản phẩm
        let doc_archive = l.files.iter().any(|f| ARCHIVES.contains(&ext_of(f).as_str()) && starts_with_doc(f));
        if doc_archive || l.dirs.iter().any(|d| starts_with_doc(d)) {
            return (Kind::Pack, eff, "has docs");
        }
        if n_dirs >= 2 && common_prefix_share(&l.dirs) >= 0.8 {
            return (Kind::Pack, eff, "children share prefix");
        }
        let previews = l.files.iter().filter(|f| matches!(ext_of(f).as_str(), "jpg" | "jpeg" | "png" | "pdf")).count();
        if previews > 0 && n_dirs <= 4 && arch_files == 0 {
            return (Kind::Pack, eff, "product preview");
        }
    } else if res_files == 0 && arch_files == 0 {
        return (Kind::Pack, eff, "only meta files");
    }
    (Kind::Container, eff, "mixed children")
}

/// Ghi đè của người dùng: rel_path -> "container" | "pack"
pub type Overrides = HashMap<String, String>;

pub struct AutoCandidate {
    pub abs: PathBuf,
    pub rel: String,
    pub name: String,
    /// folder | archive | file
    pub kind: &'static str,
}

fn rel_of(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().to_string()
}

pub fn candidates(root: &Path, overrides: &Overrides) -> Vec<AutoCandidate> {
    let mut out = Vec::new();
    // Thư mục gốc luôn là thư mục chứa
    walk_container(root, root, overrides, 0, &mut out);
    out
}

fn walk_container(root: &Path, dir: &Path, overrides: &Overrides, level: usize, out: &mut Vec<AutoCandidate>) {
    let l = listing(dir);
    for f in &l.files {
        let x = ext_of(f);
        if CONTAINER_SKIP.contains(&x.as_str()) {
            continue;
        }
        let abs = dir.join(f);
        out.push(AutoCandidate {
            rel: rel_of(root, &abs),
            name: f.clone(),
            kind: if ARCHIVES.contains(&x.as_str()) { "archive" } else { "file" },
            abs,
        });
    }
    for d in &l.dirs {
        let abs = dir.join(d);
        let rel = rel_of(root, &abs);
        let forced = overrides.get(&rel).map(String::as_str);
        let (kind, eff) = match forced {
            Some("pack") => (Kind::Pack, abs.clone()),
            Some("container") => (Kind::Container, abs.clone()),
            _ => {
                let (k, e, _) = classify(&abs);
                (k, e)
            }
        };
        match kind {
            Kind::Empty => {}
            Kind::Pack => out.push(AutoCandidate { abs: abs.clone(), rel, name: d.clone(), kind: "folder" }),
            Kind::Container if level >= 6 => out.push(AutoCandidate { abs: abs.clone(), rel, name: d.clone(), kind: "folder" }),
            Kind::Container => {
                // đi vào thư mục hiệu dụng (bỏ lớp bọc), trừ khi người dùng ép tách đúng thư mục này
                let next = if forced == Some("container") { abs.clone() } else { eff };
                walk_container(root, &next, overrides, level + 1, out);
            }
        }
    }
}

/// Gợi ý phân loại từ tên các thư mục chứa phía trên (vd: "Luts", "SFX", "DR plugin", "Preset LR").
pub fn hints_from_path(rel: &str) -> (Vec<&'static str>, Vec<&'static str>) {
    const TYPE_RULES: &[(&[&str], &str)] = &[
        (&["luts", "lut"], "LUT"),
        (&["powergrade", "powergrades", "power grade", "power grades"], "PowerGrade"),
        (&["sfx", "sound effect", "sound effects"], "SFX"),
        (&["fonts", "font"], "Font"),
        (&["transitions", "transition"], "Transition"),
        (&["overlays", "overlay"], "Overlay"),
        (&["presets", "preset"], "Preset"),
        (&["templates", "template"], "Template"),
        (&["titles"], "Title"),
        (&["plugin", "plugins", "plug in", "plug ins"], "Plugin"),
    ];
    const APP_RULES: &[(&[&str], &str)] = &[
        (&["dr", "davinci", "resolve"], "DaVinci Resolve"),
        (&["lr", "lightroom"], "Lightroom"),
        (&["capcut", "cap cut"], "CapCut"),
        (&["ae", "after effects"], "After Effects"),
        (&["pr", "premiere"], "Premiere Pro"),
        (&["ps", "photoshop"], "Photoshop"),
    ];
    let parts: Vec<&str> = rel.split(['\\', '/']).collect();
    let ancestors = &parts[..parts.len().saturating_sub(1)];
    let has = |w: &str, keys: &[&str]| keys.iter().any(|k| w.contains(&format!(" {k} ")));
    // Loại: lấy từ thư mục chứa gần nhất có từ khóa
    let mut types = Vec::new();
    'outer: for part in ancestors.iter().rev() {
        let w = format!(" {} ", norm_words(part));
        for (keys, t) in TYPE_RULES {
            if has(&w, keys) {
                types.push(*t);
                break 'outer;
            }
        }
    }
    // Phần mềm: mọi tầng phía trên
    let mut apps = Vec::new();
    for part in ancestors {
        let w = format!(" {} ", norm_words(part));
        for (keys, a) in APP_RULES {
            if has(&w, keys) && !apps.contains(a) {
                apps.push(*a);
            }
        }
    }
    (types, apps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mrm_auto_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }
    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    #[test]
    fn words_and_names() {
        assert!(is_generic("02 GRAIN"));
        assert!(is_generic("Film Grain Overlays"));
        assert!(is_generic("Help Documentation"));
        assert!(!is_generic("punch-hole-transitions-for-davinci-resolve-59247993"));
        assert!(!is_generic("punch-hole-transitions-59247993"));
        assert!(is_generic("OVERLAYS & MATTE"));
        assert!(!is_generic("Greg Edits Essentials Preset Pack"));
        assert!(is_numbered("1. Abril-fatface"));
        assert!(is_numbered("A1 DJI D LOG V1.0"));
        assert!(!is_numbered("Cinematic 700"));
        assert!(is_id_like("10023930519432473731"));
        assert!(starts_with_doc("Help Documentation ( Open This First)"));
    }

    /// Cấu trúc thu gọn từ thư viện thật của người dùng.
    #[test]
    fn mixed_library() {
        let r = tmp("lib");
        // Nhóm phân loại chứa các gói
        touch(&r.join("DR plugin/Luts/Vintage Lut's pack_2/Vintage Lut's pack_2/a.cube"));
        touch(&r.join("DR plugin/Luts/Vintage Lut's pack_2/Vintage Lut's pack_2/b.cube"));
        touch(&r.join("DR plugin/Luts/Vintage Lut's pack_2.rar"));
        touch(&r.join("DR plugin/Luts/Ultimate LUT Pack/Ultimate LUT Pack/Conversion LUTs/a.cube"));
        touch(&r.join("DR plugin/Luts/Ultimate LUT Pack/Ultimate LUT Pack/Creative LUTs/b.cube"));
        touch(&r.join("DR plugin/Luts/Helio Dlog.cube"));
        // Gói nằm thẳng ở tầng 2 cạnh các nhóm
        touch(&r.join("DR plugin/Greg Edits Essentials Preset Pack/1. PAID Version/a.drfx"));
        touch(&r.join("DR plugin/Greg Edits Essentials Preset Pack/2. FREE Version/a.drfx"));
        touch(&r.join("DR plugin/punch-hole-transitions-59247993/Help Documentation/a.mp4"));
        touch(&r.join("DR plugin/punch-hole-transitions-59247993/Punch Hole.dra/p.drp"));
        touch(&r.join("DR plugin/punch-hole-transitions-59247993/preview.jpg"));
        touch(&r.join("DR plugin/readme.txt"));
        // Bundle lớn có thư mục hướng dẫn
        touch(&r.join("DR plugin/Super_Creator_Pack V7/Super_Creator_Pack V7/Help Documentation ( Open This First)/x.pdf"));
        touch(&r.join("DR plugin/Super_Creator_Pack V7/Super_Creator_Pack V7/Super_Creators_Pack/COLOR_LUTs/Aerial/a.cube"));
        touch(&r.join("DR plugin/Super_Creator_Pack V7/Super_Creator_Pack V7/Super_Creators_Pack/SOUND_FX/Mix/a.wav"));
        // Lightroom: gói chứa file preset trực tiếp
        touch(&r.join("Preset LR/15 Cinematic Film Look/a.lrtemplate"));
        touch(&r.join("Preset LR/15 Cinematic Film Look/b.lrtemplate"));
        touch(&r.join("Preset LR/Power Grade/pg1.zip"));
        touch(&r.join("Preset LR/Power Grade/pg2.zip"));
        touch(&r.join("Preset LR/aphoto.vn 50 preset lightroom dep/a.lrtemplate"));
        touch(&r.join("Preset LR/Creativemarket Beautiful Film Presets Vol 1 155820/a.lrtemplate"));
        // Ứng dụng: không được chẻ vụn
        touch(&r.join("Cap cut/CapCut_full_apps_folder/CapCut/Apps/7.7/capcut.exe"));
        touch(&r.join("Cap cut/CapCut_full_apps_folder/CapCut/User Data/Cache/1002393051943247/a.json"));
        touch(&r.join("Cap cut/CapCut_full_apps_folder.zip"));
        // Rác
        fs::create_dir_all(r.join("DR plugin/__MACOSX/x")).unwrap();

        let c = candidates(&r, &Overrides::new());
        let mut rels: Vec<String> = c.iter().map(|x| x.rel.replace('\\', "/")).collect();
        rels.sort();
        let expect = [
            "Cap cut/CapCut_full_apps_folder",
            "Cap cut/CapCut_full_apps_folder.zip",
            "DR plugin/Greg Edits Essentials Preset Pack",
            "DR plugin/Luts/Helio Dlog.cube",
            "DR plugin/Luts/Ultimate LUT Pack",
            "DR plugin/Luts/Vintage Lut's pack_2",
            "DR plugin/Luts/Vintage Lut's pack_2.rar",
            "DR plugin/Super_Creator_Pack V7",
            "DR plugin/punch-hole-transitions-59247993",
            "Preset LR/15 Cinematic Film Look",
            "Preset LR/Creativemarket Beautiful Film Presets Vol 1 155820",
            "Preset LR/Power Grade/pg1.zip",
            "Preset LR/Power Grade/pg2.zip",
            "Preset LR/aphoto.vn 50 preset lightroom dep",
        ];
        assert_eq!(rels, expect);

        // Người dùng ép tách bundle và ép gộp nhóm Luts
        let mut ov = Overrides::new();
        ov.insert(format!("DR plugin{}Luts", std::path::MAIN_SEPARATOR), "pack".into());
        let c = candidates(&r, &ov);
        let rels: Vec<String> = c.iter().map(|x| x.rel.replace('\\', "/")).collect();
        assert!(rels.contains(&"DR plugin/Luts".to_string()));
        assert!(!rels.iter().any(|x| x.starts_with("DR plugin/Luts/")));
    }

    #[test]
    fn path_hints() {
        let (t, a) = hints_from_path("DR plugin\\Luts\\Kodak Pack");
        assert_eq!(t, vec!["LUT"]);
        assert_eq!(a, vec!["DaVinci Resolve"]);
        let (t, a) = hints_from_path("Preset LR\\15 Cinematic");
        assert_eq!(t, vec!["Preset"]);
        assert_eq!(a, vec!["Lightroom"]);
    }
}
