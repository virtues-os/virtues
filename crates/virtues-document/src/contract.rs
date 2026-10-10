//! The document contract: one JSON file that names every node, mark and
//! attribute a document may hold, and the HTML each one reads from and writes
//! to. The browser editor builds its ProseMirror schema from the same file.
//!
//! Order in the file is part of the contract: mark order is nesting order in
//! HTML (proposals outermost, then `link`) and attribute order is rendering
//! order. serde_json's
//! `Map` sorts its keys unless the `preserve_order` feature is on, and features
//! unify across the workspace, so turning it on here would change key order in
//! every crate that serializes JSON. Every object whose order matters is read
//! through [`Ordered`] instead, straight from the text.

use regex::Regex;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;

/// The contract file, compiled in. The web editor imports the same file.
pub const CONTRACT_JSON: &str = include_str!("../contract.json");

/// A JSON object in the order its keys appear in the text.
#[derive(Debug, Clone, PartialEq)]
pub struct Ordered<V>(Vec<(String, V)>);

impl<V> Default for Ordered<V> {
    fn default() -> Self {
        Ordered(Vec::new())
    }
}

impl<V> Ordered<V> {
    pub fn iter(&self) -> std::slice::Iter<'_, (String, V)> {
        self.0.iter()
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|(k, _)| k.as_str())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'a, V> IntoIterator for &'a Ordered<V> {
    type Item = &'a (String, V);
    type IntoIter = std::slice::Iter<'a, (String, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for Ordered<V> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Entries<V>(PhantomData<V>);

        impl<'de, V: Deserialize<'de>> Visitor<'de> for Entries<V> {
            type Value = Ordered<V>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut out: Vec<(String, V)> = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some(key) = map.next_key::<String>()? {
                    if out.iter().any(|(k, _)| *k == key) {
                        return Err(de::Error::custom(format!("duplicate key `{key}`")));
                    }
                    let value = map.next_value()?;
                    out.push((key, value));
                }
                Ok(Ordered(out))
            }
        }

        d.deserialize_map(Entries(PhantomData))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttrType {
    Int,
    String,
    Bool,
    Url,
    /// A ref to something on the box, `/kind/id` ([`is_route`]).
    Route,
}

impl AttrType {
    fn describe(self) -> &'static str {
        match self {
            AttrType::Int => "a whole number",
            AttrType::String => "text",
            AttrType::Bool => "true or false",
            AttrType::Url => "a URL",
            AttrType::Route => "a ref on this box, `/kind/id`",
        }
    }
}

/// The largest whole number an `int` attribute may be, either side of zero:
/// the largest a JavaScript number holds exactly (`Number.MAX_SAFE_INTEGER`).
/// The browser reads every number as one, and yrs writes an integer past it
/// as a BigInt, which the browser reads as another type; within it, both
/// sides read the same number back.
pub const MAX_INT: i64 = (1 << 53) - 1;

/// Is `route` a ref to something on the box, as the box's ref grammar reads
/// one (`api::refs::split_ref` in virtues-core): `/kind/id`, both parts
/// non-empty, a viewer's `?…` or `#…` not part of the id. The kind is
/// letters, digits, `_` and `-`, so a route is never read as another site:
/// `//host` and `/\host` are paths to another host for a browser, and a tab
/// or newline inside is removed before it reads the path. Any kind counts,
/// so a kind added later needs no change here, and a route is kept as
/// written: the box reads `/notebook/` as `/project/`, and the export still
/// holds what the page held. Markdown's `[@Label](/kind/id)` becomes a
/// mention only when this holds, so a converted mention is always one the
/// contract takes.
pub fn is_route(route: &str) -> bool {
    let Some((kind, id)) = route.strip_prefix('/').and_then(|r| r.split_once('/')) else {
        return false;
    };
    let id = id.split(['?', '#']).next().unwrap_or(id);
    !kind.is_empty()
        && kind
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        && !id.is_empty()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttrSpec {
    #[serde(rename = "type")]
    pub ty: AttrType,
    /// `None` when the contract omits it; a JSON `null` default is `Some(Null)`.
    #[serde(default, deserialize_with = "present")]
    pub default: Option<Value>,
    #[serde(default)]
    pub required: bool,
    #[serde(default, rename = "enum")]
    pub allowed: Option<Vec<Value>>,
    /// The least an `int` may be (a table cell spans at least one column).
    #[serde(default)]
    pub min: Option<i64>,
    /// The most an `int` may be (a table cell spans at most 1,000 columns).
    #[serde(default)]
    pub max: Option<i64>,
    /// The HTML attribute this one reads from and renders to.
    #[serde(default)]
    pub html: Option<String>,
    /// Fallback: read the value off the first child element's class with this
    /// prefix (`<pre><code class="language-rust">`).
    #[serde(default)]
    pub from_child_class: Option<String>,
}

fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

/// A URL as a browser reads it before looking at its scheme (the WHATWG URL
/// parser): every ASCII tab and newline removed, wherever it is, and C0
/// controls and spaces trimmed from both ends. Checked any other way,
/// `java&#9;script:` and `&#1;javascript:` pass a prefix test and still run
/// as `javascript:` when followed.
pub fn normalize_url(raw: &str) -> String {
    raw.chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect::<String>()
        .trim_matches(|c: char| c <= ' ')
        .to_string()
}

/// The scheme of a normalized URL, lowercased; `None` for a path. As the URL
/// parser reads it: an ASCII letter, then letters, digits, `+`, `-` or `.`,
/// up to the first `:`. Anything else before the colon makes it a path.
pub fn url_scheme(url: &str) -> Option<String> {
    let (head, _) = url.split_once(':')?;
    let mut chars = head.chars();
    let first = chars.next()?;
    let scheme = first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    scheme.then(|| head.to_ascii_lowercase())
}

impl AttrSpec {
    /// The value an attribute takes when the HTML does not set it.
    pub fn default_value(&self) -> Value {
        self.default.clone().unwrap_or(Value::Null)
    }

    /// Does `value` fit this attribute? Null stands for "not set", which only
    /// a required attribute refuses. A URL is checked against `c`'s schemes.
    pub fn check(&self, c: &Contract, key: &str, value: &Value) -> Result<(), String> {
        if value.is_null() {
            return if self.required {
                Err(format!("`{key}` is required"))
            } else {
                Ok(())
            };
        }
        let fits = match self.ty {
            AttrType::Int => value.as_i64().is_some(),
            AttrType::Bool => value.is_boolean(),
            AttrType::Route => value.as_str().is_some_and(is_route),
            AttrType::String | AttrType::Url => value.is_string(),
        };
        if !fits {
            return Err(format!(
                "`{key}` must be {}; got {value}",
                self.ty.describe()
            ));
        }
        if self.ty == AttrType::Url {
            if let Some(why) = value.as_str().and_then(|v| c.refuse_url(v)) {
                return Err(format!("`{key}` {why}"));
            }
        }
        if let Some(n) = value.as_i64() {
            let min = self.min.unwrap_or(-MAX_INT);
            let max = self.max.unwrap_or(MAX_INT);
            if n < min {
                return Err(format!("`{key}` is at least {min}; got {value}"));
            }
            if n > max {
                return Err(format!("`{key}` is at most {max}; got {value}"));
            }
        }
        if let Some(allowed) = &self.allowed {
            if !allowed.contains(value) {
                let list: Vec<String> = allowed.iter().map(Value::to_string).collect();
                return Err(format!(
                    "`{key}` is one of {}; got {value}",
                    list.join(", ")
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HtmlRule {
    pub tag: String,
    /// Attributes this rule sets on the node (`h2` sets `level: 2`).
    #[serde(default)]
    pub set: Ordered<Value>,
    /// HTML attributes that must be present with these values for the rule to
    /// match, and that rendering writes back (`data-type="taskList"`).
    #[serde(default, rename = "match")]
    pub matches: Ordered<Value>,
    /// Wrapper tags read through as if absent (`tbody` inside `table`).
    #[serde(default)]
    pub transparent: Vec<String>,
    /// Accepted on the way in, never rendered (`<b>` for bold).
    #[serde(default)]
    pub parse_only: bool,
    /// Rendering wraps the children in this tag (`<table><tbody>`).
    #[serde(default)]
    pub wrap: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeSpec {
    #[serde(default)]
    pub group: Option<String>,
    /// ProseMirror content expression. `None` is a leaf.
    #[serde(default)]
    pub content: Option<String>,
    /// Marks the node's inline content may carry: `None` for all, `""` for
    /// none, otherwise a space-separated list.
    #[serde(default)]
    pub marks: Option<String>,
    #[serde(default)]
    pub code: bool,
    #[serde(default)]
    pub atom: bool,
    #[serde(default)]
    pub inline: bool,
    /// The node carries a block id.
    #[serde(default)]
    pub id: bool,
    #[serde(default)]
    pub attrs: Ordered<AttrSpec>,
    #[serde(default)]
    pub html: Vec<HtmlRule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkSpec {
    #[serde(default)]
    pub attrs: Ordered<AttrSpec>,
    #[serde(default)]
    pub html: Vec<HtmlRule>,
    /// ProseMirror `excludes`: marks this one cannot coexist with. `None` is
    /// ProseMirror's default (only itself); `"_"` is every mark.
    #[serde(default)]
    pub excludes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdSpec {
    /// The node attribute that holds a block id.
    pub attr: String,
    /// The HTML attribute it reads from and renders to.
    pub html: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawContract {
    #[serde(rename = "$comment", default)]
    _comment: Option<String>,
    version: u32,
    fragment: String,
    id: IdSpec,
    #[serde(rename = "urlSchemes")]
    url_schemes: Vec<String>,
    nodes: Ordered<NodeSpec>,
    marks: Ordered<MarkSpec>,
}

#[derive(Debug)]
pub struct Node {
    pub name: String,
    pub spec: NodeSpec,
    content: Option<Regex>,
}

#[derive(Debug)]
pub struct Mark {
    pub name: String,
    pub spec: MarkSpec,
}

/// The parsed contract. Nodes and marks keep the file's order.
#[derive(Debug)]
pub struct Contract {
    pub version: u32,
    /// Name of the Yjs `XmlFragment` that holds the document.
    pub fragment: String,
    pub id: IdSpec,
    /// The schemes a `url` attribute may carry, lowercase. A URL without a
    /// scheme is a path on the box (`/person/p_1`, `#notes`, `?q=x`) and is
    /// always allowed; any other scheme is refused: `javascript:` and
    /// `vbscript:` run code when followed, `data:` can carry a page.
    pub url_schemes: Vec<String>,
    pub nodes: Vec<Node>,
    pub marks: Vec<Mark>,
    node_index: HashMap<String, usize>,
    mark_index: HashMap<String, usize>,
}

/// Marks only a page's owner makes, in the editor: a proposal is the
/// owner's to accept or reject, so a writer's HTML never holds one, and
/// [`Contract::tag_guide`] leaves them out.
pub const OWNER_MARKS: &[&str] = &["proposedDeletion", "proposedInsertion"];

/// HTML elements that take no end tag.
const VOID_TAGS: &[&str] = &["img", "br", "hr"];

/// One private-use character per node type, so a content expression compiles
/// to an ordinary regex over a string of children.
fn letter(i: usize) -> char {
    char::from_u32(0xE000 + i as u32).expect("private use area")
}

impl Contract {
    /// The compiled-in contract. Prefer [`crate::contract`], which parses it
    /// once.
    pub fn load() -> Self {
        Self::parse(CONTRACT_JSON).expect("contract.json is valid")
    }

    pub fn parse(json: &str) -> anyhow::Result<Self> {
        let raw: RawContract = serde_json::from_str(json)?;
        let nodes: Vec<Node> = raw
            .nodes
            .0
            .into_iter()
            .map(|(name, spec)| Node {
                name,
                spec,
                content: None,
            })
            .collect();
        let marks: Vec<Mark> = raw
            .marks
            .0
            .into_iter()
            .map(|(name, spec)| Mark { name, spec })
            .collect();
        let node_index = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.name.clone(), i))
            .collect();
        let mark_index = marks
            .iter()
            .enumerate()
            .map(|(i, m)| (m.name.clone(), i))
            .collect();
        let mut c = Contract {
            version: raw.version,
            fragment: raw.fragment,
            id: raw.id,
            url_schemes: raw.url_schemes,
            nodes,
            marks,
            node_index,
            mark_index,
        };
        for i in 0..c.nodes.len() {
            if let Some(expr) = c.nodes[i].spec.content.clone() {
                let re = c.compile(&expr)?;
                c.nodes[i].content = Some(re);
            }
        }
        c.check()?;
        Ok(c)
    }

    /// Inconsistencies inside the file that would otherwise surface as a
    /// render rule that never matches or a default the parser refuses.
    fn check(&self) -> anyhow::Result<()> {
        if self.node(&self.fragment).is_none() {
            anyhow::bail!("fragment `{}` is not a node", self.fragment);
        }
        for scheme in &self.url_schemes {
            if url_scheme(&format!("{scheme}:")).as_deref() != Some(scheme.as_str()) {
                anyhow::bail!("`{scheme}` in urlSchemes is not a lowercase URL scheme");
            }
        }
        for n in &self.nodes {
            for (key, a) in n.attrs() {
                if (a.min.is_some() || a.max.is_some()) && a.ty != AttrType::Int {
                    anyhow::bail!(
                        "node `{}`: `{key}` has a `min` or `max` but is not an int",
                        n.name
                    );
                }
                let (min, max) = (a.min.unwrap_or(-MAX_INT), a.max.unwrap_or(MAX_INT));
                if min < -MAX_INT || max > MAX_INT || min > max {
                    anyhow::bail!(
                        "node `{}`: `{key}` runs from {min} to {max}; an int stays within {MAX_INT} of zero",
                        n.name
                    );
                }
                if let Some(d) = &a.default {
                    a.check(self, key, d)
                        .map_err(|e| anyhow::anyhow!("node `{}` default: {e}", n.name))?;
                }
            }
            if !n.spec.html.is_empty() && n.spec.html.iter().all(|r| r.parse_only) {
                anyhow::bail!("node `{}` has no rule it renders with", n.name);
            }
            for rule in &n.spec.html {
                for (key, v) in &rule.set {
                    let a = n.attrs().get(key).ok_or_else(|| {
                        anyhow::anyhow!(
                            "node `{}`: <{}> sets `{key}`, which it does not have",
                            n.name,
                            rule.tag
                        )
                    })?;
                    a.check(self, key, v)
                        .map_err(|e| anyhow::anyhow!("node `{}`: <{}> {e}", n.name, rule.tag))?;
                }
            }
            if let Some(list) = n.spec.marks.as_deref() {
                for m in list.split_whitespace().filter(|m| *m != "_") {
                    if self.mark(m).is_none() {
                        anyhow::bail!(
                            "node `{}` allows mark `{m}`, which is not in the contract",
                            n.name
                        );
                    }
                }
            }
        }
        for m in &self.marks {
            if !m.spec.html.iter().any(|r| !r.parse_only) {
                anyhow::bail!("mark `{}` has no rule it renders with", m.name);
            }
            if let Some(list) = m.spec.excludes.as_deref() {
                for x in list.split_whitespace().filter(|x| *x != "_") {
                    if self.mark(x).is_none() {
                        anyhow::bail!(
                            "mark `{}` excludes `{x}`, which is not in the contract",
                            m.name
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Why a URL is refused, or `None` when a page may hold it.
    pub fn refuse_url(&self, raw: &str) -> Option<String> {
        let scheme = url_scheme(&normalize_url(raw))?;
        if self.url_schemes.contains(&scheme) {
            return None;
        }
        Some(format!(
            "cannot use `{scheme}:`; a page links to {} URLs, or to a path on this box",
            self.url_schemes.join(", ")
        ))
    }

    /// Names a content expression may use: node types and groups.
    fn expand(&self, name: &str) -> anyhow::Result<String> {
        if let Some(&i) = self.node_index.get(name) {
            return Ok(letter(i).to_string());
        }
        let members: String = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.groups().any(|g| g == name))
            .map(|(i, _)| letter(i))
            .collect();
        if members.is_empty() {
            anyhow::bail!("content expression names `{name}`, which is neither a node nor a group");
        }
        Ok(format!("[{members}]"))
    }

    /// ProseMirror content expression → anchored regex over child letters.
    fn compile(&self, expr: &str) -> anyhow::Result<Regex> {
        let mut out = String::from("^(?:");
        let mut chars = expr.chars().peekable();
        while let Some(&ch) = chars.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        name.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if name.chars().all(|c| c.is_ascii_digit()) {
                    out.push_str(&name);
                } else {
                    out.push_str(&self.expand(&name)?);
                }
            } else {
                chars.next();
                match ch {
                    '(' => out.push_str("(?:"),
                    ')' | '|' | '*' | '+' | '?' | '{' | '}' | ',' => out.push(ch),
                    c if c.is_whitespace() => {}
                    c => {
                        anyhow::bail!("unsupported character `{c}` in content expression `{expr}`")
                    }
                }
            }
        }
        out.push_str(")$");
        Ok(Regex::new(&out)?)
    }

    pub fn node(&self, name: &str) -> Option<&Node> {
        self.node_index.get(name).map(|&i| &self.nodes[i])
    }

    pub fn mark(&self, name: &str) -> Option<&Mark> {
        self.mark_index.get(name).map(|&i| &self.marks[i])
    }

    /// Position in the contract's mark order: lower nests outside higher.
    pub fn mark_rank(&self, name: &str) -> usize {
        self.mark_index.get(name).copied().unwrap_or(usize::MAX)
    }

    /// Does `parent` accept exactly this sequence of child types?
    pub fn content_matches(&self, parent: &str, children: &[&str]) -> bool {
        let Some(node) = self.node(parent) else {
            return false;
        };
        let Some(re) = &node.content else {
            return children.is_empty();
        };
        let mut s = String::new();
        for child in children {
            match self.node_index.get(*child) {
                Some(&i) => s.push(letter(i)),
                None => return false,
            }
        }
        re.is_match(&s)
    }

    /// Can `parent` hold a child of this type at all (anywhere)?
    pub fn may_contain(&self, parent: &str, child: &str) -> bool {
        let Some(node) = self.node(parent) else {
            return false;
        };
        let Some(expr) = &node.spec.content else {
            return false;
        };
        let tokens: Vec<&str> = expr
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|t| !t.is_empty() && !t.chars().all(|c| c.is_ascii_digit()))
            .collect();
        let child_node = self.node(child);
        tokens.iter().any(|t| {
            *t == child
                || child_node
                    .map(|n| n.groups().any(|g| g == *t))
                    .unwrap_or(false)
        })
    }

    /// A node whose content is inline: text, marks and inline atoms.
    pub fn is_textblock(&self, name: &str) -> bool {
        self.node(name)
            .and_then(|n| n.spec.content.as_deref())
            .map(|c| c.contains("inline") || c.contains("text"))
            .unwrap_or(false)
    }

    pub fn is_inline(&self, name: &str) -> bool {
        name == "text"
            || self
                .node(name)
                .map(|n| n.spec.inline || n.groups().any(|g| g == "inline"))
                .unwrap_or(false)
    }

    /// Can text inside `node` carry `mark`? ProseMirror's default for a node
    /// with inline content is every mark.
    pub fn allows_mark(&self, node: &str, mark: &str) -> bool {
        match self.node(node).map(|n| n.spec.marks.as_deref()) {
            None => false,
            Some(None) => true,
            Some(Some(list)) => list.split_whitespace().any(|m| m == "_" || m == mark),
        }
    }

    /// Two marks that ProseMirror would refuse to put on the same text.
    pub fn marks_exclude(&self, a: &str, b: &str) -> bool {
        let excl =
            |m: &str, other: &str| match self.mark(m).and_then(|s| s.spec.excludes.as_deref()) {
                None => m == other,
                Some(list) => list.split_whitespace().any(|x| x == "_" || x == other),
            };
        a == b || excl(a, b) || excl(b, a)
    }

    /// Rules that can turn this HTML tag into a node, most specific first.
    pub fn node_rules_for_tag<'a>(&'a self, tag: &str) -> Vec<(&'a Node, &'a HtmlRule)> {
        let mut rules: Vec<(&Node, &HtmlRule)> = self
            .nodes
            .iter()
            .flat_map(|n| n.spec.html.iter().map(move |r| (n, r)))
            .filter(|(_, r)| r.tag == tag)
            .collect();
        rules.sort_by_key(|(_, r)| std::cmp::Reverse(r.matches.len()));
        rules
    }

    pub fn mark_rule_for_tag<'a>(&'a self, tag: &str) -> Option<(&'a Mark, &'a HtmlRule)> {
        self.marks
            .iter()
            .flat_map(|m| m.spec.html.iter().map(move |r| (m, r)))
            .find(|(_, r)| r.tag == tag)
    }

    /// The tags a writer may use, one line each for blocks, inline nodes and
    /// marks, in contract order: each with the attributes it takes, the
    /// values a match needs (`data-type="taskList"`), and the choices of an
    /// attribute with a few (`data-tone="note|tip|warning"`). An element
    /// that holds nothing and is not void is shown with its end tag, which
    /// it needs. Generated from the contract, so it holds what ingest takes.
    pub fn tag_guide(&self) -> String {
        let mut blocks = vec![];
        let mut inline = vec![];
        for n in &self.nodes {
            let rules: Vec<&HtmlRule> = n.spec.html.iter().filter(|r| !r.parse_only).collect();
            let Some(first) = rules.first() else { continue };
            let set_keys: Vec<&str> = rules.iter().flat_map(|r| r.set.keys()).collect();
            let attrs = guide_attrs(first, n.attrs(), &set_keys);
            let tags = if rules.len() > 2 {
                format!("<{}>…<{}>", rules[0].tag, rules[rules.len() - 1].tag)
            } else {
                rules
                    .iter()
                    .map(|r| guide_tag(r, &attrs, n.spec.content.is_none()))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            if self.is_inline(&n.name) {
                inline.push(tags);
            } else {
                blocks.push(tags);
            }
        }
        let marks: Vec<String> = self
            .marks
            .iter()
            .filter(|m| !OWNER_MARKS.contains(&m.name.as_str()))
            .map(|m| {
                let rule = m.render_rule();
                guide_tag(rule, &guide_attrs(rule, m.attrs(), &[]), false)
            })
            .collect();
        format!(
            "Blocks: {}\nInline: {}\nMarks: {}",
            blocks.join(" "),
            inline.join(" "),
            marks.join(" ")
        )
    }

    /// Every attribute a node of this type carries once parsed, defaults
    /// filled: what ProseMirror's `node.attrs` holds.
    pub fn default_attrs(&self, name: &str) -> Map<String, Value> {
        let mut m = Map::new();
        if let Some(n) = self.node(name) {
            for (k, a) in n.attrs() {
                m.insert(k.clone(), a.default_value());
            }
            if n.spec.id {
                m.insert(self.id.attr.clone(), Value::Null);
            }
        }
        m
    }
}

/// A rule's match values and the attributes it takes, as the tag guide
/// writes them; `skip`: attributes a rule sets, which its tag says.
fn guide_attrs(rule: &HtmlRule, attrs: &Ordered<AttrSpec>, skip: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = rule
        .matches
        .iter()
        .map(|(k, v)| format!("{k}=\"{}\"", v.as_str().unwrap_or_default()))
        .collect();
    for (key, a) in attrs {
        let Some(html) = &a.html else { continue };
        if skip.contains(&key.as_str()) {
            continue;
        }
        let choices: Option<Vec<String>> = match (&a.allowed, a.ty) {
            (Some(values), _) if values.len() <= 4 => Some(
                values
                    .iter()
                    .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))
                    .collect(),
            ),
            (_, AttrType::Bool) => Some(vec!["true".into(), "false".into()]),
            _ => None,
        };
        out.push(match choices {
            Some(c) => format!("{html}=\"{}\"", c.join("|")),
            None => html.clone(),
        });
    }
    out
}

fn guide_tag(rule: &HtmlRule, attrs: &[String], empty: bool) -> String {
    let mut s = format!("<{}", rule.tag);
    for a in attrs {
        s.push(' ');
        s.push_str(a);
    }
    s.push('>');
    if empty && !VOID_TAGS.contains(&rule.tag.as_str()) {
        s.push_str(&format!("</{}>", rule.tag));
    }
    s
}

impl Node {
    pub fn attrs(&self) -> &Ordered<AttrSpec> {
        &self.spec.attrs
    }

    pub fn groups(&self) -> impl Iterator<Item = &str> {
        self.spec.group.as_deref().unwrap_or("").split_whitespace()
    }

    /// The rule a node with these attributes renders with: the first one,
    /// not parse-only, whose `set` the attributes agree with.
    pub fn render_rule(&self, attrs: &Map<String, Value>) -> Option<&HtmlRule> {
        self.spec
            .html
            .iter()
            .filter(|r| !r.parse_only)
            .find(|r| r.set.iter().all(|(k, v)| attrs.get(k) == Some(v)))
    }
}

impl Mark {
    pub fn attrs(&self) -> &Ordered<AttrSpec> {
        &self.spec.attrs
    }

    pub fn render_rule(&self) -> &HtmlRule {
        self.spec
            .html
            .iter()
            .find(|r| !r.parse_only)
            .expect("checked at load: every mark renders")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn content_expressions_compile_and_match() {
        let c = Contract::load();
        assert!(c.content_matches("doc", &["paragraph"]));
        assert!(!c.content_matches("doc", &[]));
        assert!(c.content_matches("listItem", &["paragraph", "bulletList"]));
        assert!(!c.content_matches("listItem", &["bulletList"]));
        assert!(c.content_matches("tableRow", &["tableHeader", "tableCell"]));
        assert!(c.content_matches("tableRow", &[]));
        assert!(!c.content_matches("bulletList", &["paragraph"]));
        assert!(c.content_matches("paragraph", &["text", "mention", "hardBreak", "text"]));
        assert!(c.content_matches("horizontalRule", &[]));
        assert!(!c.content_matches("horizontalRule", &["text"]));
    }

    #[test]
    fn group_membership_and_kinds() {
        let c = Contract::load();
        assert!(c.is_textblock("paragraph"));
        assert!(c.is_textblock("codeBlock"));
        assert!(!c.is_textblock("blockquote"));
        assert!(c.is_inline("mention"));
        assert!(!c.allows_mark("codeBlock", "bold"));
        assert!(c.allows_mark("paragraph", "bold"));
        assert!(c.may_contain("taskList", "taskItem"));
        assert!(!c.may_contain("bulletList", "paragraph"));
    }

    #[test]
    fn contract_keeps_the_files_order() {
        let c = Contract::load();
        let marks: Vec<&str> = c.marks.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            marks,
            [
                "proposedDeletion",
                "proposedInsertion",
                "link",
                "bold",
                "italic",
                "underline",
                "strike",
                "highlight",
                "code"
            ]
        );
        // Proposals outermost, so a proposal over differently formatted
        // text stays one element; then link.
        assert_eq!(c.mark_rank("proposedDeletion"), 0);
        assert_eq!(c.mark_rank("link"), 2);
        let nodes: Vec<&str> = c.nodes.iter().take(4).map(|n| n.name.as_str()).collect();
        assert_eq!(nodes, ["doc", "paragraph", "heading", "blockquote"]);
        let image: Vec<&str> = c.node("image").unwrap().attrs().keys().collect();
        assert_eq!(image, ["src", "alt", "width"]);
        let cell: Vec<&str> = c.node("tableCell").unwrap().attrs().keys().collect();
        assert_eq!(cell, ["colspan", "rowspan", "align"]);
    }

    #[test]
    fn order_comes_from_the_text_not_the_alphabet() {
        let json = r#"{
            "version": 1, "fragment": "doc", "id": { "attr": "id", "html": "data-id" },
            "urlSchemes": [],
            "nodes": {
                "doc": { "content": "block+" },
                "text": { "group": "inline" },
                "paragraph": { "group": "block", "content": "inline*", "html": [{ "tag": "p" }],
                    "attrs": { "zeta": { "type": "int", "default": null, "html": "z" },
                               "alpha": { "type": "int", "default": null, "html": "a" } } }
            },
            "marks": {
                "zed": { "html": [{ "tag": "u" }] },
                "abc": { "html": [{ "tag": "b" }] }
            }
        }"#;
        let c = Contract::parse(json).unwrap();
        assert_eq!(c.mark_rank("zed"), 0);
        assert_eq!(c.mark_rank("abc"), 1);
        let names: Vec<&str> = c.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["doc", "text", "paragraph"]);
        let attrs: Vec<&str> = c.node("paragraph").unwrap().attrs().keys().collect();
        assert_eq!(attrs, ["zeta", "alpha"]);
    }

    #[test]
    fn a_malformed_contract_is_refused() {
        let base = |nodes: &str, marks: &str| {
            format!(
                r#"{{ "version": 1, "fragment": "doc", "id": {{ "attr": "id", "html": "data-id" }}, "urlSchemes": [], "nodes": {nodes}, "marks": {marks} }}"#
            )
        };
        let doc = r#""doc": { "content": "block+" }, "text": { "group": "inline" }"#;
        // A duplicate key would otherwise keep only the last entry.
        let dup = base(
            &format!(
                r#"{{ {doc}, "p": {{ "group": "block", "content": "inline*" }}, "p": {{ "group": "block" }} }}"#
            ),
            "{}",
        );
        assert!(Contract::parse(&dup)
            .unwrap_err()
            .to_string()
            .contains("duplicate key"));
        // A misspelt field would otherwise be ignored.
        let typo = base(
            &format!(
                r#"{{ {doc}, "p": {{ "group": "block", "content": "inline*", "html": [{{ "tag": "p", "parseonly": true }}] }} }}"#
            ),
            "{}",
        );
        assert!(Contract::parse(&typo).is_err());
        // A rule that sets a value its own enum refuses.
        let set = base(
            &format!(
                r#"{{ {doc}, "h": {{ "group": "block", "content": "inline*", "attrs": {{ "level": {{ "type": "int", "enum": [1, 2], "default": 1 }} }}, "html": [{{ "tag": "h3", "set": {{ "level": 3 }} }}] }} }}"#
            ),
            "{}",
        );
        assert!(Contract::parse(&set)
            .unwrap_err()
            .to_string()
            .contains("level"));
        // A mark with nothing to render as.
        let mark = base(
            &format!(
                r#"{{ {doc}, "p": {{ "group": "block", "content": "inline*", "html": [{{ "tag": "p" }}] }} }}"#
            ),
            r#"{ "bold": { "html": [{ "tag": "b", "parseOnly": true }] } }"#,
        );
        assert!(Contract::parse(&mark).is_err());
    }

    #[test]
    fn headings_run_one_to_six() {
        let c = Contract::load();
        let h = c.node("heading").unwrap();
        for level in 1..=6 {
            let tag = format!("h{level}");
            let rules = c.node_rules_for_tag(&tag);
            assert_eq!(rules.len(), 1, "{tag}");
            assert_eq!(rules[0].0.name, "heading");
            let mut attrs = c.default_attrs("heading");
            attrs.insert("level".into(), json!(level));
            assert_eq!(h.render_rule(&attrs).unwrap().tag, tag);
        }
        assert!(h
            .attrs()
            .get("level")
            .unwrap()
            .check(&c, "level", &json!(7))
            .is_err());
    }

    #[test]
    fn urls_are_read_the_way_a_browser_reads_them() {
        let c = Contract::load();
        let refuse_url = |u: &str| c.refuse_url(u);
        // What the URL parser strips before reading the scheme hides nothing.
        for hidden in [
            "javascript:alert(1)",
            " JavaScript:alert(1)",
            "java\tscript:alert(1)",
            "java\nscript:alert(1)",
            "java\rscript:alert(1)",
            "\u{1}javascript:alert(1)",
            "\u{0}\u{1f} vbscript:x",
            "javascript:alert(1)\u{0}",
            "data:text/html,<script>alert(1)</script>",
            "file:///etc/passwd",
            "localhost:3000/x",
        ] {
            assert!(refuse_url(hidden).is_some(), "{hidden:?} allowed");
        }
        for fine in [
            "https://example.com/a?b=1&c=2",
            "HTTP://EXAMPLE.COM",
            "mailto:nick@example.com",
            "tel:+15125550100",
            "/person/p_1",
            "#notes",
            "?q=a:b",
            "relative/path:with-colon",
            "a b:c",
            "",
        ] {
            assert_eq!(refuse_url(fine), None, "{fine:?} refused");
        }
        assert_eq!(
            normalize_url(" \u{1}https://ex\tample.com/\n "),
            "https://example.com/"
        );
        // A non-breaking space is not a C0 space: the parser keeps it, and so
        // does this, which makes the scheme a path.
        assert_eq!(url_scheme(&normalize_url("\u{a0}javascript:x")), None);
    }

    #[test]
    fn table_cells_take_an_alignment() {
        let c = Contract::load();
        for cell in ["tableCell", "tableHeader"] {
            let a = c.node(cell).unwrap().attrs().get("align").unwrap();
            assert_eq!(a.html.as_deref(), Some("align"));
            assert_eq!(a.default_value(), Value::Null);
            assert!(a.check(&c, "align", &json!("center")).is_ok());
            assert!(a.check(&c, "align", &Value::Null).is_ok());
            assert!(a.check(&c, "align", &json!("justify")).is_err());
        }
    }

    #[test]
    fn a_cell_spans_at_least_one_row_and_column_and_at_most_what_a_browser_draws() {
        let c = Contract::load();
        for cell in ["tableCell", "tableHeader"] {
            for (key, most) in [("colspan", 1_000), ("rowspan", 65_534)] {
                let a = c.node(cell).unwrap().attrs().get(key).unwrap();
                assert!(a.check(&c, key, &json!(1)).is_ok());
                assert!(a.check(&c, key, &json!(3)).is_ok());
                assert!(a.check(&c, key, &json!(most)).is_ok(), "{cell}.{key}");
                assert!(a.check(&c, key, &json!(0)).is_err(), "{cell}.{key}");
                assert!(a.check(&c, key, &json!(-2)).is_err(), "{cell}.{key}");
                assert!(a.check(&c, key, &json!(most + 1)).is_err(), "{cell}.{key}");
                assert!(a.check(&c, key, &json!(i64::MAX)).is_err(), "{cell}.{key}");
            }
        }
    }

    /// Every int stays where a JavaScript number holds it exactly, so the
    /// browser and the server read one number back.
    #[test]
    fn an_int_is_one_a_browser_holds_exactly() {
        let c = Contract::load();
        let width = c.node("image").unwrap().attrs().get("width").unwrap();
        for fine in [0, 2_147_483_648, 9_000_000_000_000_000, MAX_INT, -MAX_INT] {
            assert!(width.check(&c, "width", &json!(fine)).is_ok(), "{fine}");
        }
        for past in [MAX_INT + 1, -MAX_INT - 1, i64::MAX, i64::MIN] {
            assert!(width.check(&c, "width", &json!(past)).is_err(), "{past}");
        }
        assert!(width.check(&c, "width", &json!(1.5)).is_err());
    }

    #[test]
    fn a_mention_points_at_a_ref_on_the_box() {
        let c = Contract::load();
        let to = c.node("mention").unwrap().attrs().get("to").unwrap();
        for fine in [
            "/person/p_1",
            "/page/pg_abc?page=3",
            "/drive/a/b.pdf#hl",
            "/wiki-article/x",
            "/notebook/n1",
        ] {
            assert!(to.check(&c, "to", &json!(fine)).is_ok(), "{fine:?} refused");
        }
        for refused in [
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "https://evil.example.com/x",
            "//evil.example.com/x",
            "/\\evil.example.com/x",
            "/\tperson/x",
            " /person/x",
            "/person/",
            "/person/?q=1",
            "/person",
            "person/p_1",
            "",
        ] {
            assert!(
                to.check(&c, "to", &json!(refused)).is_err(),
                "{refused:?} allowed"
            );
        }
        assert!(to.check(&c, "to", &json!(1)).is_err());
    }
}
