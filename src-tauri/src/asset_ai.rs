//! Phase 5 — AI cho từng file media (Media Browser), chạy offline qua Ollama (bge-m3):
//! - Embedding tên file + thư mục chứa + tag -> tìm theo ý nghĩa (hiểu tiếng Việt: "tiếng súng" -> AK47.wav).
//! - Danh mục AI (zero-shot): so với bộ nhãn SFX / hình ảnh, chỉ gán khi đủ chắc chắn, không đoán bừa.
//! - File tương tự (kNN trên embedding).
//! Danh mục AI chỉ là metadata gợi ý (cột ai_category) — không tự thành tag; người dùng bấm để gắn tag thật.
use crate::ai;
use crate::commands::AppState;
use rusqlite::params;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

type Res<T> = Result<T, String>;
fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

const BATCH: i64 = 64;
/// Ngưỡng gán danh mục (đo trên thư viện thật: ~1/3 file được gán, độ chính xác cao).
const CAT_MIN: f32 = 0.5;
const CAT_MARGIN: f32 = 0.05;

/// (tên hiển thị, nhãn để embed — Anh + Việt)
const AUDIO_CATS: &[(&str, &str)] = &[
    ("Whoosh", "Whoosh swoosh swish — tiếng vút, lướt gió nhanh"),
    ("Impact", "Impact hit boom slam — tiếng va chạm, đập mạnh"),
    ("Riser", "Riser build up tension — tiếng tăng dần, căng thẳng"),
    ("Transition", "Transition swipe — chuyển cảnh"),
    ("Glitch", "Glitch digital noise static — tiếng nhiễu, lỗi kỹ thuật số"),
    ("Click / UI", "Click button UI interface — tiếng bấm nút, giao diện"),
    ("Pop", "Pop bubble — tiếng bốp, bong bóng"),
    ("Notification", "Notification ding chime alert — tiếng thông báo"),
    ("Typing", "Typing keyboard typewriter — tiếng gõ phím"),
    ("Camera", "Camera shutter photo — tiếng chụp ảnh"),
    ("Gun", "Gun shot weapon reload — tiếng súng"),
    ("Explosion", "Explosion blast — tiếng nổ"),
    ("Punch / Fight", "Punch fight kick — tiếng đấm, đánh nhau"),
    ("Footsteps", "Footsteps walking — tiếng bước chân"),
    ("Door", "Door open close knock — tiếng cửa"),
    ("Vehicle", "Car vehicle engine horn — tiếng xe, động cơ, còi"),
    ("Weather", "Wind rain thunder weather nature — tiếng gió, mưa, sấm"),
    ("Water", "Water splash drop — tiếng nước"),
    ("Fire", "Fire burning — tiếng lửa cháy"),
    ("Animal", "Animal dog cat bird — tiếng động vật"),
    ("Applause / Crowd", "Crowd applause cheering — tiếng vỗ tay, đám đông reo hò"),
    ("Laugh / Meme", "Laugh funny meme — tiếng cười, meme hài hước"),
    ("Voice", "Voice vocal speech — giọng nói"),
    ("Cartoon", "Cartoon comedy boing — âm thanh hoạt hình"),
    ("Magic", "Magic sparkle shimmer — tiếng phép thuật, lấp lánh"),
    ("Sci-fi", "Sci-fi laser futuristic — âm thanh khoa học viễn tưởng"),
    ("Horror", "Horror scary creepy — âm thanh kinh dị"),
    ("Money", "Cash money coin register — tiếng tiền, đồng xu"),
    ("Phone", "Phone ringtone vibration — tiếng điện thoại"),
    ("Beep / Alarm", "Beep alarm siren — tiếng bíp, báo động"),
    ("Heartbeat", "Heartbeat — tiếng tim đập"),
    ("Music", "Music loop beat background music — nhạc nền"),
    ("Drum", "Drum percussion — tiếng trống"),
    ("Ambience", "Ambience room tone atmosphere — âm thanh môi trường"),
    ("Scratch", "Record scratch rewind — tiếng tua băng, scratch"),
    ("Paper", "Paper swipe page flip — tiếng lật giấy"),
    ("Clock", "Clock tick countdown timer — tiếng đồng hồ tích tắc"),
    ("Bell", "Bell — tiếng chuông"),
    ("Game", "Game 8-bit arcade — âm thanh game"),
];
const VISUAL_CATS: &[(&str, &str)] = &[
    ("Light leak", "Overlay light leak — lớp phủ ánh sáng"),
    ("Film grain", "Film grain dust scratches — hạt phim, bụi"),
    ("Texture", "Texture paper grunge — texture, chất liệu giấy"),
    ("Background", "Background wallpaper — hình nền"),
    ("Transition", "Transition — chuyển cảnh"),
    ("Title / Text", "Lower third title text — tiêu đề, chữ"),
    ("Icon / Emoji", "Icon emoji symbol — biểu tượng"),
    ("Logo", "Logo brand — logo"),
    ("Frame", "Frame border — khung viền"),
    ("Particles", "Particles sparkles — hạt, lấp lánh"),
    ("Smoke", "Smoke fog — khói, sương"),
    ("Fire", "Fire explosion — lửa, vụ nổ"),
    ("Lens flare", "Bokeh lens flare — bokeh, lóa ống kính"),
    ("Glitch", "Glitch distortion — hiệu ứng glitch"),
    ("Shape", "Shape element graphic — hình khối đồ họa"),
    ("Sticker", "Sticker cartoon — sticker hoạt hình"),
    ("People", "Portrait person woman man — chân dung người"),
    ("Landscape", "Landscape nature mountain — phong cảnh thiên nhiên"),
    ("City", "City street building — thành phố"),
    ("Sky", "Sky clouds — bầu trời, mây"),
    ("Mockup", "Mockup — mockup"),
    ("Screenshot / UI", "Screenshot UI thumbnail preview — ảnh chụp màn hình, ảnh xem trước"),
    ("Green screen", "Green screen chroma key — phông xanh"),
    ("Mask / Matte", "Mask matte alpha — mặt nạ, matte"),
    ("Countdown", "Countdown numbers — đếm ngược"),
    ("Arrow / Cursor", "Arrow pointer cursor — mũi tên, con trỏ"),
    ("Social media", "Social media subscribe like button — nút subscribe, like"),
    ("Gradient", "Gradient color — dải màu"),
    ("Brush / Ink", "Paint brush ink stroke — nét cọ, mực"),
];

// ---------------------------------------------------------------- text

/// Tên file/thư mục -> chữ đọc được: "LASRBeam_Tight-Ray.wav" -> "LASRBeam Tight Ray".
pub fn clean(s: &str) -> String {
    let s = match s.rfind('.') {
        Some(i) if s.len() - i <= 5 && i > 0 => &s[..i],
        _ => s,
    };
    let mut out = String::with_capacity(s.len() + 8);
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if matches!(c, '_' | '-' | '.' | '+' | '(' | ')' | '[' | ']') {
            out.push(' ');
        } else {
            if let Some(p) = prev {
                if p.is_lowercase() && c.is_uppercase() {
                    out.push(' ');
                }
            }
            out.push(c);
        }
        prev = Some(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Thư mục không mang nghĩa (bỏ khỏi ngữ cảnh để khỏi nhiễu).
fn generic_folder(p: &str) -> bool {
    let l = p.to_lowercase();
    l.is_empty()
        || l.chars().all(|c| c.is_ascii_digit() || c == ' ')
        || matches!(
            l.as_str(),
            "sfx" | "sound fx" | "sound effect" | "sound effects" | "sounds" | "sound" | "audio" | "media files" | "media"
                | "files" | "file" | "new fx new update" | "random" | "mix" | "misc" | "footage" | "footages" | "element"
                | "elements" | "overlays & matte" | "assets"
        )
}

/// Văn bản đại diện cho một file: tên + 2 thư mục chứa có nghĩa + tag.
pub fn asset_text(filename: &str, rel_path: &str, tags: Option<&str>, caption: Option<&str>) -> String {
    let parts: Vec<&str> = rel_path.split(['\\', '/']).collect::<Vec<_>>();
    let dirs: Vec<String> = parts[..parts.len().saturating_sub(1)].iter().map(|p| clean(p)).filter(|p| !generic_folder(p)).collect();
    let ctx = dirs[dirs.len().saturating_sub(2)..].join(" / ");
    let mut t = clean(filename);
    if !ctx.is_empty() {
        t.push_str(&format!(" ({ctx})"));
    }
    if let Some(tags) = tags.filter(|t| !t.is_empty()) {
        t.push_str(&format!(" · {tags}"));
    }
    if let Some(c) = caption.filter(|c| !c.is_empty()) {
        t.push_str(&format!(". {c}"));
    }
    t
}

// ---------------------------------------------------------------- state trong RAM

struct Index {
    model: String,
    /// asset id -> (là âm thanh?, vector)
    vectors: HashMap<i64, (bool, Vec<f32>)>,
    audio_cats: Vec<Vec<f32>>,
    visual_cats: Vec<Vec<f32>>,
}

static INDEX: Mutex<Option<Index>> = Mutex::new(None);

fn ensure_loaded(app: &AppHandle, model: &str) -> Res<()> {
    if INDEX.lock().unwrap().as_ref().map(|i| i.model == model).unwrap_or(false) {
        return Ok(());
    }
    let audio_cats = ai::embed(model, &AUDIO_CATS.iter().map(|c| c.1.to_string()).collect::<Vec<_>>())?;
    let visual_cats = ai::embed(model, &VISUAL_CATS.iter().map(|c| c.1.to_string()).collect::<Vec<_>>())?;
    let state = app.state::<AppState>();
    let vectors = {
        let conn = state.db.lock().unwrap();
        let mut s = conn
            .prepare("SELECT x.asset_id, a.media_type = 'audio', x.vector FROM asset_embeddings x JOIN media_assets a ON a.id = x.asset_id WHERE x.model = ?1")
            .map_err(e)?;
        let v = s
            .query_map([model], |r| Ok((r.get::<_, i64>(0)?, (r.get::<_, bool>(1)?, ai::from_blob(&r.get::<_, Vec<u8>>(2)?)))))
            .map_err(e)?
            .flatten()
            .collect();
        v
    };
    *INDEX.lock().unwrap() = Some(Index { model: model.to_string(), vectors, audio_cats, visual_cats });
    Ok(())
}

fn classify(idx: &Index, audio: bool, v: &[f32]) -> Option<(&'static str, f32)> {
    let (cats, vecs) = if audio { (AUDIO_CATS, &idx.audio_cats) } else { (VISUAL_CATS, &idx.visual_cats) };
    let mut best = (0usize, f32::MIN);
    let mut second = f32::MIN;
    for (i, c) in vecs.iter().enumerate() {
        let s = ai::dot(v, c);
        if s > best.1 {
            second = best.1;
            best = (i, s);
        } else if s > second {
            second = s;
        }
    }
    (best.1 >= CAT_MIN && best.1 - second >= CAT_MARGIN).then(|| (cats[best.0].0, best.1))
}

/// Một vòng lập chỉ mục file media (gọi từ luồng AI nền khi resource đã xong). Trả về số file đã xử lý.
pub fn index_round(app: &AppHandle) -> Res<usize> {
    let model = ai::embed_model(app);
    ensure_loaded(app, &model)?;
    let state = app.state::<AppState>();
    type Row = (i64, String, String, String, Option<String>, Option<String>);
    let (batch, remaining, total): (Vec<Row>, i64, i64) = {
        let conn = state.db.lock().unwrap();
        let from = "FROM media_assets a LEFT JOIN asset_embeddings x ON x.asset_id = a.id AND x.model = ?1 WHERE x.asset_id IS NULL";
        let mut s = conn
            .prepare(&format!(
                "SELECT a.id, a.media_type, a.filename, a.rel_path,
                        (SELECT group_concat(t.name, ', ') FROM asset_tags g JOIN tags t ON t.id = g.tag_id WHERE g.asset_id = a.id),
                        (SELECT caption FROM asset_captions k WHERE k.asset_id = a.id)
                 {from} ORDER BY a.id LIMIT {BATCH}"
            ))
            .map_err(e)?;
        let batch = s
            .query_map([&model], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
            .map_err(e)?
            .flatten()
            .collect();
        let remaining = conn.query_row(&format!("SELECT count(*) {from}"), [&model], |r| r.get(0)).map_err(e)?;
        let total = conn.query_row("SELECT count(*) FROM media_assets", [], |r| r.get(0)).map_err(e)?;
        (batch, remaining, total)
    };
    if batch.is_empty() {
        return Ok(0);
    }
    ai::set_progress(app, "indexing", "Đang lập chỉ mục AI cho file media…", (total - remaining).max(0) as u64, total as u64);
    let texts: Vec<String> = batch.iter().map(|(_, _, f, r, t, c)| asset_text(f, r, t.as_deref(), c.as_deref())).collect();
    let vecs = ai::embed(&model, &texts)?;
    let now = crate::db::now();
    let mut guard = INDEX.lock().unwrap();
    let idx = guard.as_mut().ok_or("AI index chưa sẵn sàng")?;
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    for ((id, mt, _, _, _, _), (text, v)) in batch.iter().zip(texts.iter().zip(vecs)) {
        let audio = mt == "audio";
        let cat = classify(idx, audio, &v);
        tx.execute(
            "INSERT INTO asset_embeddings (asset_id, model, text_hash, vector, embedded_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(asset_id) DO UPDATE SET model = excluded.model, text_hash = excluded.text_hash,
             vector = excluded.vector, embedded_at = excluded.embedded_at",
            params![id, model, ai::text_hash(text), ai::to_blob(&v), now],
        )
        .map_err(e)?;
        tx.execute(
            "UPDATE media_assets SET ai_category = ?1, ai_score = ?2 WHERE id = ?3",
            params![cat.map(|c| c.0), cat.map(|c| c.1 as f64), id],
        )
        .map_err(e)?;
        idx.vectors.insert(*id, (audio, v));
    }
    tx.commit().map_err(e)?;
    Ok(batch.len())
}

/// Tìm theo ý nghĩa: (asset_id, độ giống) — rỗng nếu AI chưa sẵn sàng.
pub fn semantic(app: &AppHandle, query: &str) -> Vec<(i64, f32)> {
    if query.trim().is_empty() || !ai::ai_enabled(app) || !ai::server_up() {
        return vec![];
    }
    let model = ai::embed_model(app);
    if ensure_loaded(app, &model).is_err() {
        return vec![];
    }
    let Ok(mut q) = ai::embed(&model, &[query.to_string()]) else { return vec![] };
    let Some(qv) = q.pop() else { return vec![] };
    let guard = INDEX.lock().unwrap();
    let Some(idx) = guard.as_ref() else { return vec![] };
    let mut scored: Vec<(i64, f32)> = idx.vectors.iter().map(|(id, (_, v))| (*id, ai::dot(&qv, v))).collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let best = scored.first().map(|x| x.1).unwrap_or(0.0);
    scored.into_iter().take(600).filter(|(_, s)| *s >= 0.42 && *s >= best - 0.12).collect()
}

/// File giống nhất cùng loại (âm thanh với âm thanh, hình với hình).
pub fn similar(id: i64, limit: usize) -> Vec<(i64, f32)> {
    let guard = INDEX.lock().unwrap();
    let Some(idx) = guard.as_ref() else { return vec![] };
    let Some((audio, me)) = idx.vectors.get(&id) else { return vec![] };
    let mut scored: Vec<(i64, f32)> =
        idx.vectors.iter().filter(|(oid, (a, _))| **oid != id && a == audio).map(|(oid, (_, v))| (*oid, ai::dot(me, v))).collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(limit).filter(|(_, s)| *s >= 0.55).collect()
}

/// Asset đổi tag / đổi tên -> tính lại embedding ở vòng sau.
pub fn invalidate(conn: &rusqlite::Connection, ids: &[i64]) {
    for id in ids {
        let _ = conn.execute("DELETE FROM asset_embeddings WHERE asset_id = ?1", [id]);
    }
}

#[derive(Serialize)]
pub struct AssetAiStatus {
    indexed: i64,
    total: i64,
    categorized: i64,
}

#[tauri::command]
pub fn asset_ai_status(state: tauri::State<AppState>) -> Res<AssetAiStatus> {
    let conn = state.db.lock().unwrap();
    let model = crate::db::get_setting(&conn, "ai_embed_model").unwrap_or_else(|| ai::DEFAULT_EMBED_MODEL.into());
    Ok(AssetAiStatus {
        indexed: conn.query_row("SELECT count(*) FROM asset_embeddings WHERE model = ?1", [model], |r| r.get(0)).map_err(e)?,
        total: conn.query_row("SELECT count(*) FROM media_assets", [], |r| r.get(0)).map_err(e)?,
        categorized: conn.query_row("SELECT count(*) FROM media_assets WHERE ai_category IS NOT NULL", [], |r| r.get(0)).map_err(e)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_clean_and_skips_generic_folders() {
        assert_eq!(clean("LASRBeam_Tight-Ray.wav"), "LASRBeam Tight Ray");
        assert_eq!(clean("whoosh (1).mp3"), "whoosh 1");
        assert_eq!(
            asset_text("AK47.wav", "Sound Effects\\Random\\guns\\AK47.wav", Some("gun"), None),
            "AK47 (guns) · gun",
            "bỏ thư mục chung chung: Sound Effects, Random"
        );
        assert_eq!(
            asset_text("01.wav", "DR plugin\\Super_Creator_Pack V7\\SOUND_FX\\OLD_FX\\Glitch_&_Woosh\\SWOOSH_FX_01\\01.wav", None, None),
            "01 (Glitch & Woosh / SWOOSH FX 01)"
        );
        assert_eq!(asset_text("x.png", "x.png", None, None), "x");
        assert_eq!(asset_text("bg 01.jpg", "BG\\bg 01.jpg", None, Some("Blue sky with clouds")), "bg 01 (BG). Blue sky with clouds");
    }

    #[test]
    fn classify_needs_confidence_and_margin() {
        let idx = Index {
            model: "m".into(),
            vectors: HashMap::new(),
            audio_cats: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
            visual_cats: vec![],
        };
        assert_eq!(classify(&idx, true, &[0.9, 0.1]).map(|c| c.0), Some("Whoosh"));
        assert_eq!(classify(&idx, true, &[0.6, 0.58]), None, "hai nhãn sát nhau -> không gán");
        assert_eq!(classify(&idx, true, &[0.3, 0.1]), None, "độ giống thấp -> không gán");
    }
}
