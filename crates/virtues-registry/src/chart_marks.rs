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
    /// The story the data tells, for the tool description.
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
}

pub const PURPOSES: &[ChartPurpose] = &[
    ChartPurpose { id: "change_over_time", story: "how a measure moves over time" },
    ChartPurpose { id: "compare_groups", story: "one measure across a few groups or categories" },
    ChartPurpose { id: "events", story: "what happened when, item by item" },
    ChartPurpose { id: "place", story: "where things are or happened" },
    ChartPurpose { id: "in_sentence", story: "one to four figures beside the sentence" },
    ChartPurpose { id: "table", story: "rows to read rather than a shape to see" },
];

/// Within a purpose, the picker tries marks in this order.
pub const MARKS: &[ChartMark] = &[
    ChartMark {
        mark: "line",
        name: "Line chart",
        kind: "chart",
        purposes: &["change_over_time"],
        line: "a date or time first, then 1-3 numeric columns",
    },
    ChartMark {
        mark: "bar",
        name: "Bar chart",
        kind: "chart",
        purposes: &["change_over_time", "compare_groups"],
        line: "a label or period first, then 1-3 numeric columns",
    },
    ChartMark {
        mark: "timeline",
        name: "Timeline",
        kind: "timeline",
        purposes: &["events"],
        line: "start, optional end, and label",
    },
    ChartMark {
        mark: "map",
        name: "Map",
        kind: "map",
        purposes: &["place"],
        line: "lat, lon and an optional label",
    },
    ChartMark {
        mark: "numbers",
        name: "Numbers",
        kind: "numbers",
        purposes: &["in_sentence"],
        line: "one row of 1-4 columns, each a figure",
    },
    ChartMark {
        mark: "table",
        name: "Table",
        kind: "table",
        purposes: &["table"],
        line: "up to 8 columns; 50 rows show",
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

/// The `purpose` argument's description: each purpose, its story, and the
/// marks that draw it with the columns each one needs.
pub fn purpose_catalog() -> String {
    let mut out = String::from(
        "What the figure is for; the server picks the chart from the query's columns. Give purpose, \
         or kind.",
    );
    for p in PURPOSES {
        out.push_str(&format!("\n{}: {}.", p.id, p.story));
        for m in marks_for(p.id) {
            out.push_str(&format!(" {} ({}): {}.", m.name, m.mark, m.line));
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
