//! The assembly seam for the system prompt.
//!
//! One ordered list of named blocks, rendered in list order — the registry
//! the formula doc (docs/narrative-identity.md, "Its place in the system
//! prompt") builds toward. Slice 1 of that build: this module changes NOTHING
//! about the rendered bytes. The blocks wrap the existing builders in the
//! existing order, so `chat.rs` stops being a 160-line push_str chain and the
//! coming changes (rules last, the quantized clock, the cache breakpoint,
//! per-block budgets) become one-list edits here instead of surgery there.
//!
//! Error policy: a block that fails renders nothing and says so in the log —
//! never a default, never fabricated bytes (the house swallowed-query rule,
//! made structural). Today every wrapped builder still handles its own
//! errors internally, exactly as it did before this seam existed; new blocks
//! get the policy for free by returning `None` on failure and logging.
//!
//! Determinism: every query inside a block must end in a total ORDER BY, and
//! no block may call `Utc::now()` more than once per assemble — unstable
//! bytes silently kill provider prompt caching. Enforced by review until the
//! cadence test lands with the reorder slice.

use futures::future::BoxFuture;

/// Who authors a block's content. Rendered into nothing today; the
/// precedence preamble renders from this once the reorder slice lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Author {
    /// Us — the product's own voice (persona, tool guidance).
    System,
    /// The person — authored, ratified words (narrative identity, rules).
    User,
    /// The machine's accumulated notes (memory).
    Machine,
    /// Deterministic computation over the record (clock, user context).
    Computed,
    /// The UI's live state (open notebook, open page).
    Ui,
}

/// Declarative blocks describe; imperative blocks bind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Mood {
    Declarative,
    Imperative,
}

/// How often the rendered bytes change — cache metadata. Once the reorder
/// slice lands, registry order must be non-decreasing in cadence (stable
/// prefix first), with the one deliberate exception of rules-last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub enum Cadence {
    /// Never changes for a given box (persona, tool guidance).
    Static,
    /// Months–years (narrative identity, rules).
    Slow,
    /// Changes within a session but not per turn (memory, notebook).
    Session,
    /// Changes on a quantized clock (the datetime / situation block).
    Quantized,
    /// Changes every turn (the open page's live content).
    PerTurn,
}

/// One named section of the prompt.
pub struct BlockMeta {
    /// The XML tag (and audit key). For the fused head block — base + persona
    /// + narrative identity + tools, still one string from
    /// `build_personalized_prompt` — this is `"base"` until the reorder slice
    /// splits it.
    pub tag: &'static str,
    #[allow(dead_code)]
    pub author: Author,
    #[allow(dead_code)]
    pub mood: Mood,
    /// Precedence rung: higher outranks lower when sections conflict.
    #[allow(dead_code)]
    pub rung: u8,
    #[allow(dead_code)]
    pub cadence: Cadence,
}

/// A block: metadata plus the future that renders it. `None` = legitimately
/// absent — the section renders nothing at all (an empty `<rules>` block
/// would teach the model the section is usually noise).
pub struct Block<'a> {
    pub meta: BlockMeta,
    pub body: BoxFuture<'a, Option<String>>,
}

/// One rendered block, for audits: which tag produced how many chars.
#[derive(Debug)]
pub struct RenderedBlock {
    pub tag: &'static str,
    #[allow(dead_code)]
    pub chars: usize,
}

/// Render the blocks: fan the bodies out concurrently, concatenate strictly
/// in list order. Blocks own their separators (each body starts with the
/// `\n\n` its section always carried), so assembly is pure concatenation and
/// the output is byte-identical to the old push_str chain.
pub async fn assemble(blocks: Vec<Block<'_>>) -> (String, Vec<RenderedBlock>) {
    let (metas, bodies): (Vec<_>, Vec<_>) =
        blocks.into_iter().map(|b| (b.meta, b.body)).unzip();
    let results = futures::future::join_all(bodies).await;

    let mut out = String::new();
    let mut rendered = Vec::new();
    for (meta, body) in metas.into_iter().zip(results) {
        if let Some(body) = body {
            rendered.push(RenderedBlock { tag: meta.tag, chars: body.chars().count() });
            out.push_str(&body);
        }
    }
    (out, rendered)
}
