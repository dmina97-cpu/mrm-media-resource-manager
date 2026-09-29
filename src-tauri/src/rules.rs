//! Phase 4 — Luật phân loại do người dùng định nghĩa.
//! Rule "auto_apply" tự gắn tag khi quét; rule thường chỉ tạo gợi ý trong Inspector.

use crate::commands::AppState;
use crate::db;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::State;

type Res<T> = Result<T, String>;
fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Condition {
    /// name | source | path | ext | library
    pub field: String,
    /// contains | not_contains | equals | starts_with | ends_with
    pub op: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Rule {
    #[serde(default)]
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub auto_apply: bool,
    pub match_all: bool,
    pub conditions: Vec<Condition>,
    pub tag_ids: Vec<i64>,
}

/// Dữ liệu của một resource dùng để so khớp rule.
pub struct Facts {
    pub name: String,
    pub sources: Vec<String>,
    pub paths: Vec<String>,
    pub exts: Vec<String>,
    pub libraries: Vec<String>,
}

pub fn facts(conn: &Connection, resource_id: i64) -> rusqlite::Result<Facts> {
    let name: String = conn.query_row("SELECT name FROM resources WHERE id = ?1", [resource_id], |r| r.get(0))?;
    let mut f = Facts { name: name.to_lowercase(), sources: vec![], paths: vec![], exts: vec![], libraries: vec![] };
    let mut s = conn.prepare(
        "SELECT s.name, l.path, s.rel_path, s.ext_summary, l.name FROM resource_sources s
         JOIN libraries l ON l.id = s.library_id WHERE s.resource_id = ?1",
    )?;
    let rows = s.query_map([resource_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, String>(4)?))
    })?;
    for row in rows {
        let (sname, lpath, rel, ext, lname) = row?;
        f.sources.push(sname.to_lowercase());
        f.paths.push(std::path::Path::new(&lpath).join(&rel).to_string_lossy().to_lowercase().replace('\\', "/"));
        if let Some(ext) = ext.and_then(|x| serde_json::from_str::<std::collections::HashMap<String, u64>>(&x).ok()) {
            f.exts.extend(ext.into_keys());
        }
        f.libraries.push(lname.to_lowercase());
    }
    Ok(f)
}

fn test_one(values: &[String], op: &str, v: &str) -> bool {
    let check = |x: &String| match op {
        "equals" => x == v,
        "starts_with" => x.starts_with(v),
        "ends_with" => x.ends_with(v),
        _ => x.contains(v),
    };
    if op == "not_contains" {
        !values.iter().any(|x| x.contains(v))
    } else {
        values.iter().any(check)
    }
}

pub fn matches(rule: &Rule, f: &Facts) -> bool {
    if rule.conditions.is_empty() {
        return false;
    }
    let eval = |c: &Condition| {
        let v = c.value.trim().to_lowercase().replace('\\', "/");
        let v = if c.field == "ext" { v.trim_start_matches('.').to_string() } else { v };
        if v.is_empty() {
            return false;
        }
        let name = [f.name.clone()];
        let values: &[String] = match c.field.as_str() {
            "name" => &name,
            "source" => &f.sources,
            "path" => &f.paths,
            "ext" => &f.exts,
            "library" => &f.libraries,
            _ => &[],
        };
        test_one(values, &c.op, &v)
    };
    if rule.match_all { rule.conditions.iter().all(eval) } else { rule.conditions.iter().any(eval) }
}

fn row_to_rule(r: &rusqlite::Row<'_>) -> rusqlite::Result<Rule> {
    let cond: String = r.get(5)?;
    let tags: String = r.get(6)?;
    Ok(Rule {
        id: r.get(0)?,
        name: r.get(1)?,
        enabled: r.get::<_, i64>(2)? != 0,
        auto_apply: r.get::<_, i64>(3)? != 0,
        match_all: r.get::<_, i64>(4)? != 0,
        conditions: serde_json::from_str(&cond).unwrap_or_default(),
        tag_ids: serde_json::from_str(&tags).unwrap_or_default(),
    })
}

const SELECT: &str = "SELECT id, name, enabled, auto_apply, match_all, conditions, tag_ids FROM rules";

pub fn load_enabled(conn: &Connection) -> rusqlite::Result<Vec<Rule>> {
    let mut s = conn.prepare(&format!("{SELECT} WHERE enabled = 1 ORDER BY id"))?;
    let v = s.query_map([], row_to_rule)?.collect();
    v
}

/// Gắn tag của các rule auto_apply khớp với resource.
pub fn apply_auto(conn: &Connection, rules: &[Rule], resource_id: i64) -> rusqlite::Result<usize> {
    let auto: Vec<&Rule> = rules.iter().filter(|r| r.auto_apply).collect();
    if auto.is_empty() {
        return Ok(0);
    }
    let f = facts(conn, resource_id)?;
    let mut n = 0;
    for r in auto {
        if matches(r, &f) {
            for t in &r.tag_ids {
                if crate::autotag::insert_auto(conn, resource_id, *t, "rule", &format!("Rule: {}", r.name))? {
                    n += 1;
                }
            }
        }
    }
    Ok(n)
}

/// Tag gợi ý từ các rule không tự áp dụng: (tag_id, tên rule).
pub fn suggestions(conn: &Connection, resource_id: i64) -> rusqlite::Result<Vec<(i64, String)>> {
    let rules: Vec<Rule> = load_enabled(conn)?.into_iter().filter(|r| !r.auto_apply).collect();
    if rules.is_empty() {
        return Ok(vec![]);
    }
    let f = facts(conn, resource_id)?;
    let mut out = Vec::new();
    for r in rules {
        if matches(&r, &f) {
            for t in r.tag_ids {
                out.push((t, r.name.clone()));
            }
        }
    }
    Ok(out)
}

#[tauri::command]
pub fn list_rules(state: State<AppState>) -> Res<Vec<Rule>> {
    let conn = state.db.lock().unwrap();
    let mut s = conn.prepare(&format!("{SELECT} ORDER BY id")).map_err(e)?;
    let v = s.query_map([], row_to_rule).map_err(e)?.collect::<Result<Vec<_>, _>>().map_err(e);
    v
}

#[tauri::command]
pub fn save_rule(state: State<AppState>, rule: Rule) -> Res<i64> {
    if rule.name.trim().is_empty() || rule.conditions.is_empty() || rule.tag_ids.is_empty() {
        return Err("Rule cần tên, ít nhất một điều kiện và một tag".into());
    }
    let conn = state.db.lock().unwrap();
    let cond = serde_json::to_string(&rule.conditions).map_err(e)?;
    let tags = serde_json::to_string(&rule.tag_ids).map_err(e)?;
    if rule.id > 0 {
        conn.execute(
            "UPDATE rules SET name=?1, enabled=?2, auto_apply=?3, match_all=?4, conditions=?5, tag_ids=?6 WHERE id=?7",
            params![rule.name.trim(), rule.enabled, rule.auto_apply, rule.match_all, cond, tags, rule.id],
        )
        .map_err(e)?;
        Ok(rule.id)
    } else {
        conn.execute(
            "INSERT INTO rules (name, enabled, auto_apply, match_all, conditions, tag_ids, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![rule.name.trim(), rule.enabled, rule.auto_apply, rule.match_all, cond, tags, db::now()],
        )
        .map_err(e)?;
        Ok(conn.last_insert_rowid())
    }
}

#[tauri::command]
pub fn delete_rule(state: State<AppState>, id: i64) -> Res<()> {
    state.db.lock().unwrap().execute("DELETE FROM rules WHERE id = ?1", [id]).map_err(e)?;
    Ok(())
}

#[derive(Serialize)]
pub struct RuleTest {
    count: usize,
    sample: Vec<String>,
}

fn matching_resources(conn: &Connection, rule: &Rule) -> Res<Vec<(i64, String)>> {
    let ids: Vec<(i64, String)> = {
        let mut s = conn.prepare("SELECT id, name FROM resources ORDER BY name COLLATE NOCASE").map_err(e)?;
        let v = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(e)?.collect::<Result<_, _>>().map_err(e)?;
        v
    };
    let mut out = Vec::new();
    for (id, name) in ids {
        if matches(rule, &facts(conn, id).map_err(e)?) {
            out.push((id, name));
        }
    }
    Ok(out)
}

/// Xem trước rule khớp bao nhiêu resource hiện có.
#[tauri::command]
pub fn test_rule(state: State<AppState>, rule: Rule) -> Res<RuleTest> {
    let conn = state.db.lock().unwrap();
    let m = matching_resources(&conn, &rule)?;
    Ok(RuleTest { count: m.len(), sample: m.into_iter().take(12).map(|x| x.1).collect() })
}

/// Áp dụng rule cho toàn bộ resource hiện có (chỉ gắn tag — thay đổi metadata, không đụng file).
#[tauri::command]
pub fn apply_rule_now(state: State<AppState>, id: i64) -> Res<usize> {
    let mut conn = state.db.lock().unwrap();
    let tx = conn.transaction().map_err(e)?;
    let rule = tx.query_row(&format!("{SELECT} WHERE id = ?1"), [id], row_to_rule).map_err(e)?;
    let m = matching_resources(&tx, &rule)?;
    for (rid, _) in &m {
        for t in &rule.tag_ids {
            crate::autotag::insert_auto(&tx, *rid, *t, "rule", &format!("Rule: {}", rule.name)).map_err(e)?;
        }
        db::reindex(&tx, *rid).map_err(e)?;
    }
    tx.commit().map_err(e)?;
    Ok(m.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            name: "kodak 2383 lut pack".into(),
            sources: vec!["kodak 2383 lut pack".into()],
            paths: vec!["d:/resources/luts/kodak 2383 lut pack".into()],
            exts: vec!["cube".into()],
            libraries: vec!["resources".into()],
        }
    }

    fn rule(conds: Vec<(&str, &str, &str)>, all: bool) -> Rule {
        Rule {
            id: 0,
            name: "t".into(),
            enabled: true,
            auto_apply: false,
            match_all: all,
            conditions: conds.into_iter().map(|(f, o, v)| Condition { field: f.into(), op: o.into(), value: v.into() }).collect(),
            tag_ids: vec![1],
        }
    }

    #[test]
    fn evaluates_conditions() {
        let f = facts();
        assert!(matches(&rule(vec![("ext", "equals", ".cube")], true), &f));
        assert!(matches(&rule(vec![("path", "contains", "D:\\Resources\\LUTs")], true), &f));
        assert!(!matches(&rule(vec![("name", "contains", "lut"), ("ext", "equals", "wav")], true), &f));
        assert!(matches(&rule(vec![("name", "contains", "lut"), ("ext", "equals", "wav")], false), &f));
        assert!(matches(&rule(vec![("name", "not_contains", "sfx")], true), &f));
    }
}
