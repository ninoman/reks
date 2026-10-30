//! The rule tree (D29): nested all/any groups of filters, where any node can
//! be negated, and the SQL every filter compiles to. `query.rs` uses the same
//! compiler for its flat filter list, so a filter means the same thing in
//! the bar and in a rule.
//!
//! A playlist filter resolves smart playlists: one on a smart list is that
//! list's rule, and one on a folder also matches the rules of the smart
//! lists below it. A list reached again while its own rule is being
//! expanded is a cycle: refused when saving (`Compiler::strict`), and
//! matching nothing (with a warning) when querying.
//!
//! NOT compiles as `NOT coalesce((cond), 0)`: a condition on a NULL column
//! (no genre, no key) is unknown, and a plain `NOT` would keep it unknown and
//! drop the track. Under AND/OR an unknown already acts as false, so NOT is
//! the only place that needs it.

use std::collections::HashMap;

use rusqlite::types::Value;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::query::{Filter, PlaylistId, Sort, SortColumn};

/// Deepest rule tree accepted; a lone filter has depth 1.
pub const MAX_RULE_DEPTH: usize = 8;
/// Most nodes (groups and filters) in one rule tree.
pub const MAX_RULE_NODES: usize = 200;

/// Most nodes compiled for one query once smart playlists are expanded.
const MAX_EXPANDED_NODES: usize = 1_000;

/// How a group combines its rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Match {
    /// Every rule holds (an empty group: every track).
    All,
    /// At least one rule holds (an empty group: no track).
    Any,
}

/// A node of a rule tree. `not` negates the node.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RuleNode {
    Group {
        r#match: Match,
        #[serde(default)]
        not: bool,
        #[serde(default)]
        rules: Vec<RuleNode>,
    },
    Filter {
        #[serde(default)]
        not: bool,
        filter: Filter,
    },
}

impl RuleNode {
    /// Rejects a tree deeper than [`MAX_RULE_DEPTH`] or larger than
    /// [`MAX_RULE_NODES`], so a bad payload can't build huge SQL.
    pub fn validate(&self) -> Result<()> {
        let mut nodes = 0;
        self.walk(1, &mut nodes)
    }

    fn walk(&self, depth: usize, nodes: &mut usize) -> Result<()> {
        *nodes += 1;
        if depth > MAX_RULE_DEPTH {
            return Err(Error::InvalidField {
                field: "rule",
                reason: format!("nested deeper than {MAX_RULE_DEPTH} levels"),
            });
        }
        if *nodes > MAX_RULE_NODES {
            return Err(Error::InvalidField {
                field: "rule",
                reason: format!("more than {MAX_RULE_NODES} conditions and groups"),
            });
        }
        if let Self::Group { rules, .. } = self {
            for r in rules {
                r.walk(depth + 1, nodes)?;
            }
        }
        Ok(())
    }

    /// Whether a playlist filter appears anywhere in the tree.
    pub(crate) fn mentions_playlist(&self) -> bool {
        match self {
            Self::Filter { filter, .. } => matches!(filter, Filter::Playlist { .. }),
            Self::Group { rules, .. } => rules.iter().any(Self::mentions_playlist),
        }
    }

    /// An empty, un-negated "all" group: holds for every track.
    pub(crate) fn is_everything(&self) -> bool {
        matches!(self, Self::Group { r#match: Match::All, not: false, rules } if rules.is_empty())
    }
}

/// A smart playlist's order when it has none of its own: date added,
/// newest first.
pub const DEFAULT_SMART_SORT: Sort = Sort {
    column: SortColumn::DateAdded,
    descending: true,
};

/// A smart playlist's stored definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SmartDef {
    /// `None`: the stored JSON does not parse; the list matches nothing.
    pub rule: Option<RuleNode>,
    pub sort: Sort,
}

#[derive(Debug, Clone)]
struct IndexNode {
    is_folder: bool,
    smart: Option<SmartDef>,
}

/// The playlist tree as playlist filters need it: which ids are folders and
/// which are smart, with their rules. Loaded once per query.
#[derive(Debug, Clone, Default)]
pub(crate) struct PlaylistIndex {
    nodes: HashMap<PlaylistId, IndexNode>,
    /// Children in display order.
    children: HashMap<PlaylistId, Vec<PlaylistId>>,
}

impl PlaylistIndex {
    pub fn load(conn: &Connection) -> Result<Self> {
        let mut out = Self::default();
        let mut stmt = conn.prepare_cached(
            "SELECT id, parent_id, is_folder, smart_rule, smart_sort FROM playlist
             ORDER BY sort_order, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, PlaylistId>(0)?,
                r.get::<_, Option<PlaylistId>>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?;
        for row in rows {
            let (id, parent, is_folder, rule, sort) = row?;
            let smart = rule.map(|rule| SmartDef {
                rule: serde_json::from_str(&rule)
                    .inspect_err(|e| {
                        tracing::warn!(playlist = id, error = %e, "smart rule does not parse");
                    })
                    .ok(),
                sort: sort
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(DEFAULT_SMART_SORT),
            });
            if let Some(p) = parent {
                out.children.entry(p).or_default().push(id);
            }
            out.nodes.insert(id, IndexNode { is_folder, smart });
        }
        Ok(out)
    }

    pub fn smart(&self, id: PlaylistId) -> Option<&SmartDef> {
        self.nodes.get(&id).and_then(|n| n.smart.as_ref())
    }

    /// A playlist whose rows are its own entries: not a folder, not smart.
    pub fn is_static_playlist(&self, id: PlaylistId) -> bool {
        self.nodes
            .get(&id)
            .is_some_and(|n| !n.is_folder && n.smart.is_none())
    }

    /// Ids of every smart list, in no particular order.
    pub fn smart_ids(&self) -> impl Iterator<Item = PlaylistId> + '_ {
        self.nodes
            .iter()
            .filter(|(_, n)| n.smart.is_some())
            .map(|(&id, _)| id)
    }

    /// Smart lists anywhere below `folder`, depth first.
    fn smart_below(&self, folder: PlaylistId, out: &mut Vec<PlaylistId>) {
        for &c in self.children.get(&folder).into_iter().flatten() {
            match self.nodes.get(&c) {
                Some(n) if n.smart.is_some() => out.push(c),
                Some(n) if n.is_folder => self.smart_below(c, out),
                _ => {}
            }
        }
    }
}

/// Builds SQL conditions on `track t`; user values go to `params`, in the
/// order their `?` appear.
pub(crate) struct Compiler<'a> {
    pub params: Vec<Value>,
    index: &'a PlaylistIndex,
    /// Smart lists whose rules are being expanded, outermost first.
    expanding: Vec<PlaylistId>,
    /// Rule nodes compiled so far, smart lists' rules included.
    nodes: usize,
    /// A cycle is an error rather than "matches nothing".
    strict: bool,
}

impl<'a> Compiler<'a> {
    pub fn new(index: &'a PlaylistIndex) -> Self {
        Self {
            params: Vec::new(),
            index,
            expanding: Vec::new(),
            nodes: 0,
            strict: false,
        }
    }

    /// For saving: a cycle fails with [`Error::SmartPlaylistCycle`].
    pub fn strict(index: &'a PlaylistIndex) -> Self {
        Self {
            strict: true,
            ..Self::new(index)
        }
    }

    /// A rule tree's condition. Call [`RuleNode::validate`] first.
    pub fn rule(&mut self, node: &RuleNode) -> Result<String> {
        self.nodes += 1;
        if self.nodes > MAX_EXPANDED_NODES {
            return Err(Error::InvalidField {
                field: "rule",
                reason: format!(
                    "more than {MAX_EXPANDED_NODES} conditions once smart playlists are expanded"
                ),
            });
        }
        let (cond, not) = match node {
            RuleNode::Filter { not, filter } => (self.filter(filter)?, *not),
            RuleNode::Group {
                r#match,
                not,
                rules,
            } => {
                let (op, empty) = match r#match {
                    Match::All => (" AND ", "1"),
                    Match::Any => (" OR ", "0"),
                };
                let conds = rules
                    .iter()
                    .map(|r| self.rule(r))
                    .collect::<Result<Vec<_>>>()?;
                let cond = if conds.is_empty() {
                    empty.to_owned()
                } else {
                    format!("({})", conds.join(op))
                };
                (cond, *not)
            }
        };
        Ok(if not {
            format!("NOT coalesce(({cond}), 0)")
        } else {
            cond
        })
    }

    /// In playlist `id`: a smart list's rule; a static list's entries; a
    /// folder's entries below it, or any smart list's rule below it.
    fn playlist(&mut self, id: PlaylistId) -> Result<String> {
        if self.index.smart(id).is_some() {
            return self.smart(id);
        }
        self.params.push(Value::Integer(id));
        let mut conds = vec![PLAYLIST_SUBTREE.to_owned()];
        let mut smart = Vec::new();
        self.index.smart_below(id, &mut smart);
        for s in smart {
            conds.push(self.smart(s)?);
        }
        Ok(if conds.len() == 1 {
            conds.remove(0)
        } else {
            format!("({})", conds.join(" OR "))
        })
    }

    fn smart(&mut self, id: PlaylistId) -> Result<String> {
        let index = self.index;
        if self.expanding.contains(&id) {
            if self.strict {
                return Err(Error::SmartPlaylistCycle(id));
            }
            tracing::warn!(
                playlist = id,
                "smart playlist reached through its own rules; that reference matches nothing"
            );
            return Ok("0".into());
        }
        let Some(rule) = index.smart(id).and_then(|d| d.rule.as_ref()) else {
            return Ok("0".into());
        };
        self.expanding.push(id);
        let cond = self.rule(rule);
        self.expanding.pop();
        cond
    }

    /// One filter's condition.
    pub fn filter(&mut self, f: &Filter) -> Result<String> {
        let p = &mut self.params;
        Ok(match f {
            Filter::Genre { genre } => {
                p.push(Value::Text(genre.clone()));
                "t.genre = ? COLLATE NOCASE".into()
            }
            Filter::KeyIn { keys } => {
                if keys.is_empty() {
                    return Ok("0".into());
                }
                p.extend(keys.iter().map(|k| Value::Text(k.canonical().to_owned())));
                format!("t.key IN ({})", placeholders(keys.len()))
            }
            Filter::BpmRange { min, max } => {
                p.push(Value::Integer(i64::from(*min)));
                p.push(Value::Integer(i64::from(*max)));
                "t.bpm_x100 BETWEEN ? AND ?".into()
            }
            Filter::Color { color } => {
                p.push(Value::Text(color.clone()));
                "t.color = ? COLLATE NOCASE".into()
            }
            Filter::RatingMin { rating } => {
                p.push(Value::Integer(i64::from(*rating)));
                "t.rating >= ?".into()
            }
            Filter::Playlist { id } => return self.playlist(*id),
            Filter::Missing { missing } => {
                p.push(Value::Integer(i64::from(*missing)));
                "t.missing = ?".into()
            }
            Filter::NeverPlayed => "(t.play_count = 0 AND t.last_played IS NULL)".into(),
            Filter::PlayCountRange { min, max } => {
                p.push(Value::Integer(i64::from(*min)));
                p.push(Value::Integer(i64::from(*max)));
                "t.play_count BETWEEN ? AND ?".into()
            }
            Filter::PlayedSince { date } => {
                p.push(Value::Text(date.clone()));
                "t.last_played >= ?".into()
            }
            Filter::RatingRange { min, max } => {
                p.push(Value::Integer(i64::from(*min)));
                p.push(Value::Integer(i64::from(*max)));
                "t.rating BETWEEN ? AND ?".into()
            }
            Filter::TagAll { ids } => {
                let mut unique = ids.clone();
                unique.sort_unstable();
                unique.dedup();
                if unique.is_empty() {
                    return Ok("0".into());
                }
                p.extend(unique.iter().map(|&id| Value::Integer(id)));
                p.push(Value::Integer(unique.len() as i64));
                format!(
                    "(SELECT COUNT(*) FROM track_tag tt WHERE tt.track_id = t.id
                       AND tt.tag_id IN ({})) = ?",
                    placeholders(unique.len())
                )
            }
            Filter::TagAny { ids } => {
                if ids.is_empty() {
                    return Ok("0".into());
                }
                p.extend(ids.iter().map(|&id| Value::Integer(id)));
                format!(
                    "EXISTS (SELECT 1 FROM track_tag tt WHERE tt.track_id = t.id
                       AND tt.tag_id IN ({}))",
                    placeholders(ids.len())
                )
            }
        })
    }
}

/// The track is an entry of the playlist `?`, or of any playlist below the
/// folder `?`. Smart lists have no entries.
const PLAYLIST_SUBTREE: &str = "EXISTS (SELECT 1 FROM playlist_track pt WHERE pt.track_id = t.id
   AND pt.playlist_id IN (
     WITH RECURSIVE sub(id) AS (
       SELECT ? UNION SELECT p.id FROM playlist p JOIN sub ON p.parent_id = sub.id)
     SELECT id FROM sub))";

pub(crate) fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf() -> RuleNode {
        RuleNode::Filter {
            not: false,
            filter: Filter::NeverPlayed,
        }
    }

    fn group(rules: Vec<RuleNode>) -> RuleNode {
        RuleNode::Group {
            r#match: Match::All,
            not: false,
            rules,
        }
    }

    #[test]
    fn limits() {
        let mut deep = leaf();
        for _ in 1..MAX_RULE_DEPTH {
            deep = group(vec![deep]);
        }
        assert!(deep.validate().is_ok());
        assert!(matches!(
            group(vec![deep]).validate(),
            Err(Error::InvalidField { field: "rule", .. })
        ));

        let wide = group(vec![leaf(); MAX_RULE_NODES - 1]);
        assert!(wide.validate().is_ok());
        assert!(group(vec![wide]).validate().is_err());
    }

    #[test]
    fn compiles_groups_and_not() {
        let index = PlaylistIndex::default();
        let mut c = Compiler::new(&index);
        let sql = c
            .rule(&RuleNode::Group {
                r#match: Match::Any,
                not: true,
                rules: vec![
                    leaf(),
                    RuleNode::Filter {
                        not: true,
                        filter: Filter::RatingMin { rating: 3 },
                    },
                ],
            })
            .unwrap();
        assert_eq!(
            sql,
            "NOT coalesce((((t.play_count = 0 AND t.last_played IS NULL) OR \
             NOT coalesce((t.rating >= ?), 0))), 0)"
        );
        assert_eq!(c.params, [Value::Integer(3)]);
        assert_eq!(c.rule(&group(vec![])).unwrap(), "1");
        assert_eq!(
            c.rule(&RuleNode::Group {
                r#match: Match::Any,
                not: false,
                rules: vec![]
            })
            .unwrap(),
            "0"
        );
    }
}
