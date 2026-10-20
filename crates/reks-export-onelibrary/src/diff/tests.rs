use serde_json::json;

use super::*;
use crate::dump::SchemaEntry;

fn table(columns: &[&str], rows: Vec<Vec<Value>>) -> Table {
    Table {
        columns: columns.iter().map(|c| (*c).to_owned()).collect(),
        rows,
    }
}

fn dump(artist: &str, artist_id: i64, title: &str, entries: &[i64]) -> Dump {
    let mut d = Dump::default();
    d.schema.push(SchemaEntry {
        kind: "table".into(),
        name: "artist".into(),
        tbl_name: "artist".into(),
        sql: Some("CREATE TABLE artist(artist_id integer primary key, name varchar)".into()),
    });
    d.tables.insert(
        "artist".into(),
        table(
            &["artist_id", "name"],
            vec![vec![json!(artist_id), json!(artist)]],
        ),
    );
    d.tables.insert(
        "content".into(),
        table(
            &[
                "content_id",
                "title",
                "path",
                "artist_id_artist",
                "masterContentId",
            ],
            vec![
                vec![
                    json!(10 + artist_id),
                    json!(title),
                    json!("/Contents/a.mp3"),
                    json!(artist_id),
                    json!(artist_id * 99),
                ],
                vec![
                    json!(20 + artist_id),
                    json!("B"),
                    json!("/Contents/b.mp3"),
                    Value::Null,
                    json!(1),
                ],
            ],
        ),
    );
    d.tables.insert(
        "playlist".into(),
        table(
            &[
                "playlist_id",
                "sequenceNo",
                "name",
                "image_id",
                "attribute",
                "playlist_id_parent",
            ],
            vec![
                vec![
                    json!(1),
                    json!(0),
                    json!("F"),
                    Value::Null,
                    json!(1),
                    json!(0),
                ],
                vec![
                    json!(2),
                    json!(0),
                    json!("L"),
                    Value::Null,
                    json!(0),
                    json!(1),
                ],
            ],
        ),
    );
    d.tables.insert(
        "playlist_content".into(),
        table(
            &["playlist_id", "content_id", "sequenceNo"],
            entries
                .iter()
                .zip(1..)
                .map(|(&c, n)| vec![json!(2), json!(c + artist_id), json!(n)])
                .collect(),
        ),
    );
    d
}

#[test]
fn ids_that_differ_between_writers_are_not_differences() {
    // Same names and order, different ids.
    let a = dump("Art", 1, "A", &[10, 20]);
    let b = dump("Art", 5, "A", &[10, 20]);
    assert_eq!(diff(&a, &b, DiffOptions::default()), []);
    assert!(!diff(&a, &b, DiffOptions { strict: true }).is_empty());
}

#[test]
fn lists_every_differing_column_and_entry() {
    let a = dump("Art", 1, "A", &[10, 20]);
    let b = dump("Other", 1, "A2", &[20, 10]);
    let d = diff(&a, &b, DiffOptions::default());
    let text: Vec<String> = d.iter().map(ToString::to_string).collect();
    assert!(
        text.contains(&"content /Contents/a.mp3 title: ours \"A\" | theirs \"A2\"".into()),
        "{text:#?}"
    );
    assert!(
        text.contains(
            &"content /Contents/a.mp3 artist_id_artist: ours \"Art\" | theirs \"Other\"".into()
        ),
        "{text:#?}"
    );
    assert!(d.iter().any(|x| matches!(x, Difference::LookupOnlyIn { table, side: Side::Ours, name } if table == "artist" && name == "Art")));
    assert!(d
        .iter()
        .any(|x| matches!(x, Difference::PlaylistEntries { path, .. } if path == "F/L")));
}

#[test]
fn schema_and_missing_rows_show_up() {
    let a = dump("Art", 1, "A", &[10]);
    let mut b = a.clone();
    b.schema[0].sql = Some("CREATE TABLE artist(artist_id integer)".into());
    b.tables.get_mut("content").unwrap().rows.pop();
    let d = diff(&a, &b, DiffOptions::default());
    assert!(d
        .iter()
        .any(|x| matches!(x, Difference::Schema { name, .. } if name == "artist")));
    assert!(d.contains(&Difference::ContentOnlyIn {
        side: Side::Ours,
        path: "/Contents/b.mp3".into()
    }));
}
