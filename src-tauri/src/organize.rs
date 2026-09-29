//! Phase 4 — Trùng lặp, version family, quan hệ giữa resource, gợi ý học từ cách người dùng gắn tag.

use crate::commands::AppState;
use crate::db;
use crate::normalize::{normalize, similarity};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tauri::State;

type Res<T> = Result<T, String>;
fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

fn collect<T, F>(conn: &Connection, sql: &str, p: impl rusqlite::Params, f: F) -> Res<Vec<T>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut stmt = conn.prepare(sql).map_err(e)?;
    let rows = stmt.query_map(p, f).map_err(e)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(e)
}

// ================================================================ version helpers

pub fn parse_version(v: &str) -> Vec<u64> {
    v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

pub fn cmp_version(a: &str, b: &str) -> std::cmp::Ordering {
    let (x, y) = (parse_version(a), parse_version(b));
    for i in 0..x.len().max(y.len()) {
        let c = x.get(i).unwrap_or(&0).cmp(y.get(i).unwrap_or(&0));
        if c != std::cmp::Ordering::Equal {
            return c;
        }
    }
    std::cmp::Ordering::Equal
}

// ================================================================ brief info

#[derive(Serialize, Clone)]
pub struct ResourceBrief {
    id: i64,
    name: String,
    version: Option<String>,
    size: i64,
    locations: Vec<String>,
}

fn briefs(conn: &Connection, ids: &[i64]) -> Res<Vec<ResourceBrief>> {
    let mut out = Vec::new();
    for &id in ids {
        let (name, version, size): (String, Option<String>, i64) = conn
            .query_row(
                "SELECT name, version, coalesce((SELECT sum(size) FROM resource_sources WHERE resource_id = r.id), 0)
                 FROM resources r WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(e)?;
        let locations = collect(
            conn,
            "SELECT l.name || ' › ' || s.rel_path FROM resource_sources s JOIN libraries l ON l.id = s.library_id WHERE s.resource_id = ?1",
            [id],
            |r| r.get::<_, String>(0),
        )?;
        out.push(ResourceBrief { id, name, version, size, locations });
    }
    Ok(out)
}

// ================================================================ duplicates

#[derive(Serialize)]
pub struct DuplicateGroup {
    /// content = nội dung giống hệt; name = cùng tên + version
    reason: String,
    resources: Vec<ResourceBrief>,
}

fn dismissed(conn: &Connection) -> Res<HashSet<(i64, i64)>> {
    Ok(collect(conn, "SELECT a, b FROM duplicate_dismissed", [], |r| Ok((r.get(0)?, r.get(1)?)))?.into_iter().collect())
}

fn pair(a: i64, b: i64) -> (i64, i64) {
    if a < b { (a, b) } else { (b, a) }
}

/// Bỏ các thành viên đã được xác nhận "không trùng" với mọi thành viên còn lại.
fn filter_group(mut ids: Vec<i64>, dis: &HashSet<(i64, i64)>) -> Vec<i64> {
    ids.sort();
    ids.dedup();
    ids.iter()
        .copied()
        .filter(|&a| ids.iter().any(|&b| a != b && !dis.contains(&pair(a, b))))
        .collect()
}

#[tauri::command]
pub fn find_duplicates(state: State<AppState>) -> Res<Vec<DuplicateGroup>> {
    let conn = state.db.lock().unwrap();
    let dis = dismissed(&conn)?;
    let mut groups = Vec::new();
    let mut covered: HashSet<i64> = HashSet::new();

    // 1. Nội dung giống hệt (chữ ký trùng) ở các resource khác nhau
    let mut by_sig: HashMap<String, Vec<i64>> = HashMap::new();
    for (sig, rid) in collect(
        &conn,
        "SELECT content_sig, resource_id FROM resource_sources WHERE content_sig IS NOT NULL AND coalesce(content_size, 0) > 0",
        [],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
    )? {
        by_sig.entry(sig).or_default().push(rid);
    }
    let mut seen_sets: HashSet<Vec<i64>> = HashSet::new();
    for ids in by_sig.into_values() {
        let ids = filter_group(ids, &dis);
        if ids.len() >= 2 && seen_sets.insert(ids.clone()) {
            covered.extend(&ids);
            groups.push(DuplicateGroup { reason: "content".into(), resources: briefs(&conn, &ids)? });
        }
    }

    // 2. Cùng tên chuẩn hóa + cùng version
    let mut by_name: HashMap<String, Vec<i64>> = HashMap::new();
    for (id, name) in collect(&conn, "SELECT id, name FROM resources", [], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let n = normalize(&name, false);
        if n.compact.len() >= 3 {
            by_name.entry(format!("{}@{}", n.compact, n.version.unwrap_or_default())).or_default().push(id);
        }
    }
    for ids in by_name.into_values() {
        let ids = filter_group(ids, &dis);
        if ids.len() >= 2 && !ids.iter().all(|i| covered.contains(i)) {
            groups.push(DuplicateGroup { reason: "name".into(), resources: briefs(&conn, &ids)? });
        }
    }
    groups.sort_by(|a, b| a.reason.cmp(&b.reason).then(b.resources.len().cmp(&a.resources.len())));
    Ok(groups)
}

#[tauri::command]
pub fn dismiss_duplicates(state: State<AppState>, ids: Vec<i64>) -> Res<()> {
    let conn = state.db.lock().unwrap();
    for i in 0..ids.len() {
        for j in i + 1..ids.len() {
            let (a, b) = pair(ids[i], ids[j]);
            conn.execute("INSERT OR IGNORE INTO duplicate_dismissed (a, b) VALUES (?1, ?2)", params![a, b]).map_err(e)?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub struct ExistingHit {
    id: i64,
    name: String,
    version: Option<String>,
    score: f64,
}

/// "Đã có chưa?" — so tên gói định tải với toàn bộ thư viện.
#[tauri::command]
pub fn check_existing(state: State<AppState>, name: String) -> Res<Vec<ExistingHit>> {
    let q = normalize(name.trim(), name.contains('.'));
    if q.compact.is_empty() {
        return Ok(vec![]);
    }
    let conn = state.db.lock().unwrap();
    let mut hits: Vec<ExistingHit> = collect(&conn, "SELECT id, name, version FROM resources", [], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
    })?
    .into_iter()
    .filter_map(|(id, n, version)| {
        let norm = normalize(&n, false);
        let mut score = similarity(&q, &norm);
        // chứa trọn cụm tên -> vẫn đáng chú ý
        if score < 60.0 && norm.compact.len() >= 4 && (norm.compact.contains(&q.compact) || q.compact.contains(&norm.compact)) {
            score = 70.0;
        }
        (score >= 60.0).then_some(ExistingHit { id, name: n, version, score })
    })
    .collect();
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(10);
    Ok(hits)
}

// ================================================================ versions

#[derive(Serialize, Clone)]
pub struct VersionItem {
    pub id: i64,
    pub name: String,
    pub version: Option<String>,
    pub latest: bool,
}

fn families(conn: &Connection) -> Res<Vec<Vec<VersionItem>>> {
    let mut by: HashMap<String, Vec<VersionItem>> = HashMap::new();
    for (id, name, version) in collect(conn, "SELECT id, name, version FROM resources", [], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
    })? {
        let n = normalize(&name, false);
        if n.compact.len() < 3 {
            continue;
        }
        let version = version.or(n.version);
        by.entry(n.compact).or_default().push(VersionItem { id, name, version, latest: false });
    }
    let mut out = Vec::new();
    for mut v in by.into_values() {
        let distinct: HashSet<Option<String>> = v.iter().map(|x| x.version.clone()).collect();
        if v.len() < 2 || distinct.len() < 2 || v.iter().all(|x| x.version.is_none()) {
            continue;
        }
        v.sort_by(|a, b| match (&a.version, &b.version) {
            (Some(x), Some(y)) => cmp_version(y, x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        });
        let top = v[0].version.clone();
        for x in &mut v {
            x.latest = x.version == top;
        }
        out.push(v);
    }
    out.sort_by(|a, b| a[0].name.to_lowercase().cmp(&b[0].name.to_lowercase()));
    Ok(out)
}

pub fn family_of(conn: &Connection, resource_id: i64) -> Res<Vec<VersionItem>> {
    Ok(families(conn)?.into_iter().find(|f| f.iter().any(|x| x.id == resource_id)).unwrap_or_default())
}

#[tauri::command]
pub fn list_version_families(state: State<AppState>) -> Res<Vec<Vec<VersionItem>>> {
    families(&state.db.lock().unwrap())
}

/// Gắn status "Update Available" cho các bản cũ, gỡ khỏi bản mới nhất.
#[tauri::command]
pub fn mark_outdated_versions(state: State<AppState>) -> Res<usize> {
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    let tag = match db::tag_id(&tx, "status", "Update Available").map_err(e)? {
        Some(t) => t,
        None => {
            tx.execute("INSERT INTO tags (kind, name) VALUES ('status', 'Update Available')", []).map_err(e)?;
            tx.last_insert_rowid()
        }
    };
    let mut n = 0;
    for fam in families(&tx)? {
        for item in fam {
            if item.latest {
                tx.execute("DELETE FROM resource_tags WHERE resource_id = ?1 AND tag_id = ?2", params![item.id, tag]).map_err(e)?;
            } else {
                n += tx.execute("INSERT OR IGNORE INTO resource_tags (resource_id, tag_id) VALUES (?1, ?2)", params![item.id, tag]).map_err(e)?;
            }
            db::reindex(&tx, item.id).map_err(e)?;
        }
    }
    tx.commit().map_err(e)?;
    Ok(n)
}

// ================================================================ relations

#[derive(Serialize)]
pub struct Relation {
    other_id: i64,
    other_name: String,
    kind: String,
    /// true: resource hiện tại là `a` (ví dụ: "là add-on của" other)
    outgoing: bool,
}

pub const RELATION_KINDS: &[&str] = &["addon_of", "requires", "alternative", "related"];

pub fn relations_of(conn: &Connection, id: i64) -> Res<Vec<Relation>> {
    collect(
        conn,
        "SELECT rr.b, r.name, rr.kind, 1 FROM resource_relations rr JOIN resources r ON r.id = rr.b WHERE rr.a = ?1
         UNION ALL
         SELECT rr.a, r.name, rr.kind, 0 FROM resource_relations rr JOIN resources r ON r.id = rr.a WHERE rr.b = ?1",
        [id],
        |r| Ok(Relation { other_id: r.get(0)?, other_name: r.get(1)?, kind: r.get(2)?, outgoing: r.get::<_, i64>(3)? != 0 }),
    )
}

#[tauri::command]
pub fn add_relation(state: State<AppState>, a: i64, b: i64, kind: String) -> Res<()> {
    if a == b || !RELATION_KINDS.contains(&kind.as_str()) {
        return Err("Quan hệ không hợp lệ".into());
    }
    let conn = state.db.lock().unwrap();
    // quan hệ đối xứng lưu một chiều
    let (a, b) = if matches!(kind.as_str(), "alternative" | "related") { pair(a, b) } else { (a, b) };
    conn.execute(
        "INSERT OR IGNORE INTO resource_relations (a, b, kind, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![a, b, kind, db::now()],
    )
    .map_err(e)?;
    Ok(())
}

#[tauri::command]
pub fn remove_relation(state: State<AppState>, a: i64, b: i64, kind: String) -> Res<()> {
    let conn = state.db.lock().unwrap();
    conn.execute(
        "DELETE FROM resource_relations WHERE kind = ?3 AND ((a = ?1 AND b = ?2) OR (a = ?2 AND b = ?1))",
        params![a, b, kind],
    )
    .map_err(e)?;
    Ok(())
}

// ================================================================ learned suggestions

/// Thống kê token (từ trong tên, extension) ↔ tag từ các resource đã được người dùng gắn tag.
pub struct LearnModel {
    built_at: std::time::Instant,
    token_total: HashMap<String, u32>,
    token_tag: HashMap<String, HashMap<i64, u32>>,
}

pub fn tokens_for(name: &str, source_names: &[String], exts: &[String]) -> HashSet<String> {
    let mut t: HashSet<String> = HashSet::new();
    for n in std::iter::once(name.to_string()).chain(source_names.iter().cloned()) {
        for tok in normalize(&n, false).tokens {
            if tok.chars().count() >= 3 && !tok.chars().all(|c| c.is_ascii_digit()) {
                t.insert(tok);
            }
        }
    }
    for x in exts {
        t.insert(format!("ext:{x}"));
    }
    t
}

fn resource_tokens(conn: &Connection) -> Res<HashMap<i64, HashSet<String>>> {
    let mut names: HashMap<i64, (String, Vec<String>, Vec<String>)> = HashMap::new();
    for (id, name) in collect(conn, "SELECT id, name FROM resources", [], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        names.insert(id, (name, vec![], vec![]));
    }
    for (rid, sname, ext) in collect(conn, "SELECT resource_id, name, ext_summary FROM resource_sources", [], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
    })? {
        if let Some(entry) = names.get_mut(&rid) {
            entry.1.push(sname);
            if let Some(m) = ext.and_then(|x| serde_json::from_str::<HashMap<String, u64>>(&x).ok()) {
                let mut v: Vec<(String, u64)> = m.into_iter().collect();
                v.sort_by(|a, b| b.1.cmp(&a.1));
                entry.2.extend(v.into_iter().take(3).map(|x| x.0));
            }
        }
    }
    Ok(names.into_iter().map(|(id, (n, s, x))| (id, tokens_for(&n, &s, &x))).collect())
}

fn build_model(conn: &Connection) -> Res<LearnModel> {
    let tokens = resource_tokens(conn)?;
    let mut tags: HashMap<i64, Vec<i64>> = HashMap::new();
    for (rid, tid) in collect(
        conn,
        "SELECT rt.resource_id, rt.tag_id FROM resource_tags rt JOIN tags t ON t.id = rt.tag_id WHERE t.kind IN ('app','type','function','personal')",
        [],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
    )? {
        tags.entry(rid).or_default().push(tid);
    }
    let mut m = LearnModel { built_at: std::time::Instant::now(), token_total: HashMap::new(), token_tag: HashMap::new() };
    for (rid, toks) in &tokens {
        let Some(ts) = tags.get(rid) else { continue };
        for tok in toks {
            *m.token_total.entry(tok.clone()).or_default() += 1;
            let per = m.token_tag.entry(tok.clone()).or_default();
            for t in ts {
                *per.entry(*t).or_default() += 1;
            }
        }
    }
    Ok(m)
}

pub fn learn_model(state: &AppState, conn: &Connection) -> Res<Arc<LearnModel>> {
    let mut guard = state.learn.lock().unwrap();
    if let Some(m) = guard.as_ref() {
        if m.built_at.elapsed().as_secs() < 30 {
            return Ok(m.clone());
        }
    }
    let m = Arc::new(build_model(conn)?);
    *guard = Some(m.clone());
    Ok(m)
}

/// (tag_id, độ tin cậy 0..1, token làm căn cứ)
pub fn learned_suggestions(model: &LearnModel, toks: &HashSet<String>) -> Vec<(i64, f64, String)> {
    let mut best: HashMap<i64, (f64, String)> = HashMap::new();
    for tok in toks {
        let Some(&total) = model.token_total.get(tok) else { continue };
        if total < 3 {
            continue;
        }
        for (&tag, &n) in &model.token_tag[tok] {
            if n < 3 {
                continue;
            }
            let p = n as f64 / total as f64;
            if p >= 0.6 && best.get(&tag).map(|b| p > b.0).unwrap_or(true) {
                best.insert(tag, (p, tok.clone()));
            }
        }
    }
    let mut v: Vec<(i64, f64, String)> = best.into_iter().map(|(t, (p, tok))| (t, p, tok)).collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    v.truncate(6);
    v
}

pub fn tokens_of_resource(conn: &Connection, id: i64) -> Res<HashSet<String>> {
    let name: String = conn.query_row("SELECT name FROM resources WHERE id = ?1", [id], |r| r.get(0)).map_err(e)?;
    let mut sources = Vec::new();
    let mut exts = Vec::new();
    for (sname, ext) in collect(conn, "SELECT name, ext_summary FROM resource_sources WHERE resource_id = ?1", [id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
    })? {
        sources.push(sname);
        if let Some(m) = ext.and_then(|x| serde_json::from_str::<HashMap<String, u64>>(&x).ok()) {
            let mut v: Vec<(String, u64)> = m.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            exts.extend(v.into_iter().take(3).map(|x| x.0));
        }
    }
    Ok(tokens_for(&name, &sources, &exts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert_eq!(cmp_version("1.10", "1.9"), std::cmp::Ordering::Greater);
        assert_eq!(cmp_version("2.0", "2"), std::cmp::Ordering::Equal);
    }

    #[test]
    fn learning() {
        let mut m = LearnModel { built_at: std::time::Instant::now(), token_total: HashMap::new(), token_tag: HashMap::new() };
        m.token_total.insert("whoosh".into(), 4);
        m.token_tag.insert("whoosh".into(), [(7, 4), (8, 1)].into_iter().collect());
        let s = learned_suggestions(&m, &["whoosh".to_string()].into_iter().collect());
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].0, 7);
    }

    #[test]
    fn dismissed_filter() {
        let dis: HashSet<(i64, i64)> = [(1, 2)].into_iter().collect();
        assert!(filter_group(vec![1, 2], &dis).is_empty());
        assert_eq!(filter_group(vec![1, 2, 3], &dis), vec![1, 2, 3]);
    }
}
