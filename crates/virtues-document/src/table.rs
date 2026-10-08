//! A table's shape: every row covers the same columns.
//!
//! The browser editor's table plugin (prosemirror-tables, under Tiptap's
//! `Table`) rewrites any table that is not a grid the moment a client opens
//! it: it adds cells to short rows, shortens a span that runs past the table
//! or into another cell, and deletes a table with no columns. The rewrite
//! reaches the shared document with the client's next edit. So a table
//! written here is checked the way the plugin checks one ([`problems`], its
//! `TableMap`): the model's write is refused, and a converted page's table
//! is evened out the way the plugin would ([`even_out`]).

use crate::contract::Contract;
use crate::model::Node;
use serde_json::Value;

/// The most cells a table's grid may hold for its shape to be checked: its
/// width (the columns its widest row covers) times its rows. The check maps
/// every one of them, so this bounds what checking one table costs; a page's
/// table is far smaller. A larger table is named, not mapped.
pub const MAX_TABLE_CELLS: usize = 1 << 20;

/// How the plugin would find a table wrong. Positions are a row's index and
/// a cell's index in its row.
#[derive(Debug, Clone, PartialEq)]
enum Wrong {
    /// A cell's rowspan runs `n` rows past the table's last row.
    Overlong { row: usize, cell: usize, n: i64 },
    /// A cell in `row` covers `n` columns another cell already covers.
    Collision { row: usize, cell: usize, n: i64 },
    /// `row` covers `n` fewer columns than the table is wide.
    Missing { row: usize, n: i64 },
    /// The table has no rows or no columns.
    Empty,
    /// The table's grid holds more than [`MAX_TABLE_CELLS`] cells.
    TooLarge { width: i64, height: usize },
}

fn span(cell: &Node, key: &str) -> i64 {
    cell.attrs.get(key).and_then(Value::as_i64).unwrap_or(1).max(1)
}

/// The table's width as the plugin takes it (`findWidth`): the widest row,
/// counting the columns that cells from rows above cover with their rowspan.
/// One pass: a cell spanning rows adds its columns to each row below it that
/// it reaches, kept as a running sum of what starts and stops at each row.
/// Sums saturate, so a span past any table's size cannot wrap.
fn width(rows: &[Node]) -> i64 {
    // `carried[r]`: columns that cells from rows above start or stop
    // covering at row `r`.
    let mut carried = vec![0i64; rows.len() + 1];
    let mut from_above = 0i64;
    let mut width = 0i64;
    for (r, row) in rows.iter().enumerate() {
        from_above = from_above.saturating_add(carried[r]);
        let mut w = from_above;
        for cell in &row.content {
            let colspan = span(cell, "colspan");
            w = w.saturating_add(colspan);
            let rowspan = span(cell, "rowspan");
            if rowspan > 1 {
                carried[r + 1] = carried[r + 1].saturating_add(colspan);
                let stop = usize::try_from(rowspan)
                    .ok()
                    .and_then(|n| r.checked_add(n))
                    .map_or(rows.len(), |stop| stop.min(rows.len()));
                carried[stop] = carried[stop].saturating_sub(colspan);
            }
        }
        width = width.max(w);
    }
    width
}

/// What is wrong with `table`'s shape, as the plugin's `TableMap` finds it,
/// and the table's width.
fn wrongs(table: &Node) -> (Vec<Wrong>, i64) {
    let rows = &table.content;
    let height = rows.len();
    let table_width = width(rows);
    let mut found = vec![];
    if table_width <= 0 || height == 0 {
        found.push(Wrong::Empty);
        return (found, table_width);
    }
    let cells = usize::try_from(table_width)
        .ok()
        .and_then(|w| w.checked_mul(height))
        .filter(|&cells| cells <= MAX_TABLE_CELLS);
    let Some(cells) = cells else {
        found.push(Wrong::TooLarge {
            width: table_width,
            height,
        });
        return (found, table_width);
    };
    let width = table_width as usize;
    let mut map = vec![false; cells];
    let mut at = 0usize;
    for (r, row) in rows.iter().enumerate() {
        for i in 0..=row.content.len() {
            while at < map.len() && map[at] {
                at += 1;
            }
            let Some(cell) = row.content.get(i) else {
                break;
            };
            let (colspan, rowspan) = (span(cell, "colspan"), span(cell, "rowspan"));
            for h in 0..rowspan {
                if r + h as usize >= height {
                    found.push(Wrong::Overlong {
                        row: r,
                        cell: i,
                        n: rowspan - h,
                    });
                    break;
                }
                let start = at + h as usize * width;
                for w in 0..colspan {
                    match map.get_mut(start + w as usize) {
                        Some(taken) if !*taken => *taken = true,
                        _ => found.push(Wrong::Collision {
                            row: r,
                            cell: i,
                            n: colspan - w,
                        }),
                    }
                }
            }
            at = at.saturating_add(colspan as usize);
        }
        let expected = (r + 1) * width;
        let mut missing = 0;
        while at < expected {
            if !map[at] {
                missing += 1;
            }
            at += 1;
        }
        if missing > 0 {
            found.push(Wrong::Missing { row: r, n: missing });
        }
    }
    (found, table_width)
}

/// What is wrong with `table`'s shape, for a person or the model to read.
/// Empty when every row covers the same columns. A cell that runs into
/// several columns another covers is named once.
pub fn problems(table: &Node) -> Vec<String> {
    let (found, width) = wrongs(table);
    let mut messages: Vec<String> = vec![];
    for w in found {
        let message = match w {
            Wrong::Empty => "a table needs at least one row with a cell".to_string(),
            Wrong::TooLarge { width, height } => format!(
                "a table holds at most {MAX_TABLE_CELLS} cells, counting each column a span covers; \
                 this one is {width} columns by {height} rows"
            ),
            Wrong::Overlong { row, n, .. } => format!(
                "a cell in row {} spans {n} more rows than the table has",
                row + 1
            ),
            Wrong::Collision { row, .. } => format!(
                "a cell in row {} spans into a column another cell covers",
                row + 1
            ),
            Wrong::Missing { row, n } => format!(
                "row {} covers {} of the table's {width} columns; every row covers them all",
                row + 1,
                width - n
            ),
        };
        if messages.last() != Some(&message) {
            messages.push(message);
        }
    }
    messages
}

/// Even out `table` as the plugin would: shorten spans that run past the
/// table or into another cell, and add empty cells to short rows. Returns
/// whether it changed the table; `None` when there is no table left to keep
/// (no rows or no columns), for the caller to drop. A table too large to
/// map ([`MAX_TABLE_CELLS`]) is left as it is.
pub fn even_out(c: &Contract, table: &mut Node) -> Option<bool> {
    let mut changed = false;
    // Each pass mends what the last one found; a span shortened in one can
    // leave a row short, found by the next.
    for _ in 0..table.content.len() + 2 {
        let (found, _) = wrongs(table);
        if found.is_empty() {
            return Some(changed);
        }
        let mut add = vec![0i64; table.content.len()];
        for w in found {
            match w {
                Wrong::Empty => return None,
                // Too large to map, so too large to mend: kept as it is.
                Wrong::TooLarge { .. } => return Some(changed),
                Wrong::Overlong { row, cell, n } => {
                    let attrs = &mut table.content[row].content[cell].attrs;
                    let rowspan = attrs.get("rowspan").and_then(Value::as_i64).unwrap_or(1);
                    attrs.insert("rowspan".into(), Value::from((rowspan - n).max(1)));
                }
                Wrong::Collision { row, cell, n } => {
                    let node = &mut table.content[row].content[cell];
                    let colspan = span(node, "colspan");
                    let rowspan = span(node, "rowspan");
                    node.attrs
                        .insert("colspan".into(), Value::from((colspan - n).max(1)));
                    for r in row..(row + rowspan as usize).min(add.len()) {
                        add[r] += n;
                    }
                }
                Wrong::Missing { row, n } => add[row] += n,
            }
        }
        for (row, n) in table.content.iter_mut().zip(add) {
            let kind = row
                .content
                .first()
                .map(|cell| cell.kind.clone())
                .unwrap_or_else(|| "tableCell".into());
            for _ in 0..n {
                let para = Node::element("paragraph", c.default_attrs("paragraph"), vec![]);
                row.content
                    .push(Node::element(&kind, c.default_attrs(&kind), vec![para]));
            }
        }
        changed = true;
    }
    Some(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::{parse, Mode};
    use serde_json::Map;

    /// The table as written: on the model's path a table that is not a grid
    /// is refused, with the table still built.
    fn table(html: &str) -> Node {
        let o = parse(&Contract::load(), html, "doc", Mode::Strict);
        o.nodes.into_iter().next().unwrap()
    }

    #[test]
    fn a_grid_has_no_problems() {
        for html in [
            "<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>",
            "<table><tr><td colspan=\"2\">wide</td></tr><tr><td rowspan=\"2\">tall</td><td>b</td></tr><tr><td>c</td></tr></table>",
        ] {
            assert_eq!(problems(&table(html)), Vec::<String>::new(), "{html}");
        }
    }

    #[test]
    fn what_the_plugin_would_rewrite_is_named() {
        for (html, want) in [
            (
                "<table><tr><th>A</th><th>B</th></tr><tr><td>1</td></tr></table>",
                "row 2 covers 1 of the table's 2 columns",
            ),
            (
                "<table><tr><td>a</td><td>b</td></tr><tr></tr></table>",
                "row 2 covers 0 of the table's 2 columns",
            ),
            (
                "<table><tr><td rowspan=\"3\">a</td></tr><tr></tr></table>",
                "spans 1 more rows",
            ),
            (
                "<table><tr><td>a</td><td rowspan=\"2\">b</td></tr><tr><td colspan=\"2\">c</td></tr></table>",
                "spans into a column",
            ),
        ] {
            let p = problems(&table(html));
            assert!(p.iter().any(|m| m.contains(want)), "{html}: {p:?}");
        }
    }

    #[test]
    fn evening_out_leaves_a_grid() {
        let c = Contract::load();
        for html in [
            "<table><tr><th>A</th><th>B</th></tr><tr><td>1</td></tr></table>",
            "<table><tr><td>a</td><td>b</td></tr><tr></tr></table>",
            "<table><tr><td rowspan=\"3\">a</td></tr><tr></tr></table>",
            "<table><tr><td rowspan=\"2\">a</td><td>b</td></tr><tr><td colspan=\"2\">c</td></tr></table>",
            "<table><tr><td>a</td><td rowspan=\"2\">b</td></tr><tr><td colspan=\"2\">c</td></tr></table>",
        ] {
            let mut t = table(html);
            assert_eq!(even_out(&c, &mut t), Some(true), "{html}");
            assert_eq!(problems(&t), Vec::<String>::new(), "{html}");
        }
        let mut empty = Node::element("table", Map::new(), vec![Node::element("tableRow", Map::new(), vec![])]);
        assert_eq!(even_out(&c, &mut empty), None);
    }

    fn cell(colspan: i64, rowspan: i64) -> Node {
        let mut attrs = Map::new();
        attrs.insert("colspan".into(), Value::from(colspan));
        attrs.insert("rowspan".into(), Value::from(rowspan));
        Node::element("tableCell", attrs, vec![])
    }

    fn rows(rows: Vec<Vec<Node>>) -> Node {
        let rows = rows
            .into_iter()
            .map(|cells| Node::element("tableRow", Map::new(), cells))
            .collect();
        Node::element("table", Map::new(), rows)
    }

    /// Spans no table could have are named, not mapped: mapping one would
    /// allocate its every cell, and adding them up would overflow.
    #[test]
    fn a_table_too_large_to_map_is_named() {
        let c = Contract::load();
        for mut t in [
            rows(vec![vec![cell(i64::MAX, 1)]]),
            rows(vec![vec![cell(i64::MAX / 2 + 1, 1), cell(i64::MAX / 2 + 1, 1)]]),
            rows(vec![vec![cell(8_000_000_000_000_000, 1)]]),
            rows(vec![vec![cell(1_000, 1); 2_000]]),
        ] {
            let p = problems(&t);
            assert!(p.iter().any(|m| m.contains("at most")), "{p:?}");
            assert_eq!(even_out(&c, &mut t), Some(false));
        }
        // A rowspan past the table is an overlong span, however long.
        let p = problems(&rows(vec![vec![cell(1, i64::MAX)], vec![cell(1, 1)]]));
        assert!(p[0].contains("more rows than the table has"), "{p:?}");
    }

    #[test]
    fn width_counts_what_spans_carry_down() {
        // A tall cell covers its column in the rows below it, and no more.
        let t = rows(vec![
            vec![cell(2, 3), cell(1, 1)],
            vec![cell(1, 1)],
            vec![cell(1, 1)],
            vec![cell(1, 1), cell(1, 1), cell(1, 1)],
        ]);
        assert_eq!(width(&t.content), 3);
        assert_eq!(problems(&t), Vec::<String>::new());
        let short = rows(vec![vec![cell(1, 2), cell(1, 1)], vec![]]);
        assert_eq!(width(&short.content), 2);
        assert_eq!(
            problems(&short),
            ["row 2 covers 1 of the table's 2 columns; every row covers them all"]
        );
    }
}
