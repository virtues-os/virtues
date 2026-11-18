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
//!
//! A call says what the figure is for (`purpose`) and the server picks the
//! mark from the columns the query returns, trying the purpose's marks in the
//! order `virtues_registry::chart_marks::MARKS` lists them. A call that names
//! `kind` (and `mark`) is drawn as it says, as before `purpose` existed.

use serde::Deserialize;
use serde_json::{json, Map, Value};
use virtues_registry::chart_marks::{self, ChartMark};

use super::executor::{ToolError, ToolResult};
use super::sql_query::SqlQueryTool;

#[derive(Debug, Deserialize)]
struct ShowArgs {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    purpose: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    sql: Option<String>,
    #[serde(default)]
    mark: Option<String>,
    #[serde(default)]
    options: Option<Vec<String>>,
    /// interrupted: the date of the change, and the change in a few words.
    #[serde(default)]
    split_at: Option<String>,
    #[serde(default)]
    split_label: Option<String>,
    /// cumulative: what a step is (day, weekday or hour), and the unit.
    #[serde(default)]
    step_kind: Option<String>,
    #[serde(default)]
    unit: Option<String>,
}

/// The call fields a chart mark reads beyond its rows, trimmed.
#[derive(Debug, Default, Clone)]
struct ChartFields {
    split_at: Option<String>,
    split_label: Option<String>,
    step_kind: Option<String>,
    unit: Option<String>,
}

const MAX_STATES: usize = 6;
const MAX_UNIT_CHARS: usize = 12;
const MAX_SPLIT_LABEL_CHARS: usize = 60;

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
/// `schema` is the applet whose run or turn this is, whose own tables the
/// SQL names without it.
pub async fn execute(
    sql_query: &SqlQueryTool,
    arguments: Value,
    timezone: Option<&str>,
    schema: Option<&str>,
) -> Result<ToolResult, ToolError> {
    let args: ShowArgs = serde_json::from_value(arguments)
        .map_err(|e| invalid(format!("Invalid arguments: {e}")))?;
    let trimmed = |s: &Option<String>| s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let (purpose, named_mark) = purpose_and_mark(trimmed(&args.purpose), trimmed(&args.mark))?;
    let fields = ChartFields {
        split_at: trimmed(&args.split_at),
        split_label: trimmed(&args.split_label),
        step_kind: trimmed(&args.step_kind),
        unit: trimmed(&args.unit),
    };
    let kind = match trimmed(&args.kind) {
        Some(k) => Kind::parse(&k)?,
        None => kind_before_query(purpose, named_mark.as_deref())?,
    };
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
    // A chart's mark is settled before the query when the call names it, and
    // picked from the columns after it when only the purpose is given.
    let named_chart_mark = match (kind, named_mark.as_deref()) {
        (Kind::Chart, Some(m)) => match chart_marks::mark(m) {
            Some(cm) if cm.kind == "chart" => Some(cm.mark),
            _ => {
                let known: Vec<&str> = chart_marks::MARKS.iter().filter(|m| m.kind == "chart").map(|m| m.mark).collect();
                return Err(invalid(format!("mark '{m}' is not a chart mark: use one of {}", known.join(", "))));
            }
        },
        _ => None,
    };

    // One more than the most any mark this call may draw with takes, so a cut
    // result is seen rather than drawn. sql_query's own `execute` clamps to
    // 200 rows; `query_for_show` takes the mark's limit.
    let candidates = marks_for_call(kind, purpose, named_chart_mark);
    let limit = candidates.iter().map(|m| m.max_rows).max().unwrap_or(0) as u32 + 1;
    let result = sql_query.query_for_show(sql, limit, timezone, schema).await?;
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
    let drawn = match (kind, named_chart_mark) {
        (Kind::Chart, Some(m)) => chart_marks::mark(m).expect("a named chart mark is in the table"),
        (Kind::Chart, None) => pick_chart_mark(purpose, &fields, &columns, &rows),
        _ => candidates[0],
    };
    let (columns, rows) = mark_columns(drawn.mark, columns, rows);
    // Three chart marks have rows `shape`'s bar and line checks don't fit.
    let mut shaped = match drawn.mark {
        "states" => shape_states(drawn.max_rows, &columns, rows)?,
        "multiples" => shape_multiples(drawn.max_rows, &columns, rows)?,
        "stack" => shape_stack(drawn.max_rows, &columns, rows)?,
        _ => shape(kind, drawn.max_rows, &columns, rows)?,
    };
    if kind == Kind::Chart {
        check_mark(drawn.mark, &fields, &columns, &mut shaped)?;
        shaped.insert("mark".into(), json!(drawn.mark));
    }
    shaped.insert("title".into(), json!(title));
    if let Some(p) = purpose {
        shaped.insert("purpose".into(), json!(p.id));
    }
    let mut note = String::from(
        "shown to the owner as drawn from these rows; describe what they show, and quote no figure that is not in them",
    );
    // A model that sees an offset may conclude the figure is in the wrong
    // zone and draw it again. The rows already carry the owner's offset and
    // the client draws every time on their clock, so neither needs it.
    if shaped["rows"].as_array().is_some_and(|rows| rows.iter().any(has_offset_time)) {
        note.push_str(
            ". Times are already on the owner's own clock, and so is the figure: \
             do not draw it again or convert them. Quote the clock times as given",
        );
    }
    shaped.insert("note".into(), json!(note));
    Ok(ToolResult::success(Value::Object(shaped)))
}

/// The call's purpose and mark. A mark's name given as the purpose
/// (`purpose: "cumulative"`) is read as that mark with its first purpose:
/// models do write it, and refusing it only buys a retry.
fn purpose_and_mark(
    purpose: Option<String>,
    mark: Option<String>,
) -> Result<(Option<&'static chart_marks::ChartPurpose>, Option<String>), ToolError> {
    let Some(p) = purpose else { return Ok((None, mark)) };
    if chart_marks::purpose(&p).is_none() {
        if let Some(m) = chart_marks::mark(&p) {
            let first = m.purposes.first().and_then(|id| chart_marks::purpose(id));
            return Ok((first, mark.or(Some(p))));
        }
    }
    Ok((Some(purpose_named(&p)?), mark))
}

fn purpose_named(id: &str) -> Result<&'static chart_marks::ChartPurpose, ToolError> {
    chart_marks::purpose(id).ok_or_else(|| {
        let known: Vec<&str> = chart_marks::PURPOSES.iter().map(|p| p.id).collect();
        invalid(format!("unknown purpose '{id}'. Use one of: {}", known.join(", ")))
    })
}

/// The kind to draw when the call gives no `kind`: the named mark's, else the
/// purpose's. Every purpose's marks share one kind
/// (`chart_marks::tests::a_purpose_draws_with_one_kind`), so the query's
/// limit and column names are known before it runs.
fn kind_before_query(purpose: Option<&chart_marks::ChartPurpose>, named_mark: Option<&str>) -> Result<Kind, ToolError> {
    let m = match (named_mark, purpose) {
        (Some(name), _) => chart_marks::mark(name).ok_or_else(|| {
            let known: Vec<&str> = chart_marks::MARKS.iter().map(|m| m.mark).collect();
            invalid(format!("unknown mark '{name}'. Use one of: {}, or leave mark out", known.join(", ")))
        })?,
        (None, Some(p)) => chart_marks::marks_for(p.id).next().expect("every purpose has a mark (chart_marks tests)"),
        (None, None) => {
            let known: Vec<&str> = chart_marks::PURPOSES.iter().map(|p| p.id).collect();
            return Err(invalid(format!(
                "say what the figure is for with purpose ({}), or give kind",
                known.join(", ")
            )));
        }
    };
    Kind::parse(m.kind)
}

/// The marks a call may end up drawing with: the named chart mark, else the
/// purpose's marks of this kind, else bar for a chart (what the picker draws
/// with no purpose) and the kind's mark for the rest. Never empty for a kind
/// that takes sql (`every_kind_with_sql_has_a_mark`).
fn marks_for_call(
    kind: Kind,
    purpose: Option<&chart_marks::ChartPurpose>,
    named_chart_mark: Option<&str>,
) -> Vec<&'static ChartMark> {
    if let Some(m) = named_chart_mark.and_then(chart_marks::mark) {
        return vec![m];
    }
    let of_kind = |m: &&'static ChartMark| m.kind == kind.name();
    if let Some(p) = purpose {
        let marks: Vec<_> = chart_marks::marks_for(p.id).filter(of_kind).collect();
        if !marks.is_empty() {
            return marks;
        }
    }
    if kind == Kind::Chart {
        return vec![chart_marks::mark("bar").expect("bar is a chart mark")];
    }
    chart_marks::MARKS.iter().filter(of_kind).collect()
}

/// The chart mark for a call that gave no mark: the first of the purpose's
/// chart marks whose columns fit, and bar when nothing else does or the call
/// gave no purpose. Pure, so each pick is tested without a database.
fn pick_chart_mark(
    purpose: Option<&chart_marks::ChartPurpose>,
    fields: &ChartFields,
    columns: &[String],
    rows: &[Value],
) -> &'static ChartMark {
    let bar = chart_marks::mark("bar").expect("bar is a chart mark");
    let Some(p) = purpose else { return bar };
    let marks: Vec<_> = chart_marks::marks_for(p.id).filter(|m| m.kind == "chart").collect();
    // The purpose's only mark is drawn even when its columns are off, so its
    // own refusal names the fix; bar would draw something the call didn't ask for.
    if let [only] = marks.as_slice() {
        return only;
    }
    marks.into_iter().find(|m| chart_fits(m.mark, fields, columns, rows)).unwrap_or(bar)
}

fn chart_fits(mark: &str, fields: &ChartFields, columns: &[String], rows: &[Value]) -> bool {
    match mark {
        // Only a literal step column or step_kind: an ordinary `day, steps`
        // query is a bar or a line, though `day` is a step's alias once named.
        "cumulative" => fields.step_kind.is_some() || columns.iter().any(|c| c == "step"),
        "interrupted" => fields.split_at.is_some(),
        "baseline" => has_column(mark, "usual_low", columns) && has_column(mark, "usual_high", columns),
        "states" => ["day", "start", "end", "state"].iter().all(|c| has_column(mark, c, columns)),
        "multiples" => has_column(mark, "panel", columns),
        // A text category and exactly two numbers: then and now.
        "dumbbell" => columns.len() == 3 && rows.len() <= 20 && is_label_column(rows, &columns[0]),
        // A text label and one value, few enough to list; a dated first
        // column is a trend and stays a bar.
        "list" => columns.len() == 2 && rows.len() <= 20 && is_label_column(rows, &columns[0]),
        "waterfall" => columns.len() == 2 && values_of(rows, &columns[1]).any(|v| v < 0.0),
        "share" => {
            columns.len() == 2 && (2..=8).contains(&rows.len()) && values_of(rows, &columns[1]).all(|v| v >= 0.0)
        }
        "stack" => columns.len() >= 3,
        // A line runs over real time: every value in the first column is a
        // time or a date, and at least one is there.
        "line" => columns.first().is_some_and(|x| {
            let mut values = rows.iter().filter_map(|r| r.get(x)).filter(|v| !v.is_null()).peekable();
            values.peek().is_some() && values.all(|v| v.as_str().is_some_and(is_time))
        }),
        "bar" => true,
        _ => false,
    }
}

/// The columns a chart mark reads by name, and the names a model writes for
/// them first. Applied only once the mark is chosen, so a bar chart's
/// `date` column keeps the label the owner reads.
const MARK_ALIASES: &[(&str, &str, &[&str])] = &[
    ("cumulative", "step", &["day"]),
    ("baseline", "usual_low", &["low"]),
    ("baseline", "usual_high", &["high"]),
    ("states", "day", &["date"]),
    ("states", "start", &["started_at"]),
    ("states", "end", &["ended_at"]),
    ("states", "state", &["what", "place", "label"]),
    ("multiples", "panel", &["group"]),
];

fn has_column(mark: &str, want: &str, columns: &[String]) -> bool {
    columns.iter().any(|c| c == want)
        || MARK_ALIASES
            .iter()
            .filter(|(m, w, _)| *m == mark && *w == want)
            .any(|(_, _, aliases)| columns.iter().any(|c| aliases.contains(&c.as_str())))
}

/// Rename the first alias present to the mark's column, unless the column is
/// already there.
fn mark_columns(mark: &str, mut columns: Vec<String>, mut rows: Vec<Value>) -> (Vec<String>, Vec<Value>) {
    for (m, want, aliases) in MARK_ALIASES {
        if *m != mark || columns.iter().any(|c| c == want) {
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

/// The checks a chart mark makes beyond the bar and line ones `shape` already
/// ran, and the call fields it carries to the client. Pure.
fn check_mark(mark: &str, fields: &ChartFields, columns: &[String], out: &mut Map<String, Value>) -> Result<(), ToolError> {
    match mark {
        "cumulative" => {
            let step_kind = fields.step_kind.as_deref().unwrap_or("day");
            let range = match step_kind {
                "day" => 1..=31,
                "weekday" => 1..=7,
                "hour" => 0..=23,
                other => {
                    return Err(invalid(format!("step_kind '{other}' is not one: use day, weekday or hour")))
                }
            };
            if columns.len() != 3 {
                return Err(invalid(format!(
                    "a cumulative comparison is step, then this period's running total and the last one's; \
                     this query has {} columns",
                    columns.len()
                )));
            }
            if let Some(u) = &fields.unit {
                if u.chars().count() > MAX_UNIT_CHARS {
                    return Err(invalid(format!("unit is a word of at most {MAX_UNIT_CHARS} characters, such as USD or steps")));
                }
            }
            let rows = out["rows"].as_array().cloned().unwrap_or_default();
            let mut seen = Vec::new();
            for row in &rows {
                let step = row.get(&columns[0]).and_then(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok()));
                let Some(step) = step.filter(|s| s.fract() == 0.0 && range.contains(&(*s as i64))) else {
                    return Err(invalid(format!(
                        "each step must be a whole number from {} to {} for step_kind {step_kind}; one row's is {}",
                        range.start(),
                        range.end(),
                        row.get(&columns[0]).unwrap_or(&Value::Null)
                    )));
                };
                if seen.contains(&(step as i64)) {
                    return Err(invalid(format!("step {step} appears twice; give one row per step")));
                }
                seen.push(step as i64);
                for total in &columns[1..] {
                    if row.get(total).and_then(Value::as_f64).is_some_and(|v| v < 0.0) {
                        return Err(invalid(format!("\"{total}\" is a running total and can't go below zero")));
                    }
                }
            }
            out.insert("step_kind".into(), json!(step_kind));
            if let Some(u) = &fields.unit {
                out.insert("unit".into(), json!(u));
            }
        }
        "interrupted" => {
            let (Some(at), Some(label)) = (&fields.split_at, &fields.split_label) else {
                return Err(invalid(
                    "an interrupted time series needs split_at (the date of the change) and split_label \
                     (the change in a few words)",
                ));
            };
            if label.chars().count() > MAX_SPLIT_LABEL_CHARS {
                return Err(invalid(format!("split_label is a few words, at most {MAX_SPLIT_LABEL_CHARS} characters")));
            }
            if columns.len() != 2 {
                return Err(invalid(format!(
                    "an interrupted time series is the x axis, then one value; this query has {} columns",
                    columns.len()
                )));
            }
            out.insert("split_at".into(), json!(at));
            out.insert("split_label".into(), json!(label));
        }
        "dumbbell" if columns.len() != 3 => {
            return Err(invalid(format!(
                "a dumbbell plot is the category, then exactly two numbers for the two moments; this query has {} columns",
                columns.len()
            )))
        }
        "list" | "share" | "waterfall" if columns.len() != 2 => {
            let name = chart_marks::mark(mark).map_or(mark, |m| m.name);
            let second = if mark == "waterfall" { "an amount" } else { "a value" };
            return Err(invalid(format!(
                "a {} is a label, then {second}; this query has {} columns",
                name.to_lowercase(),
                columns.len()
            )));
        }
        "share" => {
            let values: Vec<f64> = out["rows"].as_array().map(|r| values_of(r, &columns[1]).collect()).unwrap_or_default();
            if values.iter().any(|v| *v < 0.0) || values.iter().sum::<f64>() <= 0.0 {
                return Err(invalid(
                    "a 100% bar draws parts of one whole, so every value is zero or more and they add to more than zero",
                ));
            }
        }
        "baseline" => {
            if !(columns.iter().any(|c| c == "usual_low") && columns.iter().any(|c| c == "usual_high")) || columns.len() != 4 {
                return Err(invalid(format!(
                    "a baseline band is the x axis, the value, then usual_low and usual_high; this query returns {}",
                    columns.join(", ")
                )));
            }
        }
        _ => {}
    }
    Ok(())
}

/// A state timeline's rows: day, start, end and state for each stretch.
/// `shape`'s bar and line checks don't fit it, since three of its four
/// columns are times and names. Pure.
fn shape_states(max_rows: usize, columns: &[String], rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if rows.len() > max_rows {
        return Err(invalid(format!(
            "a state timeline draws at most {max_rows} stretches; narrow the window or merge short ones"
        )));
    }
    if let Some(missing) = ["day", "start", "end", "state"].iter().find(|c| !columns.iter().any(|have| have == *c)) {
        return Err(invalid(format!(
            "a state timeline needs day, start, end and state columns (date, started_at, ended_at and \
             what work too); {missing} is missing from {}",
            columns.join(", ")
        )));
    }
    let mut states: Vec<&str> = Vec::new();
    for row in &rows {
        for c in ["day", "start", "end"] {
            match row[c].as_str() {
                Some(s) if is_time(s) || is_clock(s) => {}
                other => {
                    return Err(invalid(format!(
                        "{c} must be a timestamp, a date or a time of day, and one row's is {}",
                        other.map_or_else(|| row[c].to_string(), |s| format!("'{s}'"))
                    )))
                }
            }
        }
        let state = row["state"].as_str().unwrap_or("");
        if !states.contains(&state) {
            states.push(state);
        }
    }
    if states.len() > MAX_STATES {
        return Err(invalid(format!(
            "a state timeline tells at most {MAX_STATES} states apart, and this query has {}; \
             group the rest into one (Elsewhere)",
            states.len()
        )));
    }
    Ok(chart_out(columns, rows))
}

/// Small multiples' rows: panel, then the x axis and the value. The x axis
/// is shared by every panel, so it may be a date. Pure.
fn shape_multiples(max_rows: usize, columns: &[String], mut rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if rows.len() > max_rows {
        return Err(invalid(format!("small multiples draw at most {max_rows} points; aggregate to fewer")));
    }
    if columns.len() != 3 {
        return Err(invalid(format!(
            "small multiples are panel (the group's name), the x axis and the value; this query returns {}",
            columns.join(", ")
        )));
    }
    numeric_column(&mut rows, &columns[2], "the value")?;
    let panel = columns.iter().find(|c| *c == "panel").unwrap_or(&columns[0]);
    let mut panels: Vec<String> = Vec::new();
    for row in &rows {
        let p = row.get(panel).map(Value::to_string).unwrap_or_default();
        if !panels.contains(&p) {
            panels.push(p);
        }
    }
    if !(2..=12).contains(&panels.len()) {
        return Err(invalid(format!(
            "small multiples draw 2 to 12 panels, and this query has {}; group the panels differently",
            panels.len()
        )));
    }
    Ok(chart_out(columns, rows))
}

/// Stacked bars' rows: the x axis, then 2 to 5 parts. Pure.
fn shape_stack(max_rows: usize, columns: &[String], mut rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if rows.len() > max_rows {
        return Err(invalid(format!("stacked bars draw at most {max_rows} bars; aggregate to fewer")));
    }
    if !(3..=6).contains(&columns.len()) {
        return Err(invalid(format!(
            "stacked bars are the x axis, then 2 to 5 part columns; this query has {} columns",
            columns.len()
        )));
    }
    for part in &columns[1..] {
        numeric_column(&mut rows, part, "a part")?;
        if values_of(&rows, part).any(|v| v < 0.0) {
            return Err(invalid(format!("\"{part}\" goes below zero, and a stacked part can't; draw it as a bar or a line")));
        }
    }
    Ok(chart_out(columns, rows))
}

fn chart_out(columns: &[String], rows: Vec<Value>) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert("kind".into(), json!("chart"));
    out.insert("columns".into(), json!(columns));
    out.insert("row_count".into(), json!(rows.len()));
    out.insert("rows".into(), Value::Array(rows));
    out
}

/// The numbers in a column, reading numeric strings as `numeric_column` does.
fn values_of<'a>(rows: &'a [Value], column: &'a str) -> impl Iterator<Item = f64> + 'a {
    rows.iter().filter_map(move |r| {
        let v = r.get(column)?;
        v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())
    })
}

/// Whether a column holds labels: every value is text that is not a time.
fn is_label_column(rows: &[Value], column: &str) -> bool {
    let mut values = rows.iter().filter_map(|r| r.get(column)).filter(|v| !v.is_null()).peekable();
    values.peek().is_some() && values.all(|v| v.as_str().is_some_and(|s| !is_time(s) && s.trim().parse::<f64>().is_err()))
}

/// What every drawn figure needs: some rows, and no two columns with one name.
fn rows_and_names(columns: &[String], rows: &[Value]) -> Result<(), ToolError> {
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
    Ok(())
}

/// `HH:MM` or `HH:MM:SS`, as a `time` column arrives.
fn is_clock(s: &str) -> bool {
    chrono::NaiveTime::parse_from_str(s, "%H:%M:%S%.f").is_ok() || chrono::NaiveTime::parse_from_str(s, "%H:%M").is_ok()
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
/// `max_rows` is the drawn mark's (`ChartMark::max_rows`). Pure, so every
/// refusal is tested without a database.
fn shape(kind: Kind, max_rows: usize, columns: &[String], mut rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    let more = rows.len() > max_rows;
    let mut out = Map::new();
    out.insert("kind".into(), json!(kind.name()));

    match kind {
        Kind::Chart => {
            if more {
                return Err(invalid(format!(
                    "a chart draws at most {} points; aggregate to fewer (GROUP BY a week or a month)",
                    max_rows
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
            rows.truncate(max_rows);
            out.insert("more".into(), json!(more));
        }
        Kind::Map => {
            if more {
                return Err(invalid(format!(
                    "a map draws at most {} places; group nearby points or narrow the window",
                    max_rows
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
                    max_rows
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

    /// The row cap of the kind's marks, as a kind-only call gets it.
    fn cap(kind: Kind) -> usize {
        marks_for_call(kind, None, None).iter().map(|m| m.max_rows).max().unwrap()
    }

    #[test]
    fn every_kind_with_sql_has_a_mark() {
        for kind in [Kind::Chart, Kind::Numbers, Kind::Table, Kind::Map, Kind::Timeline] {
            assert!(!marks_for_call(kind, None, None).is_empty(), "{} has no mark", kind.name());
        }
    }

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
        let out = shape(Kind::Chart, cap(Kind::Chart), &cols(&["Week", "Hours asleep"]), rows).unwrap();
        assert_eq!(out["rows"][0]["Hours asleep"], json!(7.25));
        assert_eq!(out["rows"][1]["Hours asleep"], Value::Null);
        assert_eq!(out["columns"], json!(["Week", "Hours asleep"]));
    }

    #[test]
    fn a_chart_refuses_text_in_a_series_and_names_the_fix() {
        let rows = vec![json!({ "x": "a", "y": "lots" }), json!({ "x": "b", "y": "1" })];
        let m = err(shape(Kind::Chart, cap(Kind::Chart), &cols(&["x", "y"]), rows));
        assert!(m.contains("\"y\"") && m.contains("::float8"), "{m}");
    }

    #[test]
    fn a_chart_refuses_a_series_with_no_values() {
        let rows = vec![json!({ "x": "a", "y": 1, "z": null }), json!({ "x": "b", "y": 2, "z": null })];
        let m = err(shape(Kind::Chart, cap(Kind::Chart), &cols(&["x", "y", "z"]), rows));
        assert!(m.contains("\"z\" is null in every row"), "{m}");
    }

    #[test]
    fn a_chart_refuses_more_points_than_it_draws() {
        let rows = (0..200).map(|i| json!({ "x": i, "y": i })).collect();
        let m = err(shape(Kind::Chart, cap(Kind::Chart), &cols(&["x", "y"]), rows));
        assert!(m.contains("GROUP BY"), "{m}");
    }

    #[test]
    fn a_chart_needs_an_axis_and_a_series() {
        let rows = vec![json!({ "x": 1 }), json!({ "x": 2 })];
        assert!(err(shape(Kind::Chart, cap(Kind::Chart), &cols(&["x"]), rows)).contains("1 to 3"));
    }

    #[test]
    fn no_rows_is_said_in_words_not_drawn() {
        assert!(err(shape(Kind::Table, cap(Kind::Table), &cols(&["a"]), vec![])).contains("in words"));
    }

    #[test]
    fn numbers_is_one_row() {
        let rows = vec![json!({ "a": 1 }), json!({ "a": 2 })];
        assert!(err(shape(Kind::Numbers, cap(Kind::Numbers), &cols(&["a"]), rows)).contains("one row"));
        let out = shape(Kind::Numbers, cap(Kind::Numbers), &cols(&["Nights", "Best day"]), vec![json!({ "Nights": "41", "Best day": "Tuesday" })])
            .unwrap();
        assert_eq!(out["rows"][0]["Nights"], json!(41.0));
        assert_eq!(out["rows"][0]["Best day"], json!("Tuesday"));
    }

    #[test]
    fn a_long_table_is_cut_and_says_so() {
        let rows = (0..51).map(|i| json!({ "n": i })).collect();
        let out = shape(Kind::Table, cap(Kind::Table), &cols(&["n"]), rows).unwrap();
        assert_eq!(out["row_count"], json!(50));
        assert_eq!(out["more"], json!(true));
    }

    #[test]
    fn a_map_needs_lat_and_lon_and_drops_out_of_range_points() {
        let m = err(shape(Kind::Map, cap(Kind::Map), &cols(&["y", "x"]), vec![json!({ "y": 1, "x": 2 })]));
        assert!(m.contains("returns y, x"), "{m}");
        let rows = vec![json!({ "lat": "51.5", "lon": -0.12 }), json!({ "lat": 200, "lon": 0 })];
        let out = shape(Kind::Map, cap(Kind::Map), &cols(&["lat", "lon"]), rows).unwrap();
        assert_eq!(out["row_count"], json!(1));
        assert_eq!(out["rows"][0]["lat"], json!(51.5));
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
        assert!(err(shape(Kind::Timeline, cap(Kind::Timeline), &cols(&["start", "label"]), rows)).contains("null"));
    }

    #[test]
    fn a_timeline_start_must_be_a_time_not_a_label() {
        let rows = vec![json!({ "start": "9:45 AM", "label": "Standup" })];
        assert!(err(shape(Kind::Timeline, cap(Kind::Timeline), &cols(&["start", "label"]), rows)).contains("to_char"));
        for ok in ["2026-10-06T14:45:00+00:00", "2026-10-06 09:45:00", "2026-10-06 09:45:00.5", "2026-10-06"] {
            assert!(is_time(ok), "{ok}");
        }
    }

    #[test]
    fn two_columns_with_one_name_are_refused() {
        let rows = vec![json!({ "title": "a" })];
        assert!(err(shape(Kind::Table, cap(Kind::Table), &cols(&["title", "title"]), rows)).contains("both named"));
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

    fn purpose(id: &str) -> Option<&'static chart_marks::ChartPurpose> {
        Some(purpose_named(id).unwrap())
    }

    #[test]
    fn change_over_time_draws_a_line_over_dates_and_bars_over_labels() {
        let dated = vec![json!({ "Week": "2026-09-01", "Steps": 1 }), json!({ "Week": "2026-09-08", "Steps": 2 })];
        let labelled = vec![json!({ "Day": "Mon", "Steps": 1 }), json!({ "Day": "Tue", "Steps": 2 })];
        let p = purpose("change_over_time");
        assert_eq!(pick_chart_mark(p, &ChartFields::default(), &cols(&["Week", "Steps"]), &dated).mark, "line");
        assert_eq!(pick_chart_mark(p, &ChartFields::default(), &cols(&["Day", "Steps"]), &labelled).mark, "bar");
    }

    #[test]
    fn a_line_needs_every_x_value_to_be_a_time() {
        let mixed = vec![json!({ "x": "2026-09-01", "y": 1 }), json!({ "x": "Later", "y": 2 })];
        let empty = vec![json!({ "x": null, "y": 1 })];
        let p = purpose("change_over_time");
        assert_eq!(pick_chart_mark(p, &ChartFields::default(), &cols(&["x", "y"]), &mixed).mark, "bar");
        assert_eq!(pick_chart_mark(p, &ChartFields::default(), &cols(&["x", "y"]), &empty).mark, "bar");
    }

    #[test]
    fn compare_groups_and_no_purpose_draw_bars_even_over_dates() {
        let dated = vec![json!({ "Week": "2026-09-01", "Steps": 1 }), json!({ "Week": "2026-09-08", "Steps": 2 })];
        assert_eq!(pick_chart_mark(purpose("compare_groups"), &ChartFields::default(), &cols(&["Week", "Steps"]), &dated).mark, "bar");
        assert_eq!(pick_chart_mark(None, &ChartFields::default(), &cols(&["Week", "Steps"]), &dated).mark, "bar");
    }

    #[test]
    fn a_purpose_or_a_named_mark_settles_the_kind_without_kind() {
        assert_eq!(kind_before_query(purpose("change_over_time"), None).unwrap(), Kind::Chart);
        assert_eq!(kind_before_query(purpose("place"), None).unwrap(), Kind::Map);
        assert_eq!(kind_before_query(purpose("events"), None).unwrap(), Kind::Timeline);
        assert_eq!(kind_before_query(purpose("in_sentence"), None).unwrap(), Kind::Numbers);
        assert_eq!(kind_before_query(purpose("table"), None).unwrap(), Kind::Table);
        assert_eq!(kind_before_query(None, Some("line")).unwrap(), Kind::Chart);
        assert_eq!(kind_before_query(purpose("place"), Some("table")).unwrap(), Kind::Table);
    }

    #[test]
    fn neither_purpose_nor_kind_is_refused_with_the_purposes() {
        let Err(ToolError::InvalidParameters(m)) = kind_before_query(None, None) else { panic!() };
        assert!(m.contains("change_over_time") && m.contains("or give kind"), "{m}");
    }

    /// The whole call against a scratch database: the query runs through
    /// sql_query's executor, then the picker and the shape. Requires a live
    /// Postgres (`#[sqlx::test]` provisions and drops the database).
    #[sqlx::test]
    async fn a_purpose_call_runs_the_query_and_picks_the_chart(pool: sqlx::PgPool) {
        let tool = SqlQueryTool::new(std::sync::Arc::new(pool));
        let weeks = r#"SELECT d::date AS "Week", n AS "Steps" FROM (VALUES ('2026-09-01', 4100), ('2026-09-08', 5200), ('2026-09-15', 4800)) v(d, n)"#;
        let days = r#"SELECT d AS "Day", n AS "Steps" FROM (VALUES ('Mon', 4100), ('Tue', 5200)) v(d, n)"#;
        let call = |args: Value| {
            let tool = &tool;
            async move { execute(tool, args, None, None).await }
        };

        let out = call(json!({ "purpose": "change_over_time", "sql": weeks })).await.unwrap().data;
        assert_eq!((out["kind"].as_str(), out["mark"].as_str()), (Some("chart"), Some("line")), "{out}");
        assert_eq!(out["purpose"], json!("change_over_time"));

        let out = call(json!({ "purpose": "change_over_time", "sql": days })).await.unwrap().data;
        assert_eq!(out["mark"], json!("bar"), "{out}");

        let out = call(json!({ "purpose": "in_sentence", "sql": r#"SELECT 7.25 AS "Hours asleep""# })).await.unwrap().data;
        assert_eq!(out["kind"], json!("numbers"), "{out}");
        assert!(out.get("mark").is_none(), "{out}");

        // A call written before purpose existed draws as it did: bars by default.
        let out = call(json!({ "kind": "chart", "sql": weeks })).await.unwrap().data;
        assert_eq!(out["mark"], json!("bar"), "{out}");
        assert!(out.get("purpose").is_none(), "{out}");

        let out = call(json!({ "purpose": "change_over_time", "mark": "bar", "sql": weeks })).await.unwrap().data;
        assert_eq!(out["mark"], json!("bar"), "a named mark wins over the pick: {out}");

        let refused = call(json!({ "sql": weeks })).await;
        assert!(matches!(refused, Err(ToolError::InvalidParameters(ref m)) if m.contains("purpose")), "{refused:?}");
    }

    #[test]
    fn an_unknown_purpose_or_mark_lists_the_real_ones() {
        let Err(ToolError::InvalidParameters(m)) = purpose_named("vibes") else { panic!() };
        assert!(m.contains("change_over_time, before_and_after"), "{m}");
        let Err(ToolError::InvalidParameters(m)) = kind_before_query(None, Some("pie")) else { panic!() };
        assert!(m.contains("line, bar"), "{m}");
    }

    fn fields(f: impl FnOnce(&mut ChartFields)) -> ChartFields {
        let mut out = ChartFields::default();
        f(&mut out);
        out
    }

    #[test]
    fn change_over_time_draws_a_cumulative_comparison_only_when_asked() {
        let steps = vec![json!({ "step": 1, "This month": 40, "Last month": 35 })];
        let days = vec![json!({ "day": "2026-09-01", "Steps": 4100 })];
        let p = purpose("change_over_time");
        let none = ChartFields::default();
        assert_eq!(pick_chart_mark(p, &none, &cols(&["step", "This month", "Last month"]), &steps).mark, "cumulative");
        let by_kind = fields(|f| f.step_kind = Some("weekday".into()));
        assert_eq!(pick_chart_mark(p, &by_kind, &cols(&["Weekday", "This week", "Last week"]), &steps).mark, "cumulative");
        // `day` is a step's alias only once cumulative is named, never a reason to pick it.
        assert_eq!(pick_chart_mark(p, &none, &cols(&["day", "Steps"]), &days).mark, "line");
    }

    #[test]
    fn a_purpose_with_one_mark_draws_it_so_its_refusal_names_the_fix() {
        let rows = vec![json!({ "Week": "2026-09-01", "Hours": 7 })];
        let none = ChartFields::default();
        assert_eq!(pick_chart_mark(purpose("usual_range"), &none, &cols(&["Week", "Hours"]), &rows).mark, "baseline");
        assert_eq!(pick_chart_mark(purpose("before_and_after"), &none, &cols(&["Week", "Hours"]), &rows).mark, "interrupted");
        assert_eq!(pick_chart_mark(purpose("daily_rhythm"), &none, &cols(&["Week", "Hours"]), &rows).mark, "states");
    }

    fn checked(mark: &str, f: &ChartFields, columns: &[&str], rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
        let (columns, rows) = mark_columns(mark, cols(columns), rows);
        let cap = chart_marks::mark(mark).unwrap().max_rows;
        let mut out = shape(Kind::Chart, cap, &columns, rows)?;
        check_mark(mark, f, &columns, &mut out)?;
        Ok(out)
    }

    #[test]
    fn a_cumulative_comparison_checks_its_steps_and_carries_its_fields() {
        let rows = |a: Value, b: Value| vec![json!({ "day": a, "This": 10, "Last": 8 }), json!({ "day": b, "This": 20, "Last": 18 })];
        let usd = fields(|f| f.unit = Some("USD".into()));
        let out = checked("cumulative", &usd, &["day", "This", "Last"], rows(json!(1), json!(2))).unwrap();
        assert_eq!(out["columns"][0], json!("step"), "day is renamed to step once cumulative is drawn");
        assert_eq!((out["step_kind"].as_str(), out["unit"].as_str()), (Some("day"), Some("USD")));
        assert!(err(checked("cumulative", &usd, &["day", "This", "Last"], rows(json!(1), json!(1)))).contains("appears twice"));
        assert!(err(checked("cumulative", &usd, &["day", "This", "Last"], rows(json!(1), json!(32)))).contains("from 1 to 31"));
        let hours = fields(|f| f.step_kind = Some("hour".into()));
        assert!(checked("cumulative", &hours, &["day", "This", "Last"], rows(json!(0), json!(23))).is_ok());
        let bad = fields(|f| f.step_kind = Some("fortnight".into()));
        assert!(err(checked("cumulative", &bad, &["day", "This", "Last"], rows(json!(1), json!(2)))).contains("day, weekday or hour"));
        let long = fields(|f| f.unit = Some("kilowatt-hours".into()));
        assert!(err(checked("cumulative", &long, &["day", "This", "Last"], rows(json!(1), json!(2)))).contains("unit"));
    }

    #[test]
    fn an_interrupted_series_needs_the_change_and_one_value() {
        let rows = vec![json!({ "Week": "2026-09-01", "Hours": 7 }), json!({ "Week": "2026-09-08", "Hours": 6.5 })];
        let change = fields(|f| {
            f.split_at = Some("2026-09-05".into());
            f.split_label = Some("New schedule".into());
        });
        let out = checked("interrupted", &change, &["Week", "Hours"], rows.clone()).unwrap();
        assert_eq!((out["split_at"].as_str(), out["split_label"].as_str()), (Some("2026-09-05"), Some("New schedule")));
        assert!(err(checked("interrupted", &ChartFields::default(), &["Week", "Hours"], rows)).contains("split_at"));
    }

    #[test]
    fn a_baseline_band_needs_its_usual_range_and_reads_low_and_high() {
        let rows = vec![
            json!({ "Day": "2026-09-01", "Rest": 55, "low": 54, "high": 57 }),
            json!({ "Day": "2026-09-02", "Rest": 58, "low": 54, "high": 57 }),
        ];
        let out = checked("baseline", &ChartFields::default(), &["Day", "Rest", "low", "high"], rows).unwrap();
        assert_eq!(out["columns"], json!(["Day", "Rest", "usual_low", "usual_high"]));
        let two = vec![json!({ "Day": "2026-09-01", "Rest": 55 }), json!({ "Day": "2026-09-02", "Rest": 58 })];
        assert!(err(checked("baseline", &ChartFields::default(), &["Day", "Rest"], two)).contains("usual_low and usual_high"));
    }

    #[test]
    fn a_state_timeline_reads_its_aliases_and_refuses_too_many_states() {
        let stretch = |state: &str| json!({ "date": "2026-09-01", "started_at": "09:00", "ended_at": "10:30", "what": state });
        let (columns, rows) = mark_columns("states", cols(&["date", "started_at", "ended_at", "what"]), vec![stretch("Home"), stretch("Work")]);
        let out = shape_states(600, &columns, rows).unwrap();
        assert_eq!(out["columns"], json!(["day", "start", "end", "state"]));
        let seven: Vec<Value> = ["A", "B", "C", "D", "E", "F", "G"].iter().map(|s| stretch(s)).collect();
        let (columns, rows) = mark_columns("states", cols(&["date", "started_at", "ended_at", "what"]), seven);
        assert!(err(shape_states(600, &columns, rows)).contains("at most 6 states"));
        assert!(err(shape_states(600, &cols(&["day", "start", "state"]), vec![json!({ "day": "2026-09-01", "start": "09:00", "state": "Home" })])).contains("end is missing"));
        let bad = vec![json!({ "day": "2026-09-01", "start": "morning", "end": "10:30", "state": "Home" })];
        assert!(err(shape_states(600, &cols(&["day", "start", "end", "state"]), bad)).contains("time of day"));
    }

    #[test]
    fn a_bar_chart_keeps_a_column_named_date() {
        let (columns, _) = mark_columns("bar", cols(&["date", "Steps"]), vec![]);
        assert_eq!(columns, cols(&["date", "Steps"]));
    }

    #[test]
    fn a_mark_given_as_the_purpose_is_read_as_that_mark() {
        let (p, m) = purpose_and_mark(Some("cumulative".into()), None).unwrap();
        assert_eq!((p.map(|p| p.id), m.as_deref()), (Some("change_over_time"), Some("cumulative")));
        // A mark the call names itself wins over the one in purpose.
        let (_, m) = purpose_and_mark(Some("cumulative".into()), Some("line".into())).unwrap();
        assert_eq!(m.as_deref(), Some("line"));
        assert!(purpose_and_mark(Some("vibes".into()), None).is_err());
    }

    #[test]
    fn compare_groups_picks_by_shape() {
        let p = purpose("compare_groups");
        let none = ChartFields::default();
        let panels = vec![json!({ "panel": "Mon", "Week": "2026-09-07", "Hours": 7 })];
        assert_eq!(pick_chart_mark(p, &none, &cols(&["panel", "Week", "Hours"]), &panels).mark, "multiples");
        let then_now = vec![json!({ "Day": "Mon", "October": 7, "November": 6.5 })];
        assert_eq!(pick_chart_mark(p, &none, &cols(&["Day", "October", "November"]), &then_now).mark, "dumbbell");
        let named = vec![json!({ "Place": "Home", "Hours": 300 }), json!({ "Place": "Work", "Hours": 40 })];
        assert_eq!(pick_chart_mark(p, &none, &cols(&["Place", "Hours"]), &named).mark, "list");
        let dated = vec![json!({ "Week": "2026-09-07", "Hours": 7 })];
        assert_eq!(pick_chart_mark(p, &none, &cols(&["Week", "Hours"]), &dated).mark, "bar");
    }

    #[test]
    fn part_to_whole_picks_waterfall_share_or_stack() {
        let p = purpose("part_to_whole");
        let none = ChartFields::default();
        let steps = vec![json!({ "Step": "Start", "Amount": 1000 }), json!({ "Step": "Rent", "Amount": -800 })];
        assert_eq!(pick_chart_mark(p, &none, &cols(&["Step", "Amount"]), &steps).mark, "waterfall");
        let parts = vec![json!({ "Kind": "Food", "Spent": 300 }), json!({ "Kind": "Travel", "Spent": 120 })];
        assert_eq!(pick_chart_mark(p, &none, &cols(&["Kind", "Spent"]), &parts).mark, "share");
        let weeks = vec![json!({ "Week": "2026-09-07", "Deep": 1, "Light": 4, "REM": 1.5 })];
        assert_eq!(pick_chart_mark(p, &none, &cols(&["Week", "Deep", "Light", "REM"]), &weeks).mark, "stack");
    }

    #[test]
    fn small_multiples_and_stacks_check_their_own_rows() {
        let row = |p: &str, v: Value| json!({ "group": p, "Week": "2026-09-07", "Hours": v });
        let (columns, rows) = mark_columns("multiples", cols(&["group", "Week", "Hours"]), vec![row("Mon", json!(7)), row("Tue", json!("6.5"))]);
        let out = shape_multiples(400, &columns, rows).unwrap();
        assert_eq!((out["columns"][0].as_str(), &out["rows"][1]["Hours"]), (Some("panel"), &json!(6.5)));
        assert!(err(shape_multiples(400, &cols(&["panel", "Week", "Hours"]), vec![row("Mon", json!(7))])).contains("2 to 12 panels"));
        let bar = |v: Value| json!({ "Week": "2026-09-07", "Deep": 1, "Light": v });
        assert!(shape_stack(199, &cols(&["Week", "Deep", "Light"]), vec![bar(json!(4))]).is_ok());
        assert!(err(shape_stack(199, &cols(&["Week", "Deep", "Light"]), vec![bar(json!(-1))])).contains("below zero"));
        assert!(err(shape_stack(199, &cols(&["Week", "Deep"]), vec![json!({ "Week": "2026-09-07", "Deep": 1 })])).contains("2 to 5 part"));
    }

    #[test]
    fn a_share_needs_a_whole_and_a_list_two_columns() {
        let parts = vec![json!({ "Kind": "Food", "Spent": 300 }), json!({ "Kind": "Travel", "Spent": -5 })];
        assert!(err(checked("share", &ChartFields::default(), &["Kind", "Spent"], parts)).contains("zero or more"));
        let three = vec![json!({ "Kind": "Food", "A": 1, "B": 2 }), json!({ "Kind": "Travel", "A": 1, "B": 2 })];
        assert!(err(checked("list", &ChartFields::default(), &["Kind", "A", "B"], three.clone())).contains("sorted bar list is a label"));
        assert!(checked("dumbbell", &ChartFields::default(), &["Kind", "A", "B"], three).is_ok());
    }
}
