//! The marks `show` can draw, and the purposes a model picks them by.
//!
//! `show` checks a call's purpose and mark against this table and picks from
//! it. The model reads the same charts in the charts skill
//! (`applets/charts/SKILL.md`), which it loads before drawing; a test keeps
//! the two naming the same purposes and marks. The plan is
//! `agents/plan/chart-purpose-plan.md`.
//!
//! A mark is listed here only once the client can draw it
//! (`apps/web/src/lib/components/chat/show/`). A purpose is offered only
//! while at least one listed mark serves it.

/// What a chart is for, in words a model can match to the owner's question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartPurpose {
    /// The value of `show`'s `chart` argument that names it.
    pub id: &'static str,
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
    /// The most rows it draws. show asks the query for one more, so a result
    /// that would be cut is refused (or, for a table, cut and said so).
    pub max_rows: usize,
}

pub const PURPOSES: &[ChartPurpose] = &[
    ChartPurpose { id: "change_over_time" },
    ChartPurpose { id: "before_and_after" },
    ChartPurpose { id: "usual_range" },
    ChartPurpose { id: "compare_groups" },
    ChartPurpose { id: "part_to_whole" },
    ChartPurpose { id: "relationship" },
    ChartPurpose { id: "daily_rhythm" },
    ChartPurpose { id: "events" },
    ChartPurpose { id: "place" },
    ChartPurpose { id: "in_sentence" },
    ChartPurpose { id: "table" },
];

/// Within a purpose, the picker tries marks in this order.
pub const MARKS: &[ChartMark] = &[
    ChartMark {
        mark: "cumulative",
        name: "Cumulative comparison",
        kind: "chart",
        purposes: &["change_over_time"],
        max_rows: 31,
    },
    ChartMark {
        mark: "interrupted",
        name: "Interrupted time series",
        kind: "chart",
        purposes: &["before_and_after"],
        max_rows: 199,
    },
    ChartMark {
        mark: "aligned",
        name: "Event-aligned overlay",
        kind: "chart",
        purposes: &["before_and_after"],
        max_rows: 1200,
    },
    ChartMark {
        mark: "baseline",
        name: "Baseline band",
        kind: "chart",
        purposes: &["usual_range"],
        max_rows: 199,
    },
    ChartMark {
        mark: "states",
        name: "State timeline",
        kind: "chart",
        purposes: &["daily_rhythm"],
        max_rows: 600,
    },
    ChartMark {
        mark: "ridgeline",
        name: "Ridgeline plot",
        kind: "chart",
        purposes: &["daily_rhythm"],
        max_rows: 1200,
    },
    ChartMark {
        mark: "diurnal",
        name: "Diurnal profile",
        kind: "chart",
        purposes: &["daily_rhythm"],
        max_rows: 48,
    },
    ChartMark {
        mark: "actogram",
        name: "Actogram",
        kind: "chart",
        purposes: &["daily_rhythm"],
        max_rows: 400,
    },
    ChartMark {
        mark: "dotplot",
        name: "Dot plot",
        kind: "chart",
        purposes: &["usual_range"],
        max_rows: 400,
    },
    ChartMark {
        mark: "scatter",
        name: "Scatter plot",
        kind: "chart",
        purposes: &["relationship"],
        max_rows: 400,
    },
    ChartMark {
        mark: "area",
        name: "Stacked area chart",
        kind: "chart",
        purposes: &["change_over_time"],
        max_rows: 199,
    },
    ChartMark {
        mark: "multiples",
        name: "Small multiples",
        kind: "chart",
        purposes: &["compare_groups"],
        max_rows: 400,
    },
    ChartMark {
        mark: "dumbbell",
        name: "Dumbbell plot",
        kind: "chart",
        purposes: &["compare_groups"],
        max_rows: 20,
    },
    ChartMark {
        mark: "list",
        name: "Sorted bar list",
        kind: "chart",
        purposes: &["compare_groups"],
        max_rows: 20,
    },
    ChartMark {
        mark: "share",
        name: "100% bar",
        kind: "chart",
        purposes: &["part_to_whole"],
        max_rows: 8,
    },
    ChartMark {
        mark: "waterfall",
        name: "Waterfall chart",
        kind: "chart",
        purposes: &["part_to_whole"],
        max_rows: 20,
    },
    ChartMark {
        mark: "stack",
        name: "Stacked bar chart",
        kind: "chart",
        purposes: &["part_to_whole"],
        max_rows: 199,
    },
    ChartMark {
        mark: "line",
        name: "Line chart",
        kind: "chart",
        purposes: &["change_over_time"],
        max_rows: 199,
    },
    ChartMark {
        mark: "bar",
        name: "Bar chart",
        kind: "chart",
        purposes: &["change_over_time", "compare_groups"],
        max_rows: 199,
    },
    ChartMark {
        mark: "swimlanes",
        name: "Swimlane event plot",
        kind: "timeline",
        purposes: &["events"],
        max_rows: 400,
    },
    ChartMark {
        mark: "lollipop",
        name: "Lollipop timeline",
        kind: "timeline",
        purposes: &["events"],
        max_rows: 40,
    },
    ChartMark {
        mark: "itinerary",
        name: "Itinerary strip map",
        kind: "timeline",
        purposes: &["events"],
        max_rows: 30,
    },
    ChartMark {
        mark: "timeline",
        name: "Timeline",
        kind: "timeline",
        purposes: &["events"],
        max_rows: 199,
    },
    ChartMark {
        mark: "route",
        name: "Route map",
        kind: "map",
        purposes: &["place"],
        max_rows: 199,
    },
    ChartMark {
        mark: "density",
        name: "Dot density map",
        kind: "map",
        purposes: &["place"],
        max_rows: 400,
    },
    ChartMark {
        mark: "map",
        name: "Map",
        kind: "map",
        purposes: &["place"],
        max_rows: 199,
    },
    ChartMark {
        mark: "numbers",
        name: "Numbers",
        kind: "numbers",
        purposes: &["in_sentence"],
        max_rows: 1,
    },
    ChartMark {
        mark: "spark",
        name: "Sparkline",
        kind: "numbers",
        purposes: &["in_sentence"],
        max_rows: 1,
    },
    ChartMark {
        mark: "table",
        name: "Table",
        kind: "table",
        purposes: &["table"],
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

    /// The charts skill is what the model reads to pick a chart, so it
    /// names every purpose and every mark this table has, and each of its
    /// references is about a mark here.
    #[test]
    fn the_charts_skill_names_every_purpose_and_mark() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../applets/charts");
        let skill = std::fs::read_to_string(dir.join("SKILL.md")).expect("applets/charts/SKILL.md");
        for id in PURPOSES.iter().map(|p| p.id).chain(MARKS.iter().map(|m| m.mark)) {
            assert!(skill.contains(&format!("`{id}`")), "the charts skill never names `{id}`");
        }
        for entry in std::fs::read_dir(dir.join("references")).expect("applets/charts/references") {
            let name = entry.unwrap().file_name().into_string().unwrap();
            let stem = name.strip_suffix(".md").expect("a reference is markdown");
            assert!(mark(stem).is_some(), "references/{name} is not about a mark in MARKS");
        }
    }
}
