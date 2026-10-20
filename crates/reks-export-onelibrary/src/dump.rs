//! A plain dump of a whole `exportLibrary.db`: `sqlite_master` and every
//! table's rows, as JSON. The reference export is committed in this form
//! (`fixtures/onelibrary/ref-<ver>/dump.json`) so tests compare against it
//! without the key; [`crate::diff`] compares two dumps.

use std::collections::BTreeMap;

use rusqlite::types::ValueRef;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Result;

/// One `sqlite_master` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaEntry {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub tbl_name: String,
    pub sql: Option<String>,
}

/// A table's columns and its rows in rowid order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl Table {
    /// Index of `column`.
    pub fn col(&self, column: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == column)
    }

    /// Rows as column → value maps.
    pub fn records(&self) -> impl Iterator<Item = BTreeMap<&str, &Value>> {
        self.rows.iter().map(|r| {
            self.columns
                .iter()
                .map(String::as_str)
                .zip(r.iter())
                .collect()
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Dump {
    /// `sqlite_master` in creation order.
    pub schema: Vec<SchemaEntry>,
    pub tables: BTreeMap<String, Table>,
}

impl Dump {
    /// Reads every table of `conn` (already unlocked).
    pub fn read(conn: &Connection) -> Result<Self> {
        let mut stmt =
            conn.prepare("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY rowid")?;
        let schema = stmt
            .query_map([], |r| {
                Ok(SchemaEntry {
                    kind: r.get(0)?,
                    name: r.get(1)?,
                    tbl_name: r.get(2)?,
                    sql: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut tables = BTreeMap::new();
        for entry in schema.iter().filter(|e| e.kind == "table") {
            tables.insert(entry.name.clone(), read_table(conn, &entry.name)?);
        }
        Ok(Self { schema, tables })
    }

    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables.get(name)
    }

    /// Stable JSON, one row per line, for fixtures and review.
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n  \"schema\": [\n");
        let n = self.schema.len();
        for (i, e) in self.schema.iter().enumerate() {
            out.push_str("    ");
            out.push_str(&line(e));
            out.push_str(if i + 1 < n { ",\n" } else { "\n" });
        }
        out.push_str("  ],\n  \"tables\": {\n");
        let n = self.tables.len();
        for (i, (name, t)) in self.tables.iter().enumerate() {
            out.push_str(&format!(
                "    {}: {{\n      \"columns\": {},\n      \"rows\": [",
                line(name),
                line(&t.columns)
            ));
            let m = t.rows.len();
            for (j, row) in t.rows.iter().enumerate() {
                out.push_str("\n        ");
                out.push_str(&line(row));
                if j + 1 < m {
                    out.push(',');
                }
            }
            out.push_str(if m > 0 {
                "\n      ]\n    }"
            } else {
                "]\n    }"
            });
            out.push_str(if i + 1 < n { ",\n" } else { "\n" });
        }
        out.push_str("  }\n}\n");
        out
    }

    pub fn from_json(text: &str) -> serde_json::Result<Self> {
        serde_json::from_str(text)
    }
}

/// Compact JSON of plain data (strings, numbers, arrays), which cannot fail.
fn line<T: Serialize + ?Sized>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

fn read_table(conn: &Connection, name: &str) -> Result<Table> {
    let quoted = name.replace('"', "\"\"");
    let mut stmt = conn.prepare(&format!("SELECT * FROM \"{quoted}\" ORDER BY rowid"))?;
    let columns: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
    let width = columns.len();
    let mut rows = Vec::new();
    let mut q = stmt.query([])?;
    while let Some(r) = q.next()? {
        let mut row = Vec::with_capacity(width);
        for i in 0..width {
            row.push(json(r.get_ref(i)?));
        }
        rows.push(row);
    }
    Ok(Table { columns, rows })
}

fn json(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => i.into(),
        ValueRef::Real(f) => f.into(),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned().into(),
        ValueRef::Blob(b) => {
            let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
            format!("x'{hex}'").into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE t (a integer, b varchar); \
             INSERT INTO t VALUES (1, 'x'), (NULL, '\u{fffa}y\u{fffb}'); \
             CREATE TABLE e (a integer); CREATE INDEX i ON t(a);",
        )
        .unwrap();
        let d = Dump::read(&conn).unwrap();
        assert_eq!(d.schema.len(), 3);
        assert_eq!(d.tables["t"].rows.len(), 2);
        assert_eq!(d.tables["t"].rows[1][0], Value::Null);
        let text = d.to_json();
        assert_eq!(Dump::from_json(&text).unwrap(), d);
        assert!(text.contains("\"e\": {\n      \"columns\": [\"a\"],\n      \"rows\": []"));
    }
}
