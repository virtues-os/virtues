//! The marks `show` can draw, and the purposes a model picks them by.
//!
//! One table, read two ways: `show_tool`'s description is built from it, and
//! `show` checks a call's `purpose` and `mark` against it and picks from it.
//! The plan is `agents/plan/chart-purpose-plan.md`.
//!
//! A mark is listed here only once the client can draw it
//! (`apps/web/src/lib/components/chat/show/`). A purpose is offered only
//! while at least one listed mark serves it.

/// What a chart is for, in words a model can match to the owner's question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartPurpose {
    /// The value of `show`'s `purpose` argument.
    pub id: &'static str,
    /// The owner's question it answers, for the tool description.
    pub story: &'static str,
}

/// One thing `show` can draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartMark {
    /// The value of `show`'s `mark` argument, and the mark a result carries.
    pub mark: &'static str,
    /// The chart's name.
    pub name: &'static str,
    /// The `kind` the client draws it with.
    pub kind: &'static str,
    /// The purposes it serves, in the order the picker tries them.
    pub purposes: &'static [&'static str],
    /// One line for the tool description.
    pub line: &'static str,
    /// The most rows it draws. show asks the query for one more, so a result
    /// that would be cut is refused (or, for a table, cut and said so).
    pub max_rows: usize,
}

pub const PURPOSES: &[ChartPurpose] = &[
    ChartPurpose { id: "change_over_time", story: "how has this changed over time?" },
    ChartPurpose { id: "before_and_after", story: "did it change after a dated event?" },
    ChartPurpose { id: "usual_range", story: "is this normal for me, and how much does it vary?" },
    ChartPurpose { id: "compare_groups", story: "how do these groups compare, or then against now?" },
    ChartPurpose { id: "part_to_whole", story: "what is the total made of?" },
    ChartPurpose { id: "relationship", story: "does one measure move with another, or when in the week does it happen?" },
    ChartPurpose { id: "daily_rhythm", story: "when in the day do things usually happen, across many days?" },
    ChartPurpose { id: "events", story: "what happened, item by item?" },
    ChartPurpose { id: "place", story: "where?" },
    ChartPurpose { id: "in_sentence", story: "which one to four figures belong in the sentence?" },
    ChartPurpose { id: "table", story: "which rows should the owner read?" },
];

/// Within a purpose, the picker tries marks in this order.
pub const MARKS: &[ChartMark] = &[
    ChartMark {
        mark: "cumulative",
        name: "Cumulative comparison",
        kind: "chart",
        purposes: &["change_over_time"],
        line: "this period's running total against the last: step, then the two totals; give step_kind and unit",
        max_rows: 31,
    },
    ChartMark {
        mark: "interrupted",
        name: "Interrupted time series",
        kind: "chart",
        purposes: &["before_and_after"],
        line: "x, then one value; give split_at (the date) and split_label",
        max_rows: 199,
    },
    ChartMark {
        mark: "baseline",
        name: "Baseline band",
        kind: "chart",
        purposes: &["usual_range"],
        line: "x, the value, usual_low and usual_high",
        max_rows: 199,
    },
    ChartMark {
        mark: "states",
        name: "State timeline",
        kind: "chart",
        purposes: &["daily_rhythm"],
        line: "day, start, end and state, at most 6 states",
        max_rows: 600,
    },
    ChartMark {
        mark: "ridgeline",
        name: "Ridgeline plot",
        kind: "chart",
        purposes: &["daily_rhythm"],
        line: "day, hour (0-24) or minute, then the value",
        max_rows: 1200,
    },
    ChartMark {
        mark: "diurnal",
        name: "Diurnal profile",
        kind: "chart",
        purposes: &["daily_rhythm"],
        line: "hour (0-23), days (weekdays or weekends) and share (0-1)",
        max_rows: 48,
    },
    ChartMark {
        mark: "actogram",
        name: "Actogram",
        kind: "chart",
        purposes: &["daily_rhythm"],
        line: "a daily window (first to last, asleep to awake) across days: day, started_at, ended_at",
        max_rows: 400,
    },
    ChartMark {
        mark: "dotplot",
        name: "Dot plot",
        kind: "chart",
        purposes: &["usual_range"],
        line: "one value per record, an optional label first",
        max_rows: 400,
    },
    ChartMark {
        mark: "scatter",
        name: "Scatter plot",
        kind: "chart",
        purposes: &["relationship"],
        line: "a date or label, then two numbers, or one column of times; give finding",
        max_rows: 400,
    },
    ChartMark {
        mark: "area",
        name: "Stacked area chart",
        kind: "chart",
        purposes: &["change_over_time"],
        line: "the stacked area chart, a filled area over time with 1-3 series",
        max_rows: 199,
    },
    ChartMark {
        mark: "multiples",
        name: "Small multiples",
        kind: "chart",
        purposes: &["compare_groups"],
        line: "panel, x and value, 2-12 panels",
        max_rows: 400,
    },
    ChartMark {
        mark: "dumbbell",
        name: "Dumbbell plot",
        kind: "chart",
        purposes: &["compare_groups"],
        line: "a category, then two numbers for two moments",
        max_rows: 20,
    },
    ChartMark {
        mark: "list",
        name: "Sorted bar list",
        kind: "chart",
        purposes: &["compare_groups"],
        line: "a label, then a value, largest first; never to rank people",
        max_rows: 20,
    },
    ChartMark {
        mark: "share",
        name: "100% bar",
        kind: "chart",
        purposes: &["part_to_whole"],
        line: "a label, then a value, 2-8 parts of one whole",
        max_rows: 8,
    },
    ChartMark {
        mark: "waterfall",
        name: "Waterfall chart",
        kind: "chart",
        purposes: &["part_to_whole"],
        line: "a label, then an amount that adds or takes away",
        max_rows: 20,
    },
    ChartMark {
        mark: "stack",
        name: "Stacked bar chart",
        kind: "chart",
        purposes: &["part_to_whole"],
        line: "stacked bars, x then 2-5 parts",
        max_rows: 199,
    },
    ChartMark {
        mark: "line",
        name: "Line chart",
        kind: "chart",
        purposes: &["change_over_time"],
        line: "a date or time first, then 1-3 numeric columns",
        max_rows: 199,
    },
    ChartMark {
        mark: "bar",
        name: "Bar chart",
        kind: "chart",
        purposes: &["change_over_time", "compare_groups"],
        line: "a label or period first, then 1-3 numeric columns",
        max_rows: 199,
    },
    ChartMark {
        mark: "timeline",
        name: "Timeline",
        kind: "timeline",
        purposes: &["events"],
        line: "start, optional end, and label",
        max_rows: 199,
    },
    ChartMark {
        mark: "map",
        name: "Map",
        kind: "map",
        purposes: &["place"],
        line: "lat, lon and an optional label",
        max_rows: 199,
    },
    ChartMark {
        mark: "numbers",
        name: "Numbers",
        kind: "numbers",
        purposes: &["in_sentence"],
        line: "one row of 1-4 columns, each a figure",
        max_rows: 1,
    },
    ChartMark {
        mark: "table",
        name: "Table",
        kind: "table",
        purposes: &["table"],
        line: "up to 8 columns; 50 rows show",
        max_rows: 50,
    },
];

pub fn purpose(id: &str) -> Option<&'static ChartPurpose> {
    PURPOSES.iter().find(|p| p.id == id)
}

pub fn mark(name: &str) -> Option<&'static ChartMark> {
    MARKS.iter().find(|m| m.mark == name)
}

/// The marks that serve `purpose`, in the order the picker tries them.
pub fn marks_for(purpose: &str) -> impl Iterator<Item = &'static ChartMark> + '_ {
    MARKS.iter().filter(move |m| m.purposes.contains(&purpose))
}

/// The values `show`'s `chart` argument takes: every purpose, every mark
/// whose name is not already a purpose's, and choices for reply buttons.
pub fn chart_values() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = PURPOSES.iter().map(|p| p.id).collect();
    for m in MARKS {
        if !out.contains(&m.mark) {
            out.push(m.mark);
        }
    }
    out.push("choices");
    out
}

/// The `chart` argument's description: each purpose, its story, and the
/// marks that draw it with the columns each one needs.
pub fn purpose_catalog() -> String {
    let mut out = String::from(
        "Give the purpose whose question the figure answers; the server picks the chart from the \
         columns your query returns. Give a chart's own name only when the owner asked for that chart, \
         or choices for reply buttons (no sql, give options). Purposes, and the columns each chart reads:",
    );
    for p in PURPOSES {
        out.push_str(&format!("\n{}, {}", p.id, p.story));
        for m in marks_for(p.id) {
            out.push_str(&format!(" {}: {}.", m.mark, m.line));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mark_serves_a_listed_purpose_and_every_purpose_has_a_mark() {
        for m in MARKS {
            assert!(!m.purposes.is_empty(), "{} serves no purpose", m.mark);
            for p in m.purposes {
                assert!(purpose(p).is_some(), "{} names unknown purpose {p}", m.mark);
            }
        }
        for p in PURPOSES {
            assert!(marks_for(p.id).next().is_some(), "purpose {} has no mark", p.id);
        }
    }

    /// show.rs settles the kind, and so the query's limit and column names,
    /// before the query runs. A purpose whose marks span kinds would need the
    /// pick moved before that.
    #[test]
    fn a_purpose_draws_with_one_kind() {
        for p in PURPOSES {
            let first = marks_for(p.id).next().map(|m| m.kind);
            assert!(marks_for(p.id).all(|m| Some(m.kind) == first), "purpose {} spans kinds", p.id);
        }
    }

    #[test]
    fn marks_and_purposes_are_unique() {
        for (i, m) in MARKS.iter().enumerate() {
            assert!(!MARKS[..i].iter().any(|o| o.mark == m.mark), "mark {} twice", m.mark);
        }
        for (i, p) in PURPOSES.iter().enumerate() {
            assert!(!PURPOSES[..i].iter().any(|o| o.id == p.id), "purpose {} twice", p.id);
        }
    }

    /// The catalog is read on every turn that offers `show`. About four
    /// characters make a token, so 2,400 characters is about 600 tokens, the
    /// plan's limit with every mark listed.
    #[test]
    fn the_catalog_stays_short() {
        let n = purpose_catalog().chars().count();
        assert!(n <= 2_400, "purpose catalog is {n} characters, over the 2,400 limit");
    }
}
