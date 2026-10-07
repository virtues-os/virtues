//! `show`: something drawn inside a reply.
//!
//! The kinds are a fixed set and the client draws each with its own component
//! (`apps/web/src/lib/components/chat/show/`), so nothing here is markup and
//! nothing the model writes is ever rendered as HTML.
//!
//! Every kind that shows data takes `sql`, never values. The box runs the
//! query through `sql_query`'s own executor (read-only, as
//! `virtues_face_reader`, under its timeout and byte budget), so a bar is
//! exactly as tall as the record says. The rows go back to the model too, so
//! the sentence beside the chart describes the same numbers the chart draws.
//!
//! A query that cannot be drawn is refused with the fix in the message: the
//! reader is a model in a retry loop.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::executor::{ToolError, ToolResult};
use super::sql_query::SqlQueryTool;

#[derive(Debug, Deserialize)]
struct ShowArgs {
    kind: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    sql: Option<String>,
    #[serde(default)]
    mark: Option<String>,
    #[serde(default)]
    options: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Chart,
    Numbers,
    Table,
    Map,
    Timeline,
    Choices,
}

impl Kind {
    fn parse(s: &str) -> Result<Self, ToolError> {
        Ok(match s {
            "chart" => Kind::Chart,
            "numbers" => Kind::Numbers,
            "table" => Kind::Table,
            "map" => Kind::Map,
            "timeline" => Kind::Timeline,
            "choices" => Kind::Choices,
            other => {
                return Err(invalid(format!(
                    "unknown kind '{other}'. Use one of: chart, numbers, table, map, timeline, choices"
                )))
            }
        })
    }

    fn name(self) -> &'static str {
        match self {
            Kind::Chart => "chart",
            Kind::Numbers => "numbers",
            Kind::Table => "table",
            Kind::Map => "map",
            Kind::Timeline => "timeline",
            Kind::Choices => "choices",
        }
    }

    /// Rows the kind draws. The query is asked for one more, so a result
    /// that would be cut is known to be one.
    fn max_rows(self) -> usize {
        match self {
            Kind::Chart | Kind::Timeline | Kind::Map => 199,
            Kind::Table => 50,
            Kind::Numbers => 1,
            Kind::Choices => 0,
        }
    }
}

/// A table wider than this does not fit the reply's column.
const MAX_TABLE_COLUMNS: usize = 8;
const MAX_NUMBERS: usize = 4;
const MAX_SERIES: usize = 3;
const MAX_TITLE_CHARS: usize = 80;
const MAX_OPTION_CHARS: usize = 80;

fn invalid(msg: impl Into<String>) -> ToolError {
    ToolError::InvalidParameters(msg.into())
}

/// `timezone` is the owner's, as for sql_query: the SQL's dates and labels
/// are read in it.
pub async fn execute(
    sql_query: &SqlQueryTool,
    arguments: Value,
    timezone: Option<&str>,
) -> Result<ToolResult, ToolError> {
    let args: ShowArgs = serde_json::from_value(arguments)
        .map_err(|e| invalid(format!("Invalid arguments: {e}")))?;
    let kind = Kind::parse(args.kind.trim())?;
    let title = args.title.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(|t| {
        t.chars().take(MAX_TITLE_CHARS).collect::<String>()
    });

    if kind == Kind::Choices {
        let options = choices(args.options.unwrap_or_default())?;
        return Ok(ToolResult::success(json!({
            "kind": "choices",
            "title": title,
            "options": options,
            "note": "shown as buttons; the owner's tap arrives as their next message",
        })));
    }

    let sql = args
        .sql
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(format!("a {} needs `sql`: the SELECT whose rows it draws", kind.name())))?;
    let mark = match (kind, args.mark.as_deref()) {
        (Kind::Chart, None | Some("bar")) => Some("bar"),
        (Kind::Chart, Some("line")) => Some("line"),
        (Kind::Chart, Some(other)) => return Err(invalid(format!("mark '{other}' is not one: use bar or line"))),
        _ => None,
    };

    // One more than the kind draws, so a cut result is seen rather than drawn.
    let limit = kind.max_rows() as u32 + 1;
    let result = sql_query
        .execute(json!({ "operation": "query", "sql": sql, "limit": limit }), None, timezone)
        .await?;
    let data = result.data;
    if data.get("truncated").is_some() {
        return Err(invalid(
            "the result was too large to carry, so it cannot be drawn whole. Aggregate it \
             (GROUP BY a day or a week), or select fewer columns",
        ));
    }
    let columns: Vec<String> = data["columns"]
        .as_array()
        .map(|c| c.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let rows: Vec<Value> = data["rows"].as_array().cloned().unwrap_or_default();

    let (columns, rows) = canonical_columns(kind, columns, rows);
    let mut shaped = shape(kind, &columns, rows)?;
    shaped.insert("title".into(), json!(title));
    if let Some(mark) = mark {
        shaped.insert("mark".into(), json!(mark));
    }
    let mut note = String::from(
        "shown to the owner as drawn from these rows; describe what they show, and quote no figure that is not in them",
    );
    // A model that sees `+00:00` concludes the figure is in the wrong zone
    // and draws it again, so the reply carries both. The client puts every
    // time on the owner's own clock; the zone only matters to the prose.
    if shaped["rows"].as_array().is_some_and(|rows| rows.iter().any(has_offset_time)) {
        note.push_str(
            ". Times with an offset are drawn on the owner's own clock, so the figure is right as it is: \
             do not draw it again to convert them. Convert them only in what you write",
        );
    }
    shaped.insert("note".into(), json!(note));
    Ok(ToolResult::success(Value::Object(shaped)))
}

/// The columns a map and a timeline read, and the table names a model writes
/// for them first. A query straight off `data_location_visit` says
/// `latitude` and `started_at`; refusing that only buys a retry.
const ALIASES: &[(Kind, &str, &[&str])] = &[
    (Kind::Map, "lat", &["latitude"]),
    (Kind::Map, "lon", &["longitude", "lng"]),
    (Kind::Map, "label", &["place_name", "name", "title"]),
    (Kind::Timeline, "start", &["started_at", "occurred_at"]),
    (Kind::Timeline, "end", &["ended_at"]),
    (Kind::Timeline, "label", &["title", "name", "place_name"]),
];

/// Rename the first alias present to its column, unless the column is
/// already there.
fn canonical_columns(kind: Kind, mut columns: Vec<String>, mut rows: Vec<Value>) -> (Vec<String>, Vec<Value>) {
    for (k, want, aliases) in ALIASES {
        if *k != kind || columns.iter().any(|c| c == want) {
            continue;
        }
        let Some(at) = columns.iter().position(|c| aliases.contains(&c.as_str())) else { continue };
        let from = std::mem::replace(&mut columns[at], want.to_string());
        for row in rows.iter_mut() {
            if let Some(obj) = row.as_object_mut() {
                if let Some(v) = obj.remove(&from) {
                    obj.insert(want.to_string(), v);
                }
            }
        }
    }
    (columns, rows)
}

fn choices(options: Vec<String>) -> Result<Vec<String>, ToolError> {
    let options: Vec<String> =
        options.into_iter().map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect();
    if !(2..=4).contains(&options.len()) {
        return Err(invalid(format!("choices takes 2 to 4 options, not {}", options.len())));
    }
    if let Some(long) = options.iter().find(|o| o.chars().count() > MAX_OPTION_CHARS) {
        return Err(invalid(format!(
            "an option is a short reply, at most {MAX_OPTION_CHARS} characters; '{long}' is longer"
        )));
    }
    Ok(options)
}

/// Check the rows against what the kind draws and put them in its shape.
/// Pure, so every refusal is tested without a database.
fn shape(kind: Kind, columns: &[String], mut rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    if rows.is_empty() {
        return Err(invalid(
            "the query returned no rows, so there is nothing to draw. Say so in words, \
             naming the window you looked at, rather than showing an empty figure",
        ));
    }
    // Each row is a JSON object, so two columns with one name are one value:
    // `SELECT a.title, b.title` would draw the second twice.
    if let Some(dup) = columns.iter().enumerate().find_map(|(i, c)| columns[..i].contains(c).then_some(c)) {
        return Err(invalid(format!(
            "two columns are both named \"{dup}\"; alias them apart (AS \"...\")"
        )));
    }
    let more = rows.len() > kind.max_rows();
    let mut out = Map::new();
    out.insert("kind".into(), json!(kind.name()));

    match kind {
        Kind::Chart => {
            if more {
                return Err(invalid(format!(
                    "a chart draws at most {} points; aggregate to fewer (GROUP BY a week or a month)",
                    kind.max_rows()
                )));
            }
            if columns.len() < 2 || columns.len() > MAX_SERIES + 1 {
                return Err(invalid(format!(
                    "a chart is the x axis as the first column, then 1 to {MAX_SERIES} numeric series; \
                     this query has {} columns",
                    columns.len()
                )));
            }
            if rows.len() < 2 {
                return Err(invalid("a chart needs at least two rows; one value belongs in a sentence, or kind numbers"));
            }
            for series in &columns[1..] {
                numeric_column(&mut rows, series, "a chart series")?;
                // A series with no values draws nothing but its legend entry.
                if rows.iter().all(|r| r.get(series).is_none_or(Value::is_null)) {
                    return Err(invalid(format!(
                        "column \"{series}\" is null in every row, so it has nothing to draw; leave it out"
                    )));
                }
            }
        }
        Kind::Numbers => {
            if rows.len() != 1 {
                return Err(invalid(
                    "numbers shows one row: aggregate to a single row with one column per figure, \
                     each named by its alias",
                ));
            }
            if columns.len() > MAX_NUMBERS {
                return Err(invalid(format!(
                    "numbers shows at most {MAX_NUMBERS} figures; this query has {} columns",
                    columns.len()
                )));
            }
            for c in columns {
                // A figure may be text ("Tuesday"), but a numeric string is
                // made a number so the client formats it.
                for row in rows.iter_mut() {
                    if let Some(v) = row.get_mut(c) {
                        if let Some(n) = v.as_str().and_then(|s| s.parse::<f64>().ok()) {
                            *v = json!(n);
                        }
                    }
                }
            }
        }
        Kind::Table => {
            if columns.len() > MAX_TABLE_COLUMNS {
                return Err(invalid(format!(
                    "a table fits {MAX_TABLE_COLUMNS} columns in a reply; this query has {}. Select the ones that answer the question",
                    columns.len()
                )));
            }
            rows.truncate(kind.max_rows());
            out.insert("more".into(), json!(more));
        }
        Kind::Map => {
            if more {
                return Err(invalid(format!(
                    "a map draws at most {} places; group nearby points or narrow the window",
                    kind.max_rows()
                )));
            }
            for c in ["lat", "lon"] {
                if !columns.iter().any(|have| have == c) {
                    return Err(invalid(format!(
                        "a map needs lat and lon columns (latitude and longitude work too), \
                         and optionally label; this query returns {}",
                        columns.join(", ")
                    )));
                }
                numeric_column(&mut rows, c, "a coordinate")?;
            }
            let in_range = |row: &Value| {
                let lat = row["lat"].as_f64();
                let lon = row["lon"].as_f64();
                matches!((lat, lon), (Some(a), Some(o)) if (-90.0..=90.0).contains(&a) && (-180.0..=180.0).contains(&o))
            };
            rows.retain(in_range);
            if rows.is_empty() {
                return Err(invalid("no row has a coordinate in range (lat -90..90, lon -180..180)"));
            }
        }
        Kind::Timeline => {
            if more {
                return Err(invalid(format!(
                    "a timeline draws at most {} items; narrow the window or group them",
                    kind.max_rows()
                )));
            }
            for c in ["start", "label"] {
                if !columns.iter().any(|have| have == c) {
                    return Err(invalid(format!(
                        "a timeline needs start and label columns (started_at and title work too), \
                         and optionally end; this query returns {}",
                        columns.join(", ")
                    )));
                }
            }
            for row in &rows {
                match row["start"].as_str() {
                    None => {
                        return Err(invalid("every timeline row needs a start time; filter out rows where it is null"))
                    }
                    Some(s) if !is_time(s) => {
                        return Err(invalid(format!(
                            "start must be a timestamp or a date, and one row's is '{s}'. Select the column \
                             itself rather than formatting it (no to_char)"
                        )))
                    }
                    Some(_) => {}
                }
            }
        }
        Kind::Choices => unreachable!("choices take no sql"),
    }

    out.insert("columns".into(), json!(columns));
    out.insert("row_count".into(), json!(rows.len()));
    out.insert("rows".into(), Value::Array(rows));
    Ok(out)
}

/// Whether `s` is a time as `convert_rows_to_json` writes one: RFC 3339
/// (`timestamptz`), `YYYY-MM-DD HH:MM:SS[.f]` (`timestamp`), or a date.
fn is_time(s: &str) -> bool {
    use chrono::{DateTime, NaiveDate, NaiveDateTime};
    DateTime::parse_from_rfc3339(s).is_ok()
        || NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f").is_ok()
        || NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

/// Whether a row holds an RFC 3339 time with its zone, as `timestamptz` arrives.
fn has_offset_time(row: &Value) -> bool {
    row.as_object().is_some_and(|o| {
        o.values().any(|v| {
            v.as_str().is_some_and(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok())
        })
    })
}

/// Make `column` numbers in every row, or say which value is not one. Postgres
/// `numeric` (what `avg` and `sum` return) arrives as a string to keep its
/// precision; a chart needs it as a number. Null stays null: a gap.
fn numeric_column(rows: &mut [Value], column: &str, what: &str) -> Result<(), ToolError> {
    for row in rows.iter_mut() {
        let Some(v) = row.get_mut(column) else { continue };
        match v {
            Value::Number(_) | Value::Null => {}
            Value::String(s) => match s.trim().parse::<f64>() {
                Ok(n) if n.is_finite() => *v = json!(n),
                _ => {
                    return Err(invalid(format!(
                        "{what} must be a number, and column \"{column}\" holds '{s}'. \
                         Cast it (::float8), or move it to the first column"
                    )))
                }
            },
            other => {
                return Err(invalid(format!(
                    "{what} must be a number, and column \"{column}\" holds {other}"
                )))
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn err(r: Result<Map<String, Value>, ToolError>) -> String {
        match r {
            Err(ToolError::InvalidParameters(m)) => m,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_chart_turns_numeric_strings_into_numbers_and_keeps_gaps() {
        let rows = vec![
            json!({ "Week": "2026-09-01", "Hours asleep": "7.25" }),
            json!({ "Week": "2026-09-08", "Hours asleep": null }),
            json!({ "Week": "2026-09-15", "Hours asleep": 6 }),
        ];
        let out = shape(Kind::Chart, &cols(&["Week", "Hours asleep"]), rows).unwrap();
        assert_eq!(out["rows"][0]["Hours asleep"], json!(7.25));
        assert_eq!(out["rows"][1]["Hours asleep"], Value::Null);
        assert_eq!(out["columns"], json!(["Week", "Hours asleep"]));
    }

    #[test]
    fn a_chart_refuses_text_in_a_series_and_names_the_fix() {
        let rows = vec![json!({ "x": "a", "y": "lots" }), json!({ "x": "b", "y": "1" })];
        let m = err(shape(Kind::Chart, &cols(&["x", "y"]), rows));
        assert!(m.contains("\"y\"") && m.contains("::float8"), "{m}");
    }

    #[test]
    fn a_chart_refuses_a_series_with_no_values() {
        let rows = vec![json!({ "x": "a", "y": 1, "z": null }), json!({ "x": "b", "y": 2, "z": null })];
        let m = err(shape(Kind::Chart, &cols(&["x", "y", "z"]), rows));
        assert!(m.contains("\"z\" is null in every row"), "{m}");
    }

    #[test]
    fn a_chart_refuses_more_points_than_it_draws() {
        let rows = (0..200).map(|i| json!({ "x": i, "y": i })).collect();
        let m = err(shape(Kind::Chart, &cols(&["x", "y"]), rows));
        assert!(m.contains("GROUP BY"), "{m}");
    }

    #[test]
    fn a_chart_needs_an_axis_and_a_series() {
        let rows = vec![json!({ "x": 1 }), json!({ "x": 2 })];
        assert!(err(shape(Kind::Chart, &cols(&["x"]), rows)).contains("1 to 3"));
    }

    #[test]
    fn no_rows_is_said_in_words_not_drawn() {
        assert!(err(shape(Kind::Table, &cols(&["a"]), vec![])).contains("in words"));
    }

    #[test]
    fn numbers_is_one_row() {
        let rows = vec![json!({ "a": 1 }), json!({ "a": 2 })];
        assert!(err(shape(Kind::Numbers, &cols(&["a"]), rows)).contains("one row"));
        let out = shape(Kind::Numbers, &cols(&["Nights", "Best day"]), vec![json!({ "Nights": "41", "Best day": "Tuesday" })])
            .unwrap();
        assert_eq!(out["rows"][0]["Nights"], json!(41.0));
        assert_eq!(out["rows"][0]["Best day"], json!("Tuesday"));
    }

    #[test]
    fn a_long_table_is_cut_and_says_so() {
        let rows = (0..51).map(|i| json!({ "n": i })).collect();
        let out = shape(Kind::Table, &cols(&["n"]), rows).unwrap();
        assert_eq!(out["row_count"], json!(50));
        assert_eq!(out["more"], json!(true));
    }

    #[test]
    fn a_map_needs_lat_and_lon_and_drops_out_of_range_points() {
        let m = err(shape(Kind::Map, &cols(&["y", "x"]), vec![json!({ "y": 1, "x": 2 })]));
        assert!(m.contains("returns y, x"), "{m}");
        let rows = vec![json!({ "lat": "30.27", "lon": -97.74 }), json!({ "lat": 200, "lon": 0 })];
        let out = shape(Kind::Map, &cols(&["lat", "lon"]), rows).unwrap();
        assert_eq!(out["row_count"], json!(1));
        assert_eq!(out["rows"][0]["lat"], json!(30.27));
    }

    #[test]
    fn a_map_and_a_timeline_read_the_table_column_names_too() {
        let (c, r) = canonical_columns(
            Kind::Map,
            cols(&["latitude", "longitude", "place_name"]),
            vec![json!({ "latitude": 1.0, "longitude": 2.0, "place_name": "Office" })],
        );
        assert_eq!(c, cols(&["lat", "lon", "label"]));
        assert_eq!(r[0], json!({ "lat": 1.0, "lon": 2.0, "label": "Office" }));

        // A column already named wins; the alias is left alone.
        let (c, _) = canonical_columns(Kind::Timeline, cols(&["start", "started_at", "title"]), vec![]);
        assert_eq!(c, cols(&["start", "started_at", "label"]));
        // Another kind's names are not touched.
        let (c, _) = canonical_columns(Kind::Table, cols(&["latitude"]), vec![]);
        assert_eq!(c, cols(&["latitude"]));
    }

    #[test]
    fn an_offset_time_is_recognised() {
        assert!(has_offset_time(&json!({ "start": "2026-10-06T14:45:00+00:00" })));
        assert!(!has_offset_time(&json!({ "start": "2026-10-06 09:45:00", "n": 3 })));
    }

    #[test]
    fn a_timeline_needs_a_start_on_every_row() {
        let rows = vec![json!({ "start": null, "label": "x" })];
        assert!(err(shape(Kind::Timeline, &cols(&["start", "label"]), rows)).contains("null"));
    }

    #[test]
    fn a_timeline_start_must_be_a_time_not_a_label() {
        let rows = vec![json!({ "start": "9:45 AM", "label": "Standup" })];
        assert!(err(shape(Kind::Timeline, &cols(&["start", "label"]), rows)).contains("to_char"));
        for ok in ["2026-10-06T14:45:00+00:00", "2026-10-06 09:45:00", "2026-10-06 09:45:00.5", "2026-10-06"] {
            assert!(is_time(ok), "{ok}");
        }
    }

    #[test]
    fn two_columns_with_one_name_are_refused() {
        let rows = vec![json!({ "title": "a" })];
        assert!(err(shape(Kind::Table, &cols(&["title", "title"]), rows)).contains("both named"));
    }

    #[test]
    fn choices_are_two_to_four_short_replies() {
        assert!(choices(vec!["Yes".into()]).is_err());
        assert!(choices(vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()]).is_err());
        assert!(choices(vec!["Yes".into(), "x".repeat(81)]).is_err());
        assert_eq!(choices(vec![" Yes ".into(), "".into(), "No".into()]).unwrap(), vec!["Yes", "No"]);
    }

    #[test]
    fn an_unknown_kind_lists_the_real_ones() {
        let Err(ToolError::InvalidParameters(m)) = Kind::parse("pie") else { panic!() };
        assert!(m.contains("chart, numbers, table, map, timeline, choices"));
    }
}
