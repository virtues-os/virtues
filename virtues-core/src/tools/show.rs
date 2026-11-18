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
    /// What the figure answers: a purpose, a mark, or choices. The one field
    /// the tool offers; kind, purpose and mark are read for calls written
    /// before it.
    #[serde(default)]
    chart: Option<String>,
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
    /// scatter: one sentence on what lines up, from rows already read.
    #[serde(default)]
    finding: Option<String>,
    /// aligned: the event at zero, in a few words.
    #[serde(default)]
    event_label: Option<String>,
    /// lollipop: the window's length in days, 7 to 92.
    #[serde(default)]
    days: Option<Value>,
    /// density: the stretch the minutes cover, in a few words.
    #[serde(default)]
    period: Option<String>,
    /// numbers: for a figure that stands for a series, its column and the
    /// SELECT of the rows it summarizes.
    #[serde(default)]
    series: Option<std::collections::BTreeMap<String, String>>,
}

/// The call fields a chart mark reads beyond its rows, trimmed.
#[derive(Debug, Default, Clone)]
struct ChartFields {
    split_at: Option<String>,
    split_label: Option<String>,
    step_kind: Option<String>,
    unit: Option<String>,
    finding: Option<String>,
    event_label: Option<String>,
    days: Option<Value>,
    period: Option<String>,
}

const MAX_STATES: usize = 6;
const MAX_UNIT_CHARS: usize = 12;
const MAX_SPLIT_LABEL_CHARS: usize = 60;
const MAX_FINDING_CHARS: usize = 240;
const MAX_EVENT_LABEL_CHARS: usize = 40;
const MAX_PERIOD_CHARS: usize = 40;
const MAX_MODE_CHARS: usize = 24;
const MAX_ALIGNED_EVENTS: usize = 40;
const MAX_LANES: usize = 12;
/// A figure's series has as many rows as a chart.
const MAX_SERIES_ROWS: usize = 199;

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
    let (purpose_in, kind_in) = chart_into(trimmed(&args.chart), trimmed(&args.purpose), trimmed(&args.kind));
    let (purpose, mut named_mark) = purpose_and_mark(purpose_in, trimmed(&args.mark))?;
    let fields = ChartFields {
        split_at: trimmed(&args.split_at),
        split_label: trimmed(&args.split_label),
        step_kind: trimmed(&args.step_kind),
        unit: trimmed(&args.unit),
        finding: trimmed(&args.finding),
        event_label: trimmed(&args.event_label),
        days: args.days.clone(),
        period: trimmed(&args.period),
    };
    let kind = match kind_in {
        // A mark's name given as the kind (`kind: "share"`) is read as that
        // mark, as one given as the purpose is; refusing it only buys a retry.
        Some(k) => match (Kind::parse(&k), chart_marks::mark(&k)) {
            (Ok(kind), _) => kind,
            (Err(_), Some(m)) => {
                named_mark.get_or_insert(k);
                Kind::parse(m.kind)?
            }
            (Err(e), None) => return Err(e),
        },
        None => kind_before_query(purpose, named_mark.as_deref())?,
    };
    // A known mark draws with its own kind, so `kind: "chart", mark:
    // "swimlanes"` is a swimlane plot rather than a refusal.
    let kind = match named_mark.as_deref().and_then(chart_marks::mark) {
        Some(m) if kind != Kind::Choices => Kind::parse(m.kind)?,
        _ => kind,
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
    // The mark is settled before the query when the call names it, and
    // picked from the columns after it when only the purpose is given.
    let named_chart_mark = match named_mark.as_deref() {
        Some(m) => match chart_marks::mark(m) {
            Some(cm) if cm.kind == kind.name() => Some(cm.mark),
            _ => {
                let known: Vec<&str> =
                    chart_marks::MARKS.iter().filter(|m| m.kind == kind.name()).map(|m| m.mark).collect();
                return Err(invalid(format!("mark '{m}' is not a {} mark: use one of {}", kind.name(), known.join(", "))));
            }
        },
        None => None,
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
        (_, Some(m)) => chart_marks::mark(m).expect("a named mark is in the table"),
        (Kind::Chart, None) => pick_chart_mark(purpose, &fields, &columns, &rows),
        (Kind::Timeline | Kind::Map, None) => pick_mark(kind, purpose, &fields, &columns, &rows),
        _ => candidates[0],
    };
    let (columns, rows) = mark_columns(drawn.mark, columns, rows);
    // These chart marks have rows `shape`'s bar and line checks don't fit.
    let mut shaped = match drawn.mark {
        "states" => shape_states(drawn.max_rows, &columns, rows)?,
        "multiples" => shape_multiples(drawn.max_rows, &columns, rows)?,
        "stack" => shape_stack(drawn.max_rows, &columns, rows)?,
        "ridgeline" => shape_ridgeline(drawn.max_rows, &columns, rows)?,
        "diurnal" => shape_diurnal(drawn.max_rows, &columns, rows)?,
        "actogram" => shape_actogram(drawn.max_rows, &columns, rows)?,
        "dotplot" => shape_dotplot(drawn.max_rows, &columns, rows)?,
        "scatter" => shape_scatter(drawn.max_rows, &columns, rows)?,
        "aligned" => shape_aligned(drawn.max_rows, &columns, rows)?,
        "swimlanes" => shape_swimlanes(drawn.max_rows, &columns, rows)?,
        "lollipop" => shape_lollipop(drawn.max_rows, &columns, rows)?,
        "itinerary" => shape_itinerary(drawn.max_rows, &columns, rows)?,
        "route" => shape_route(drawn.max_rows, &columns, rows)?,
        "density" => shape_density(drawn.max_rows, &columns, rows)?,
        _ => shape(kind, drawn.max_rows, &columns, rows)?,
    };
    // A plain map or timeline carries no mark, as before the others existed.
    if kind == Kind::Chart || drawn.mark != kind.name() {
        check_mark(drawn.mark, &fields, &columns, &mut shaped)?;
        shaped.insert("mark".into(), json!(drawn.mark));
    }
    // The window these marks draw starts on the day the reply was written,
    // so a saved chat keeps its window.
    if matches!(drawn.mark, "swimlanes" | "lollipop") {
        let today = crate::timezone::owner_rfc3339(chrono::Utc::now(), timezone);
        shaped.insert("as_of".into(), json!(today.get(..10).unwrap_or(&today)));
    }
    match (kind, args.series) {
        (Kind::Numbers, Some(series)) if !series.is_empty() => {
            let figures = shaped
                .get("columns")
                .and_then(Value::as_array)
                .map(|c| c.iter().filter_map(|v| v.as_str().map(str::to_string)).collect::<Vec<_>>())
                .unwrap_or_default();
            let mut drawn_series = Map::new();
            for (column, series_sql) in series {
                if !figures.contains(&column) {
                    return Err(invalid(format!(
                        "series names \"{column}\", which the figures query does not return; its keys are the figure columns: {}",
                        figures.join(", ")
                    )));
                }
                let result = sql_query
                    .query_for_show(series_sql.trim(), MAX_SERIES_ROWS as u32 + 1, timezone, schema)
                    .await?;
                if result.data.get("truncated").is_some() {
                    return Err(invalid(format!(
                        "series \"{column}\" is too large to carry; aggregate it (GROUP BY a day or a week)"
                    )));
                }
                let cols: Vec<String> = result.data["columns"]
                    .as_array()
                    .map(|c| c.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                let rows: Vec<Value> = result.data["rows"].as_array().cloned().unwrap_or_default();
                drawn_series.insert(column.clone(), Value::Object(shape_series(&column, &cols, rows)?));
            }
            shaped.insert("series".into(), Value::Object(drawn_series));
            // The figures and every series reach the model in one result.
            if serde_json::to_vec(&shaped).map_or(0, |b| b.len()) > super::sql_query::MAX_RESULT_BYTES {
                return Err(invalid(
                    "the series are too large to carry together; give each fewer rows (a month by day, a year by week)",
                ));
            }
        }
        (Kind::Numbers, _) if drawn.mark == "spark" => {
            return Err(invalid(
                "mark spark draws each figure's series as a sparkline; add series, which maps each figure's \
                 column to the SELECT of the rows it summarizes",
            ))
        }
        (Kind::Numbers, _) | (_, None) => {}
        (_, Some(_)) => {
            return Err(invalid(
                "series belongs to numbers (in_sentence or spark): it maps a figure's column to the SELECT of \
                 the rows that figure summarizes",
            ))
        }
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

/// Where `chart` goes: choices and chart are kinds; anything else is read as
/// a purpose, which `purpose_and_mark` also reads as a mark. A purpose or
/// kind the call gives by name wins.
fn chart_into(chart: Option<String>, purpose: Option<String>, kind: Option<String>) -> (Option<String>, Option<String>) {
    match chart {
        Some(c) if c == "choices" || c == "chart" => (purpose, kind.or(Some(c))),
        Some(c) => (purpose.or(Some(c)), kind),
        None => (purpose, kind),
    }
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
        invalid(format!("'{id}' is not a purpose or a chart. Give a purpose: {}", known.join(", ")))
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
    if let Some(m) = marks.iter().find(|m| chart_fits(m.mark, fields, columns, rows)) {
        return m;
    }
    // When nothing fits: bar if it serves the purpose, else the purpose's
    // first mark, so its own refusal names the fix rather than a bar chart
    // drawing something the call didn't ask for.
    if marks.iter().any(|m| m.mark == "bar") {
        bar
    } else {
        marks.first().copied().unwrap_or(bar)
    }
}

/// The mark for a map or a timeline that names none: the first of the
/// purpose's marks of that kind whose columns fit, else the kind's own plain
/// mark (`map`, `timeline`), which every earlier call drew. Pure.
fn pick_mark(
    kind: Kind,
    purpose: Option<&chart_marks::ChartPurpose>,
    fields: &ChartFields,
    columns: &[String],
    rows: &[Value],
) -> &'static ChartMark {
    let plain = chart_marks::mark(kind.name()).expect("a map and a timeline are marks of their own kind");
    let Some(p) = purpose else { return plain };
    chart_marks::marks_for(p.id)
        .filter(|m| m.kind == kind.name())
        .find(|m| chart_fits(m.mark, fields, columns, rows))
        .unwrap_or(plain)
}

fn chart_fits(mark: &str, fields: &ChartFields, columns: &[String], rows: &[Value]) -> bool {
    match mark {
        // Only a literal step column or step_kind: an ordinary `day, steps`
        // query is a bar or a line, though `day` is a step's alias once named.
        "cumulative" => fields.step_kind.is_some() || columns.iter().any(|c| c == "step"),
        "interrupted" => fields.split_at.is_some(),
        "baseline" => has_column(mark, "usual_low", columns) && has_column(mark, "usual_high", columns),
        "states" => ["day", "start", "end", "state"].iter().all(|c| has_column(mark, c, columns)),
        // A panel column by name, or three columns whose first repeats: each
        // group comes back on many rows, one per point of its small chart.
        "multiples" => {
            has_column(mark, "panel", columns)
                || (columns.len() == 3 && is_label_column(rows, &columns[0]) && first_repeats(rows, &columns[0]))
        }
        // A text category on each row once, and exactly two numbers: then and now.
        "dumbbell" => {
            columns.len() == 3
                && rows.len() <= 20
                && is_label_column(rows, &columns[0])
                && !first_repeats(rows, &columns[0])
        }
        // A text label and one value, few enough to list; a dated first
        // column is a trend and stays a bar.
        "list" => columns.len() == 2 && rows.len() <= 20 && is_label_column(rows, &columns[0]),
        "waterfall" => columns.len() == 2 && values_of(rows, &columns[1]).any(|v| v < 0.0),
        "share" => {
            columns.len() == 2 && (2..=8).contains(&rows.len()) && values_of(rows, &columns[1]).all(|v| v >= 0.0)
        }
        "stack" => columns.len() >= 3,
        "ridgeline" => {
            columns.len() == 3
                && has_column(mark, "day", columns)
                && (has_column(mark, "hour", columns) || has_column(mark, "minute", columns))
        }
        "diurnal" => ["hour", "days", "share"].iter().all(|c| has_column(mark, c, columns)),
        "actogram" => ["day", "started_at", "ended_at"].iter().all(|c| has_column(mark, c, columns)),
        "dotplot" => (1..=2).contains(&columns.len()),
        "aligned" => has_column(mark, "event", columns) && has_column(mark, "offset", columns),
        "swimlanes" => ["lane", "at", "what"].iter().all(|c| has_column(mark, c, columns)),
        "lollipop" => has_column(mark, "due", columns),
        // A column named place, and a start and an end (a timeline would
        // otherwise draw the same stays as bars).
        "itinerary" => {
            columns.iter().any(|c| c.eq_ignore_ascii_case("place"))
                && (has_column(mark, "started_at", columns) || has_column("timeline", "start", columns))
                && (has_column(mark, "ended_at", columns) || has_column("timeline", "end", columns))
        }
        "route" => has_column(mark, "at", columns),
        "density" => has_column(mark, "minutes", columns),
        "timeline" | "map" => true,
        // The weekly-rhythm form and every shape it takes are its own refusal's to explain.
        "scatter" => true,
        // Same rows as a line; drawn only when the call names it.
        "area" => false,
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
    ("ridgeline", "day", &["date", "night"]),
    ("ridgeline", "hour", &[]),
    ("ridgeline", "minute", &[]),
    ("diurnal", "hour", &[]),
    ("diurnal", "days", &[]),
    ("diurnal", "share", &[]),
    ("actogram", "day", &["night", "date"]),
    ("actogram", "started_at", &["asleep_at", "start"]),
    ("actogram", "ended_at", &["woke_at", "end"]),
    ("aligned", "event", &[]),
    ("aligned", "offset", &[]),
    ("swimlanes", "lane", &["thread", "project"]),
    ("swimlanes", "at", &[]),
    ("swimlanes", "what", &[]),
    ("lollipop", "due", &[]),
    ("lollipop", "label", &[]),
    ("lollipop", "amount", &[]),
    ("itinerary", "place", &[]),
    ("itinerary", "started_at", &["start", "arrived_at"]),
    ("itinerary", "ended_at", &["end", "left_at"]),
    ("route", "at", &["occurred_at", "recorded_at", "time"]),
    ("density", "minutes", &["duration_minutes", "mins"]),
];

/// Whether a mark's column is there by its name or an alias, in any case:
/// a model aliases columns for the owner, so `Hour` is `hour`.
fn has_column(mark: &str, want: &str, columns: &[String]) -> bool {
    columns.iter().any(|c| names_column(mark, want, c))
}

fn names_column(mark: &str, want: &str, column: &str) -> bool {
    column.eq_ignore_ascii_case(want)
        || MARK_ALIASES
            .iter()
            .filter(|(m, w, _)| *m == mark && *w == want)
            .any(|(_, _, aliases)| aliases.iter().any(|a| column.eq_ignore_ascii_case(a)))
}

/// Rename the first column that names one of the mark's columns (by name
/// in any case, or an alias) to it, unless the column is already there.
fn mark_columns(mark: &str, mut columns: Vec<String>, mut rows: Vec<Value>) -> (Vec<String>, Vec<Value>) {
    for (m, want, _) in MARK_ALIASES {
        if *m != mark || columns.iter().any(|c| c == want) {
            continue;
        }
        let Some(at) = columns.iter().position(|c| names_column(mark, want, c)) else { continue };
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
        "scatter" => {
            if let Some(f) = &fields.finding {
                if f.chars().count() > MAX_FINDING_CHARS {
                    return Err(invalid(format!("finding is one sentence, at most {MAX_FINDING_CHARS} characters")));
                }
                out.insert("finding".into(), json!(f));
            }
        }
        "aligned" => {
            let Some(label) = &fields.event_label else {
                return Err(invalid(
                    "an event-aligned overlay needs event_label, the event at zero in a few words (Payday, Rowing starts)",
                ));
            };
            if label.chars().count() > MAX_EVENT_LABEL_CHARS {
                return Err(invalid(format!("event_label is a few words, at most {MAX_EVENT_LABEL_CHARS} characters")));
            }
            out.insert("event_label".into(), json!(label));
        }
        "lollipop" => {
            if let Some(v) = &fields.days {
                let days = v.as_f64().or_else(|| v.as_str()?.trim().parse().ok());
                match days {
                    Some(d) if d.fract() == 0.0 && (7.0..=92.0).contains(&d) => {
                        out.insert("days".into(), json!(d as i64));
                    }
                    _ => return Err(invalid(format!("days is a whole number from 7 to 92, not {v}"))),
                }
            }
        }
        "density" => {
            if let Some(period) = &fields.period {
                if period.chars().count() > MAX_PERIOD_CHARS {
                    return Err(invalid(format!(
                        "period is a few words for the stretch (in October), at most {MAX_PERIOD_CHARS} characters"
                    )));
                }
                out.insert("period".into(), json!(period));
            }
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

/// A ridgeline plot's rows: the day, the position in it (hour 0-24, or
/// minute since the row began), and the value, a number of zero or more or a
/// sleep stage name. Pure.
fn shape_ridgeline(max_rows: usize, columns: &[String], mut rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if rows.len() > max_rows {
        return Err(invalid(format!("a ridgeline plot draws at most {max_rows} points; bin the positions wider")));
    }
    if columns.len() != 3 || !(columns[1] == "hour" || columns[1] == "minute") {
        return Err(invalid(format!(
            "a ridgeline plot is the day, then hour (0 to 24) or minute, then the value; this query returns {}",
            columns.join(", ")
        )));
    }
    numeric_column(&mut rows, &columns[1], "the position")?;
    if columns[1] == "hour" && values_of(&rows, "hour").any(|h| !(0.0..=24.0).contains(&h)) {
        return Err(invalid("hour runs from 0 to 24 (6.5 is 6:30)"));
    }
    // A numeric string becomes a number; a stage name stays text.
    for row in rows.iter_mut() {
        if let Some(v) = row.get_mut(&columns[2]) {
            if let Some(n) = v.as_str().and_then(|s| s.trim().parse::<f64>().ok()) {
                *v = json!(n);
            }
        }
    }
    if rows.iter().any(|r| r[&columns[2]].as_f64().is_some_and(|v| v < 0.0)) {
        return Err(invalid(format!("\"{}\" goes below zero, and a ridge can't", columns[2])));
    }
    let mut days: Vec<String> = Vec::new();
    for row in &rows {
        let d = row[&columns[0]].to_string();
        if !days.contains(&d) {
            days.push(d);
        }
    }
    if days.len() > 100 {
        return Err(invalid(format!("a ridgeline plot stacks at most 100 days, and this query has {}", days.len())));
    }
    Ok(chart_out(columns, rows))
}

/// A diurnal profile's rows: hour (0-23), days (weekdays or weekends) and
/// share (0-1). Pure.
fn shape_diurnal(max_rows: usize, columns: &[String], mut rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if rows.len() > max_rows {
        return Err(invalid(format!("a diurnal profile has at most {max_rows} rows, one per hour of each kind of day")));
    }
    if let Some(missing) = ["hour", "days", "share"].iter().find(|c| !columns.iter().any(|have| have == *c)) {
        return Err(invalid(format!(
            "a diurnal profile needs hour, days and share columns; {missing} is missing from {}",
            columns.join(", ")
        )));
    }
    numeric_column(&mut rows, "hour", "hour")?;
    numeric_column(&mut rows, "share", "share")?;
    for row in &rows {
        if !row["hour"].as_f64().is_some_and(|h| h.fract() == 0.0 && (0.0..=23.0).contains(&h)) {
            return Err(invalid(format!("hour is a whole number from 0 to 23, and one row's is {}", row["hour"])));
        }
        if !row["share"].as_f64().is_some_and(|s| (0.0..=1.0).contains(&s)) {
            return Err(invalid(format!("share runs from 0 to 1, and one row's is {}", row["share"])));
        }
        if !row["days"].as_str().is_some_and(|d| d.eq_ignore_ascii_case("weekdays") || d.eq_ignore_ascii_case("weekends")) {
            return Err(invalid(format!("days is weekdays or weekends, and one row's is {}", row["days"])));
        }
    }
    Ok(chart_out(columns, rows))
}

/// An actogram's rows: the day, then started_at and ended_at of its interval.
/// A day whose interval breaks has one row per stretch. Pure.
fn shape_actogram(max_rows: usize, columns: &[String], rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if rows.len() > max_rows {
        return Err(invalid(format!("an actogram draws at most {max_rows} stretches; narrow the window")));
    }
    if let Some(missing) = ["day", "started_at", "ended_at"].iter().find(|c| !columns.iter().any(|have| have == *c)) {
        return Err(invalid(format!(
            "an actogram needs day, started_at and ended_at (night, asleep_at and woke_at work too); \
             {missing} is missing from {}",
            columns.join(", ")
        )));
    }
    for row in &rows {
        for c in ["started_at", "ended_at"] {
            if !row[c].as_str().is_some_and(is_time) {
                return Err(invalid(format!("{c} must be a timestamp, and one row's is {}", row[c])));
            }
        }
    }
    Ok(chart_out(columns, rows))
}

/// A dot plot's rows: one value per record (a number, a duration or a time
/// of day), with an optional first column naming each record. Pure.
fn shape_dotplot(max_rows: usize, columns: &[String], mut rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if !(3..=max_rows).contains(&rows.len()) {
        return Err(invalid(format!(
            "a dot plot draws 3 to {max_rows} records, one dot each; this query has {}. Don't aggregate them",
            rows.len()
        )));
    }
    let Some(value) = columns.last().filter(|_| columns.len() <= 2) else {
        return Err(invalid(format!(
            "a dot plot is one value column, with an optional label first; this query has {} columns",
            columns.len()
        )));
    };
    // A number, or text the client reads as a duration or a clock time.
    for row in rows.iter_mut() {
        match row.get_mut(value.as_str()) {
            Some(v @ Value::String(_)) => {
                if let Some(n) = v.as_str().and_then(|s| s.trim().parse::<f64>().ok()) {
                    *v = json!(n);
                }
            }
            Some(Value::Number(_) | Value::Null) | None => {}
            Some(other) => {
                return Err(invalid(format!("\"{value}\" must be a number, a duration or a time of day, not {other}")))
            }
        }
    }
    Ok(chart_out(columns, rows))
}

/// A scatter plot's rows, in either form: a date or label then two numbers
/// (or two numbers alone), or one column of times for a weekly rhythm. Pure.
fn shape_scatter(max_rows: usize, columns: &[String], mut rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if !(3..=max_rows).contains(&rows.len()) {
        return Err(invalid(format!("a scatter plot draws 3 to {max_rows} points; this query has {}", rows.len())));
    }
    match columns.len() {
        1 => {
            if !rows.iter().all(|r| r[&columns[0]].as_str().is_some_and(is_time)) {
                return Err(invalid("a weekly rhythm is one column of times; every row needs a timestamp"));
            }
        }
        2 | 3 => {
            for measure in &columns[columns.len() - 2..] {
                numeric_column(&mut rows, measure, "a measure")?;
            }
        }
        n => {
            return Err(invalid(format!(
                "a scatter plot is a date or label then two numbers, or one column of times; this query has {n} columns"
            )))
        }
    }
    Ok(chart_out(columns, rows))
}

/// An event-aligned overlay's rows: the event (when each occurrence began),
/// the offset from it in minutes, and the value. Pure.
fn shape_aligned(max_rows: usize, columns: &[String], mut rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if rows.len() > max_rows {
        return Err(invalid(format!(
            "an event-aligned overlay draws at most {max_rows} points; bin the offsets wider (5 minutes, or a day)"
        )));
    }
    if columns.len() != 3 || !["event", "offset"].iter().all(|c| columns.iter().any(|have| have == c)) {
        return Err(invalid(format!(
            "an event-aligned overlay is event (when each occurrence began), offset (minutes from it, \
             negative before) and the value; this query returns {}",
            columns.join(", ")
        )));
    }
    let value = columns.iter().find(|c| *c != "event" && *c != "offset").expect("three columns, two named");
    numeric_column(&mut rows, "offset", "offset")?;
    numeric_column(&mut rows, value, "the value")?;
    let mut events: Vec<String> = Vec::new();
    for row in &rows {
        if !row["event"].as_str().is_some_and(is_time) {
            return Err(invalid(format!("event must be the time or date each occurrence began, and one row's is {}", row["event"])));
        }
        if row["offset"].is_null() {
            return Err(invalid("every row needs an offset, in minutes from its event"));
        }
        let e = row["event"].to_string();
        if !events.contains(&e) {
            events.push(e);
        }
    }
    if events.len() > MAX_ALIGNED_EVENTS {
        return Err(invalid(format!(
            "an event-aligned overlay lines up at most {MAX_ALIGNED_EVENTS} occurrences, and this query has {}; \
             narrow the window",
            events.len()
        )));
    }
    Ok(chart_out(columns, rows))
}

/// A swimlane event plot's rows: lane (the group), at (a time or date) and
/// what (a few words). Pure.
fn shape_swimlanes(max_rows: usize, columns: &[String], rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if rows.len() > max_rows {
        return Err(invalid(format!("a swimlane plot draws at most {max_rows} events; narrow the window")));
    }
    if let Some(missing) = ["lane", "at", "what"].iter().find(|c| !columns.iter().any(|have| have == *c)) {
        return Err(invalid(format!(
            "a swimlane plot needs lane (the group; thread or project work too), at and what; \
             {missing} is missing from {}",
            columns.join(", ")
        )));
    }
    let mut lanes: Vec<String> = Vec::new();
    for row in &rows {
        if !row["at"].as_str().is_some_and(is_time) {
            return Err(invalid(format!("at must be a timestamp or a date, and one row's is {}", row["at"])));
        }
        let lane = row["lane"].to_string();
        if !lanes.contains(&lane) {
            lanes.push(lane);
        }
    }
    if lanes.len() > MAX_LANES {
        return Err(invalid(format!(
            "a swimlane plot draws at most {MAX_LANES} lanes, and this query has {}; keep the groups the owner asked about",
            lanes.len()
        )));
    }
    Ok(figure_out(Kind::Timeline, columns, rows))
}

/// A lollipop timeline's rows: due (a date), label, and an optional amount.
/// Pure.
fn shape_lollipop(max_rows: usize, columns: &[String], mut rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if rows.len() > max_rows {
        return Err(invalid(format!("a lollipop timeline draws at most {max_rows} items; narrow the window")));
    }
    if !["due", "label"].iter().all(|c| columns.iter().any(|have| have == c)) {
        return Err(invalid(format!(
            "a lollipop timeline needs due (a date) and label, and optionally amount; this query returns {}",
            columns.join(", ")
        )));
    }
    if columns.iter().any(|c| c == "amount") {
        numeric_column(&mut rows, "amount", "amount")?;
    }
    for row in &rows {
        if !row["due"].as_str().is_some_and(is_time) {
            return Err(invalid(format!("due must be a date, and one row's is {}", row["due"])));
        }
        if row["label"].as_str().is_none_or(|l| l.trim().is_empty()) {
            return Err(invalid("every item needs a label; filter out rows where it is empty"));
        }
    }
    Ok(figure_out(Kind::Timeline, columns, rows))
}

/// An itinerary's rows: one stay each, in order, with place, started_at and
/// ended_at, and an optional mode (how the stay was reached). Pure.
fn shape_itinerary(max_rows: usize, columns: &[String], rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if rows.len() > max_rows {
        return Err(invalid(format!("an itinerary draws at most {max_rows} stays; merge short ones or narrow the trip")));
    }
    if let Some(missing) = ["place", "started_at", "ended_at"].iter().find(|c| !columns.iter().any(|have| have == *c)) {
        return Err(invalid(format!(
            "an itinerary needs place, started_at and ended_at for each stay (start and end work too); \
             {missing} is missing from {}",
            columns.join(", ")
        )));
    }
    let time = |row: &Value, c: &str| -> Result<Option<chrono::DateTime<chrono::FixedOffset>>, ToolError> {
        match &row[c] {
            Value::Null => Ok(None),
            Value::String(s) if is_time(s) => Ok(parse_time(s)),
            other => Err(invalid(format!("{c} must be a timestamp or a date, and one row's is {other}"))),
        }
    };
    let last = rows.len() - 1;
    let mut previous_start = None;
    for (i, row) in rows.iter().enumerate() {
        let (start, end) = (time(row, "started_at")?, time(row, "ended_at")?);
        if start.is_none() && i != 0 {
            return Err(invalid("only the first stay may have no started_at (where the run leaves from)"));
        }
        if end.is_none() && i != last {
            return Err(invalid("only the last stay may have no ended_at (where the run arrives)"));
        }
        if let (Some(s), Some(e)) = (start, end) {
            if e < s {
                return Err(invalid(format!("a stay ends before it starts, at {}", row["place"])));
            }
        }
        if let (Some(p), Some(s)) = (previous_start, start) {
            if s < p {
                return Err(invalid("stays go in order: ORDER BY started_at"));
            }
        }
        previous_start = start.or(previous_start);
        if let Some(mode) = row.get("mode").filter(|m| !m.is_null()) {
            if !mode.as_str().is_some_and(|m| m.chars().count() <= MAX_MODE_CHARS) {
                return Err(invalid(format!("mode is a word or two (Train, Flight), at most {MAX_MODE_CHARS} characters")));
            }
        }
    }
    Ok(figure_out(Kind::Timeline, columns, rows))
}

/// A route's rows: lat, lon and at, joined in the order of at. Pure.
fn shape_route(max_rows: usize, columns: &[String], mut rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if !(2..=max_rows).contains(&rows.len()) {
        return Err(invalid(format!(
            "a route draws 2 to {max_rows} points, and this query has {}; thin the points (every nth) or narrow the window",
            rows.len()
        )));
    }
    coordinates(columns, &mut rows, "a route needs lat, lon and at")?;
    if !columns.iter().any(|c| c == "at") {
        return Err(invalid(format!("a route needs at, the time of each point; this query returns {}", columns.join(", "))));
    }
    if !rows.iter().all(|r| r["at"].as_str().is_some_and(is_time)) {
        return Err(invalid("at must be a timestamp on every point"));
    }
    if rows.len() < 2 {
        return Err(invalid("a route needs at least two points with coordinates in range"));
    }
    Ok(figure_out(Kind::Map, columns, rows))
}

/// A dot density map's rows: lat, lon and minutes, with an optional label.
/// A row with no minutes is left out. Pure.
fn shape_density(max_rows: usize, columns: &[String], mut rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if rows.len() > max_rows {
        return Err(invalid(format!("a dot density map draws at most {max_rows} places; group nearby ones")));
    }
    coordinates(columns, &mut rows, "a dot density map needs lat, lon and minutes")?;
    if !columns.iter().any(|c| c == "minutes") {
        return Err(invalid(format!(
            "a dot density map needs minutes, the time spent at each place; this query returns {}",
            columns.join(", ")
        )));
    }
    numeric_column(&mut rows, "minutes", "minutes")?;
    rows.retain(|r| r["minutes"].as_f64().is_some());
    if values_of(&rows, "minutes").any(|m| m < 0.0) {
        return Err(invalid("minutes can't go below zero"));
    }
    if rows.is_empty() {
        return Err(invalid("no place has minutes; filter out rows where it is null"));
    }
    Ok(figure_out(Kind::Map, columns, rows))
}

/// A figure's series: the x axis first, one value, and figure, the figure's
/// level on the rows it summarizes and null on the rows shown around them.
/// Pure.
fn shape_series(name: &str, columns: &[String], mut rows: Vec<Value>) -> Result<Map<String, Value>, ToolError> {
    rows_and_names(columns, &rows)?;
    if rows.len() > MAX_SERIES_ROWS {
        return Err(invalid(format!(
            "series \"{name}\" draws at most {MAX_SERIES_ROWS} rows; aggregate to fewer (GROUP BY a week)"
        )));
    }
    if rows.len() < 2 {
        return Err(invalid(format!("series \"{name}\" needs at least two rows; one value belongs in the sentence")));
    }
    let figure = columns.iter().position(|c| c == "figure");
    if columns.len() != 3 || figure.is_none_or(|i| i == 0) {
        return Err(invalid(format!(
            "series \"{name}\" is the x axis as its first column, one numeric column, and figure; this query returns {}",
            columns.join(", ")
        )));
    }
    let value = columns[1..].iter().find(|c| *c != "figure").expect("three columns, one of them figure");
    numeric_column(&mut rows, value, "a series value")?;
    numeric_column(&mut rows, "figure", "a series' figure")?;
    if rows.iter().all(|r| r["figure"].is_null()) {
        return Err(invalid(format!(
            "series \"{name}\": figure is null in every row, so the figure stands for no row. Set it to the \
             figure's level on the rows it summarizes"
        )));
    }
    let mut out = Map::new();
    out.insert("columns".into(), json!(columns));
    out.insert("row_count".into(), json!(rows.len()));
    out.insert("rows".into(), Value::Array(rows));
    Ok(out)
}

/// lat and lon as numbers in range, the rows outside it left out.
fn coordinates(columns: &[String], rows: &mut Vec<Value>, need: &str) -> Result<(), ToolError> {
    for c in ["lat", "lon"] {
        if !columns.iter().any(|have| have == c) {
            return Err(invalid(format!("{need} (latitude and longitude work too); this query returns {}", columns.join(", "))));
        }
        numeric_column(rows, c, "a coordinate")?;
    }
    rows.retain(|row| {
        matches!((row["lat"].as_f64(), row["lon"].as_f64()),
            (Some(a), Some(o)) if (-90.0..=90.0).contains(&a) && (-180.0..=180.0).contains(&o))
    });
    if rows.is_empty() {
        return Err(invalid("no row has a coordinate in range (lat -90..90, lon -180..180)"));
    }
    Ok(())
}

/// A time as `is_time` accepts it, with a date or a naive time read as UTC.
fn parse_time(s: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    use chrono::{DateTime, NaiveDate, NaiveDateTime};
    let utc = chrono::FixedOffset::east_opt(0)?;
    DateTime::parse_from_rfc3339(s)
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f").ok().map(|t| t.and_utc().with_timezone(&utc)))
        .or_else(|| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|t| t.and_utc().with_timezone(&utc))
        })
}

fn chart_out(columns: &[String], rows: Vec<Value>) -> Map<String, Value> {
    figure_out(Kind::Chart, columns, rows)
}

fn figure_out(kind: Kind, columns: &[String], rows: Vec<Value>) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert("kind".into(), json!(kind.name()));
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

/// Whether some value in the column appears on more than one row.
fn first_repeats(rows: &[Value], column: &str) -> bool {
    let mut seen: Vec<&Value> = Vec::new();
    rows.iter().filter_map(|r| r.get(column)).any(|v| {
        let again = seen.contains(&v);
        seen.push(v);
        again
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

        // The one field the tool offers: a purpose, a mark, or choices.
        let out = call(json!({ "chart": "change_over_time", "sql": weeks })).await.unwrap().data;
        assert_eq!((out["mark"].as_str(), out["purpose"].as_str()), (Some("line"), Some("change_over_time")), "{out}");
        let out = call(json!({ "chart": "share", "sql": days })).await.unwrap().data;
        assert_eq!(out["mark"], json!("share"), "{out}");
        let out = call(json!({ "chart": "choices", "options": ["Yes", "No"] })).await.unwrap().data;
        assert_eq!(out["kind"], json!("choices"), "{out}");

        let out = call(json!({ "kind": "share", "sql": days })).await.unwrap().data;
        assert_eq!((out["kind"].as_str(), out["mark"].as_str()), (Some("chart"), Some("share")), "a mark given as the kind: {out}");

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
        assert_eq!(pick_chart_mark(purpose("relationship"), &none, &cols(&["Week", "Hours"]), &rows).mark, "scatter");
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
    fn chart_routes_to_a_kind_or_a_purpose_and_a_named_field_wins() {
        let s = |v: &str| Some(v.to_string());
        assert_eq!(chart_into(s("choices"), None, None), (None, s("choices")));
        assert_eq!(chart_into(s("usual_range"), None, None), (s("usual_range"), None));
        assert_eq!(chart_into(s("dumbbell"), None, None), (s("dumbbell"), None));
        assert_eq!(chart_into(s("dumbbell"), s("compare_groups"), None), (s("compare_groups"), None));
        assert_eq!(chart_into(None, s("place"), s("map")), (s("place"), s("map")));
    }

    #[test]
    fn a_mark_reads_its_columns_in_any_case() {
        let p = purpose("daily_rhythm");
        let rows = vec![json!({ "Day": "2026-09-01", "Hour": 14, "Pages": 2 })];
        assert_eq!(pick_chart_mark(p, &ChartFields::default(), &cols(&["Day", "Hour", "Pages"]), &rows).mark, "ridgeline");
        let (columns, rows) = mark_columns("ridgeline", cols(&["Day", "Hour", "Pages"]), rows);
        assert_eq!(columns, cols(&["day", "hour", "Pages"]), "the value keeps the owner's label");
        assert!(shape_ridgeline(1200, &columns, rows).is_ok());
        // A bar chart's columns are its labels, and aren't renamed.
        let (columns, _) = mark_columns("bar", cols(&["Day", "Hour"]), vec![]);
        assert_eq!(columns, cols(&["Day", "Hour"]));
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
        // No panel column by name: a weekday repeated across weeks is still small multiples.
        let repeated = vec![
            json!({ "Weekday": "Mon", "Week": "2026-09-07", "Pages": 2 }),
            json!({ "Weekday": "Mon", "Week": "2026-09-14", "Pages": 1 }),
            json!({ "Weekday": "Tue", "Week": "2026-09-07", "Pages": 0 }),
        ];
        assert_eq!(pick_chart_mark(p, &none, &cols(&["Weekday", "Week", "Pages"]), &repeated).mark, "multiples");
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

    #[test]
    fn daily_rhythm_picks_each_mark_by_its_columns() {
        let p = purpose("daily_rhythm");
        let none = ChartFields::default();
        let r = vec![json!({})];
        let pick = |c: &[&str]| pick_chart_mark(p, &none, &cols(c), &r).mark;
        assert_eq!(pick(&["day", "start", "end", "state"]), "states");
        assert_eq!(pick(&["date", "hour", "Steps"]), "ridgeline");
        assert_eq!(pick(&["hour", "days", "share"]), "diurnal");
        assert_eq!(pick(&["night", "asleep_at", "woke_at"]), "actogram");
    }

    #[test]
    fn usual_range_draws_a_dot_plot_without_a_usual_range() {
        let p = purpose("usual_range");
        let none = ChartFields::default();
        let r = vec![json!({ "Night": "2026-09-01", "Bedtime": "23:10" })];
        assert_eq!(pick_chart_mark(p, &none, &cols(&["Night", "Bedtime"]), &r).mark, "dotplot");
        assert_eq!(pick_chart_mark(p, &none, &cols(&["Day", "Rest", "usual_low", "usual_high"]), &r).mark, "baseline");
    }

    #[test]
    fn area_is_drawn_only_when_named() {
        let dated = vec![json!({ "Week": "2026-09-07", "Steps": 4100 })];
        assert_eq!(pick_chart_mark(purpose("change_over_time"), &ChartFields::default(), &cols(&["Week", "Steps"]), &dated).mark, "line");
    }

    #[test]
    fn the_rhythm_shapes_check_their_rows() {
        let ridge = |h: Value, v: Value| json!({ "day": "2026-09-01", "hour": h, "Steps": v });
        assert!(shape_ridgeline(1200, &cols(&["day", "hour", "Steps"]), vec![ridge(json!(6.5), json!("120"))]).is_ok());
        assert!(err(shape_ridgeline(1200, &cols(&["day", "hour", "Steps"]), vec![ridge(json!(25), json!(1))])).contains("0 to 24"));
        assert!(err(shape_ridgeline(1200, &cols(&["day", "hour", "Steps"]), vec![ridge(json!(3), json!("-4"))])).contains("below zero"));
        assert!(shape_ridgeline(1200, &cols(&["day", "hour", "Stage"]), vec![ridge(json!(3), json!("REM"))]).is_ok());

        let band = |h: Value, d: &str, s: Value| json!({ "hour": h, "days": d, "share": s });
        assert!(shape_diurnal(48, &cols(&["hour", "days", "share"]), vec![band(json!(18), "Weekdays", json!(0.8))]).is_ok());
        assert!(err(shape_diurnal(48, &cols(&["hour", "days", "share"]), vec![band(json!(18), "Mondays", json!(0.8))])).contains("weekdays or weekends"));
        assert!(err(shape_diurnal(48, &cols(&["hour", "days", "share"]), vec![band(json!(24), "weekends", json!(0.8))])).contains("0 to 23"));

        let (columns, rows) = mark_columns(
            "actogram",
            cols(&["night", "asleep_at", "woke_at"]),
            vec![json!({ "night": "2026-09-02", "asleep_at": "2026-09-01 23:05:00", "woke_at": "2026-09-02 07:10:00" })],
        );
        let out = shape_actogram(400, &columns, rows).unwrap();
        assert_eq!(out["columns"], json!(["day", "started_at", "ended_at"]));
    }

    #[test]
    fn a_dot_plot_and_a_scatter_need_three_records_and_their_forms() {
        let rec = |v: Value| json!({ "Night": "2026-09-01", "Bedtime": v });
        let three = vec![rec(json!("23:10")), rec(json!("23:40")), rec(json!("0.5"))];
        let out = shape_dotplot(400, &cols(&["Night", "Bedtime"]), three).unwrap();
        assert_eq!(out["rows"][2]["Bedtime"], json!(0.5));
        assert!(err(shape_dotplot(400, &cols(&["Night", "Bedtime"]), vec![rec(json!(1)), rec(json!(2))])).contains("3 to 400"));

        let pair = |a: Value, b: Value| json!({ "Day": "2026-09-01", "Rowing": a, "Sleep": b });
        let pairs = vec![pair(json!(60), json!("7.2")), pair(json!(0), json!(7.9)), pair(json!(90), json!(6.8))];
        assert!(shape_scatter(400, &cols(&["Day", "Rowing", "Sleep"]), pairs).is_ok());
        let times = vec![json!({ "at": "2026-09-01 12:10:00" }), json!({ "at": "2026-09-02 12:20:00" }), json!({ "at": "Lunch" })];
        assert!(err(shape_scatter(400, &cols(&["at"]), times)).contains("one column of times"));
        let mut out = Map::new();
        let long = fields(|f| f.finding = Some("x".repeat(241)));
        assert!(err(check_mark("scatter", &long, &cols(&["at"]), &mut out).map(|_| Map::new())).contains("finding"));
    }
    #[test]
    fn events_and_place_pick_their_marks_from_the_columns_and_fall_back_to_the_plain_one() {
        let none = ChartFields::default();
        let ev = purpose("events");
        let row = vec![json!({})];
        assert_eq!(pick_mark(Kind::Timeline, ev, &none, &cols(&["thread", "at", "what"]), &row).mark, "swimlanes");
        assert_eq!(pick_mark(Kind::Timeline, ev, &none, &cols(&["due", "label", "amount"]), &row).mark, "lollipop");
        assert_eq!(pick_mark(Kind::Timeline, ev, &none, &cols(&["place", "start", "end"]), &row).mark, "itinerary");
        // A visit query's place_name is a timeline's label, as it always was.
        assert_eq!(pick_mark(Kind::Timeline, ev, &none, &cols(&["label", "start", "end"]), &row).mark, "timeline");
        let pl = purpose("place");
        assert_eq!(pick_mark(Kind::Map, pl, &none, &cols(&["lat", "lon", "occurred_at"]), &row).mark, "route");
        assert_eq!(pick_mark(Kind::Map, pl, &none, &cols(&["lat", "lon", "duration_minutes", "label"]), &row).mark, "density");
        assert_eq!(pick_mark(Kind::Map, pl, &none, &cols(&["lat", "lon", "label"]), &row).mark, "map");
        assert_eq!(pick_mark(Kind::Map, None, &none, &cols(&["lat", "lon", "at"]), &row).mark, "map");
        let rows = vec![json!({ "event": "2026-09-01", "offset": -30, "Heart rate": 61 })];
        assert_eq!(pick_chart_mark(purpose("before_and_after"), &none, &cols(&["event", "offset", "Heart rate"]), &rows).mark, "aligned");
    }

    #[test]
    fn an_event_aligned_overlay_needs_its_event_label_and_time_events() {
        let r = |e: &str, o: i64, v: f64| json!({ "event": e, "offset": o, "Heart rate": v });
        let rows = vec![r("2026-09-01 06:00:00", -30, 58.0), r("2026-09-01 06:00:00", 0, 92.0), r("2026-09-03 06:10:00", 0, 95.0)];
        let mut out = shape_aligned(1200, &cols(&["event", "offset", "Heart rate"]), rows.clone()).unwrap();
        assert_eq!(out["kind"], json!("chart"));
        assert!(err(check_mark("aligned", &ChartFields::default(), &cols(&["event", "offset", "Heart rate"]), &mut out).map(|_| Map::new())).contains("event_label"));
        let labelled = fields(|f| f.event_label = Some("Rowing starts".into()));
        check_mark("aligned", &labelled, &cols(&["event", "offset", "Heart rate"]), &mut out).unwrap();
        assert_eq!(out["event_label"], json!("Rowing starts"));
        assert!(err(shape_aligned(1200, &cols(&["event", "Heart rate"]), vec![json!({ "event": "2026-09-01", "Heart rate": 60 })])).contains("offset"));
        let bad = vec![json!({ "event": "Monday", "offset": 0, "Heart rate": 60 })];
        assert!(err(shape_aligned(1200, &cols(&["event", "offset", "Heart rate"]), bad)).contains("time or date"));
    }

    #[test]
    fn a_swimlane_plot_reads_thread_as_lane_and_caps_its_lanes() {
        let (columns, rows) = mark_columns("swimlanes", cols(&["thread", "at", "what"]), vec![json!({ "thread": "Garden", "at": "2026-09-02", "what": "Seeds in" })]);
        let out = shape_swimlanes(400, &columns, rows).unwrap();
        assert_eq!((out["kind"].as_str(), out["columns"][0].as_str()), (Some("timeline"), Some("lane")));
        let many: Vec<Value> = (0..13).map(|i| json!({ "lane": format!("Lane {i}"), "at": "2026-09-02", "what": "x" })).collect();
        assert!(err(shape_swimlanes(400, &cols(&["lane", "at", "what"]), many)).contains("at most 12 lanes"));
        let bad = vec![json!({ "lane": "Garden", "at": "soon", "what": "x" })];
        assert!(err(shape_swimlanes(400, &cols(&["lane", "at", "what"]), bad)).contains("timestamp or a date"));
    }

    #[test]
    fn a_lollipop_timeline_checks_its_dates_amounts_and_window() {
        let rows = vec![json!({ "due": "2026-09-14", "label": "Library card", "amount": null }), json!({ "due": "2026-09-20", "label": "Phone plan", "amount": "35.00" })];
        let mut out = shape_lollipop(40, &cols(&["due", "label", "amount"]), rows).unwrap();
        assert_eq!(out["rows"][1]["amount"], json!(35.0));
        check_mark("lollipop", &fields(|f| f.days = Some(json!(14))), &cols(&["due", "label", "amount"]), &mut out).unwrap();
        assert_eq!(out["days"], json!(14));
        assert!(err(check_mark("lollipop", &fields(|f| f.days = Some(json!(400))), &cols(&["due", "label"]), &mut out).map(|_| Map::new())).contains("7 to 92"));
        let bad = vec![json!({ "due": "next week", "label": "Phone plan" })];
        assert!(err(shape_lollipop(40, &cols(&["due", "label"]), bad)).contains("due must be a date"));
    }

    #[test]
    fn an_itinerary_keeps_its_stays_in_order_with_open_ends_only_at_the_ends() {
        let s = |p: &str, a: Value, b: Value| json!({ "place": p, "started_at": a, "ended_at": b });
        let trip = vec![
            s("Home", Value::Null, json!("2026-06-12")),
            s("Lisbon", json!("2026-06-12"), json!("2026-06-16")),
            s("Porto", json!("2026-06-16"), Value::Null),
        ];
        let out = shape_itinerary(30, &cols(&["place", "started_at", "ended_at"]), trip).unwrap();
        assert_eq!(out["kind"], json!("timeline"));
        let gap = vec![s("Lisbon", json!("2026-06-12"), Value::Null), s("Porto", json!("2026-06-16"), json!("2026-06-18"))];
        assert!(err(shape_itinerary(30, &cols(&["place", "started_at", "ended_at"]), gap)).contains("only the last"));
        let back = vec![s("Lisbon", json!("2026-06-16"), json!("2026-06-18")), s("Porto", json!("2026-06-12"), json!("2026-06-14"))];
        assert!(err(shape_itinerary(30, &cols(&["place", "started_at", "ended_at"]), back)).contains("ORDER BY started_at"));
        let inside_out = vec![s("Lisbon", json!("2026-06-16"), json!("2026-06-12"))];
        assert!(err(shape_itinerary(30, &cols(&["place", "started_at", "ended_at"]), inside_out)).contains("ends before it starts"));
    }

    #[test]
    fn a_route_and_a_density_map_check_their_columns() {
        let p = |lat: f64, at: &str| json!({ "lat": lat, "lon": -97.74, "at": at });
        let walk = vec![p(30.26, "2026-09-01 12:00:00"), p(30.27, "2026-09-01 12:20:00")];
        let out = shape_route(199, &cols(&["lat", "lon", "at"]), walk).unwrap();
        assert_eq!(out["kind"], json!("map"));
        assert!(err(shape_route(199, &cols(&["lat", "lon", "at"]), vec![p(30.26, "2026-09-01 12:00:00")])).contains("2 to 199"));
        let (columns, rows) = mark_columns("density", cols(&["lat", "lon", "duration_minutes"]), vec![
            json!({ "lat": 30.26, "lon": -97.74, "duration_minutes": "120" }),
            json!({ "lat": 30.27, "lon": -97.75, "duration_minutes": null }),
        ]);
        let mut out = shape_density(400, &columns, rows).unwrap();
        assert_eq!((out["row_count"].as_u64(), out["rows"][0]["minutes"].as_f64()), (Some(1), Some(120.0)));
        check_mark("density", &fields(|f| f.period = Some("in September".into())), &columns, &mut out).unwrap();
        assert_eq!(out["period"], json!("in September"));
    }

    #[test]
    fn a_series_is_the_x_axis_a_value_and_its_figure() {
        let r = |d: &str, v: Value, f: Value| json!({ "Night": d, "Hours": v, "figure": f });
        let rows = vec![r("2026-09-01", json!("7.5"), Value::Null), r("2026-09-02", json!(6.9), json!(7.2))];
        let out = shape_series("Average", &cols(&["Night", "Hours", "figure"]), rows).unwrap();
        assert_eq!((out["rows"][0]["Hours"].as_f64(), out["row_count"].as_u64()), (Some(7.5), Some(2)));
        let no_figure = vec![r("2026-09-01", json!(7.5), Value::Null), r("2026-09-02", json!(6.9), Value::Null)];
        assert!(err(shape_series("Average", &cols(&["Night", "Hours", "figure"]), no_figure)).contains("null in every row"));
        let one = vec![r("2026-09-01", json!(7.5), json!(7.5))];
        assert!(err(shape_series("Average", &cols(&["Night", "Hours", "figure"]), one)).contains("at least two rows"));
        let two = vec![json!({ "Night": "2026-09-01", "Hours": 7 }), json!({ "Night": "2026-09-02", "Hours": 7 })];
        assert!(err(shape_series("Average", &cols(&["Night", "Hours"]), two)).contains("and figure"));
    }

    #[sqlx::test]
    async fn numbers_carry_a_series_per_figure_and_spark_needs_one(pool: sqlx::PgPool) {
        let tool = SqlQueryTool::new(std::sync::Arc::new(pool));
        let figure = r#"SELECT 7.2 AS "Average night""#;
        let nights = r#"SELECT d::date AS "Night", h AS "Hours", f AS figure FROM (VALUES ('2026-09-01', 7.5, 7.2), ('2026-09-02', 6.9, 7.2)) v(d, h, f)"#;
        let out = execute(&tool, json!({ "chart": "in_sentence", "sql": figure, "series": { "Average night": nights } }), None, None)
            .await
            .unwrap()
            .data;
        assert_eq!(out["series"]["Average night"]["row_count"], json!(2), "{out}");
        let out = execute(&tool, json!({ "chart": "spark", "sql": figure, "series": { "Average night": nights } }), None, None)
            .await
            .unwrap()
            .data;
        assert_eq!((out["kind"].as_str(), out["mark"].as_str()), (Some("numbers"), Some("spark")), "{out}");
        let Err(ToolError::InvalidParameters(m)) = execute(&tool, json!({ "chart": "spark", "sql": figure }), None, None).await else { panic!() };
        assert!(m.contains("add series"), "{m}");
        let Err(ToolError::InvalidParameters(m)) =
            execute(&tool, json!({ "chart": "in_sentence", "sql": figure, "series": { "Total": nights } }), None, None).await
        else {
            panic!()
        };
        assert!(m.contains("figure columns: Average night"), "{m}");
    }

    #[test]
    fn a_named_mark_draws_with_its_own_kind() {
        assert_eq!(kind_before_query(None, Some("lollipop")).unwrap(), Kind::Timeline);
        assert_eq!(kind_before_query(None, Some("density")).unwrap(), Kind::Map);
        let cap = marks_for_call(Kind::Timeline, purpose("events"), None).iter().map(|m| m.max_rows).max();
        assert_eq!(cap, Some(400), "an events query is asked for enough rows for a swimlane plot");
    }
}
