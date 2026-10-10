//! The day's picture: about once a week, a painting of one place a day's page
//! names, set into the page as a figure.
//!
//! Only what an image model can draw truthfully: a well-known public place the
//! page names (a church, an airport, a stadium, a museum, a city skyline) or a
//! generic setting the page describes (an airport gate, a beach, a rainy
//! street). Never a particular person; people only as a few small figures seen
//! from behind or far away. The style is the owner's, `day_pictures` in the
//! assistant profile's `ui_preferences`: oil when unset, or watercolor,
//! pencil, gouache, or off.
//!
//! 1. **Find** the candidates in code ([`candidates`]): in each section that
//!    holds no picture, the places its own lines veil that read as names and
//!    are no person, home or medical place, and the generic settings its
//!    words cue. None is nothing to paint, with no model call.
//! 2. **Direct** on the Lite slot: read the page, footnotes stripped, and
//!    choose one candidate by its number, with a scene for the painter and a
//!    caption, or nothing. The section, the subject and the time are the
//!    candidate's, never the director's words.
//! 3. **Check** the pick in code ([`check_pick`]): the candidate is on the
//!    list; its section holds no picture yet; the subject is on the page, or
//!    is a generic setting the section describes; no person and no veiled
//!    phrase but the subject reaches the painter or the caption; the scene and
//!    the caption both name the subject. A failed check is asked once more,
//!    told why, and a second failure is nothing to paint.
//! 4. **Paint** on the Image slot: the style's recipe, the subject, the
//!    scene, and the guardrails. Stored as a JPEG, longest side 1400px, in
//!    the media store.
//! 5. **Set** it into the page as a `figure` block at the end of its section,
//!    through the editor's document the way a rewrite writes
//!    ([`set_into_page`]).
//!
//! The owner asks for one from the day page's menu (`paint_day`); otherwise a
//! daily tick ([`spawn`]) paints one when the last week's pages hold none.

use std::collections::HashMap;
use std::sync::LazyLock;

use chrono::{Duration, NaiveDate, NaiveTime, Utc};
use chrono_tz::Tz;
use regex::Regex;
use serde::Deserialize;
use sqlx::PgPool;
use virtues_registry::models::ModelSlot;

use crate::api::day_summary::{self, DayPage};
use crate::api::drive::DriveConfig;
use crate::error::{Error, Result};
use crate::server::yjs::YjsState;
use crate::virtues_api::client::Purpose;
use crate::virtues_api::request::Thinking;

/// The key in `ui_preferences` that holds the owner's choice.
pub const PREFERENCE_KEY: &str = "day_pictures";
/// The description a picture's version carries in History. The weekly gate
/// reads it too: a picture painted this week, on any day, is this week's.
pub const PICTURE_VERSION: &str = "Added a picture";
/// The feature the image call is recorded under in `app_ai_calls`. The
/// weekly gate reads it too: a paid call this week is this week's picture.
const IMAGE_FEATURE: &str = "day_picture";

/// How far back the weekly tick looks, for a picture and for days to paint.
const WEEK_DAYS: i64 = 7;
/// Days one tick may ask the director about.
const MAX_DAYS_ASKED: usize = 3;
const CAPTION_MAX_WORDS: usize = 25;
/// The director's tries at one day: its pick, and one more after a refusal.
const DIRECTOR_ATTEMPTS: usize = 2;
/// The most candidates the director is shown, places first.
const MAX_CANDIDATES: usize = 8;
/// The director is asked for 60; past this the scene is not a scene.
const SCENE_MAX_WORDS: usize = 90;
const SUBJECT_MAX_CHARS: usize = 80;
/// Longest side of the stored picture, in pixels.
const MAX_SIDE: u32 = 1400;
const JPEG_QUALITY: u8 = 82;

// ── Style ────────────────────────────────────────────────────────────────────

/// How the picture is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Oil,
    Watercolor,
    Pencil,
    Gouache,
}

impl Style {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "oil" => Some(Self::Oil),
            "watercolor" => Some(Self::Watercolor),
            "pencil" => Some(Self::Pencil),
            "gouache" => Some(Self::Gouache),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Oil => "oil",
            Self::Watercolor => "watercolor",
            Self::Pencil => "pencil",
            Self::Gouache => "gouache",
        }
    }

    /// The painter's half of the prompt. Each was tried against the Image
    /// slot's model on the same scene before it was kept.
    fn recipe(self) -> &'static str {
        match self {
            Self::Oil => "Traditional oil painting, golden-age, slightly impressionist, in the manner of Joaquin Sorolla's sunlit scenes. Thick impasto, paint laid on with a loaded brush and palette knife, ridges of pigment catching the light, visible canvas weave. Warm but muted, tonal, low-chroma, no acid color.",
            Self::Watercolor => "Loose wet-on-wet watercolor on cold-press paper, granulating pigment, soft blooms, bare white paper at the edges, a limited palette of sepia, Payne's grey, ochre and a little cerulean, editorial and restrained.",
            Self::Pencil => "A graphite pencil drawing in a sketchbook: confident loose lines, soft hatched shading, smudged tones, the paper left bare in the sunlit areas, monochrome grey, like an architect's travel sketch.",
            Self::Gouache => "A modern editorial illustration in flat gouache: simplified shapes, a limited palette of warm ivory, deep navy, muted claret red and soft gold, subtle paper grain, calm and graphic, like a magazine spot illustration.",
        }
    }
}

/// What every picture is told, whatever its style.
const GUARDRAILS: &str = "No text, no lettering, no logos, no signature, no frame. Any people are a few small figures seen from behind or far away, with no faces. Landscape format, 3:2.";

/// The style, then the subject named on its own before the scene, so the
/// painter never has to infer what it is painting from the scene's words.
fn paint_prompt(style: Style, subject: &str, scene: &str) -> String {
    format!("{} Subject: {subject}. {scene} {GUARDRAILS}", style.recipe())
}

/// The owner's choice, from `ui_preferences.day_pictures`: `None` is no
/// pictures. Unset is oil, and so is a value this server does not know.
pub fn chosen_from(value: Option<&str>) -> Option<Style> {
    match value {
        Some(v) if v.trim().eq_ignore_ascii_case("off") => None,
        Some(v) => Some(Style::parse(v).unwrap_or(Style::Oil)),
        None => Some(Style::Oil),
    }
}

/// The owner's choice as the profile holds it.
pub async fn chosen_style(pool: &PgPool) -> Result<Option<Style>> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT ui_preferences->>$1 FROM app_assistant_profile LIMIT 1")
            .bind(PREFERENCE_KEY)
            .fetch_optional(pool)
            .await?
            .flatten();
    Ok(chosen_from(value.as_deref()))
}

// ── Reading the page ─────────────────────────────────────────────────────────

static FOOTNOTE_MARK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\^([a-z]{2}-\d+)\]").expect("static regex"));
static FOOTNOTE_DEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[\^([a-z]{2}-\d+)\]:\s*(.*)$").expect("static regex"));
static LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\([^)\s]+\)").expect("static regex"));
static PERSON_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\(/person/([^)\s]+)\)").expect("static regex"));
static VEILED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"⟦([^⟧]+)⟧").expect("static regex"));
/// A saint's name, as public places carry them ("St. Paul's"): never a
/// person in the record, whoever else shares the name.
static SAINT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?:St\.?|Saint)\s+\p{Lu}[\p{L}'’]*").expect("static regex"));
/// A subject that is somebody's home or a medical place, whatever the page
/// calls it. Over-broad on purpose: refusing a public building with "house"
/// in its name costs a week's picture; painting someone's house costs more.
static PRIVATE_PLACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(home|apartment|condo|bedroom|bathroom|kitchen|living room|backyard|garage|hospital|clinic|doctor|doctor's|dentist|therapist|therapy|pharmacy|urgent care|emergency room|medical|surgery|rehab|hospice)\b|\b(the|a|an|our|my|your|his|her|their)\s+house\b",
    )
    .expect("static regex")
});

/// A setting a picture may show without the page naming a place.
struct Setting {
    /// The candidate's subject, and what the painter is told it paints.
    name: &'static str,
    /// The noun that tells the painter what the setting is: the scene and the
    /// caption carry it or the whole name, or the painter is left to guess
    /// (an "airport gate" with no airport in the scene came back a garden gate).
    key: &'static str,
    /// The words that say the chosen section was there.
    cues: &'static [&'static str],
}

const GENERIC_SETTINGS: &[Setting] = &[
    Setting { name: "an airport gate", key: "airport", cues: &["airport", "flight", "flew", "plane", "boarded", "boarding"] },
    Setting { name: "the view from a plane window", key: "plane window", cues: &["flight", "flew", "plane", "took off", "landed"] },
    Setting { name: "a train platform", key: "train", cues: &["train", "station"] },
    Setting { name: "a beach", key: "beach", cues: &["beach"] },
    Setting { name: "a park", key: "park", cues: &["park"] },
    Setting { name: "a rainy street", key: "street", cues: &["rain", "rained", "raining", "rainy"] },
    Setting { name: "a lakeshore", key: "lake", cues: &["lake", "lakefront", "lakeshore"] },
    Setting { name: "a hiking trail", key: "trail", cues: &["hike", "hiked", "hiking", "trail"] },
];

/// One `##` section of a page.
#[derive(Debug, Clone, PartialEq)]
struct Section {
    /// The heading as a reader sees it.
    heading: String,
    /// The heading as [`heading_key`] compares it.
    key: String,
    /// The heading's line, and the line after the section's last.
    start: usize,
    end: usize,
    has_picture: bool,
    /// The heading and the section's own lines as words, figures left out.
    words: String,
}

/// A line as words: footnote markers, link targets and veil marks removed.
fn plain_line(line: &str) -> String {
    let s = FOOTNOTE_MARK.replace_all(line, "");
    let s = LINK.replace_all(&s, "$1");
    s.replace(['⟦', '⟧'], "")
}

/// Text as it is compared: lowercase, straight quotes, single spaces.
fn fold(s: &str) -> String {
    s.to_lowercase()
        .replace(['’', '‘'], "'")
        .replace(['“', '”'], "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A heading as a pick names it, whatever markup either carries.
fn heading_key(heading: &str) -> String {
    fold(&plain_line(heading.trim().trim_start_matches('#')))
}

/// A field line inside a figure block, as `(key, value)`.
fn figure_field(line: &str) -> Option<(String, &str)> {
    let (k, v) = line.split_once(':')?;
    Some((k.trim().to_ascii_lowercase(), v.trim()))
}

/// The page's `##` sections. The footnote definitions close the body, and a
/// fenced block (a figure, a table of code) is read as one thing.
fn sections(lines: &[&str]) -> Vec<Section> {
    let mut out = Vec::new();
    let mut open: Option<Section> = None;
    // Some(is_figure) while inside a fence.
    let mut fence: Option<bool> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if let Some(is_figure) = fence {
            if t == "```" {
                fence = None;
            } else if is_figure
                && figure_field(t).is_some_and(|(k, v)| k == "kind" && v.eq_ignore_ascii_case("picture"))
            {
                if let Some(s) = open.as_mut() {
                    s.has_picture = true;
                }
            }
            continue;
        }
        if t.starts_with("```") {
            fence = Some(t == crate::api::day_article::FIGURE_FENCE);
            continue;
        }
        let definition = FOOTNOTE_DEF.is_match(t);
        if definition || line.starts_with("# ") || line.starts_with("## ") {
            if let Some(mut s) = open.take() {
                s.end = i;
                out.push(s);
            }
        }
        if definition {
            return out;
        }
        if let Some(h) = line.strip_prefix("## ") {
            let heading = plain_line(h).split_whitespace().collect::<Vec<_>>().join(" ");
            open = Some(Section {
                words: heading.clone(),
                heading,
                key: heading_key(h),
                start: i,
                end: lines.len(),
                has_picture: false,
            });
        } else if let Some(s) = open.as_mut() {
            s.words.push('\n');
            s.words.push_str(&plain_line(line));
        }
    }
    out.extend(open);
    out
}

/// Whether any section of the page holds a picture.
pub(crate) fn holds_picture(md: &str) -> bool {
    let lines: Vec<&str> = md.lines().collect();
    sections(&lines).iter().any(|s| s.has_picture)
}

/// The page's `cx` footnotes by id: each section's time span, "2:52 PM" or
/// "10:37 AM–12:32 PM".
fn cx_spans<'a>(lines: &[&'a str]) -> HashMap<&'a str, &'a str> {
    lines
        .iter()
        .copied()
        .filter_map(|l| FOOTNOTE_DEF.captures(l.trim()))
        .filter_map(|c| {
            let (id, text) = (c.get(1)?.as_str(), c.get(2)?.as_str().trim());
            id.starts_with("cx-").then_some((id, text))
        })
        .collect()
}

/// A heading line's time span, from its `cx` footnote.
fn heading_span<'a>(heading: &str, spans: &HashMap<&str, &'a str>) -> Option<&'a str> {
    FOOTNOTE_MARK
        .captures_iter(heading)
        .find_map(|c| spans.get(c.get(1)?.as_str()).copied())
}

/// When a span starts: "10:37 AM" of "10:37 AM–12:32 PM".
fn span_start(span: &str) -> Option<NaiveTime> {
    page_time(span.split('–').next()?)
}

/// The page as the director reads it: no footnotes, no figures, links as
/// their words, veil marks gone, and each section's time span (its `cx`
/// footnote) on the line under its heading.
fn director_page(md: &str) -> String {
    let lines: Vec<&str> = md.lines().collect();
    let spans = cx_spans(&lines);
    let mut out: Vec<String> = Vec::new();
    let mut fence = false;
    for line in &lines {
        let t = line.trim();
        if fence {
            fence = t != "```";
            continue;
        }
        if t.starts_with("```") {
            fence = true;
            continue;
        }
        if FOOTNOTE_DEF.is_match(t) {
            continue;
        }
        let words = plain_line(line);
        if out.last().is_some_and(|l: &String| l.trim().is_empty()) && words.trim().is_empty() {
            continue;
        }
        if let Some(h) = line.strip_prefix("## ") {
            out.push(words);
            if let Some(span) = heading_span(h, &spans) {
                out.push(format!("({span})"));
            }
            continue;
        }
        out.push(words);
    }
    out.join("\n").trim().to_string()
}

/// What the check needs to know about a page.
#[derive(Debug, Clone, Default)]
pub(crate) struct PageFacts {
    sections: Vec<Section>,
    /// The page as words, folded (see [`fold`]).
    words: String,
    /// The day's people, the owner and everyone the page links, by every name
    /// they go by: a name in a scene or a caption is a person.
    names: Vec<String>,
    /// Every phrase the page veils: the names of people and places, and the
    /// phrases about hard or intimate things.
    veiled: Vec<String>,
}

impl PageFacts {
    /// `names` is the day's people as the record knows them; the names the
    /// page links are added here.
    pub(crate) fn read(md: &str, names: Vec<String>) -> Self {
        let lines: Vec<&str> = md.lines().collect();
        let mut names = names;
        names.extend(PERSON_LINK.captures_iter(md).filter_map(|c| {
            let text = c.get(1)?.as_str().replace(['⟦', '⟧'], "");
            Some(text.trim().to_string())
        }));
        let mut seen = std::collections::HashSet::new();
        names.retain(|n| n.chars().filter(|c| c.is_alphabetic()).count() >= 2 && seen.insert(n.clone()));
        let mut seen = std::collections::HashSet::new();
        let veiled = VEILED
            .captures_iter(md)
            .filter_map(|c| Some(c.get(1)?.as_str().trim().to_string()))
            .filter(|v| !v.is_empty() && seen.insert(fold(v)))
            .collect();
        PageFacts {
            sections: sections(&lines),
            words: fold(&director_page(md)),
            names,
            veiled,
        }
    }
}

/// A name's first word, when it can stand for the person: senders the record
/// keeps as people include "The Quo Team" and "My Bank", and "The" or "My"
/// would read as a name in every scene.
fn first_name(full: &str) -> Option<String> {
    const NOT_NAMES: &[&str] = &["the", "a", "an", "my", "our", "your", "mr", "mrs", "ms", "dr", "team"];
    let first = full.split_whitespace().next()?;
    let bare = first.trim_end_matches('.').to_lowercase();
    (full.split_whitespace().count() > 1 && first.chars().count() > 1 && !NOT_NAMES.contains(&bare.as_str()))
        .then(|| first.to_string())
}

/// Every name the day's people and the owner go by: each name in full, its
/// first word, nicknames and aliases. A deny list: a common word that is
/// also someone's name costs a picture, never a leak.
async fn day_names(pool: &PgPool, date: NaiveDate, md: &str) -> Result<Vec<String>> {
    let linked: Vec<String> = PERSON_LINK
        .captures_iter(md)
        .filter_map(|c| Some(c.get(2)?.as_str().to_string()))
        .collect();
    let (start, end) = crate::timezone::day_window(pool, date).await?;
    let rows: Vec<(String, Option<String>, serde_json::Value)> = sqlx::query_as(
        "SELECT p.name, p.nickname, p.aliases FROM wiki_people p \
         WHERE p.id = ANY($1) \
            OR p.id = (SELECT self_person_id FROM app_user_profile LIMIT 1) \
            OR p.id IN (SELECT entity_id FROM wiki_refs \
                        WHERE entity_type = 'person' AND occurred_at >= $2 AND occurred_at < $3)",
    )
    .bind(&linked)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;
    let mut names = Vec::new();
    for (name, nickname, aliases) in rows {
        let full = crate::api::day_article::clean_name(&name);
        names.extend(first_name(&full));
        names.push(full);
        names.extend(nickname.map(|n| crate::api::day_article::clean_name(&n)));
        if let Some(list) = aliases.as_array() {
            names.extend(list.iter().filter_map(|a| a.as_str()).map(crate::api::day_article::clean_name));
        }
    }
    Ok(names)
}

// ── The candidates ───────────────────────────────────────────────────────────

/// Something a picture may show, found on the page by code. The director
/// only chooses among these; it never names a section or a subject itself.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Candidate {
    /// The section's [`heading_key`].
    section: String,
    /// The section's heading as a reader sees it.
    heading: String,
    /// The section's time span as the page gives it, "10:37 AM–12:32 PM".
    time: Option<String>,
    /// The place as the page names it, or the setting's name.
    subject: String,
    /// The sentence it was found in, as words.
    sentence: String,
    /// A generic setting, not a place the page names.
    setting: bool,
}

/// The words a place's name can hold in lowercase ("Girl & the Goat", "Art
/// Institute of Chicago").
const NAME_SMALL_WORDS: &[&str] = &[
    "the", "of", "and", "at", "on", "in", "by", "for", "a", "an", "de", "del", "la", "le", "du", "da", "di", "van",
    "von",
];

/// Whether a veiled phrase reads as a proper name: a capitalized word, and
/// every other word capitalized, a number, or one of [`NAME_SMALL_WORDS`].
/// The page also veils hard matters ("his sister's surgery", "the clinic"),
/// which read as ordinary words; a phrase that is not plainly a name is left
/// out.
fn reads_as_name(phrase: &str) -> bool {
    let mut capitalized = false;
    for word in phrase.split_whitespace() {
        let Some(first) = word.chars().find(|c| c.is_alphanumeric()) else {
            continue;
        };
        if first.is_uppercase() {
            capitalized = true;
        } else if !first.is_numeric() {
            let bare = word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
            if !NAME_SMALL_WORDS.contains(&bare.as_str()) {
                return false;
            }
        }
    }
    capitalized
}

/// Whether a phrase the page veils may be a candidate: it reads as a name,
/// short enough to be a subject, and is no person and no home or medical
/// place by the same tests [`check_pick`] makes.
fn paintable_place(phrase: &str, facts: &PageFacts) -> bool {
    !phrase.is_empty()
        && phrase.chars().count() <= SUBJECT_MAX_CHARS
        && reads_as_name(phrase)
        && !PRIVATE_PLACE.is_match(phrase)
        && !subject_names_person(phrase, &facts.names)
}

/// Words that end in a period without ending a sentence ("St. Paul's Cathedral").
const ABBREVIATIONS: &[&str] = &["st", "mt", "ft", "mr", "mrs", "ms", "dr", "jr", "sr", "ave", "vs", "no"];

/// A line of words cut into its sentences.
fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut from = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let Some(&(next_at, next)) = chars.peek() else {
            break;
        };
        if !matches!(c, '.' | '!' | '?') || !next.is_whitespace() {
            continue;
        }
        let word = text[from..i].rsplit(|ch: char| !ch.is_alphanumeric()).next().unwrap_or_default();
        if c == '.' && ABBREVIATIONS.contains(&word.to_lowercase().as_str()) {
            continue;
        }
        out.push(text[from..=i].trim());
        from = next_at;
    }
    out.push(text[from..].trim());
    out.retain(|s| !s.is_empty());
    out
}

/// The first sentence of `words` that `holds` says yes to; the whole line
/// when only the line as a whole does (a name split by a period nobody
/// listed); `None` when the line does not.
fn sentence_in(words: &str, holds: impl Fn(&str) -> bool) -> Option<String> {
    if !holds(words) {
        return None;
    }
    let found = sentences(words).into_iter().find(|s| holds(s));
    Some(found.unwrap_or(words.trim()).to_string())
}

/// What the page offers a picture, in page order, places before settings,
/// at most [`MAX_CANDIDATES`]: for each section that holds no picture yet,
/// every phrase its own lines veil that is [`paintable_place`], and every
/// generic setting its own words cue, each once per section with the
/// sentence that holds it. Figures inside a section are not its lines.
pub(crate) fn candidates(md: &str, facts: &PageFacts) -> Vec<Candidate> {
    let lines: Vec<&str> = md.lines().collect();
    let spans = cx_spans(&lines);
    let mut places = Vec::new();
    let mut settings = Vec::new();
    for section in facts.sections.iter().filter(|s| !s.has_picture) {
        let Some(&heading_line) = lines.get(section.start) else {
            continue;
        };
        let time = heading_span(heading_line, &spans).map(str::to_string);
        // The section's lines as written and as words, the heading last: a
        // place its heading names takes its sentence from the prose.
        let mut own: Vec<(&str, String)> = Vec::new();
        let mut fence = false;
        for &line in lines.get(section.start + 1..section.end).unwrap_or_default() {
            let t = line.trim();
            if fence {
                fence = t != "```";
            } else if t.starts_with("```") {
                fence = true;
            } else {
                own.push((line, plain_line(line)));
            }
        }
        own.push((heading_line, section.heading.clone()));
        let candidate = |subject: &str, sentence: String, setting: bool| Candidate {
            section: section.key.clone(),
            heading: section.heading.clone(),
            time: time.clone(),
            subject: subject.to_string(),
            sentence,
            setting,
        };

        let mut seen = std::collections::HashSet::new();
        for (line, words) in &own {
            for c in VEILED.captures_iter(line) {
                let phrase = c[1].trim();
                if !paintable_place(phrase, facts) || !seen.insert(fold(phrase)) {
                    continue;
                }
                let sentence = sentence_in(words, |s| has_phrase(s, phrase, false)).unwrap_or_else(|| words.clone());
                places.push(candidate(phrase, sentence, false));
            }
        }
        for setting in GENERIC_SETTINGS {
            let cued = |s: &str| setting.cues.iter().any(|c| has_phrase(s, c, false));
            // The same words the check reads, so a candidate passes it.
            if !cued(&section.words) {
                continue;
            }
            let sentence = own
                .iter()
                .find_map(|(_, words)| sentence_in(words, cued))
                .unwrap_or_else(|| section.heading.clone());
            settings.push(candidate(setting.name, sentence, true));
        }
    }
    places.extend(settings);
    places.truncate(MAX_CANDIDATES);
    places
}

// ── The director ─────────────────────────────────────────────────────────────

const DIRECTOR_PROMPT: &str = r#"You decide whether one day's page gets a picture, and of what. A painter who knows nothing about this person paints it from your words alone, so choose only what a painter can paint truthfully.

After the page comes a numbered list of candidates: the places the page names and the settings it describes, each with its section, the section's time, and the sentence it comes from. Choose at most one, by its number.

Choose:
- Only a place the person was at that day. Not a place someone mentioned, suggested or planned, in a message or in talk, and not one they only passed on the way. The sentence and its section tell you which.
- A well-known public place (a church, a museum, a stadium, an airport, a station, a bridge) over a setting, when both qualify.
- Never a person, someone's home, a medical place, a workplace, or anything from a hard or private conversation.

The scene is for the painter: the subject, named exactly as the candidate names it, then the light, the weather, the season and the time of day, in at most 60 words. Use the day's date, the section's time and the weather below, and invent nothing else about the day. Name no person, and no place but the subject. People appear only as a few small figures seen from behind or far away, with no faces.

The caption sits under the picture: at most 20 words, plain, naming the subject as the candidate names it and saying only what the page says. Not the section's heading. Name no person.

Answer with JSON only:
{"pick": {"candidate": <number>, "scene": "<the scene>", "caption": "<the caption>"}}
or, when no candidate qualifies:
{"pick": null}
Most pages have nothing to paint. Say so rather than stretch."#;

/// The day, the page, and the candidates numbered from 1, after the page.
fn director_prompt(
    date: NaiveDate,
    high_c: Option<f64>,
    low_c: Option<f64>,
    md: &str,
    candidates: &[Candidate],
) -> String {
    let mut p = format!("<day>\n{}", date.format("%A, %B %-d, %Y"));
    if let (Some(h), Some(l)) = (high_c, low_c) {
        p.push_str(&format!(". High {h:.0}°C, low {l:.0}°C"));
    }
    p.push_str(".\n</day>\n\n");
    p.push_str(&format!("<page>\n{}\n</page>\n\n<candidates>\n", director_page(md)));
    for (i, c) in candidates.iter().enumerate() {
        let kind = if c.setting { "a setting" } else { "a place the page names" };
        let section = match &c.time {
            Some(time) => format!("{}, {time}", c.heading),
            None => c.heading.clone(),
        };
        p.push_str(&format!(
            "{}. {} ({kind})\n   Section: {section}\n   Sentence: {}\n",
            i + 1,
            c.subject,
            c.sentence
        ));
    }
    p.push_str("</candidates>\n");
    p
}

/// What the director chose: a candidate by its number as it was shown, from
/// 1, and the words for it. Everything else comes from the candidate.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub(crate) struct Pick {
    #[serde(default)]
    pub candidate: usize,
    #[serde(default)]
    pub scene: String,
    #[serde(default)]
    pub caption: String,
}

#[derive(Deserialize)]
struct Answer {
    pick: Option<Pick>,
}

/// The director's JSON, from the first `{` to the last `}`. `None` when it
/// does not read; `Some(None)` when it chose nothing.
fn parse_answer(raw: &str) -> Option<Option<Pick>> {
    let s = raw.find('{')?;
    let e = raw.rfind('}')?;
    serde_json::from_str::<Answer>(raw.get(s..=e)?).ok().map(|a| a.pick)
}

// ── The check ────────────────────────────────────────────────────────────────

/// A pick that passed the check, ready for the painter and the page.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Checked {
    /// The section's [`heading_key`].
    pub section: String,
    /// What the painter is told it paints: the place as the page names it,
    /// or the setting's own name.
    pub subject: String,
    pub scene: String,
    /// The subject veiled when it names a place, as the page veils it.
    pub caption: String,
    /// When the section starts, 12-hour, when the page gives it a time.
    pub time: Option<String>,
}

/// One line: single spaces, no backticks (a figure field is one line, and a
/// backtick run could close its fence).
fn one_line(s: &str) -> String {
    s.replace('`', "").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Whether `needle` stands as a phrase of its own in `hay`: not inside a
/// longer word. Quotes are straightened on both sides first.
fn has_phrase(hay: &str, needle: &str, case_sensitive: bool) -> bool {
    let needle = needle.trim().replace(['’', '‘'], "'");
    if needle.is_empty() {
        return false;
    }
    let flags = if case_sensitive { "" } else { "(?i)" };
    let pattern = format!(r"{flags}(?:^|[^\p{{L}}\p{{N}}]){}(?:[^\p{{L}}\p{{N}}]|$)", regex::escape(&needle));
    Regex::new(&pattern).is_ok_and(|re| re.is_match(&hay.replace(['’', '‘'], "'")))
}

/// A pattern for `phrase` in any case, its apostrophes straight or curly.
fn phrase_regex(phrase: &str) -> Option<Regex> {
    let escaped = regex::escape(phrase.trim().replace(['’', '‘'], "'").as_str()).replace('\'', "['’‘]");
    Regex::new(&format!("(?i){escaped}")).ok()
}

/// `text` with every mention of `subject` taken out, so what is left is
/// everything the pick says besides its place.
fn without(text: &str, subject: &str) -> String {
    match phrase_regex(subject) {
        Some(re) => re.replace_all(text, " ").into_owned(),
        None => text.to_string(),
    }
}

/// The generic setting a subject is; `None` when it names a place instead.
fn generic_setting(subject: &str) -> Option<&'static Setting> {
    let bare = |s: &str| -> String {
        let f = fold(s);
        let f = f.trim_end_matches('.');
        ["a ", "an ", "the "]
            .iter()
            .find_map(|a| f.strip_prefix(a))
            .unwrap_or(f)
            .to_string()
    };
    let want = bare(subject);
    GENERIC_SETTINGS.iter().find(|s| bare(s.name) == want)
}

/// Whether `text` names what is painted: the place as the pick names it, or
/// the setting's name or its key noun.
fn names_subject(text: &str, subject: &str, setting: Option<&Setting>) -> bool {
    match setting {
        Some(s) => has_phrase(text, s.name, false) || has_phrase(text, s.key, false),
        None => has_phrase(text, subject, false),
    }
}

/// Whether a subject names a person: any name anyone in the day goes by,
/// a first name alone included ("Lunch with Nick at Girl & the Goat"),
/// since the scene and caption are checked with the subject taken out. A
/// saint's name is not a person ("St. Paul's Cathedral" when someone is
/// called Paul).
fn subject_names_person(subject: &str, names: &[String]) -> bool {
    let subject_folded = fold(subject);
    let unsainted = SAINT.replace_all(subject, " ");
    names
        .iter()
        .any(|n| has_phrase(&unsainted, n, true) || subject_folded == fold(n))
}

/// The candidate a pick chose, by the number the director was shown.
fn chosen<'a>(pick: &Pick, candidates: &'a [Candidate]) -> Option<&'a Candidate> {
    candidates.get(pick.candidate.checked_sub(1)?)
}

/// Code's check on the director's pick of one of `candidates`, all of it
/// again: code found the candidate, and the check still holds it to every
/// rule. `Err` says which rule refused it, for the log and the director's
/// second try.
pub(crate) fn check_pick(
    pick: &Pick,
    candidates: &[Candidate],
    facts: &PageFacts,
) -> std::result::Result<Checked, &'static str> {
    let candidate = chosen(pick, candidates).ok_or("the candidate number is not on the list")?;
    let section = facts
        .sections
        .iter()
        .find(|s| s.key == candidate.section)
        .ok_or("the section is not on the page")?;
    if section.has_picture {
        return Err("the section already holds a picture");
    }

    let subject = one_line(&candidate.subject).replace(['⟦', '⟧'], "");
    let subject = subject.trim_end_matches('.').trim().to_string();
    if subject.is_empty() || subject.chars().count() > SUBJECT_MAX_CHARS {
        return Err("no subject");
    }
    let generic = generic_setting(&subject);
    match generic {
        // The section's own words, not the page's: "boarding" in the morning
        // does not put the afternoon's lunch at an airport gate.
        Some(setting) => {
            if !setting.cues.iter().any(|c| has_phrase(&section.words, c, false)) {
                return Err("the section does not say the day was in that setting");
            }
        }
        None => {
            if !facts.words.contains(&fold(&subject)) {
                return Err("the subject is not on the page");
            }
            if PRIVATE_PLACE.is_match(&subject) {
                return Err("the subject is a home or a medical place");
            }
            if subject_names_person(&subject, &facts.names) {
                return Err("the subject names a person");
            }
        }
    }

    let scene = one_line(&pick.scene).replace(['⟦', '⟧'], "");
    if scene.is_empty() || word_count(&scene) > SCENE_MAX_WORDS {
        return Err("the scene is empty or too long");
    }
    let caption = one_line(&pick.caption).replace(['⟦', '⟧'], "");
    if caption.is_empty() || word_count(&caption) > CAPTION_MAX_WORDS {
        return Err("the caption is empty or too long");
    }

    let subject_folded = fold(&subject);
    for text in [&scene, &caption] {
        let rest = without(text, &subject);
        // A name inside the place's own name, or a saint's, is not a person.
        let unsainted = SAINT.replace_all(&rest, " ");
        if facts.names.iter().any(|n| has_phrase(&unsainted, n, true)) {
            return Err("the scene or caption names a person");
        }
        // A veiled place's proper name (the city the day was in) is setting,
        // and the name checks above already keep people out; what stays out
        // is the veil's other phrases, the hard matters said around.
        if facts
            .veiled
            .iter()
            .filter(|v| !subject_folded.contains(&fold(v)) && !reads_as_name(v))
            .any(|v| has_phrase(&rest, v, false))
        {
            return Err("the scene or caption names something the page veils");
        }
    }
    if !names_subject(&scene, &subject, generic) {
        return Err("the scene does not name the subject");
    }
    if fold(&caption).trim_end_matches('.') == section.key {
        return Err("the caption is only the section's heading");
    }
    if !names_subject(&caption, &subject, generic) {
        return Err("the caption does not name the subject");
    }

    // A named place is veiled in the caption as the page veils it, so the
    // page's "Hide names" hides it under the picture too.
    let caption = match (generic, phrase_regex(&subject)) {
        (None, Some(re)) => re.replace_all(&caption, "⟦$0⟧").into_owned(),
        _ => caption,
    };
    let time = candidate
        .time
        .as_deref()
        .and_then(span_start)
        .map(|t| t.format("%-I:%M %p").to_string());

    Ok(Checked {
        section: section.key.clone(),
        subject: generic.map_or(subject, |s| s.name.to_string()),
        scene,
        caption,
        time,
    })
}

// ── The picture ──────────────────────────────────────────────────────────────

/// The model's image as the page stores it: a JPEG, its longest side at most
/// [`MAX_SIDE`]. CPU work; run it off the async threads.
pub(crate) fn to_jpeg(raw: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(raw)
        .map_err(|e| Error::ExternalApi(format!("the painted image could not be read: {e}")))?;
    let img = if img.width().max(img.height()) > MAX_SIDE {
        img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(&rgb)
        .map_err(|e| Error::Other(format!("could not encode the picture: {e}")))?;
    Ok(out)
}

/// The figure block a picture is set into the page as.
fn figure_block(src: &str, caption: &str, style: Style, time: Option<&str>) -> String {
    let mut fields = vec![
        ("kind", "picture".to_string()),
        ("src", src.to_string()),
        ("caption", caption.to_string()),
        ("style", style.as_str().to_string()),
    ];
    if let Some(t) = time {
        fields.push(("time", t.to_string()));
    }
    crate::api::day_article::figure_block(fields.iter().map(|(k, v)| (*k, v.as_str())))
}

/// The page with `block` at the end of the section keyed `section` (after
/// its last line, before the blank lines and whatever follows), or `None`
/// when the page has no such section or the section already holds a picture.
pub(crate) fn insert_picture(md: &str, section: &str, block: &str) -> Option<String> {
    let lines: Vec<&str> = md.split('\n').collect();
    let s = sections(&lines).into_iter().find(|s| s.key == section)?;
    if s.has_picture {
        return None;
    }
    let mut k = s.end;
    while k > s.start + 1 && lines[k - 1].trim().is_empty() {
        k -= 1;
    }
    let mut out: Vec<&str> = lines[..k].to_vec();
    out.push("");
    out.extend(block.lines());
    let tail = &lines[k..];
    match tail.first() {
        // The page ended on the section's last line.
        None => out.push(""),
        Some(next) if !next.trim().is_empty() => out.push(""),
        Some(_) => {}
    }
    out.extend_from_slice(tail);
    Some(out.join("\n"))
}

/// A picture figure as a page holds it.
struct PictureBlock {
    /// The fence and everything inside it, as the page wrote it.
    text: String,
    src: Option<String>,
    time: Option<NaiveTime>,
}

/// A clock time as a page writes it, "2:52 PM".
fn page_time(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%I:%M %p").ok()
}

/// The page's picture figures, in page order.
fn picture_blocks(md: &str) -> Vec<PictureBlock> {
    let mut out = Vec::new();
    // While inside a fence: its lines, and whether it is a figure.
    let mut fence: Option<(Vec<&str>, bool)> = None;
    for line in md.lines() {
        let t = line.trim();
        match fence.as_mut() {
            Some((lines, is_figure)) => {
                lines.push(line);
                if t != "```" {
                    continue;
                }
                let is_figure = *is_figure;
                let (lines, _) = fence.take().expect("inside a fence");
                if !is_figure {
                    continue;
                }
                let fields: HashMap<String, &str> = lines.iter().filter_map(|l| figure_field(l.trim())).collect();
                if fields.get("kind").is_some_and(|k| k.eq_ignore_ascii_case("picture")) {
                    out.push(PictureBlock {
                        text: lines.join("\n"),
                        src: fields.get("src").map(|s| s.to_string()),
                        time: fields.get("time").and_then(|t| page_time(t)),
                    });
                }
            }
            None if t.starts_with("```") => {
                fence = Some((vec![line], t == crate::api::day_article::FIGURE_FENCE));
            }
            None => {}
        }
    }
    out
}

/// When each section starts: its heading's `[^cx-N]` footnote, "2:52 PM" or
/// "10:37 AM–12:32 PM". `None` for a section the page gives no time.
fn section_starts(lines: &[&str], sections: &[Section]) -> Vec<Option<NaiveTime>> {
    let spans = cx_spans(lines);
    sections
        .iter()
        .map(|s| span_start(heading_span(lines[s.start], &spans)?))
        .collect()
}

/// `draft` with every picture figure `before` holds set in again, so a
/// rewrite never drops a painted picture (the weekly gate would not paint
/// another for days). Each goes at the end of the draft's section whose time
/// holds the picture's: the last timed section starting at or before it,
/// else the first section; on a page with no timed sections, or for a
/// picture with no time, the last section. A section already holding a
/// picture is passed over, and a picture the draft already holds is not
/// set in twice.
pub(crate) fn carry_pictures(before: &str, draft: &str) -> String {
    let mut out = draft.to_string();
    for picture in picture_blocks(before) {
        let held = picture_blocks(&out).iter().any(|p| match (&p.src, &picture.src) {
            (Some(a), Some(b)) => a == b,
            _ => p.text == picture.text,
        });
        if held {
            continue;
        }
        let lines: Vec<&str> = out.split('\n').collect();
        let all = sections(&lines);
        let starts = section_starts(&lines, &all);
        let open: Vec<(&Section, Option<NaiveTime>)> =
            all.iter().zip(starts).filter(|(s, _)| !s.has_picture).collect();
        let timed = open.iter().any(|(_, start)| start.is_some());
        let target = match picture.time {
            Some(at) if timed => open
                .iter()
                .rev()
                .find(|(_, start)| start.is_some_and(|s| s <= at))
                .or(open.first()),
            _ => open.last(),
        };
        let Some((section, _)) = target else {
            continue;
        };
        if let Some(next) = insert_picture(&out, &section.key, &picture.text) {
            out = next;
        }
    }
    out
}

/// Whether the article's edition (`machine_text`) takes the picture too, so a
/// picture never makes a page look edited by its owner: when the page is the
/// edition, or when the page has no edition and nothing says the owner
/// touched it.
pub(crate) fn edition_follows(live: &str, machine_text: Option<&str>, has_your_edits: bool) -> bool {
    match machine_text {
        Some(edition) => live == edition,
        None => !has_your_edits,
    }
}

// ── Painting a day ───────────────────────────────────────────────────────────

/// What [`paint_day`] did. Only `Painted` changed the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaintOutcome {
    /// The picture is on the page. `version` is the one it cut in History,
    /// `None` when cutting it failed.
    Painted { version: Option<i64> },
    /// Nothing on the page passed: no page, the day is not over, the page
    /// offers no candidates, the director chose nothing, or the check
    /// refused its pick.
    NothingToPaint,
    /// Another writer holds the day.
    Busy,
    /// Billing refused a model call.
    Billing,
    Failed(String),
}

impl PaintOutcome {
    /// The code the page keys its copy on; `None` when it painted.
    pub fn reason(&self) -> Option<&'static str> {
        match self {
            Self::Painted { .. } => None,
            Self::NothingToPaint => Some("nothing_to_paint"),
            Self::Busy => Some("busy"),
            Self::Billing => Some("billing"),
            Self::Failed(_) => Some("failed"),
        }
    }
}

/// Paint one past day's page, in `style`, holding the day's lock so it never
/// runs beside a narration or a rewrite. Up to [`DIRECTOR_ATTEMPTS`] director
/// calls and, when a pick passes, one image call.
pub async fn paint_day(
    pool: &PgPool,
    yjs: &YjsState,
    drive: &DriveConfig,
    date: NaiveDate,
    style: Style,
) -> PaintOutcome {
    let lock = match day_summary::try_lock_day(pool, date).await {
        Ok(Some(lock)) => lock,
        Ok(None) => return PaintOutcome::Busy,
        Err(e) => return PaintOutcome::Failed(e.to_string()),
    };
    let outcome = paint_locked(pool, yjs, drive, date, style).await;
    lock.release().await;
    let outcome = outcome.unwrap_or_else(|e| {
        if crate::virtues_api::client::is_payment_refusal(&e) {
            PaintOutcome::Billing
        } else {
            PaintOutcome::Failed(e.to_string())
        }
    });
    match &outcome {
        PaintOutcome::Painted { .. } => tracing::info!(date = %date, style = style.as_str(), "day picture painted"),
        PaintOutcome::Failed(e) => tracing::warn!(date = %date, error = %e, "day picture failed"),
        other => tracing::info!(date = %date, outcome = ?other, "no day picture"),
    }
    outcome
}

async fn paint_locked(
    pool: &PgPool,
    yjs: &YjsState,
    drive: &DriveConfig,
    date: NaiveDate,
    style: Style,
) -> Result<PaintOutcome> {
    if !day_summary::day_is_over(pool, date).await? {
        return Ok(PaintOutcome::NothingToPaint);
    }
    let Some(page) = day_summary::day_page(pool, date).await? else {
        return Ok(PaintOutcome::NothingToPaint);
    };
    let text = yjs.read_text(&page.page_id).await.map_err(Error::Other)?;
    let facts = PageFacts::read(&text, day_names(pool, date, &text).await?);
    let found = candidates(&text, &facts);
    let first = async {
        let weather = crate::api::day_article::day_facts(pool, date).await?;
        Ok::<_, Error>(director_prompt(
            date,
            weather.temperature_high_c,
            weather.temperature_low_c,
            &text,
            &found,
        ))
    };
    let ask = |prompt: String| async move {
        crate::virtues_api::completion::system_completion(
            pool,
            ModelSlot::Lite,
            "day_picture_director",
            DIRECTOR_PROMPT,
            &prompt,
            Thinking::Low,
            0.3,
        )
        .await
    };
    let Some(checked) = choose(date, &facts, &found, first, ask).await? else {
        return Ok(PaintOutcome::NothingToPaint);
    };

    let image = crate::api::image_gen::generate_image_via_gateway(
        pool,
        &paint_prompt(style, &checked.subject, &checked.scene),
        IMAGE_FEATURE,
        Purpose::System,
    )
    .await?;
    let jpeg = tokio::task::spawn_blocking(move || to_jpeg(&image))
        .await
        .map_err(|e| Error::Other(format!("the picture's encoder stopped: {e}")))??;
    let stored = crate::api::media::upload_media(
        pool,
        drive,
        "day-picture.jpg",
        Some("image/jpeg".to_string()),
        axum::body::Bytes::from(jpeg),
    )
    .await?;
    let block = figure_block(&stored.url, &checked.caption, style, checked.time.as_deref());
    set_into_page(pool, yjs, &page, &checked.section, &block).await
}

/// The director's checked choice among `candidates`, or `None`: nothing to
/// paint. `first` builds the director's prompt and `ask` is one director
/// call; with no candidates neither runs.
///
/// A pick that breaks a rule is asked for once more, told which rule: a Lite
/// call costs a fraction of a cent, and one slip (a name in the caption)
/// otherwise costs the day its picture. Code checks the second answer exactly
/// as it checked the first.
async fn choose<F, Fut>(
    date: NaiveDate,
    facts: &PageFacts,
    candidates: &[Candidate],
    first: impl std::future::Future<Output = Result<String>>,
    mut ask: F,
) -> Result<Option<Checked>>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Result<String>>,
{
    if candidates.is_empty() {
        tracing::info!(date = %date, "the page offers nothing to paint");
        return Ok(None);
    }
    let first = first.await?;
    let mut prompt = first.clone();
    for attempt in 0..DIRECTOR_ATTEMPTS {
        let raw = ask(prompt).await?;
        let pick = match parse_answer(&raw) {
            Some(Some(pick)) => pick,
            Some(None) => {
                tracing::info!(date = %date, "the director found nothing to paint");
                return Ok(None);
            }
            None => {
                tracing::warn!(date = %date, "the director's answer did not read - nothing to paint");
                return Ok(None);
            }
        };
        let why = match check_pick(&pick, candidates, facts) {
            Ok(checked) => return Ok(Some(checked)),
            Err(why) => why,
        };
        tracing::info!(date = %date, why, attempt, "the director's pick did not pass");
        // The pick itself can hold names, so only at debug.
        let candidate = chosen(&pick, candidates);
        tracing::debug!(
            date = %date,
            candidate = pick.candidate,
            section = candidate.map_or("", |c| c.heading.as_str()),
            subject = candidate.map_or("", |c| c.subject.as_str()),
            scene = %pick.scene,
            caption = %pick.caption,
            "the refused pick"
        );
        prompt = format!(
            "{first}\n\nYour last answer was refused because {why}. Answer again with that fixed, or with {{\"pick\": null}}."
        );
    }
    Ok(None)
}

/// Set a figure block into the page at the end of its section, through the
/// editor's document, as `day_summary::apply_day_rewrite` writes: the page's
/// write turn, the page read live, a restore point, the change by line, and
/// a version credited to the server. The edition follows only as
/// [`edition_follows`] says. A section that has gone, or that holds a picture
/// by now, is nothing to paint.
pub(crate) async fn set_into_page(
    pool: &PgPool,
    yjs: &YjsState,
    page: &DayPage,
    section: &str,
    block: &str,
) -> Result<PaintOutcome> {
    use crate::server::yjs::TextWriteError;

    let _turn = yjs.write_turn(&page.page_id).await;
    let live = yjs
        .read_text(&page.page_id)
        .await
        .map_err(|e| Error::Other(format!("could not read the page: {e}")))?;
    let Some(next) = insert_picture(&live, section, block) else {
        return Ok(PaintOutcome::NothingToPaint);
    };
    let machine_text: Option<String> =
        sqlx::query_scalar("SELECT machine_text FROM wiki_articles WHERE id = $1")
            .bind(&page.article_id)
            .fetch_one(pool)
            .await?;
    let Some(now) = day_summary::day_page(pool, page.date).await? else {
        return Ok(PaintOutcome::NothingToPaint);
    };
    let follows = edition_follows(&live, machine_text.as_deref(), now.has_your_edits);

    crate::api::pages::cut_restore_point(pool, yjs, &page.page_id).await?;
    let (written, saved) = match yjs.replace_text(&page.page_id, &live, &next).await {
        Ok(written) => (written, true),
        Err(TextWriteError::Stale) => {
            return Ok(PaintOutcome::Failed(
                "the page changed while your server was setting the picture in".into(),
            ))
        }
        // On the page and queued to save, so it is recorded as made.
        Err(TextWriteError::NotSaved { written, error }) => {
            tracing::error!(date = %page.date, error = %error, "the picture is on the page but not saved yet");
            (written, false)
        }
        Err(TextWriteError::Other(e)) => return Ok(PaintOutcome::Failed(e)),
    };

    // The page has changed: nothing below may say it did not.
    let version =
        crate::api::pages::cut_version(pool, &page.page_id, &written, "ai", Some(PICTURE_VERSION)).await;
    if follows {
        if let Err(e) = crate::api::wiki_editor::record_edition(pool, &page.article_id, &written.text).await {
            tracing::error!(date = %page.date, error = %e, "the picture is on the page, but the edition was not updated");
        }
    }
    if !saved {
        tracing::warn!(date = %page.date, "the picture's save is queued");
    }
    Ok(PaintOutcome::Painted { version })
}

// ── Once a week ──────────────────────────────────────────────────────────────

/// Where the week stands for pictures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Week {
    /// A page dated in the last week holds a picture, one was painted in the
    /// last week on any day, or the last week paid for an image call.
    Painted,
    /// The past days of the last week that have a page to paint, most unlike
    /// the usual first.
    Candidates(Vec<NaiveDate>),
}

/// The week before `today` (the owner's own date).
///
/// A day's "unlike the usual" is its most novel event's score, the one the
/// day line draws (`local_novelty_z`, else `novelty_z`); a day with none
/// scored comes after the scored ones. A page whose upkeep the owner turned
/// off is left alone.
pub(crate) async fn this_week(pool: &PgPool, today: NaiveDate) -> Result<Week> {
    let from = today - Duration::days(WEEK_DAYS);
    let pages: Vec<String> = sqlx::query_scalar(
        "SELECT COALESCE(p.content, '') FROM wiki_days d \
         JOIN wiki_articles a ON a.subject_type = 'day' AND a.subject_id = d.id \
         JOIN app_pages p ON p.id = a.page_id \
         WHERE d.date >= $1 AND d.date <= $2",
    )
    .bind(from)
    .bind(today)
    .fetch_all(pool)
    .await?;
    if pages.iter().any(|p| holds_picture(p)) {
        return Ok(Week::Painted);
    }
    let painted_lately: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM app_page_versions \
                        WHERE created_by = 'ai' AND description = $1 \
                          AND created_at > now() - make_interval(days => $2))",
    )
    .bind(PICTURE_VERSION)
    .bind(WEEK_DAYS as i32)
    .fetch_one(pool)
    .await?;
    if painted_lately {
        return Ok(Week::Painted);
    }
    // A paid image call closes the week whether or not its picture reached a
    // page: a failure after it (the encode, the upload, a page that moved)
    // leaves no picture and no version, and would otherwise pay again daily.
    let paid_lately: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM app_ai_calls \
                        WHERE feature = $1 \
                          AND created_at > now() - make_interval(days => $2))",
    )
    .bind(IMAGE_FEATURE)
    .bind(WEEK_DAYS as i32)
    .fetch_one(pool)
    .await?;
    if paid_lately {
        return Ok(Week::Painted);
    }
    let days: Vec<NaiveDate> = sqlx::query_scalar(
        "SELECT d.date FROM wiki_days d \
         JOIN wiki_articles a ON a.subject_type = 'day' AND a.subject_id = d.id \
         JOIN app_pages p ON p.id = a.page_id \
         WHERE d.date >= $1 AND d.date < $2 \
           AND a.maintenance <> 'never' \
           AND NULLIF(TRIM(p.content), '') IS NOT NULL \
         ORDER BY (SELECT max(COALESCE(e.local_novelty_z, e.novelty_z)) FROM wiki_events e \
                   WHERE e.day_id = d.id AND NOT e.user_hidden) DESC NULLS LAST, \
                  d.date DESC",
    )
    .bind(from)
    .bind(today)
    .fetch_all(pool)
    .await?;
    Ok(Week::Candidates(days))
}

/// What one weekly tick did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Weekly {
    /// The owner turned pictures off.
    Off,
    AlreadyPainted,
    Painted(NaiveDate),
    /// No day it asked about had anything to paint.
    NothingPainted { asked: usize },
    /// A day's painting failed or billing refused it; the tick stopped there.
    Stopped { date: NaiveDate, outcome: PaintOutcome },
}

/// The owner's today, in their home zone.
async fn home_today(pool: &PgPool) -> NaiveDate {
    let now = Utc::now();
    let zone = crate::timezone::home_timezone_or_utc(pool, now.date_naive()).await;
    zone.parse::<Tz>()
        .map(|tz| now.with_timezone(&tz).date_naive())
        .unwrap_or_else(|_| now.date_naive())
}

/// One tick: when pictures are on and the week holds none, walk the week's
/// days, most unlike the usual first, until one paints, asking the director
/// about at most [`MAX_DAYS_ASKED`] of them.
pub async fn weekly(pool: &PgPool, yjs: &YjsState, drive: &DriveConfig) -> Result<Weekly> {
    let Some(style) = chosen_style(pool).await? else {
        return Ok(Weekly::Off);
    };
    let days = match this_week(pool, home_today(pool).await).await? {
        Week::Painted => return Ok(Weekly::AlreadyPainted),
        Week::Candidates(days) => days,
    };
    let mut asked = 0;
    for date in days.into_iter().take(MAX_DAYS_ASKED) {
        asked += 1;
        match paint_day(pool, yjs, drive, date, style).await {
            PaintOutcome::Painted { .. } => return Ok(Weekly::Painted(date)),
            PaintOutcome::NothingToPaint | PaintOutcome::Busy => continue,
            outcome => return Ok(Weekly::Stopped { date, outcome }),
        }
    }
    Ok(Weekly::NothingPainted { asked })
}

/// The daily tick. Runs on a box (the installer writes
/// `ENVIRONMENT=production`) or with `VIRTUES_DAY_PICTURES=1`, and nowhere
/// else: each picture is a paid image call, and a dev checkout names its
/// environment several ways or not at all.
pub fn spawn(pool: PgPool, yjs: YjsState, drive: DriveConfig) {
    let production = std::env::var("ENVIRONMENT").is_ok_and(|e| e == "production");
    let forced = std::env::var("VIRTUES_DAY_PICTURES").is_ok_and(|v| v == "1");
    if !production && !forced {
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(if forced { 10 } else { 15 * 60 })).await;
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(24 * 3600));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            match weekly(&pool, &yjs, &drive).await {
                Ok(Weekly::Painted(date)) => tracing::info!(date = %date, "day pictures: painted this week's"),
                Ok(Weekly::NothingPainted { asked }) => {
                    tracing::info!(asked, "day pictures: nothing this week's pages could paint")
                }
                Ok(Weekly::Stopped { date, outcome }) => {
                    tracing::warn!(date = %date, ?outcome, "day pictures: stopped")
                }
                Ok(other) => tracing::debug!(?other, "day pictures: nothing to do"),
                Err(e) => tracing::warn!(error = %e, "day pictures: the weekly tick failed"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "You went to [⟦St. Paul's Cathedral⟧](/place/place_1) with [⟦Nick⟧](/person/person_n), then lunch.[^ev-1]\n\n## Morning at the cathedral[^cx-1]\n\nYou sat near the back of ⟦St. Paul's Cathedral⟧ while it rained.[^ev-2] Afterwards ⟦Nick⟧ talked about ⟦his sister's surgery⟧.[^ev-3]\n\n```figure\nkind: quote\ntext: What a dome.\nwho: You\n```\n\n## The walk home[^cx-2]\n\nYou walked back along the river.[^ev-4]\n\n[^cx-1]: 10:00 AM–12:00 PM\n[^cx-2]: 12:30 PM–1:15 PM\n[^ev-1]: Recording · 10:02 AM\n[^ev-2]: Recording · 10:40 AM\n[^ev-3]: Recording · 11:50 AM\n[^ev-4]: Location · 12:40 PM\n";

    fn facts() -> PageFacts {
        PageFacts::read(PAGE, vec!["David Okafor".into(), "David".into(), "Paul".into()])
    }

    /// A director's pick of `subject` under `heading`, as if code had found
    /// it there in a section starting at 10:40: the list it chose from, and
    /// its answer.
    struct Proposed {
        candidates: Vec<Candidate>,
        pick: Pick,
    }

    fn pick(heading: &str, subject: &str, scene: &str, caption: &str) -> Proposed {
        Proposed {
            candidates: vec![Candidate {
                section: heading_key(heading),
                heading: heading.into(),
                time: Some("10:40 AM–11:00 AM".into()),
                subject: subject.into(),
                sentence: String::new(),
                setting: generic_setting(subject).is_some(),
            }],
            pick: Pick { candidate: 1, scene: scene.into(), caption: caption.into() },
        }
    }

    fn check(p: &Proposed, f: &PageFacts) -> std::result::Result<Checked, &'static str> {
        check_pick(&p.pick, &p.candidates, f)
    }

    const SCENE: &str = "The great domed front of St. Paul's Cathedral on a rainy late morning in autumn, wet stone steps, grey light, a few small figures with umbrellas far off.";

    #[test]
    fn a_public_place_the_page_names_passes() {
        let p = pick("Morning at the cathedral", "St. Paul's Cathedral", SCENE, "A wet morning at St. Paul's Cathedral.");
        let c = check(&p, &facts()).expect("passes");
        assert_eq!(c.section, "morning at the cathedral");
        assert_eq!(c.caption, "A wet morning at ⟦St. Paul's Cathedral⟧.", "the place is veiled as the page veils it");
        assert_eq!(c.time.as_deref(), Some("10:40 AM"));
        assert_eq!(c.subject, "St. Paul's Cathedral");
        // A curly apostrophe is the same place.
        assert!(check(&pick("Morning at the cathedral", "St. Paul’s Cathedral", SCENE, "A wet morning at St. Paul’s Cathedral."), &facts()).is_ok());
    }

    #[test]
    fn a_first_name_inside_the_subject_is_a_person() {
        let names = vec!["Nick".to_string(), "Paul".to_string()];
        assert!(subject_names_person("Lunch with Nick at Girl & the Goat", &names));
        assert!(subject_names_person("Nick's flat", &names));
        assert!(subject_names_person("nick", &names));
        assert!(!subject_names_person("St. Paul's Cathedral", &names), "a saint is not a person");
        assert!(!subject_names_person("Saint Paul Chapel", &names));
        assert!(!subject_names_person("Nickel Plate Depot", &names), "not inside a longer word");

        let mut f = facts();
        f.words.push_str(" lunch with nick at girl & the goat");
        let p = pick("The walk home", "Lunch with Nick at Girl & the Goat", "A busy dining room at noon.", "Lunch at Girl & the Goat.");
        assert_eq!(check(&p, &f), Err("the subject names a person"));
    }

    #[test]
    fn a_name_in_the_scene_is_refused() {
        let p = pick("Morning at the cathedral", "St. Paul's Cathedral", "Nick and a friend on the steps of St. Paul's Cathedral in the rain.", "A wet morning.");
        assert_eq!(check(&p, &facts()), Err("the scene or caption names a person"));
        let p = pick("Morning at the cathedral", "St. Paul's Cathedral", SCENE, "David Okafor at the cathedral.");
        assert_eq!(check(&p, &facts()), Err("the scene or caption names a person"));
    }

    #[test]
    fn a_first_name_inside_the_place_is_not_a_person() {
        // Someone the owner knows is called Paul; the cathedral is still a place.
        let p = pick("Morning at the cathedral", "St. Paul's Cathedral", SCENE, "Rain at St. Paul's Cathedral.");
        assert!(check(&p, &facts()).is_ok());
        // Someone's own place is refused.
        let mut f = facts();
        f.words.push_str(" nick's flat");
        assert_eq!(check(&pick("The walk home", "Nick's flat", "A flat by a river at noon.", "Home."), &f), Err("the subject names a person"));
    }

    #[test]
    fn an_unknown_section_is_refused() {
        let p = pick("Dinner", "St. Paul's Cathedral", SCENE, "A wet morning.");
        assert_eq!(check(&p, &facts()), Err("the section is not on the page"));
    }

    #[test]
    fn a_subject_neither_on_the_page_nor_generic_is_refused() {
        let p = pick("Morning at the cathedral", "Westminster Abbey", "A Gothic abbey in the rain.", "A wet morning.");
        assert_eq!(check(&p, &facts()), Err("the subject is not on the page"));
    }

    #[test]
    fn a_generic_setting_needs_its_section_to_have_been_there() {
        let rainy = pick("Morning at the cathedral", "a rainy street", "A rainy city street at noon, wet paving, grey light.", "A wet street by the cathedral.");
        let c = check(&rainy, &facts()).expect("the section says it rained");
        assert_eq!(c.subject, "a rainy street");
        let beach = pick("The walk home", "A beach", "A long beach at noon.", "A walk on the beach.");
        assert_eq!(check(&beach, &facts()), Err("the section does not say the day was in that setting"));
    }

    /// A day of travel: the boarding is in the morning, the lunch is not.
    const TRAVEL: &str = "A day of travel.[^ev-1]\n\n## Early start[^cx-1]\n\nYou were boarding by eight.[^ev-2]\n\n## Lunch talk[^cx-2]\n\nYou talked over lunch about the week.[^ev-3]\n\n[^cx-1]: 7:40 AM–8:10 AM\n[^cx-2]: 2:52 PM\n[^ev-1]: Location · 7:40 AM\n[^ev-2]: Recording · 7:55 AM\n[^ev-3]: Recording · 2:52 PM\n";

    #[test]
    fn a_settings_cue_in_another_section_does_not_count() {
        let f = PageFacts::read(TRAVEL, Vec::new());
        let gate = |section: &str| pick(section, "an airport gate", "An airport gate in thin morning light, rows of seats, a plane beyond the glass.", "Waiting at the airport gate.");
        assert_eq!(check(&gate("Lunch talk"), &f), Err("the section does not say the day was in that setting"));
        assert!(check(&gate("Early start"), &f).is_ok());
    }

    #[test]
    fn the_scene_names_the_subject_and_the_painter_hears_it_first() {
        let f = PageFacts::read(TRAVEL, Vec::new());
        let p = pick("Early start", "an airport gate", "A gate with tall windows in thin morning light.", "Waiting at the airport gate.");
        assert_eq!(check(&p, &f), Err("the scene does not name the subject"));
        let p = pick("Morning at the cathedral", "St. Paul's Cathedral", "A great dome on a rainy morning.", "Rain at St. Paul's Cathedral.");
        assert_eq!(check(&p, &facts()), Err("the scene does not name the subject"));

        // The setting's key noun is enough, and the painter is told its name.
        let p = pick("Early start", "An airport gate.", "The airport at dawn, a gate with tall windows.", "Waiting at the airport.");
        let c = check(&p, &f).expect("passes");
        assert_eq!(c.subject, "an airport gate");
        let prompt = paint_prompt(Style::Oil, &c.subject, &c.scene);
        let (subject_at, scene_at, guard_at) =
            (prompt.find("Subject: an airport gate.").unwrap(), prompt.find(&c.scene).unwrap(), prompt.find(GUARDRAILS).unwrap());
        assert!(subject_at < scene_at && scene_at < guard_at, "{prompt}");
    }

    #[test]
    fn the_caption_names_the_subject_and_is_not_the_heading() {
        let f = PageFacts::read(TRAVEL, Vec::new());
        let gate = |caption: &str| pick("Early start", "an airport gate", "An airport gate in thin morning light.", caption);
        assert_eq!(check(&gate("Early start."), &f), Err("the caption is only the section's heading"));
        assert_eq!(check(&gate("Up before eight."), &f), Err("the caption does not name the subject"));
        assert!(check(&gate("Boarding at the airport by eight."), &f).is_ok());

        let p = pick("Morning at the cathedral", "St. Paul's Cathedral", SCENE, "Morning at the cathedral");
        assert_eq!(check(&p, &facts()), Err("the caption is only the section's heading"));
        let p = pick("Morning at the cathedral", "St. Paul's Cathedral", SCENE, "A wet morning.");
        assert_eq!(check(&p, &facts()), Err("the caption does not name the subject"));
    }

    #[test]
    fn a_long_caption_is_refused() {
        let long = "word ".repeat(CAPTION_MAX_WORDS + 1);
        let p = pick("Morning at the cathedral", "St. Paul's Cathedral", SCENE, &long);
        assert_eq!(check(&p, &facts()), Err("the caption is empty or too long"));
    }

    #[test]
    fn a_veiled_phrase_other_than_the_subject_is_refused() {
        let p = pick("Morning at the cathedral", "St. Paul's Cathedral", SCENE, "A morning of talk about his sister's surgery.");
        assert_eq!(check(&p, &facts()), Err("the scene or caption names something the page veils"));
    }

    #[test]
    fn a_home_or_a_medical_place_is_refused() {
        let mut f = facts();
        f.words.push_str(" the clinic");
        let p = pick("The walk home", "the clinic", "A brick clinic at noon.", "Noon.");
        assert_eq!(check(&p, &f), Err("the subject is a home or a medical place"));
    }

    #[test]
    fn a_section_holding_a_picture_is_refused() {
        let md = insert_picture(PAGE, "the walk home", &figure_block("/a.jpg", "Rain.", Style::Oil, None)).unwrap();
        let f = PageFacts::read(&md, Vec::new());
        let p = pick("The walk home", "a rainy street", "A rainy street at noon.", "Rain.");
        assert_eq!(check(&p, &f), Err("the section already holds a picture"));
    }

    #[test]
    fn the_picture_closes_its_section() {
        let block = figure_block("/api/drive/files/f_1/download", "A wet morning at ⟦St. Paul's Cathedral⟧.", Style::Watercolor, Some("10:40 AM"));
        assert_eq!(
            block,
            "```figure\nkind: picture\nsrc: /api/drive/files/f_1/download\ncaption: A wet morning at ⟦St. Paul's Cathedral⟧.\nstyle: watercolor\ntime: 10:40 AM\n```"
        );

        // A section followed by another: after its figure, before the next heading.
        let md = insert_picture(PAGE, "morning at the cathedral", &block).expect("inserted");
        assert!(
            md.contains("who: You\n```\n\n```figure\nkind: picture\nsrc: /api/drive/files/f_1/download\n"),
            "{md}"
        );
        assert!(md.contains("time: 10:40 AM\n```\n\n## The walk home[^cx-2]"), "{md}");

        // The last section: before the footnotes.
        let md = insert_picture(PAGE, "the walk home", &block).expect("inserted");
        assert!(md.contains("along the river.[^ev-4]\n\n```figure\nkind: picture\n"), "{md}");
        assert!(md.contains("time: 10:40 AM\n```\n\n[^cx-1]: 10:00 AM"), "{md}");
        assert!(holds_picture(&md));
        assert!(!holds_picture(PAGE));

        // Once is enough, and a section that isn't there takes nothing.
        assert_eq!(insert_picture(&md, "the walk home", &block), None);
        assert_eq!(insert_picture(PAGE, "dinner", &block), None);

        // A page that ends on the section's last line, with no newline.
        let bare = "A day.\n\n## Out\n\nYou went out.";
        assert_eq!(
            insert_picture(bare, "out", "```figure\nkind: picture\n```").unwrap(),
            "A day.\n\n## Out\n\nYou went out.\n\n```figure\nkind: picture\n```\n"
        );
    }

    #[test]
    fn the_edition_takes_the_picture_only_when_the_page_is_the_servers() {
        assert!(edition_follows("page", Some("page"), false));
        assert!(!edition_follows("page, edited", Some("page"), true));
        assert!(!edition_follows("page, edited", Some("page"), false), "the text decides, not the flag");
        assert!(edition_follows("page", None, false), "no edition, nobody's edits");
        assert!(!edition_follows("page", None, true));
    }

    #[test]
    fn the_director_reads_words_and_section_times() {
        let p = director_page(PAGE);
        assert!(p.starts_with("You went to St. Paul's Cathedral with Nick, then lunch."), "{p}");
        assert!(p.contains("## Morning at the cathedral\n(10:00 AM–12:00 PM)\n"), "{p}");
        assert!(!p.contains("[^") && !p.contains("figure") && !p.contains("What a dome") && !p.contains('⟦') && !p.contains("/person/"), "{p}");
        assert!(p.ends_with("You walked back along the river."), "{p}");
    }

    #[test]
    fn the_directors_answer_reads_through_fences() {
        let raw = "```json\n{\"pick\": {\"candidate\": 2, \"scene\": \"x\", \"caption\": \"y\"}}\n```";
        assert_eq!(parse_answer(raw).unwrap().unwrap().candidate, 2);
        assert_eq!(parse_answer("{\"pick\": null}"), Some(None));
        assert_eq!(parse_answer("no"), None);
    }

    #[test]
    fn the_owners_choice() {
        assert_eq!(chosen_from(None), Some(Style::Oil), "unset is oil");
        assert_eq!(chosen_from(Some("off")), None);
        assert_eq!(chosen_from(Some("watercolor")), Some(Style::Watercolor));
        assert_eq!(chosen_from(Some("sepia")), Some(Style::Oil));
    }

    #[test]
    fn the_stored_picture_is_a_bounded_jpeg() {
        let img = image::RgbImage::from_pixel(2100, 1400, image::Rgb([200, 120, 40]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let jpeg = to_jpeg(&png).unwrap();
        assert_eq!(image::guess_format(&jpeg).unwrap(), image::ImageFormat::Jpeg);
        let back = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((back.width(), back.height()), (1400, 933));
    }

    // ── against the database ──

    async fn day_with_page(pool: &PgPool, date: NaiveDate, content: &str) -> DayPage {
        let day = crate::api::wiki_days::get_or_create_day(pool, date).await.unwrap();
        let article = crate::api::wiki_articles::create_article(pool, "day", &day.id, "A day", content)
            .await
            .unwrap();
        crate::api::wiki_editor::record_edition(pool, &article.id, content).await.unwrap();
        day_summary::day_page(pool, date).await.unwrap().expect("the day has its page")
    }

    async fn content(pool: &PgPool, page_id: &str) -> String {
        sqlx::query_scalar("SELECT content FROM app_pages WHERE id = $1")
            .bind(page_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn machine_text(pool: &PgPool, page: &DayPage) -> Option<String> {
        sqlx::query_scalar("SELECT machine_text FROM wiki_articles WHERE id = $1")
            .bind(&page.article_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test]
    async fn a_picture_on_the_servers_page_moves_its_edition(pool: PgPool) {
        let page = day_with_page(&pool, NaiveDate::from_ymd_opt(2026, 9, 10).unwrap(), PAGE).await;
        let yjs = YjsState::new(pool.clone());
        let block = figure_block("/api/drive/files/f_1/download", "Rain.", Style::Oil, None);

        let out = set_into_page(&pool, &yjs, &page, "the walk home", &block).await.unwrap();
        assert!(matches!(out, PaintOutcome::Painted { version: Some(_) }), "{out:?}");
        let now = content(&pool, &page.page_id).await;
        assert!(holds_picture(&now) && now.contains("along the river.[^ev-4]\n\n```figure"), "{now}");
        assert_eq!(machine_text(&pool, &page).await.as_deref(), Some(now.as_str()), "the edition takes the picture");
        let again = day_summary::day_page(&pool, page.date).await.unwrap().unwrap();
        assert!(!again.has_your_edits, "a picture does not read as the owner's edit");
        let described: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM app_page_versions WHERE page_id = $1 AND created_by = 'ai' AND description = $2",
        )
        .bind(&page.page_id)
        .bind(PICTURE_VERSION)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(described, 1);

        // The section has its picture now.
        let out = set_into_page(&pool, &yjs, &page, "the walk home", &block).await.unwrap();
        assert_eq!(out, PaintOutcome::NothingToPaint);
        assert_eq!(content(&pool, &page.page_id).await, now, "and the page is as it was");
    }

    #[sqlx::test]
    async fn a_picture_on_an_edited_page_leaves_the_edition(pool: PgPool) {
        let page = day_with_page(&pool, NaiveDate::from_ymd_opt(2026, 9, 11).unwrap(), PAGE).await;
        let edited = PAGE.replace("then lunch.", "then a long lunch.");
        sqlx::query("UPDATE app_pages SET content = $2 WHERE id = $1")
            .bind(&page.page_id)
            .bind(&edited)
            .execute(&pool)
            .await
            .unwrap();
        let yjs = YjsState::new(pool.clone());
        let block = figure_block("/api/drive/files/f_1/download", "Rain.", Style::Oil, None);

        let out = set_into_page(&pool, &yjs, &page, "morning at the cathedral", &block).await.unwrap();
        assert!(matches!(out, PaintOutcome::Painted { .. }), "{out:?}");
        assert!(content(&pool, &page.page_id).await.contains("a long lunch"), "the owner's words stay");
        assert_eq!(machine_text(&pool, &page).await.as_deref(), Some(PAGE), "the edition stays the server's");
    }

    #[sqlx::test]
    async fn a_picture_this_week_means_nothing_to_do(pool: PgPool) {
        let today = NaiveDate::from_ymd_opt(2026, 9, 20).unwrap();
        let quiet = day_with_page(&pool, today - Duration::days(2), PAGE).await;
        let odd = day_with_page(&pool, today - Duration::days(5), PAGE).await;
        day_with_page(&pool, today - Duration::days(9), PAGE).await; // outside the week
        let kept = day_with_page(&pool, today - Duration::days(3), PAGE).await;
        sqlx::query("UPDATE wiki_articles SET maintenance = 'never' WHERE id = $1")
            .bind(&kept.article_id)
            .execute(&pool)
            .await
            .unwrap();
        // The odd day's most unusual event outranks the quiet day's.
        for (day, z) in [(&odd, 2.5_f64), (&quiet, 0.4)] {
            sqlx::query(
                "INSERT INTO wiki_events (id, day_id, started_at, ended_at, local_novelty_z) \
                 VALUES ($1, $2, now(), now(), $3)",
            )
            .bind(format!("event_{}", day.day_id))
            .bind(&day.day_id)
            .bind(z)
            .execute(&pool)
            .await
            .unwrap();
        }

        assert_eq!(
            this_week(&pool, today).await.unwrap(),
            Week::Candidates(vec![odd.date, quiet.date]),
            "most unlike the usual first; upkeep off and last week left out"
        );

        // A picture on a page dated this week.
        let yjs = YjsState::new(pool.clone());
        let block = figure_block("/api/drive/files/f_1/download", "Rain.", Style::Oil, None);
        set_into_page(&pool, &yjs, &quiet, "the walk home", &block).await.unwrap();
        assert_eq!(this_week(&pool, today).await.unwrap(), Week::Painted);

        // A picture painted in the last week counts whichever day holds it:
        // a week whose pages hold none still has its picture.
        let later = today + Duration::days(8);
        assert_eq!(this_week(&pool, later).await.unwrap(), Week::Painted);
    }

    #[sqlx::test]
    async fn a_paid_image_call_this_week_closes_it(pool: PgPool) {
        let today = NaiveDate::from_ymd_opt(2026, 9, 20).unwrap();
        let day = day_with_page(&pool, today - Duration::days(2), PAGE).await;
        let call = |id: &'static str, feature: &'static str, days_ago: i32| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO app_ai_calls (id, feature, created_at) \
                     VALUES ($1, $2, now() - make_interval(days => $3))",
                )
                .bind(id)
                .bind(feature)
                .bind(days_ago)
                .execute(&pool)
                .await
                .unwrap();
            }
        };
        call("aic_1", "day_picture_director", 0).await;
        call("aic_2", IMAGE_FEATURE, 8).await;
        assert_eq!(
            this_week(&pool, today).await.unwrap(),
            Week::Candidates(vec![day.date]),
            "the director's call and last week's image are not this week's picture"
        );

        // Paid for, though no page holds it and no version records it.
        call("aic_3", IMAGE_FEATURE, 1).await;
        assert_eq!(this_week(&pool, today).await.unwrap(), Week::Painted);
    }

    /// A rewrite's draft of [`PAGE`]: new words, new sections, its own times.
    const DRAFT: &str = "A slower day than it looked.[^ev-1]\n\n## The cathedral[^cx-1]\n\nYou sat at the back while it rained.[^ev-2]\n\n## Lunch by the river[^cx-2]\n\nYou ate outside.[^ev-3]\n\n## Evening[^cx-3]\n\nYou read.[^ev-4]\n\n[^cx-1]: 10:00 AM–12:00 PM\n[^cx-2]: 12:35 PM\n[^cx-3]: 6:00 PM–9:00 PM\n[^ev-1]: Recording · 10:02 AM\n[^ev-2]: Recording · 10:40 AM\n[^ev-3]: Location · 12:40 PM\n[^ev-4]: Location · 7:00 PM\n";

    /// [`PAGE`] with a picture painted at `time`, and the picture's block.
    fn painted(time: &str) -> (String, String) {
        let block = figure_block("/api/drive/files/f_1/download", "Rain.", Style::Oil, Some(time));
        (insert_picture(PAGE, "the walk home", &block).unwrap(), block)
    }

    #[test]
    fn a_rewrite_keeps_the_picture_in_the_section_of_its_time() {
        let (before, block) = painted("12:40 PM");
        let out = carry_pictures(&before, DRAFT);
        assert!(out.contains(&format!("You ate outside.[^ev-3]\n\n{block}\n\n## Evening[^cx-3]")), "{out}");
        assert_eq!(out.matches("kind: picture").count(), 1, "{out}");

        // Earlier than every section: the first.
        let (before, block) = painted("9:05 AM");
        let out = carry_pictures(&before, DRAFT);
        assert!(out.contains(&format!("while it rained.[^ev-2]\n\n{block}\n\n## Lunch by the river")), "{out}");

        // At the last section's start: that section, before the footnotes.
        let (before, block) = painted("6:00 PM");
        let out = carry_pictures(&before, DRAFT);
        assert!(out.contains(&format!("You read.[^ev-4]\n\n{block}\n\n[^cx-1]:")), "{out}");
    }

    #[test]
    fn a_page_with_no_times_takes_the_picture_last() {
        let (before, block) = painted("10:40 AM");
        let draft = "A day.\n\n## Out\n\nYou went out.\n\n## Back\n\nYou came back.\n";
        assert_eq!(
            carry_pictures(&before, draft),
            format!("A day.\n\n## Out\n\nYou went out.\n\n## Back\n\nYou came back.\n\n{block}\n")
        );
    }

    #[test]
    fn a_picture_the_draft_holds_is_not_set_twice() {
        let (before, block) = painted("12:40 PM");
        let once = carry_pictures(&before, DRAFT);
        assert_eq!(carry_pictures(&before, &once), once);
        // The draft kept it, in another section: still once.
        let kept = insert_picture(DRAFT, "evening", &block).unwrap();
        assert_eq!(carry_pictures(&before, &kept), kept);
    }

    #[test]
    fn a_page_without_a_picture_leaves_the_draft_alone() {
        assert_eq!(carry_pictures(PAGE, DRAFT), DRAFT);
    }

    /// Mass, then brunch. The brunch section veils a restaurant someone only
    /// suggested; code cannot tell it from a place the owner was, and the
    /// director is told to.
    const SUNDAY: &str = "Mass, then a long brunch.[^ev-1]\n\n## Leaving Mass[^cx-1]\n\nYou walked out of ⟦St. Paul's Cathedral⟧ into the rain with [⟦Nick⟧](/person/person_n).[^ev-2] He was on his way to ⟦Lakeview Clinic⟧ about ⟦his knee surgery⟧.[^ev-3]\n\n## Brunch at ⟦the Blue Heron⟧[^cx-2]\n\n⟦David⟧ suggested ⟦Osteria Nova⟧ for Friday in the group chat.[^ev-4] You ate at ⟦the Blue Heron⟧ by the window.[^ev-5]\n\n```figure\nkind: quote\ntext: Best eggs in ⟦Boston⟧.\nwho: You\n```\n\n[^cx-1]: 11:05 AM–11:40 AM\n[^cx-2]: 12:10 PM\n[^ev-1]: Recording · 11:05 AM\n[^ev-2]: Recording · 11:10 AM\n[^ev-3]: Recording · 11:20 AM\n[^ev-4]: Message · 12:15 PM\n[^ev-5]: Recording · 12:30 PM\n";

    /// Nothing to paint: a person, and a hard matter.
    const QUIET: &str = "A quiet day.[^ev-1]\n\n## Reading[^cx-1]\n\nYou read with [⟦Nick⟧](/person/person_n) and talked about ⟦her new job⟧.[^ev-2]\n\n[^cx-1]: 2:00 PM\n[^ev-1]: Recording · 2:00 PM\n[^ev-2]: Recording · 2:10 PM\n";

    fn sunday() -> PageFacts {
        PageFacts::read(SUNDAY, vec!["David Okafor".into(), "David".into()])
    }

    fn sunday_date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 4).unwrap()
    }

    const MASS_SCENE: &str = "The stone front of St. Paul's Cathedral on a cold rainy late morning in autumn, wet steps, grey light, a few small figures with umbrellas far off.";

    #[test]
    fn the_candidates_are_each_sections_places_then_its_settings() {
        let f = sunday();
        let found = candidates(SUNDAY, &f);
        let listed: Vec<(&str, &str, bool)> =
            found.iter().map(|c| (c.heading.as_str(), c.subject.as_str(), c.setting)).collect();
        // Not Nick or David (people), Lakeview Clinic (medical), the knee
        // surgery (a hard matter), Chicago (inside a figure), or the Blue
        // Heron twice (heading and prose).
        assert_eq!(
            listed,
            vec![
                ("Leaving Mass", "St. Paul's Cathedral", false),
                ("Brunch at the Blue Heron", "Osteria Nova", false),
                ("Brunch at the Blue Heron", "the Blue Heron", false),
                ("Leaving Mass", "a rainy street", true),
            ]
        );
        let mass = "You walked out of St. Paul's Cathedral into the rain with Nick.";
        assert_eq!((found[0].time.as_deref(), found[0].sentence.as_str()), (Some("11:05 AM–11:40 AM"), mass));
        assert_eq!(found[2].sentence, "You ate at the Blue Heron by the window.");
        assert_eq!(found[3].sentence, mass, "the sentence with the cue");

        // Listed after the page, numbered as the answer names them.
        let prompt = director_prompt(sunday_date(), Some(12.0), Some(6.0), SUNDAY, &found);
        let listed_at = prompt
            .find(&format!("1. St. Paul's Cathedral (a place the page names)\n   Section: Leaving Mass, 11:05 AM–11:40 AM\n   Sentence: {mass}\n"))
            .expect(&prompt);
        assert!(prompt.find("</page>").unwrap() < listed_at, "{prompt}");
        assert!(prompt.contains("4. a rainy street (a setting)\n"), "{prompt}");

        // The obvious pick passes, its section, subject and time the candidate's.
        let p = Pick { candidate: 1, scene: MASS_SCENE.into(), caption: "Out of St. Paul's Cathedral into the rain.".into() };
        let c = check_pick(&p, &found, &f).expect("passes");
        assert_eq!((c.section.as_str(), c.subject.as_str(), c.time.as_deref()), ("leaving mass", "St. Paul's Cathedral", Some("11:05 AM")));
        assert_eq!(c.caption, "Out of ⟦St. Paul's Cathedral⟧ into the rain.");
    }

    #[test]
    fn a_section_holding_a_picture_offers_nothing() {
        let md = insert_picture(SUNDAY, "leaving mass", &figure_block("/a.jpg", "Rain.", Style::Oil, None)).unwrap();
        let found = candidates(&md, &PageFacts::read(&md, Vec::new()));
        assert!(!found.is_empty() && found.iter().all(|c| c.section != "leaving mass"), "{found:?}");
    }

    #[test]
    fn places_come_first_and_the_list_is_capped() {
        let places: Vec<String> = (1..=10).map(|n| format!("⟦Pier {n}⟧")).collect();
        let md = format!("A day.\n\n## Out in the rain\n\nYou walked past {}.\n", places.join(", "));
        let found = candidates(&md, &PageFacts::read(&md, Vec::new()));
        assert_eq!(found.len(), MAX_CANDIDATES);
        assert!(found.iter().all(|c| !c.setting), "the setting is the first left off");
    }

    #[test]
    fn a_hard_phrase_does_not_read_as_a_name() {
        for name in ["St. Paul's Cathedral", "the Blue Heron", "Girl & the Goat", "Art Institute of Chicago", "O’Hare", "Pier 39"] {
            assert!(reads_as_name(name), "{name}");
        }
        for phrase in ["his knee surgery", "the clinic", "Her divorce", "the", "a hard talk about money"] {
            assert!(!reads_as_name(phrase), "{phrase}");
        }
    }

    #[tokio::test]
    async fn a_page_with_no_candidates_asks_nothing() {
        let f = PageFacts::read(QUIET, Vec::new());
        let found = candidates(QUIET, &f);
        assert!(found.is_empty(), "{found:?}");
        let (mut built, mut calls) = (false, 0);
        let first = async {
            built = true;
            Ok::<_, Error>(String::new())
        };
        let ask = |_: String| {
            calls += 1;
            async { Ok::<_, Error>(String::new()) }
        };
        assert_eq!(choose(sunday_date(), &f, &found, first, ask).await.unwrap(), None);
        assert!(!built && calls == 0, "no prompt and no call");
    }

    #[tokio::test]
    async fn the_director_chooses_by_number() {
        let f = sunday();
        let found = candidates(SUNDAY, &f);
        let mut asked = Vec::new();
        let answer = format!(r#"{{"pick": {{"candidate": 1, "scene": "{MASS_SCENE}", "caption": "Out of St. Paul's Cathedral into the rain."}}}}"#);
        let ask = |prompt: String| {
            asked.push(prompt);
            let answer = answer.clone();
            async move { Ok::<_, Error>(answer) }
        };
        let first = async { Ok::<_, Error>("the page".to_string()) };
        let c = choose(sunday_date(), &f, &found, first, ask).await.unwrap().expect("painted");
        assert_eq!((c.subject.as_str(), asked.len()), ("St. Paul's Cathedral", 1));
    }

    #[tokio::test]
    async fn a_number_off_the_list_is_nothing_to_paint() {
        let f = sunday();
        let found = candidates(SUNDAY, &f);
        for n in [0, found.len() + 1] {
            let p = Pick { candidate: n, scene: MASS_SCENE.into(), caption: "St. Paul's Cathedral.".into() };
            assert_eq!(check_pick(&p, &found, &f), Err("the candidate number is not on the list"));
        }

        let mut asked = Vec::new();
        let ask = |prompt: String| {
            asked.push(prompt);
            async { Ok::<_, Error>(r#"{"pick": {"candidate": 9, "scene": "St. Paul's Cathedral in the rain.", "caption": "St. Paul's Cathedral."}}"#.to_string()) }
        };
        let first = async { Ok::<_, Error>("the page".to_string()) };
        assert_eq!(choose(sunday_date(), &f, &found, first, ask).await.unwrap(), None);
        assert_eq!(asked.len(), DIRECTOR_ATTEMPTS, "asked once more, told why");
        assert!(asked[1].ends_with("refused because the candidate number is not on the list. Answer again with that fixed, or with {\"pick\": null}."), "{}", asked[1]);
    }

    #[test]
    fn a_senders_article_is_not_a_first_name() {
        assert_eq!(first_name("Nick Perez").as_deref(), Some("Nick"));
        assert_eq!(first_name("The Quo Team"), None);
        assert_eq!(first_name("My Bank"), None);
        // A one-word name is pushed whole; its first word adds nothing.
        assert_eq!(first_name("Nick"), None);
    }


    #[test]
    fn a_veiled_city_may_set_the_scene() {
        let mut f = facts();
        f.veiled.push("Chicago".into());
        let scene = "A clear morning in Chicago, sun on the front of St. Paul's Cathedral, a few small figures far off.";
        let p = pick("Morning at the cathedral", "St. Paul's Cathedral", scene, "St. Paul's Cathedral on a clear morning.");
        assert!(check(&p, &f).is_ok());
    }

}
