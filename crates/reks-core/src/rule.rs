//! The rule tree (D29): nested all/any groups of filters, where any node can
//! be negated, and the SQL every filter compiles to. `query.rs` uses the same
//! compiler for its flat filter list, so a filter means the same thing in
//! the bar and in a rule.
//!
//! NOT compiles as `NOT coalesce((cond), 0)`: a condition on a NULL column
//! (no genre, no key) is unknown, and a plain `NOT` would keep it unknown and
//! drop the track. Under AND/OR an unknown already acts as false, so NOT is
//! the only place that needs it.

use rusqlite::types::Value;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::query::Filter;

/// Deepest rule tree accepted; a lone filter has depth 1.
pub const MAX_RULE_DEPTH: usize = 8;
/// Most nodes (groups and filters) in one rule tree.
pub const MAX_RULE_NODES: usize = 200;

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

    /// An empty, un-negated "all" group: holds for every track.
    pub(crate) fn is_everything(&self) -> bool {
        matches!(self, Self::Group { r#match: Match::All, not: false, rules } if rules.is_empty())
    }
}

/// Builds SQL conditions on `track t`; user values go to `params`, in the
/// order their `?` appear.
#[derive(Default)]
pub(crate) struct Compiler {
    pub params: Vec<Value>,
}

impl Compiler {
    /// A rule tree's condition. Call [`RuleNode::validate`] first.
    pub fn rule(&mut self, node: &RuleNode) -> Result<String> {
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
            Filter::Playlist { id } => {
                p.push(Value::Integer(*id));
                PLAYLIST_SUBTREE.into()
            }
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
/// folder `?`.
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
        let mut c = Compiler::default();
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
