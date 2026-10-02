//! What happened inside the stretches: conversations joined from the
//! recorder's five-minute windows, and walks inside a stay
//! (`dayback/build.py:403-434`).

use super::spine::{Kind, Move, Seg};
use super::{Ms, MIN};

/// A pause longer than this ends a conversation.
const CONVERSATION_PAUSE: Ms = 15 * MIN;
/// A movement inside a stay shorter than this AND shorter than `WALK_MIN_M` is not a moment.
const WALK_MIN: Ms = 10 * MIN;
const WALK_MIN_M: f64 = 500.0;

/// One transcription window.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Window {
    pub id: String,
    pub s: Ms,
    pub e: Ms,
    pub speakers: i32,
    pub confidence: f64,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Conversation {
    pub s: Ms,
    pub e: Ms,
    /// Named by its most confident window, not its first.
    pub title: String,
    /// The windows with two or more speakers it joined, in time order.
    pub window_ids: Vec<String>,
    /// The most speakers any of those windows heard.
    pub speakers: i32,
}

/// Windows (in time order) joined into conversations. Two or more speakers
/// start or extend one - within the same stretch of the spine and under 15
/// minutes after it last ended; one voice (a talk, a monologue) keeps a
/// running one alive but never starts one; silence closes it. Joining by time
/// alone once made one "conversation" of work, the drive and the next stop.
pub(crate) fn conversations(windows: &[Window], segs: &[Seg]) -> Vec<Conversation> {
    struct Open<'a> {
        s: Ms,
        e: Ms,
        seg: Option<usize>,
        wins: Vec<&'a Window>,
    }
    let seg_at = |t: Ms| segs.iter().position(|g| g.s <= t && t < g.e);
    let mut done: Vec<Open> = Vec::new();
    let mut open: Option<Open> = None;
    for w in windows {
        if w.speakers < 2 {
            if w.speakers == 0 {
                done.extend(open.take());
            } else if let Some(o) = open.as_mut().filter(|o| w.s - o.e < CONVERSATION_PAUSE) {
                o.e = w.e;
            }
            continue;
        }
        let seg = seg_at((w.s + w.e) / 2);
        match open.as_mut() {
            Some(o) if w.s - o.e < CONVERSATION_PAUSE && seg == o.seg => {
                o.e = w.e;
                o.wins.push(w);
            }
            _ => {
                done.extend(open.take());
                open = Some(Open { s: w.s, e: w.e, seg, wins: vec![w] });
            }
        }
    }
    done.extend(open);
    done.into_iter()
        .map(|o| {
            let mut best = o.wins[0];
            for w in &o.wins[1..] {
                if w.confidence > best.confidence {
                    best = w;
                }
            }
            Conversation {
                s: o.s,
                e: o.e,
                title: best.title.clone(),
                window_ids: o.wins.iter().map(|w| w.id.clone()).collect(),
                speakers: o.wins.iter().map(|w| w.speakers).max().unwrap_or(0),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Walk {
    pub s: Ms,
    pub e: Ms,
    pub path_m: f64,
    pub kmh: f64,
}

/// Movement lying wholly inside a stay is a moment of that stay, never a
/// stretch of its own: a walk at lunch, a lap of the airport. A stroll to the
/// printer (under 10 minutes and under 0.5 km) is not a moment.
pub(crate) fn walks(moves: &[Move], segs: &[Seg]) -> Vec<Walk> {
    moves
        .iter()
        .filter(|m| segs.iter().any(|g| matches!(g.kind, Kind::Stay(_)) && g.s <= m.s && m.e <= g.e))
        .filter(|m| !(m.e - m.s < WALK_MIN && m.path_m < WALK_MIN_M))
        .map(|m| {
            let hours = (m.e - m.s) as f64 / 3_600_000.0;
            let kmh = if hours > 0.0 { m.path_m / 1000.0 / hours } else { 0.0 };
            Walk { s: m.s, e: m.e, path_m: m.path_m, kmh }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(id: &str, start_min: i64, speakers: i32, confidence: f64) -> Window {
        Window {
            id: id.into(),
            s: start_min * MIN,
            e: (start_min + 5) * MIN,
            speakers,
            confidence,
            title: format!("title {id}"),
        }
    }

    #[test]
    fn a_conversation_is_named_by_its_most_confident_window_and_ends_at_a_new_stay() {
        let segs = vec![
            Seg { s: 0, e: 20 * MIN, kind: Kind::Stay(0) },
            Seg { s: 20 * MIN, e: 60 * MIN, kind: Kind::Stay(1) },
        ];
        let windows = vec![win("a", 0, 2, 0.5), win("b", 5, 3, 0.9), win("c", 10, 1, 0.2), win("d", 20, 2, 0.4)];
        let found = conversations(&windows, &segs);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].title, "title b");
        assert_eq!(found[0].window_ids, vec!["a", "b"]);
        assert_eq!(found[0].speakers, 3);
        assert_eq!(found[0].e, 15 * MIN, "the one-voice window extended it");
        assert_eq!(found[1].window_ids, vec!["d"]);
    }

    #[test]
    fn one_voice_never_starts_a_conversation_and_silence_ends_one() {
        let segs = vec![Seg { s: 0, e: 60 * MIN, kind: Kind::Stay(0) }];
        let windows = vec![win("a", 0, 1, 0.5), win("b", 5, 2, 0.5), win("c", 10, 0, 0.5), win("d", 15, 2, 0.5)];
        let found = conversations(&windows, &segs);
        assert_eq!(found.iter().map(|c| c.window_ids.clone()).collect::<Vec<_>>(), vec![vec!["b"], vec!["d"]]);
    }

    #[test]
    fn a_walk_is_movement_inside_a_stay_long_or_far_enough() {
        let segs = vec![Seg { s: 0, e: 60 * MIN, kind: Kind::Stay(0) }, Seg { s: 60 * MIN, e: 70 * MIN, kind: Kind::Transit }];
        let moves = vec![
            Move { s: 10 * MIN, e: 25 * MIN, path_m: 1200.0, fixes: 10 },
            Move { s: 30 * MIN, e: 35 * MIN, path_m: 200.0, fixes: 4 },
            Move { s: 61 * MIN, e: 69 * MIN, path_m: 3000.0, fixes: 9 },
        ];
        let found = walks(&moves, &segs);
        assert_eq!(found.len(), 1);
        assert!((found[0].kmh - 4.8).abs() < 1e-9);
    }
}
