//! Ghép các nguồn vật lý (ZIP/RAR ↔ folder) thành cùng một Resource Item.

use crate::normalize::{similarity, Normalized, AUTO_MATCH, POSSIBLE_MATCH};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct MatchItem {
    pub source_id: i64,
    pub is_archive: bool,
    pub is_folder: bool,
    /// Thư mục cha tương đối — auto match chỉ áp dụng khi cùng thư mục cha.
    pub parent: String,
    pub norm: Normalized,
    /// Chữ ký nội dung — trùng nhau nghĩa là cùng một gói dù tên khác.
    pub sig: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Auto,
    Possible,
}

#[derive(Debug, Clone)]
pub struct MatchPair {
    pub a: i64,
    pub b: i64,
    pub score: f64,
    pub decision: Decision,
}

fn bucket_key(n: &Normalized) -> String {
    n.compact.chars().take(3).collect()
}

/// Tìm cặp ghép. `skip` là các cặp (a,b) đã từng được đề xuất/từ chối.
pub fn find_matches(items: &[MatchItem], skip: &HashSet<(i64, i64)>) -> Vec<MatchPair> {
    let archives: Vec<&MatchItem> = items.iter().filter(|i| i.is_archive).collect();
    let folders: Vec<&MatchItem> = items.iter().filter(|i| i.is_folder).collect();

    let mut by_parent: HashMap<&str, Vec<&MatchItem>> = HashMap::new();
    let mut by_bucket: HashMap<String, Vec<&MatchItem>> = HashMap::new();
    let mut by_sig: HashMap<&str, Vec<&MatchItem>> = HashMap::new();
    for f in &folders {
        by_parent.entry(f.parent.as_str()).or_default().push(f);
        by_bucket.entry(bucket_key(&f.norm)).or_default().push(f);
        if let Some(sig) = &f.sig {
            by_sig.entry(sig.as_str()).or_default().push(f);
        }
    }

    let mut pairs: Vec<MatchPair> = Vec::new();
    let mut seen: HashSet<(i64, i64)> = HashSet::new();
    let key = |x: i64, y: i64| if x < y { (x, y) } else { (y, x) };

    let mut consider = |a: &MatchItem, b: &MatchItem, pairs: &mut Vec<MatchPair>| {
        let k = key(a.source_id, b.source_id);
        if a.source_id == b.source_id || skip.contains(&k) || !seen.insert(k) {
            return;
        }
        let same_content = a.sig.is_some() && a.sig == b.sig;
        let score = if same_content { 100.0 } else { similarity(&a.norm, &b.norm) };
        let same_parent = a.parent == b.parent;
        let both_archives = a.is_archive && b.is_archive;
        let decision = if both_archives {
            // Nhiều phần (.part1/.part2) hoặc cùng gói ở 2 định dạng nén: chỉ ghép khi trùng tuyệt đối.
            if same_parent && score >= 100.0 && (same_content || a.sig.is_none() || b.sig.is_none() || a.norm == b.norm) {
                Some(Decision::Auto)
            } else {
                None
            }
        } else if score >= AUTO_MATCH && same_parent {
            Some(Decision::Auto)
        } else if score >= POSSIBLE_MATCH {
            Some(Decision::Possible)
        } else {
            None
        };
        if let Some(decision) = decision {
            pairs.push(MatchPair { a: k.0, b: k.1, score, decision });
        }
    };

    for a in &archives {
        if let Some(list) = by_parent.get(a.parent.as_str()) {
            for f in list {
                consider(a, f, &mut pairs);
            }
        }
        if let Some(list) = by_bucket.get(&bucket_key(&a.norm)) {
            for f in list {
                consider(a, f, &mut pairs);
            }
        }
        if let Some(list) = a.sig.as_deref().and_then(|sig| by_sig.get(sig)) {
            for f in list {
                consider(a, f, &mut pairs);
            }
        }
    }
    // archive ↔ archive cùng thư mục
    let mut arch_by_parent: HashMap<&str, Vec<&MatchItem>> = HashMap::new();
    for a in &archives {
        arch_by_parent.entry(a.parent.as_str()).or_default().push(a);
    }
    for list in arch_by_parent.values() {
        for i in 0..list.len() {
            for j in i + 1..list.len() {
                consider(list[i], list[j], &mut pairs);
            }
        }
    }

    // Greedy: điểm cao trước; mỗi folder chỉ auto-ghép với 1 archive.
    pairs.sort_by(|x, y| y.score.partial_cmp(&x.score).unwrap_or(std::cmp::Ordering::Equal));
    let folder_ids: HashSet<i64> = folders.iter().map(|f| f.source_id).collect();
    let mut used_folder: HashSet<i64> = HashSet::new();
    let mut out = Vec::new();
    for mut p in pairs {
        if p.decision == Decision::Auto {
            let folder = [p.a, p.b].into_iter().find(|id| folder_ids.contains(id));
            if let Some(fid) = folder {
                if !used_folder.insert(fid) {
                    p.decision = Decision::Possible;
                }
            }
        }
        out.push(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::normalize;

    fn item(id: i64, name: &str, archive: bool, parent: &str) -> MatchItem {
        MatchItem {
            source_id: id,
            is_archive: archive,
            is_folder: !archive,
            parent: parent.into(),
            norm: normalize(name, archive),
            sig: None,
        }
    }

    #[test]
    fn same_content_different_name() {
        let mut a = item(1, "download_2931.zip", true, "");
        let mut b = item(2, "Cinematic LUTs", false, "");
        a.sig = Some("abc-3".into());
        b.sig = Some("abc-3".into());
        let m = find_matches(&[a, b], &HashSet::new());
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].decision, Decision::Auto);
        assert_eq!(m[0].score, 100.0);
    }

    #[test]
    fn auto_and_possible() {
        let items = vec![
            item(1, "FilmConvert_Nitrate_v3.52.zip", true, ""),
            item(2, "FilmConvert Nitrate 3.52", false, ""),
            item(3, "Glitch Transitions Pack.zip", true, ""),
            item(4, "Glitch Transition Pack Vol 2", false, ""),
            item(5, "Whoosh SFX", false, ""),
        ];
        let m = find_matches(&items, &HashSet::new());
        assert!(m.iter().any(|p| p.a == 1 && p.b == 2 && p.decision == Decision::Auto));
        assert!(m.iter().any(|p| p.a == 3 && p.b == 4 && p.decision == Decision::Possible));
        assert!(!m.iter().any(|p| p.a == 5 || p.b == 5));
    }

    #[test]
    fn different_parent_is_only_possible() {
        let items = vec![item(1, "Pack.zip", true, "Downloads"), item(2, "Pack", false, "Plugins")];
        let m = find_matches(&items, &HashSet::new());
        assert_eq!(m[0].decision, Decision::Possible);
    }

    #[test]
    fn one_folder_one_auto() {
        let items = vec![
            item(1, "Pack.zip", true, ""),
            item(2, "Pack.rar", true, "x"),
            item(3, "Pack", false, ""),
        ];
        let m = find_matches(&items, &HashSet::new());
        assert_eq!(m.iter().filter(|p| p.decision == Decision::Auto).count(), 1);
    }

    #[test]
    fn skip_rejected() {
        let items = vec![item(1, "Pack.zip", true, ""), item(2, "Pack", false, "")];
        let skip: HashSet<(i64, i64)> = [(1, 2)].into_iter().collect();
        assert!(find_matches(&items, &skip).is_empty());
    }
}
