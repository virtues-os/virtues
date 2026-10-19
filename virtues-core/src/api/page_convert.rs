//! Moving pages from markdown to blocks: slices 5 and 6 of
//! `agents/plan/document-contract-plan.md`, built as
//! `agents/build/document.md` describes ("Moving pages to blocks",
//! "Articles").
//!
//! A run reads one kind of page ([`Which`]): user pages (`kind = 'page'`),
//! the default, or articles (wiki articles and day pages: `kind =
//! 'article'`, and any page a `wiki_articles` row owns, whatever its kind).
//! Every page of that kind still in markdown is converted and written as a
//! tree, one page at a time:
//!
//! - **What is converted is what the server holds.** The page's queued save
//!   is written first (`YjsState::save_page_now`), so a keystroke inside the
//!   save debounce is in the conversion; then the page is read: the Y.Text
//!   of its saved state, or `content` for a page nobody has opened.
//! - **It is converted and judged** by the one conversion every move makes
//!   (`block_move::convert`). A user page is read as its markdown editor
//!   showed it (`Dialect::Page`): raw HTML and comments are text, every line
//!   of a paragraph stays a line, every step that changed something is a
//!   note, and a page whose blocks would lose a character of its text, or
//!   the address of a link or an embed with no note saying so, is not
//!   moved. Pending CriticMarkup proposals become the contract's suggestion
//!   marks (`parse_markdown`'s reading, the owner's own), the owner's to
//!   accept or reject in the page; comments and substitutions, which a page
//!   cannot hold, stay text and are noted. A proposal holding a mention, an
//!   image or a line break, which a suggestion's marks cannot cover, keeps
//!   the page in markdown, where its owner can still accept or reject it.
//! - **An article is read as an article** (`Dialect::Article`): its sources
//!   (`[^ev-1]`), figures and veiled phrases become the contract's own nodes.
//!   It stays markdown when its readers would read its blocks otherwise
//!   (`ArticleCounts::holds`), when its writers' last edition does not
//!   convert or a sentence of its owner's has no place in the blocks
//!   (`block_move::rebase`), or when a note beside one of its sentences
//!   would lose its place (`block_move::anchors_lost`). Its edition moves
//!   with it, in the flip's own transaction, so a move never reads as an
//!   edit of its owner's and the wiki editor keeps writing it.
//! - **An article nobody has opened is measured, not moved.** It moves when
//!   the server first builds its document (`block_move::first_open`), so
//!   holding a document still means someone opened it, and narration can
//!   still write a page nobody has read.
//! - **The page as it was is kept** before anything is written, as a
//!   version never pruned and always in History's list
//!   (`pages::keep_before_blocks`), and the blocks are versioned after, so
//!   History shows the move. Putting that version back converts it again
//!   and shows its notes (`pages::restore_version`).
//! - **The flip is one statement, under the page's doc lock**
//!   (`YjsState::move_to_tree`): checked against the row as it was read,
//!   so a page written meanwhile is read and converted again, and
//!   `updated_at` kept, since a page moved to blocks is not one its owner
//!   edited (the table's trigger keeps it on a change of format). The doc
//!   is evicted and every socket on it closed.
//! - **A page that fails stays markdown** and is reported with the reason
//!   and what converting it found. Nothing of it is written but the
//!   restore point of a flip that then failed, which holds what the page
//!   already said and which the report names; a page that changed before
//!   its flip, read again at the next attempt, is left none.
//! - **A page in the trash** is reported and left: History takes no version
//!   of it, so it moves once it is put back. A dry run over articles still
//!   measures it.
//! - **Idempotent and resumable**: only markdown pages are read, each page
//!   is its own write, and the page as it was is not kept a second time
//!   when the latest version already keeps it. A run stopped partway is
//!   finished by the next.
//!
//! Every run keeps its report on the box, as a JSON file per run and kind of
//! page under [`report_dir`] listing every page it read with what
//! converting it changed, and logs a line for each page with notes or a
//! failure, never quoting a page.
//!
//! Two doors: `virtues pages convert --dry-run | --apply [--articles]`
//! (`cli/page_convert.rs`; `--apply` runs here, in the server, through the
//! console door, because only the server holds the pages' live docs and
//! their queued saves, and only on the database the CLI names, by its
//! name and its Postgres server: [`apply_on`]),
//! the server's startup ([`spawn_on_startup`]): user pages unless
//! `VIRTUES_CONVERT_PAGES=0`, articles when `VIRTUES_CONVERT_ARTICLES=1`;
//! and opening a markdown user page ([`convert_on_open`]). All move pages
//! only while block pages are on (unless `VIRTUES_TREE_PAGES=0`), and
//! articles only while block articles are on too (`VIRTUES_TREE_ARTICLES=1`).

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use virtues_document::migrate::{ArticleCounts, Dialect, TextLoss};
use virtues_document::Problem;

use crate::api::block_move::{self, ArticleRebase, Converted};
use crate::api::pages::{self, PageFormat};
use crate::error::{Error, Result};
use crate::server::yjs::{TreeMove, Written, YjsState};

/// How a run treats the pages it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Run {
    /// Convert each page in memory and report it; write nothing to a page.
    DryRun,
    /// Convert each page and write it as blocks.
    Apply,
}

impl Run {
    fn as_str(self) -> &'static str {
        match self {
            Self::DryRun => "dry-run",
            Self::Apply => "apply",
        }
    }
}

/// Which pages a run reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Which {
    /// User pages (`kind = 'page'`), no article among them.
    #[default]
    Pages,
    /// Articles: wiki articles and day pages.
    Articles,
}

impl Which {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pages => "pages",
            Self::Articles => "articles",
        }
    }

    /// How a page of this kind is read: as its markdown editor showed it, or
    /// as its writers wrote it.
    fn dialect(self) -> Dialect {
        match self {
            Self::Pages => Dialect::Page,
            Self::Articles => Dialect::Article,
        }
    }
}

/// What happened to one page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The page holds blocks now.
    Converted,
    /// A dry run: the page converts.
    WouldConvert,
    /// An article nobody has opened, which converts: it moves when it is
    /// first opened, so a run leaves it as it is.
    ConvertsWhenOpened,
    /// The page stays markdown; the reason says why.
    Failed,
    /// Nothing to do: the page holds blocks already, or is gone.
    Skipped,
}

/// Where an article's edition (`wiki_articles.machine_text`, what its
/// writers last wrote) went in its move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EditionMove {
    /// The article was its edition, untouched since its writers wrote it:
    /// the edition is the export now.
    Kept,
    /// The owner had changed the article: the edition is what the export
    /// makes of it, so their changes still read as theirs.
    Rebased,
    /// The article has no edition, before or after.
    None,
}

/// What a move did to an article, and what converting it changed, for the
/// report.
#[derive(Debug, Clone, Serialize)]
pub struct ArticleResult {
    /// Someone opened the article: it holds a document.
    pub opened: bool,
    pub edition: EditionMove,
    /// The article read as its writers last left it, before the move.
    pub untouched_before: bool,
    /// The same, after: must equal `untouched_before`.
    pub untouched_after: bool,
    /// The owner's sentences (`theirs`) before the move and placed after.
    pub theirs_kept: (usize, usize),
    /// How many notes beside its sentences land, before and after.
    pub anchors_landing: (usize, usize),
    /// The article's syntax, counted on the way in and in the blocks.
    pub counts: ArticleCounts,
    /// The first line where the export departs from the markdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_changed: Option<String>,
    /// What changed between the markdown and its export, by kind:
    /// "escapes", "list markers", "table rules", "orphan notes", "veil
    /// placement", "spaces", "note order", "line breaks", "blank lines",
    /// "final newline", "other".
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changed_by: Vec<&'static str>,
}

/// One page a run read, and what converting it changed.
#[derive(Debug, Clone, Serialize)]
pub struct PageResult {
    pub id: String,
    pub title: String,
    pub outcome: Outcome,
    /// Why the page stays markdown, or was skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Each step the converter took that changed something on the way in.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Problem>,
    /// Where the blocks' text first departs from the markdown's: a page
    /// with one loses a character, and stays markdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_lost: Option<TextLoss>,
    /// The addresses of links and embeds the blocks would drop with no note
    /// saying so: a page with one stays markdown.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub addresses_lost: Vec<String>,
    /// Where the blocks' markdown export (`content`, what search reads)
    /// reads back to other blocks than the page holds. The page itself is
    /// its blocks; this is reported, not refused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_differs: Option<String>,
    /// Pending suggestions (CriticMarkup proposals) carried over as the
    /// contract's suggestion marks, each counted once.
    #[serde(skip_serializing_if = "is_zero")]
    pub suggestions: usize,
    /// The version that holds the page as it was in markdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_point: Option<i64>,
    /// The version that holds the page as blocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    /// For an article: its edition, its owner's sentences, the notes beside
    /// it and what its export changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub article: Option<ArticleResult>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

impl PageResult {
    fn new(id: &str, title: &str, outcome: Outcome) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            outcome,
            reason: None,
            notes: vec![],
            text_lost: None,
            addresses_lost: vec![],
            export_differs: None,
            suggestions: 0,
            restore_point: None,
            version: None,
            article: None,
        }
    }

    fn failed(id: &str, title: &str, reason: impl Into<String>) -> Self {
        Self {
            reason: Some(reason.into()),
            ..Self::new(id, title, Outcome::Failed)
        }
    }

    fn skipped(id: &str, title: &str, reason: &str) -> Self {
        Self {
            reason: Some(reason.to_string()),
            ..Self::new(id, title, Outcome::Skipped)
        }
    }

    /// What converting the page found, for its report.
    fn with(mut self, converted: &Converted) -> Self {
        self.notes = converted.notes.clone();
        self.text_lost = converted.text_lost.clone();
        self.addresses_lost = converted.addresses_lost.clone();
        self.export_differs = converted.export_differs.clone();
        self.suggestions = converted.suggestions;
        self
    }

    /// What judging an article found, for its report.
    fn judged(mut self, judged: Option<&Judged>) -> Self {
        self.article = judged.map(|j| j.result.clone());
        self
    }

    /// Whether converting the page changed it on the way in: a step the
    /// converter noted, an export that reads back to other blocks, or, for
    /// an article, an export that says the page otherwise than its
    /// markdown did.
    pub fn changed_on_the_way_in(&self) -> bool {
        !self.notes.is_empty()
            || self.export_differs.is_some()
            || self
                .article
                .as_ref()
                .is_some_and(|a| !a.changed_by.is_empty() || a.content_changed.is_some())
    }
}

/// What one run did, kept on the box as a file ([`report_dir`]).
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub run: Run,
    /// The kind of page the run read.
    pub which: Which,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    /// The contract version the blocks are written under.
    pub contract: u32,
    pub converted: usize,
    pub would_convert: usize,
    /// Articles nobody has opened that convert, left to move when opened.
    pub converts_when_opened: usize,
    pub failed: usize,
    pub skipped: usize,
    /// Pages whose conversion changed something on the way in
    /// ([`PageResult::changed_on_the_way_in`]).
    pub with_notes: usize,
    /// Every page the run read.
    pub pages: Vec<PageResult>,
    /// The file this report is kept in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept_at: Option<String>,
    /// Why the report file could not be written, when it could not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_kept: Option<String>,
}

/// What a version holding a page's blocks says, cut after the page moved.
pub const MOVED_TO_BLOCKS: &str = "Moved to blocks";

/// Why pages are not moved while block pages are off.
pub const BLOCK_PAGES_OFF: &str = "Block pages are turned off on this server, so it moved no page. \
     Remove VIRTUES_TREE_PAGES=0 to turn them back on.";

/// Why articles are not moved while block articles are off.
pub const ARTICLES_OFF: &str = "Block articles aren't turned on for this server, so it moved no article. \
     Turn them on with VIRTUES_TREE_ARTICLES=1 first, with block pages on.";

const STAYS_MARKDOWN: &str =
    "The page kept changing while your server converted it, so it left the page in markdown. The next run converts it.";

const ALREADY_BLOCKS: &str = "The page holds blocks already.";

const GONE: &str = "The page is gone.";

/// Why a run over user pages leaves an article.
const AN_ARTICLE: &str = "The page is an article, which a run over articles moves.";

/// Why a run over articles leaves a page that is no article.
const NOT_AN_ARTICLE: &str = "The page is no longer an article, which a run over user pages moves.";

const TRASHED: &str = "The page is in the trash. Put it back, and the next run converts it.";

/// How many times a page is read and converted again when it changes
/// between its read and its flip, before it is left for the next run.
const ATTEMPTS: usize = 3;

/// One run at a time in this process, whichever door started it.
static RUNNING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The server moves every markdown user page to blocks when it starts, and
/// one it missed when the app opens it ([`convert_on_open`]). On unless
/// `VIRTUES_CONVERT_PAGES=0`, a kill switch (`pages::on_unless_zero`): the
/// app edits only block pages. Takes effect only while block pages are on.
pub fn convert_on_startup() -> bool {
    pages::on_unless_zero(std::env::var("VIRTUES_CONVERT_PAGES").ok().as_deref())
}

/// `VIRTUES_CONVERT_ARTICLES=1`: the server moves every opened markdown
/// article to blocks when it starts. Takes effect only while block articles
/// are on (`block_move::tree_articles_enabled`).
pub fn convert_articles_on_startup() -> bool {
    converts_on_startup(std::env::var("VIRTUES_CONVERT_ARTICLES").ok().as_deref())
}

/// Whether a startup switch set to `value` turns its move on: `1` alone.
/// Anything else, `true` and `yes` included, leaves it off.
fn converts_on_startup(value: Option<&str>) -> bool {
    value.is_some_and(|v| v.trim() == "1")
}

/// Where each run's report is kept: `VIRTUES_PAGE_CONVERSION_DIR` when
/// set, else `page-conversion` under the box's state root
/// (`data_disk::data_dir`), else, in a dev checkout with no state root, a
/// gitignored folder beside the code.
pub fn report_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("VIRTUES_PAGE_CONVERSION_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let root = crate::data_disk::data_dir();
    if root.is_dir() {
        return root.join("page-conversion");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.page-conversion-state")
}

/// The switches a run of each kind waits on: block pages for any, block
/// articles as well for articles.
#[derive(Debug, Clone, Copy)]
struct Switches {
    block_pages: bool,
    block_articles: bool,
}

impl Switches {
    fn read() -> Self {
        Self {
            block_pages: pages::tree_pages_enabled(),
            block_articles: block_move::tree_articles_enabled(),
        }
    }

    /// Why a run of `which` moves nothing, or `None` when it may move.
    fn refuse(self, which: Which) -> Option<&'static str> {
        if !self.block_pages {
            Some(BLOCK_PAGES_OFF)
        } else if which == Which::Articles && !self.block_articles {
            Some(ARTICLES_OFF)
        } else {
            None
        }
    }
}

/// Start the startup conversion when [`convert_on_startup`] or
/// [`convert_articles_on_startup`] says so: in the background, so the
/// server serves meanwhile, each page moved under its doc lock as any run
/// moves it. User pages first, then articles.
pub fn spawn_on_startup(yjs: YjsState) {
    let mut runs = vec![];
    if convert_on_startup() {
        runs.push(Which::Pages);
    }
    if convert_articles_on_startup() {
        runs.push(Which::Articles);
    }
    spawn_if(runs, Switches::read(), yjs, report_dir());
}

/// [`spawn_on_startup`] with its switches read: the runs' task when `runs`
/// names any, each moving pages only while `switches` let it ([`apply`]),
/// its report kept in `dir`.
fn spawn_if(runs: Vec<Which>, switches: Switches, yjs: YjsState, dir: PathBuf) -> Option<tokio::task::JoinHandle<()>> {
    if runs.is_empty() {
        return None;
    }
    Some(tokio::spawn(async move {
        for which in runs {
            match apply_checked(&yjs, Mover::Server, which, switches, &dir).await {
                Ok(report) => tracing::info!(
                    which = which.as_str(),
                    converted = report.converted,
                    failed = report.failed,
                    with_notes = report.with_notes,
                    kept_at = report.kept_at.as_deref().unwrap_or(""),
                    "moved markdown pages to blocks at startup"
                ),
                Err(e) => tracing::error!(
                    which = which.as_str(),
                    error = %e,
                    "did not move markdown pages to blocks at startup"
                ),
            }
        }
    }))
}

/// Convert every markdown page of a kind in memory and report what
/// converting each would change, writing no page. Reads the database only,
/// so it runs in any process that can open it, a copy of a box's included.
pub async fn dry_run(pool: &PgPool, which: Which) -> Result<Report> {
    dry_run_into(pool, which, &report_dir(), None).await
}

/// [`dry_run`], and, for articles, each day page's markdown, export and
/// notes' anchors written into `texts`, a file a page
/// (`dryrun.readers.test.ts` reads them). Those files hold the pages'
/// words: the folder is the caller's to keep on scratch space and delete.
pub async fn dry_run_keeping_texts(pool: &PgPool, which: Which, texts: Option<&Path>) -> Result<Report> {
    dry_run_into(pool, which, &report_dir(), texts).await
}

/// [`dry_run_keeping_texts`], its report kept in `dir`.
pub async fn dry_run_into(pool: &PgPool, which: Which, dir: &Path, texts: Option<&Path>) -> Result<Report> {
    if let Some(texts) = texts {
        tokio::fs::create_dir_all(texts)
            .await
            .map_err(|e| Error::Other(format!("Couldn't make the folder for the pages' texts: {e}")))?;
    }
    let started_at = Utc::now();
    let mut results = vec![];
    for (n, id) in markdown_pages(pool, which).await?.iter().enumerate() {
        let texts = texts.map(|dir| dir.join(format!("{n:05}.json")));
        results.push(dry_run_page(pool, which, id, texts.as_deref()).await);
    }
    Ok(finish(Run::DryRun, which, started_at, results, dir).await)
}

/// Move every markdown page of a kind to blocks (for articles, every
/// opened one), in this process, which must be the server's: it holds the
/// pages' live docs and their queued saves. Refused while block pages are
/// off, or, for articles, block articles. A run started while another is
/// going waits for it. The server's own run, at startup; the CLI's is
/// [`apply_on`].
pub async fn apply(yjs: &YjsState, which: Which) -> Result<Report> {
    apply_checked(yjs, Mover::Server, which, Switches::read(), &report_dir()).await
}

/// Which database a connection reaches: its name, and when the Postgres
/// server holding it started, to the microsecond. `--apply` names the
/// database `DATABASE_URL` points at this way, and the server that answers
/// on the port moves pages only when its own database is that one
/// ([`apply_on`]). A name alone says too little: a copy of a box restored
/// under the box's own name into another Postgres (another port, another
/// machine) has it too. Two Postgres servers never start in the same
/// microsecond, and one holds a single database of a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct DatabaseIdentity {
    pub name: String,
    /// `pg_postmaster_start_time()`, in microseconds since the epoch.
    pub started_us: i64,
}

/// The database `pool` reaches, as [`DatabaseIdentity`] says it.
pub async fn database_identity(pool: &PgPool) -> Result<DatabaseIdentity> {
    let (name, started_us): (String, i64) = sqlx::query_as(
        "SELECT current_database()::text, \
                (extract(epoch FROM pg_postmaster_start_time()) * 1000000)::bigint",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to read which database this is: {e}")))?;
    Ok(DatabaseIdentity { name, started_us })
}

/// Why `--apply` moved nothing when it named no database: an older CLI, or
/// a caller other than the CLI.
pub const NO_DATABASE_NAMED: &str = "The request named no database, so your server moved no page. \
     Run `virtues pages convert --apply` from this machine, with DATABASE_URL set to the server's database.";

/// [`apply`] as the CLI asks for it, refused with nothing moved unless this
/// server's database is `database` ([`DatabaseIdentity`]): the CLI finds the
/// server by its port alone, so a run meant for a copy of a box must not
/// move the pages of whatever server answers there. `None`, a request that
/// names no database, is refused too.
pub async fn apply_on(yjs: &YjsState, database: Option<&DatabaseIdentity>, which: Which) -> Result<Report> {
    let Some(database) = database else {
        return Err(Error::InvalidInput(NO_DATABASE_NAMED.into()));
    };
    apply_checked(yjs, Mover::Cli(database), which, Switches::read(), &report_dir()).await
}

/// Whose run it is: the server's own, at startup, or the CLI's, for the
/// database it names.
#[derive(Clone, Copy)]
enum Mover<'a> {
    Server,
    Cli(&'a DatabaseIdentity),
}

/// A run of `which` for `mover`, with the switches it waits on, its report
/// kept in `dir`: refused with nothing moved for another database, or
/// while a switch it waits on is off.
async fn apply_checked(
    yjs: &YjsState,
    mover: Mover<'_>,
    which: Which,
    switches: Switches,
    dir: &Path,
) -> Result<Report> {
    if let Mover::Cli(named) = mover {
        let ours = database_identity(&yjs.pool).await?;
        if &ours != named {
            let which = if ours.name == named.name {
                format!("another database named {}", named.name)
            } else {
                format!("the database {}, not {}", ours.name, named.name)
            };
            return Err(Error::InvalidInput(format!(
                "The server that answered serves {which}, so it moved no page. \
                 Run the move against the server that serves the database DATABASE_URL points at."
            )));
        }
    }
    if let Some(why) = switches.refuse(which) {
        return Err(Error::InvalidInput(why.into()));
    }
    apply_which_into(yjs, which, dir).await
}

/// [`apply`] of user pages whatever the switches say, its report kept in
/// `dir`.
pub async fn apply_into(yjs: &YjsState, dir: &Path) -> Result<Report> {
    apply_which_into(yjs, Which::Pages, dir).await
}

/// [`apply`] whatever the switches say, its report kept in `dir`.
pub async fn apply_which_into(yjs: &YjsState, which: Which, dir: &Path) -> Result<Report> {
    // A second run waits for the first, then finds what it left.
    let _one = RUNNING.lock().await;
    let started_at = Utc::now();
    let mut results = vec![];
    for id in markdown_pages(&yjs.pool, which).await? {
        results.push(apply_page(yjs, which, &id).await);
    }
    Ok(finish(Run::Apply, which, started_at, results, dir).await)
}

/// Move the markdown user page `id` to blocks as the app opens it
/// (`GET /api/pages/:id`), by the one move a run makes ([`apply_page`]): a
/// page startup missed, or written since, moves as it is opened. Only while
/// block pages and their move are on. `None` when there is nothing to move:
/// the page holds blocks, is an article (which moves at first open on its
/// own, `block_move::first_open`), is in the trash or is gone. A page that
/// stays markdown is logged with why, and opens read only.
pub async fn convert_on_open(yjs: &YjsState, id: &str) -> Option<PageResult> {
    let on = Switches::read().refuse(Which::Pages).is_none() && convert_on_startup();
    convert_on_open_if(on, yjs, id).await
}

async fn convert_on_open_if(on: bool, yjs: &YjsState, id: &str) -> Option<PageResult> {
    if !on {
        return None;
    }
    let waiting: std::result::Result<Option<bool>, sqlx::Error> = sqlx::query_scalar(
        "SELECT p.format = 'markdown' AND p.kind = 'page' AND p.deleted_at IS NULL \
                AND NOT EXISTS (SELECT 1 FROM wiki_articles a WHERE a.page_id = p.id) \
         FROM app_pages p WHERE p.id = $1",
    )
    .bind(id)
    .fetch_optional(&yjs.pool)
    .await;
    match waiting {
        Ok(Some(true)) => {}
        Ok(_) => return None,
        Err(e) => {
            tracing::warn!(page_id = id, error = %e, "couldn't read whether an opened page waits to move to blocks");
            return None;
        }
    }
    let result = apply_page(yjs, Which::Pages, id).await;
    match result.outcome {
        Outcome::Converted => {
            tracing::info!(page_id = id, notes = result.notes.len(), "a markdown page moved to blocks as it was opened")
        }
        Outcome::Failed => tracing::warn!(
            page_id = id,
            reason = result.reason.as_deref().unwrap_or(""),
            "a markdown page stays markdown as it is opened"
        ),
        _ => {}
    }
    Some(result)
}

/// The markdown pages of a kind, oldest first, the trash included so the
/// report names what a run left there.
async fn markdown_pages(pool: &PgPool, which: Which) -> Result<Vec<String>> {
    let owned = "EXISTS (SELECT 1 FROM wiki_articles a WHERE a.page_id = p.id)";
    let of_the_kind = match which {
        Which::Pages => format!("p.kind = 'page' AND NOT {owned}"),
        Which::Articles => format!("(p.kind = 'article' OR {owned})"),
    };
    sqlx::query_scalar(&format!(
        "SELECT p.id FROM app_pages p WHERE p.format = $1 AND {of_the_kind} ORDER BY p.created_at, p.id"
    ))
    .bind(PageFormat::Markdown.as_str())
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list the markdown pages: {e}")))
}

/// A page's row as a conversion reads it.
#[derive(sqlx::FromRow)]
struct Row {
    title: String,
    kind: String,
    format: String,
    content: String,
    yjs_state: Option<Vec<u8>>,
    updated_at: DateTime<Utc>,
    trashed: bool,
    /// The `wiki_articles` row that owns the page, if one does: its id, its
    /// subject, and its writers' last edition with the owner's sentences.
    article_id: Option<String>,
    subject_type: Option<String>,
    subject_id: Option<String>,
    machine_text: Option<String>,
    theirs: i32,
}

impl Row {
    /// The page is an article: by its kind, or by the `wiki_articles` row
    /// that owns it whatever its kind says.
    fn is_article(&self) -> bool {
        self.kind == "article" || self.article_id.is_some()
    }

    /// The subject an article's notes are written on.
    fn subject(&self) -> Option<(&str, &str)> {
        Some((self.subject_type.as_deref()?, self.subject_id.as_deref()?))
    }
}

async fn read_row(pool: &PgPool, id: &str) -> Result<Option<Row>> {
    sqlx::query_as(
        "SELECT p.title, p.kind, p.format, p.content, p.yjs_state, p.updated_at, \
                p.deleted_at IS NOT NULL AS trashed, \
                a.id AS article_id, a.subject_type, a.subject_id, a.machine_text, \
                COALESCE(CASE WHEN jsonb_typeof(a.theirs) = 'array' THEN jsonb_array_length(a.theirs) END, 0) AS theirs \
         FROM app_pages p \
         LEFT JOIN LATERAL ( \
             SELECT id, subject_type, subject_id, machine_text, theirs FROM wiki_articles \
             WHERE page_id = p.id ORDER BY id LIMIT 1 \
         ) a ON true \
         WHERE p.id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to read page {id}: {e}")))
}

/// The page's markdown as its editor shows it, and the state that holds it:
/// the Y.Text of its saved state, or `content`, as a state of its own, for
/// a page nobody has opened. `Err` when the saved state does not read.
fn markdown_of(row: &Row) -> std::result::Result<Written, String> {
    match &row.yjs_state {
        Some(state) => {
            let text = crate::server::yjs::text_of_state_as(state, PageFormat::Markdown)
                .map_err(|e| e.to_string())?;
            Ok(Written {
                text,
                state: state.clone(),
                tree: None,
            })
        }
        None => Ok(Written {
            text: row.content.clone(),
            state: crate::server::yjs::state_from_text(&row.content),
            tree: None,
        }),
    }
}

/// Convert `markdown` as `which` reads it, off the async workers.
async fn convert(markdown: &str, which: Which) -> std::result::Result<Converted, String> {
    let markdown = markdown.to_string();
    match pages::convert_off_workers(move || block_move::convert(&markdown, which.dialect())).await {
        Ok(converted) => converted,
        Err(e) => Err(e.to_string()),
    }
}

/// What judging an article's move found: its report, the edition it moves
/// with, and why it stays markdown, when it does.
struct Judged {
    result: ArticleResult,
    rebase: Option<ArticleRebase>,
    held: Option<String>,
}

/// Judge the move of the article `row`, whose markdown `source` converted
/// as `converted`: its edition rebased ([`block_move::rebase`]), its notes'
/// anchors before and after ([`block_move::anchors_lost`]), and what the
/// export changed. Held back when the conversion is ([`Converted::held_back`]),
/// the edition does not move, or a note would lose its place.
async fn judge_article(pool: &PgPool, row: &Row, source: &str, converted: &Converted) -> Judged {
    let export = converted.blocks.text.as_str();
    let counts = converted.article.clone().unwrap_or_default();
    let mut held = converted.held_back().map(str::to_string);

    let rebase = match &row.article_id {
        Some(id) => match block_move::rebase(pool, id, source, export).await {
            Ok(rebase) => Some(rebase),
            Err(why) => {
                held.get_or_insert(why.to_string());
                None
            }
        },
        None => None,
    };

    // A note lands on a page when its words land among the page's passages:
    // the notes that land on a page are those lost from it to an empty one.
    let mut anchors_landing = (0, 0);
    if let Some(subject) = row.subject() {
        let before = block_move::anchors_lost(pool, subject, source, "").await;
        let after = block_move::anchors_lost(pool, subject, export, "").await;
        let lost = block_move::anchors_lost(pool, subject, source, export).await;
        match (before, after, lost) {
            (Ok(before), Ok(after), Ok(lost)) => {
                anchors_landing = (before.len(), after.len());
                if !lost.is_empty() {
                    held.get_or_insert(block_move::ANCHOR_LOST.to_string());
                }
            }
            (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => {
                held.get_or_insert(format!(
                    "Your server couldn't read the notes beside the article ({e}), so it left the article in markdown."
                ));
            }
        }
    }

    let edition = match &row.machine_text {
        None => EditionMove::None,
        Some(m) if m == source => EditionMove::Kept,
        Some(_) => EditionMove::Rebased,
    };
    let untouched_before = row.machine_text.as_deref() == Some(source);
    let untouched_after = match rebase.as_ref().and_then(|r| r.machine_text.as_deref()) {
        Some(edition) => block_move::reads_the_same(PageFormat::Tree, true, export, edition).await,
        None => false,
    };
    let theirs_after = rebase.as_ref().map_or(0, |r| r.theirs.len());
    let result = ArticleResult {
        opened: row.yjs_state.is_some(),
        edition,
        untouched_before,
        untouched_after,
        theirs_kept: (usize::try_from(row.theirs).unwrap_or(0), theirs_after),
        anchors_landing,
        content_changed: first_change(source, export),
        changed_by: changed_by(source, export, &counts),
        counts,
    };
    Judged { result, rebase, held }
}

/// The first line where `export` departs from `source`, numbered from 1,
/// with the export's line.
fn first_change(source: &str, export: &str) -> Option<String> {
    if source == export {
        return None;
    }
    let (a, b): (Vec<&str>, Vec<&str>) = (source.lines().collect(), export.lines().collect());
    let n = (0..a.len().max(b.len())).find(|&i| a.get(i) != b.get(i)).unwrap_or(a.len().min(b.len()));
    Some(format!("line {}: {}", n + 1, b.get(n).copied().unwrap_or("(the end)")))
}

/// One way the export spells a line differently, and how to read a line
/// past it.
type Spelling = (&'static str, fn(&str) -> String);

/// What tells a line's export from its markdown, by kind.
const SPELLINGS: [Spelling; 5] = [
    ("escapes", unescaped),
    ("table rules", table_rule),
    ("list markers", list_marker),
    ("veil placement", unveiled),
    ("spaces", spaced),
];

/// A line with each run of spaces as one, and none at its ends.
fn spaced(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A line with its escapes and character references read.
fn unescaped(line: &str) -> String {
    crate::api::wiki_editor::unescape(line)
}

/// A line without the veil's brackets.
fn unveiled(line: &str) -> String {
    line.replace(['⟦', '⟧'], "")
}

/// A table row with its cells' padding gone and its rule row's dashes as
/// one, so `|---|` and `| --- |` read alike.
fn table_rule(line: &str) -> String {
    let t = line.trim();
    if !t.starts_with('|') {
        return line.to_string();
    }
    t.trim_matches('|')
        .split('|')
        .map(|cell| {
            let cell = cell.trim();
            let rule = cell.trim_matches(':');
            if !rule.is_empty() && rule.chars().all(|c| c == '-') {
                format!("{}-{}", if cell.starts_with(':') { ":" } else { "" }, if cell.ends_with(':') { ":" } else { "" })
            } else {
                cell.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// A list item's line with its marker and indent as one, so `* a`, `- a`
/// and `1) a` read alike.
fn list_marker(line: &str) -> String {
    let t = line.trim_start();
    let marker = t
        .strip_prefix(['-', '*', '+'])
        .or_else(|| {
            let digits = t.len() - t.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            (digits > 0).then(|| t[digits..].strip_prefix(['.', ')'])).flatten()
        })
        .filter(|rest| rest.starts_with([' ', '\t']));
    match marker {
        Some(rest) => format!("• {}", rest.trim_start()),
        None => line.to_string(),
    }
}

/// The kinds of spelling (`SPELLINGS`) that tell `a` from `b`: the ones
/// without which they still differ. `None` when all of them together do
/// not make the two alike.
fn spelled_apart(a: &str, b: &str) -> Option<Vec<&'static str>> {
    let read = |line: &str, skip: Option<usize>| -> String {
        SPELLINGS
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != skip)
            .fold(line.to_string(), |l, (_, (_, f))| f(&l))
    };
    if read(a, None) != read(b, None) {
        return None;
    }
    let needed: Vec<&'static str> = (0..SPELLINGS.len())
        .filter(|&i| read(a, Some(i)) != read(b, Some(i)))
        .map(|i| SPELLINGS[i].0)
        .collect();
    if needed.is_empty() {
        // Any one of two spellings makes the lines alike: name the first
        // that changes either.
        return Some(
            SPELLINGS
                .iter()
                .find(|(_, f)| f(a) != a || f(b) != b)
                .map(|(name, _)| vec![*name])
                .unwrap_or_default(),
        );
    }
    Some(needed)
}

/// Whether a line is a source's definition, `[^label]: note`.
fn is_definition(line: &str) -> bool {
    line.strip_prefix("[^").and_then(|rest| rest.find("]:")).is_some_and(|end| end > 0)
}

/// What changed between an article's markdown and its export, by kind
/// ([`ArticleResult::changed_by`]): read line by line, each changed line
/// put down to the spellings that tell it apart ([`spelled_apart`]).
fn changed_by(source: &str, export: &str, counts: &ArticleCounts) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = vec![];
    let mut add = |kind: &'static str| {
        if !out.contains(&kind) {
            out.push(kind);
        }
    };
    if !counts.orphan_notes.is_empty() {
        add("orphan notes");
    }
    // Every line, a blank one at the end included, is a line of the body
    // ended by its newline; the newline ending the text is named on its own
    // below, so it is no line here.
    let split = |md: &str| -> (String, Vec<String>) {
        let lines = md.strip_suffix('\n').unwrap_or(md).split('\n');
        let (defs, body): (Vec<&str>, Vec<&str>) = lines.partition(|l| is_definition(l.trim_start()));
        (
            body.iter().map(|l| format!("{l}\n")).collect(),
            defs.iter().map(|d| d.trim().to_string()).collect(),
        )
    };
    let ((body_a, defs_a), (body_b, defs_b)) = (split(source), split(export));
    if defs_a != defs_b {
        let kept: Vec<&String> = defs_a
            .iter()
            .filter(|d| !counts.orphan_notes.iter().any(|o| d.starts_with(&format!("[^{o}]:"))))
            .collect();
        let (mut x, mut y): (Vec<&String>, Vec<&String>) = (kept.clone(), defs_b.iter().collect());
        x.sort();
        y.sort();
        if x == y {
            if kept != defs_b.iter().collect::<Vec<_>>() {
                add("note order");
            }
        } else {
            add("other");
        }
    }

    let diff = similar::TextDiff::from_lines(body_a.as_str(), body_b.as_str());
    for op in diff.ops() {
        let (old, new) = (op.old_range(), op.new_range());
        let a: Vec<&str> = diff.old_slices()[old].iter().map(|l| l.trim_end_matches('\n')).collect();
        let b: Vec<&str> = diff.new_slices()[new].iter().map(|l| l.trim_end_matches('\n')).collect();
        match op.tag() {
            similar::DiffTag::Equal => {}
            similar::DiffTag::Insert | similar::DiffTag::Delete => {
                if a.iter().chain(&b).all(|l| l.trim().is_empty()) {
                    add("blank lines");
                } else {
                    add("other");
                }
            }
            similar::DiffTag::Replace if a.len() == b.len() => {
                for (x, y) in a.iter().zip(&b) {
                    match spelled_apart(x, y) {
                        Some(kinds) => kinds.into_iter().for_each(&mut add),
                        None => add("other"),
                    }
                }
            }
            similar::DiffTag::Replace => {
                let (x, y) = (a.join(" "), b.join(" "));
                match spelled_apart(x.trim(), y.trim()) {
                    Some(kinds) => {
                        add("line breaks");
                        kinds.into_iter().for_each(&mut add);
                    }
                    None => add("other"),
                }
            }
        }
    }
    if source.ends_with('\n') != export.ends_with('\n') {
        add("final newline");
    }
    out
}

/// What a day page's texts file holds, for the readers' check.
#[derive(Serialize)]
struct DayTexts<'a> {
    source: &'a str,
    export: &'a str,
    anchors: Vec<serde_json::Value>,
}

/// Write a day page's markdown, its export and its open notes' anchors to
/// `path`, for the readers' check (`--texts`). A failure is the report's,
/// as a note: the run goes on.
async fn keep_texts(pool: &PgPool, row: &Row, source: &str, export: &str, path: &Path) -> std::result::Result<(), String> {
    let Some((subject_type, subject_id)) = row.subject().filter(|(t, _)| *t == "day") else {
        return Ok(());
    };
    let anchors: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT anchor FROM wiki_notes \
         WHERE subject_type = $1 AND subject_id = $2 AND resolved_at IS NULL AND anchor IS NOT NULL \
         ORDER BY id",
    )
    .bind(subject_type)
    .bind(subject_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to read the notes beside the article: {e}"))?;
    let texts = DayTexts { source, export, anchors };
    let json = serde_json::to_vec_pretty(&texts).map_err(|e| e.to_string())?;
    tokio::fs::write(path, json).await.map_err(|e| e.to_string())
}

async fn dry_run_page(pool: &PgPool, which: Which, id: &str, texts: Option<&Path>) -> PageResult {
    let row = match read_row(pool, id).await {
        Ok(Some(row)) => row,
        Ok(None) => return PageResult::skipped(id, "", GONE),
        Err(e) => return PageResult::failed(id, "", e.to_string()),
    };
    // A dry run over articles measures the ones in the trash too: they move
    // once put back, and the bar is for every article on the box.
    let trashed = which == Which::Articles && row.trashed;
    if let Some(skip) = not_to_convert(&row, which).filter(|&skip| !(trashed && skip == TRASHED)) {
        return PageResult::skipped(id, &row.title, skip);
    }
    let markdown = match markdown_of(&row) {
        Ok(written) => written.text,
        Err(e) => return PageResult::failed(id, &row.title, unreadable(&e)),
    };
    let converted = match convert(&markdown, which).await {
        Ok(converted) => converted,
        Err(reason) => return PageResult::failed(id, &row.title, reason),
    };
    let judged = match which {
        Which::Articles => Some(judge_article(pool, &row, &markdown, &converted).await),
        Which::Pages => None,
    };
    let held = match &judged {
        Some(j) => j.held.clone(),
        None => converted.held_back().map(str::to_string),
    };
    let outcome = match (&held, &judged) {
        _ if trashed => Outcome::Skipped,
        (Some(_), _) => Outcome::Failed,
        (None, Some(j)) if !j.result.opened => Outcome::ConvertsWhenOpened,
        (None, _) => Outcome::WouldConvert,
    };
    let mut result = PageResult::new(id, &row.title, outcome).with(&converted).judged(judged.as_ref());
    result.reason = if trashed { Some(TRASHED.to_string()) } else { held };
    if let Some(path) = texts {
        if let Err(e) = keep_texts(pool, &row, &markdown, &converted.blocks.text, path).await {
            result.notes.push(Problem::new("texts", format!("its texts were not written: {e}")));
        }
    }
    result
}

/// Why a row is not one for a run of `which` to convert, or `None` when it
/// is.
fn not_to_convert(row: &Row, which: Which) -> Option<&'static str> {
    match which {
        Which::Pages if row.kind != "page" || row.article_id.is_some() => return Some(AN_ARTICLE),
        Which::Articles if !row.is_article() => return Some(NOT_AN_ARTICLE),
        _ => {}
    }
    if row.format != PageFormat::Markdown.as_str() {
        return Some(ALREADY_BLOCKS);
    }
    if row.trashed {
        return Some(TRASHED);
    }
    None
}

fn unreadable(error: &str) -> String {
    format!("Your server couldn't read the page's saved document ({error}), so it left the page in markdown.")
}

/// What one attempt at a page came to.
enum Attempt {
    Done(Box<PageResult>),
    /// The page changed between its read and its flip: read it again. What
    /// converting it found, and the version that holds it as it was when an
    /// earlier run kept it so, for the report of a page that stays
    /// markdown.
    Again {
        converted: Box<Converted>,
        judged: Option<Box<ArticleResult>>,
        restore_point: Option<i64>,
    },
}

async fn apply_page(yjs: &YjsState, which: Which, id: &str) -> PageResult {
    let mut title = String::new();
    let mut last = None;
    for _ in 0..ATTEMPTS {
        match try_page(yjs, which, id, &mut title).await {
            Attempt::Done(result) => return *result,
            Attempt::Again { converted, judged, restore_point } => last = Some((converted, judged, restore_point)),
        }
    }
    let mut result = PageResult::failed(id, &title, STAYS_MARKDOWN);
    if let Some((converted, judged, restore_point)) = last {
        result = result.with(&converted);
        result.article = judged.map(|j| *j);
        result.restore_point = restore_point;
    }
    result
}

/// Save, read, convert, keep and flip one page, in the page's write turn,
/// so no machine writer of the page lands in between.
async fn try_page(yjs: &YjsState, which: Which, id: &str, title: &mut String) -> Attempt {
    let pool = &yjs.pool;
    let _turn = yjs.write_turn(id).await;
    if let Err(e) = yjs.save_page_now(id).await {
        return Attempt::Done(Box::new(PageResult::failed(
            id,
            title,
            format!("Your server couldn't save the page's last edits first, so it left the page in markdown: {e}"),
        )));
    }
    let row = match read_row(pool, id).await {
        Ok(Some(row)) => row,
        Ok(None) => return Attempt::Done(Box::new(PageResult::skipped(id, title, GONE))),
        Err(e) => return Attempt::Done(Box::new(PageResult::failed(id, title, e.to_string()))),
    };
    title.clone_from(&row.title);
    if let Some(skip) = not_to_convert(&row, which) {
        return Attempt::Done(Box::new(PageResult::skipped(id, title, skip)));
    }
    let before = match markdown_of(&row) {
        Ok(written) => written,
        Err(e) => return Attempt::Done(Box::new(PageResult::failed(id, title, unreadable(&e)))),
    };
    let converted = match convert(&before.text, which).await {
        Ok(converted) => converted,
        Err(reason) => return Attempt::Done(Box::new(PageResult::failed(id, title, reason))),
    };
    let judged = match which {
        Which::Articles => Some(judge_article(pool, &row, &before.text, &converted).await),
        Which::Pages => None,
    };
    let held = match &judged {
        Some(j) => j.held.clone(),
        None => converted.held_back().map(str::to_string),
    };
    if let Some(why) = held {
        let result = PageResult::failed(id, title, why).with(&converted).judged(judged.as_ref());
        return Attempt::Done(Box::new(result));
    }
    if let Some(j) = judged.as_ref().filter(|j| !j.result.opened) {
        // Nobody has opened it: it moves when first opened, so holding a
        // document keeps meaning someone opened it.
        let result = PageResult::new(id, title, Outcome::ConvertsWhenOpened).with(&converted).judged(Some(j));
        return Attempt::Done(Box::new(result));
    }

    let kept = match pages::keep_before_blocks(pool, id, before).await {
        Ok(kept) => kept,
        Err(e) => {
            return Attempt::Done(Box::new(PageResult::failed(
                id,
                title,
                format!("Your server couldn't keep a copy of the page as it was, so it changed nothing: {e}"),
            )))
        }
    };
    let blocks = &converted.blocks;
    let rebase = judged.as_ref().and_then(|j| j.rebase.as_ref());
    let moved = yjs
        .move_to_tree(id, row.updated_at, &blocks.state, &blocks.text, rebase)
        .await;
    after_flip(pool, id, title, kept, moved, converted, judged.map(|j| Box::new(j.result))).await
}

/// What a page's flip came to, with the copy of the page as it was that
/// was kept for it. The report names that copy wherever it stays. A page
/// that changed since its read did not move: a copy this attempt cut is
/// dropped, as the next attempt keeps the page as it then is, so a page
/// typed in at every attempt is left no copy of a move that never
/// happened (never pruned and always listed in its History).
async fn after_flip(
    pool: &PgPool,
    id: &str,
    title: &str,
    kept: pages::KeptBefore,
    moved: std::result::Result<TreeMove, String>,
    converted: Converted,
    judged: Option<Box<ArticleResult>>,
) -> Attempt {
    let restore_point = Some(kept.point.version_number);
    let mut result = match moved {
        Ok(TreeMove::Moved) => {
            let version = pages::cut_version(pool, id, &converted.blocks, "auto", Some(MOVED_TO_BLOCKS)).await;
            let mut result = PageResult::new(id, title, Outcome::Converted).with(&converted);
            result.version = version;
            result
        }
        Ok(TreeMove::Changed) if !kept.cut => {
            return Attempt::Again {
                converted: Box::new(converted),
                judged,
                restore_point,
            }
        }
        Ok(TreeMove::Changed) => match pages::drop_before_blocks(pool, id, kept.point.version_number).await {
            Ok(()) => {
                return Attempt::Again {
                    converted: Box::new(converted),
                    judged,
                    restore_point: None,
                }
            }
            Err(e) => PageResult::failed(
                id,
                title,
                format!(
                    "The page changed while your server converted it, so it left the page in markdown, \
                     with a copy of the page as it was in its History: {e}"
                ),
            )
            .with(&converted),
        },
        Ok(TreeMove::AlreadyTree) => PageResult::skipped(id, title, ALREADY_BLOCKS),
        Err(e) => PageResult::failed(id, title, e).with(&converted),
    };
    result.restore_point = restore_point;
    result.article = judged.map(|j| *j);
    Attempt::Done(Box::new(result))
}

/// Count the run's pages, log each one with notes or a failure, and keep
/// the report in `dir`.
async fn finish(run: Run, which: Which, started_at: DateTime<Utc>, pages: Vec<PageResult>, dir: &Path) -> Report {
    let count = |o: Outcome| pages.iter().filter(|p| p.outcome == o).count();
    for p in &pages {
        let categories: Vec<String> = p
            .notes
            .iter()
            .map(|n| virtues_document::migrate::category(&n.message))
            .collect();
        let changed_by = p.article.as_ref().map(|a| a.changed_by.clone()).unwrap_or_default();
        match p.outcome {
            Outcome::Failed => tracing::warn!(
                run = run.as_str(),
                which = which.as_str(),
                page = %p.id,
                reason = p.reason.as_deref().unwrap_or(""),
                notes = ?categories,
                "a markdown page stays markdown"
            ),
            _ if p.changed_on_the_way_in() => tracing::info!(
                run = run.as_str(),
                which = which.as_str(),
                page = %p.id,
                outcome = ?p.outcome,
                notes = ?categories,
                changed_by = ?changed_by,
                export_differs = p.export_differs.is_some(),
                "converting a markdown page changed it on the way in"
            ),
            _ => {}
        }
    }
    let mut report = Report {
        run,
        which,
        started_at,
        finished_at: Utc::now(),
        contract: virtues_document::contract().version,
        converted: count(Outcome::Converted),
        would_convert: count(Outcome::WouldConvert),
        converts_when_opened: count(Outcome::ConvertsWhenOpened),
        failed: count(Outcome::Failed),
        skipped: count(Outcome::Skipped),
        with_notes: pages.iter().filter(|p| p.changed_on_the_way_in()).count(),
        pages,
        kept_at: None,
        not_kept: None,
    };
    match keep(&report, dir).await {
        Ok(path) => report.kept_at = Some(path.display().to_string()),
        Err(e) => {
            tracing::warn!(dir = %dir.display(), error = %e, "did not keep the page conversion report");
            report.not_kept = Some(e);
        }
    }
    tracing::info!(
        run = run.as_str(),
        which = which.as_str(),
        converted = report.converted,
        would_convert = report.would_convert,
        converts_when_opened = report.converts_when_opened,
        failed = report.failed,
        skipped = report.skipped,
        with_notes = report.with_notes,
        "page conversion run finished"
    );
    report
}

/// Write the report as `<dir>/<started>-<run>-<which>.json`. The path, or
/// why not.
async fn keep(report: &Report, dir: &Path) -> std::result::Result<PathBuf, String> {
    let name = format!(
        "{}-{}-{}.json",
        report.started_at.format("%Y%m%dT%H%M%S%.3fZ"),
        report.run.as_str(),
        report.which.as_str()
    );
    let path = dir.join(name);
    let json = serde_json::to_vec_pretty(report).map_err(|e| e.to_string())?;
    tokio::fs::create_dir_all(dir).await.map_err(|e| e.to_string())?;
    tokio::fs::write(&path, json).await.map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::pages::CreatePageRequest;
    use virtues_document::Node;

    /// Both switches on, as a test that runs a move past them gives.
    const ON: Switches = Switches { block_pages: true, block_articles: true };

    /// `markdown` converted as a user page is.
    fn convert(markdown: &str) -> Converted {
        block_move::convert(markdown, Dialect::Page).unwrap()
    }

    /// A markdown page whose editor has saved `content`, as a page looks
    /// once it has been open.
    async fn markdown_page(pool: &PgPool, content: &str) -> String {
        let id = unopened_page(pool, content).await;
        sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
            .bind(crate::server::yjs::state_from_text(content))
            .bind(&id)
            .execute(pool)
            .await
            .unwrap();
        id
    }

    /// A markdown page nobody has opened: `content` only.
    async fn unopened_page(pool: &PgPool, content: &str) -> String {
        let req = CreatePageRequest {
            title: "Notes".into(),
            content: content.into(),
            project_id: None,
            icon: None,
            icon_color: None,
            cover_url: None,
            tags: None,
            format: Some(crate::api::pages::PageFormat::Markdown),
        };
        pages::create_page_as(pool, req, PageFormat::Markdown)
            .await
            .unwrap()
            .page
            .id
    }

    #[derive(Debug, PartialEq, sqlx::FromRow)]
    struct Stored {
        format: String,
        content: String,
        yjs_state: Option<Vec<u8>>,
        updated_at: DateTime<Utc>,
    }

    async fn stored(pool: &PgPool, id: &str) -> Stored {
        sqlx::query_as("SELECT format, content, yjs_state, updated_at FROM app_pages WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn versions(pool: &PgPool, id: &str) -> Vec<pages::PageVersionSummary> {
        pages::list_versions(pool, id, Some(100)).await.unwrap().versions
    }

    fn kinds(tree: &[Node]) -> Vec<&str> {
        tree.iter().map(|n| n.kind.as_str()).collect()
    }

    fn result<'a>(report: &'a Report, id: &str) -> &'a PageResult {
        report.pages.iter().find(|p| p.id == id).expect("the page is in the report")
    }

    /// A page moves with its text, its place in every list (its
    /// `updated_at`), a restore point holding it as it was and a version
    /// holding its blocks; the run's report is kept.
    #[sqlx::test]
    async fn a_markdown_page_moves_to_blocks_and_keeps_its_place(pool: PgPool) {
        let markdown = "## Plan\n\nLunch with [@Nick](/person/person_1) on Friday.\n\n- [ ] Book a table\n";
        let id = markdown_page(&pool, markdown).await;
        let unopened = unopened_page(&pool, "Written before anyone opened it.\n").await;
        let before = stored(&pool, &id).await;
        let unopened_before = stored(&pool, &unopened).await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();

        let report = apply_into(&yjs, dir.path()).await.unwrap();
        assert_eq!((report.converted, report.failed), (2, 0), "{report:?}");
        let page = result(&report, &id);
        assert_eq!(page.outcome, Outcome::Converted, "{page:?}");
        assert_eq!(page.notes, []);

        let after = stored(&pool, &id).await;
        assert_eq!(after.format, "tree");
        assert_eq!(after.updated_at, before.updated_at, "a move is no edit");
        assert_eq!(after.content, markdown, "the export says what the page said");
        let tree = yjs.read_tree(&id).await.unwrap();
        assert_eq!(kinds(&tree), ["heading", "paragraph", "taskList"]);
        let unopened_after = stored(&pool, &unopened).await;
        assert_eq!(unopened_after.format, "tree");
        assert_eq!(unopened_after.updated_at, unopened_before.updated_at);
        assert_eq!(yjs.read_text(&unopened).await.unwrap(), "Written before anyone opened it.\n");

        let history = versions(&pool, &id).await;
        let [blocks, kept] = &history[..] else {
            panic!("the page as it was and the blocks: {history:?}")
        };
        assert_eq!((kept.created_by.as_str(), kept.description.as_deref()), ("auto", Some(pages::BEFORE_BLOCKS)));
        assert_eq!(page.restore_point, Some(kept.version_number));
        assert_eq!(page.version, Some(blocks.version_number));
        assert_eq!(blocks.description.as_deref(), Some(MOVED_TO_BLOCKS));
        let then = pages::get_version(&pool, &kept.id).await.unwrap();
        assert_eq!((then.format, then.markdown.as_str()), (PageFormat::Markdown, markdown));
        assert_eq!(pages::get_version(&pool, &blocks.id).await.unwrap().format, PageFormat::Tree);

        let kept_at = report.kept_at.as_deref().expect("the report is kept");
        assert!(kept_at.starts_with(&dir.path().display().to_string()));
        let on_disk: serde_json::Value = serde_json::from_slice(&std::fs::read(kept_at).unwrap()).unwrap();
        assert_eq!(on_disk["run"], "apply");
        assert_eq!(on_disk["converted"], 2);
    }

    /// Run again, a run finds nothing to do and writes nothing; and a run
    /// stopped after a page's restore point does not cut it a second one.
    #[sqlx::test]
    async fn a_second_run_moves_nothing_and_a_stopped_run_resumes(pool: PgPool) {
        let moved = markdown_page(&pool, "Lunch on Friday.\n").await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        apply_into(&yjs, dir.path()).await.unwrap();
        let first = stored(&pool, &moved).await;
        let history = versions(&pool, &moved).await.len();

        let again = apply_into(&yjs, dir.path()).await.unwrap();
        assert!(again.pages.is_empty(), "{again:?}");
        assert_eq!(stored(&pool, &moved).await, first);
        assert_eq!(versions(&pool, &moved).await.len(), history);

        // Stopped after keeping the page as it was and before the flip.
        let stopped = markdown_page(&pool, "Dinner on Saturday.\n").await;
        let row = read_row(&pool, &stopped).await.unwrap().unwrap();
        pages::keep_before_blocks(&pool, &stopped, markdown_of(&row).unwrap())
            .await
            .unwrap();
        let resumed = apply_into(&yjs, dir.path()).await.unwrap();
        assert_eq!(result(&resumed, &stopped).outcome, Outcome::Converted);
        let history = versions(&pool, &stopped).await;
        assert_eq!(history.len(), 2, "the page as it was, once, and the blocks: {history:?}");
        assert_eq!(result(&resumed, &stopped).restore_point, Some(1));
    }

    /// A page that does not convert, or whose saved document does not read,
    /// stays markdown, untouched, and is reported with the reason; the rest
    /// of the run goes on.
    #[sqlx::test]
    async fn a_page_that_does_not_convert_stays_markdown_and_is_reported(pool: PgPool) {
        let rows = "| a | b | c | d |\n| - | - | - | - |\n".to_string() + &"| 1 | 2 | 3 | 4 |\n".repeat(20_000);
        let too_big = markdown_page(&pool, &rows).await;
        let unreadable = markdown_page(&pool, "Saved.\n").await;
        sqlx::query("UPDATE app_pages SET yjs_state = '\\x0102ff' WHERE id = $1")
            .bind(&unreadable)
            .execute(&pool)
            .await
            .unwrap();
        let fine = markdown_page(&pool, "Fine.\n").await;
        let before = [stored(&pool, &too_big).await, stored(&pool, &unreadable).await];
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();

        let report = apply_into(&yjs, dir.path()).await.unwrap();
        assert_eq!((report.converted, report.failed), (1, 2), "{report:?}");
        assert_eq!(result(&report, &fine).outcome, Outcome::Converted);
        let reason = |id: &str| result(&report, id).reason.clone().unwrap_or_default();
        assert!(reason(&too_big).contains("couldn't turn the page's markdown into blocks"), "{}", reason(&too_big));
        assert!(reason(&unreadable).contains("couldn't read the page's saved document"), "{}", reason(&unreadable));
        assert_eq!([stored(&pool, &too_big).await, stored(&pool, &unreadable).await], before);
        assert!(versions(&pool, &too_big).await.is_empty(), "nothing kept for a page that did not move");

        // The next run reads them again.
        let again = apply_into(&yjs, dir.path()).await.unwrap();
        assert_eq!((again.converted, again.failed), (0, 2));
    }

    /// A page typed in between its read and its flip, at every attempt,
    /// stays markdown, reported with what converting it last found, and is
    /// left no copy of itself by the run: the copy each attempt kept is
    /// dropped once the page did not move, as the next attempt keeps the
    /// page as it then is. Each was left behind, three a run, never pruned,
    /// listed in its History and named nowhere in the report, and the
    /// report dropped the conversion's notes.
    #[sqlx::test]
    async fn a_page_that_changes_at_every_flip_is_left_no_copy_by_the_run(pool: PgPool) {
        // Every copy kept of a page as it was is followed by typing on the
        // page, before the flip that checks the row.
        sqlx::query(
            "CREATE FUNCTION typed_meanwhile() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN UPDATE app_pages SET content = content || ' typed' WHERE id = NEW.page_id; RETURN NEW; END $$",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER typed_meanwhile AFTER INSERT ON app_page_versions FOR EACH ROW \
             WHEN (NEW.description = 'Before blocks') EXECUTE FUNCTION typed_meanwhile()",
        )
        .execute(&pool)
        .await
        .unwrap();
        let id = unopened_page(&pool, "Lunch\tat noon.\n").await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();

        let report = apply_into(&yjs, dir.path()).await.unwrap();
        let page = result(&report, &id);
        assert_eq!(page.outcome, Outcome::Failed, "{page:?}");
        assert_eq!(page.reason.as_deref(), Some(STAYS_MARKDOWN));
        assert!(!page.notes.is_empty(), "what converting it found: {page:?}");
        assert_eq!(page.restore_point, None, "{page:?}");
        assert_eq!(stored(&pool, &id).await.format, "markdown");
        let history = versions(&pool, &id).await;
        assert!(
            history.iter().all(|v| v.description.as_deref() != Some(pages::BEFORE_BLOCKS)),
            "no copy of a move that did not happen: {history:?}"
        );
    }

    /// A flip that fails after the page was kept as it was leaves that copy,
    /// and the report names it: it named none.
    #[sqlx::test]
    async fn a_flip_that_fails_reports_the_copy_it_kept(pool: PgPool) {
        sqlx::query(
            "CREATE FUNCTION no_blocks() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN RAISE EXCEPTION 'no blocks here'; END $$",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER no_blocks BEFORE UPDATE ON app_pages FOR EACH ROW \
             WHEN (NEW.format = 'tree') EXECUTE FUNCTION no_blocks()",
        )
        .execute(&pool)
        .await
        .unwrap();
        let id = markdown_page(&pool, "Lunch\tat noon.\n").await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();

        let report = apply_into(&yjs, dir.path()).await.unwrap();
        let page = result(&report, &id);
        assert_eq!(page.outcome, Outcome::Failed, "{page:?}");
        assert!(!page.notes.is_empty(), "{page:?}");
        let history = versions(&pool, &id).await;
        let [kept] = &history[..] else { panic!("the page as it was: {history:?}") };
        assert_eq!(kept.description.as_deref(), Some(pages::BEFORE_BLOCKS));
        assert_eq!(page.restore_point, Some(kept.version_number), "{page:?}");
        assert_eq!(stored(&pool, &id).await.format, "markdown");
    }

    /// A markdown page startup missed moves as the app opens it, by the
    /// run's own move: kept as it was, flipped, versioned as blocks.
    #[sqlx::test]
    async fn a_markdown_page_moves_to_blocks_as_it_is_opened(pool: PgPool) {
        let id = markdown_page(&pool, "Lunch with Nick.\n").await;
        let yjs = YjsState::new(pool.clone());
        let page = convert_on_open_if(true, &yjs, &id).await.expect("a markdown page moves");
        assert_eq!(page.outcome, Outcome::Converted, "{page:?}");
        assert_eq!(stored(&pool, &id).await.format, "tree");
        let history: Vec<_> = versions(&pool, &id).await.into_iter().filter_map(|v| v.description).collect();
        assert!(history.iter().any(|d| d == pages::BEFORE_BLOCKS), "{history:?}");
        assert!(history.iter().any(|d| d == MOVED_TO_BLOCKS), "{history:?}");
        // Opened again, it holds blocks: nothing to move.
        assert!(convert_on_open_if(true, &yjs, &id).await.is_none());
    }

    /// A page that cannot move stays markdown as it is opened, and the open
    /// goes on: the app shows it read only. Nothing moves with the switch
    /// off, and nothing but a user page in markdown is touched.
    #[sqlx::test]
    async fn opening_moves_nothing_it_should_not(pool: PgPool) {
        let yjs = YjsState::new(pool.clone());
        let lossy = markdown_page(&pool, LOSSY).await;
        let page = convert_on_open_if(true, &yjs, &lossy).await.expect("it was tried");
        assert_eq!(page.outcome, Outcome::Failed, "{page:?}");
        assert!(page.reason.is_some(), "{page:?}");
        assert_eq!(stored(&pool, &lossy).await.format, "markdown");

        let off = markdown_page(&pool, "Lunch.\n").await;
        assert!(convert_on_open_if(false, &yjs, &off).await.is_none());
        assert_eq!(stored(&pool, &off).await.format, "markdown");

        let trashed = markdown_page(&pool, "Lunch.\n").await;
        sqlx::query("UPDATE app_pages SET deleted_at = now() WHERE id = $1").bind(&trashed).execute(&pool).await.unwrap();
        assert!(convert_on_open_if(true, &yjs, &trashed).await.is_none());
        assert_eq!(stored(&pool, &trashed).await.format, "markdown");
        assert!(convert_on_open_if(true, &yjs, "page_gone").await.is_none());
    }

    /// The page conversion switch is on unless `VIRTUES_CONVERT_PAGES=0`.
    #[test]
    fn page_conversion_is_on_unless_switched_off_with_0() {
        assert!(pages::on_unless_zero(None));
        assert!(!pages::on_unless_zero(Some("0")));
    }

    /// A page whose blocks would lose what it says stays markdown: here a
    /// link written inside an image's description, whose address a page's
    /// image has nowhere to keep.
    #[sqlx::test]
    async fn a_page_that_would_lose_text_stays_markdown(pool: PgPool) {
        let id = markdown_page(&pool, LOSSY).await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        let report = apply_into(&yjs, dir.path()).await.unwrap();
        let page = result(&report, &id);
        assert_eq!(page.outcome, Outcome::Failed, "{page:?}");
        assert_eq!(page.addresses_lost, ["/page/page_1"]);
        assert_eq!(stored(&pool, &id).await.format, "markdown");
    }

    /// A page's link definitions are lines of its blocks, a list keeps its
    /// own numbers, spaces that collapse are noted, and an email autolink
    /// keeps the page moving: the definitions were dropped with nothing
    /// said, a list numbered by years was numbered on from its first, and
    /// the autolink left the page in markdown for an address it kept.
    #[test]
    fn what_a_pages_editor_showed_moves_or_is_noted() {
        let c = convert("Week plan\n\n[Monday]: gym\n[Source]: https://example.com/plan\n\nWrite to <nick@example.com>.\n");
        assert!(!c.loses(), "{:?} {:?}", c.text_lost, c.addresses_lost);
        let tree = c.blocks.tree.unwrap();
        let text: Vec<String> = tree.iter().map(Node::text_content).collect();
        assert!(text.iter().any(|t| t.contains("[Monday]: gym") && t.contains("[Source]: https://example.com/plan")), "{text:?}");

        let c = convert("2019. Moved to the coast\n2021. Started the boat\n\nTotal:     $5\n");
        let notes: Vec<&str> = c.notes.iter().map(|n| n.message.as_str()).collect();
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("Total:     $5"), "{notes:?}");
        let tree = c.blocks.tree.unwrap();
        assert_eq!(tree[0].attrs["start"], 2019);
        let own: Vec<&serde_json::Value> = tree[0].content.iter().map(|item| &item.attrs["value"]).collect();
        assert_eq!(own, [&serde_json::Value::Null, &serde_json::json!(2021)]);

        // The editor read no character reference, escape or single `~`:
        // they move as typed, with nothing to note. A code block's info past
        // its language is noted.
        let c = convert(
            "Write &amp; and &lt;b&gt; at \\\\server\\share, ~not struck~, A&nbsp;B.\n\n```rust title=main.rs\nfn main() {}\n```\n",
        );
        assert!(!c.loses(), "{:?} {:?}", c.text_lost, c.addresses_lost);
        let tree = c.blocks.tree.unwrap();
        assert_eq!(tree[0].text_content(), "Write &amp; and &lt;b&gt; at \\\\server\\share, ~not struck~, A&nbsp;B.");
        assert!(tree[0].content.iter().all(|n| n.marks.is_empty()), "{:?}", tree[0]);
        let notes: Vec<&str> = c.notes.iter().map(|n| n.message.as_str()).collect();
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("title=main.rs"), "{notes:?}");

        // Nor any front matter: a page opening with `---`, a line and `---`
        // showed a rule and a heading.
        let c = convert("---\nMeeting notes\n---\nDiscussed the budget.\n");
        assert!(!c.loses(), "{:?} {:?}", c.text_lost, c.addresses_lost);
        assert_eq!(c.notes, []);
        let tree = c.blocks.tree.unwrap();
        assert_eq!(kinds(&tree), ["horizontalRule", "heading", "paragraph"]);
        assert_eq!(tree[1].text_content(), "Meeting notes");
    }

    /// A pending proposal holding a mention, an image or a line break moved
    /// as marks on its text alone: rejecting one left its mention behind,
    /// accepting a change kept both mentions, and one around an image was
    /// gone with nothing said. Kept as its text, the page stays markdown,
    /// where its owner can still accept or reject it; a proposal of text
    /// alone moves as a suggestion, a mention beside it included.
    #[test]
    fn a_suggestion_holding_a_mention_an_image_or_a_line_break_keeps_the_page_in_markdown() {
        for md in [
            "Plan: {++Lunch with [@Nick](/person/p_1) on Friday.++}\n",
            "Met {--[@Nick](/person/p_1) at noon--}{++[@Nick](/person/p_1) on Friday++}\n",
            "{--![x](https://example.com/x.png)--}\n",
            "{++![x](https://example.com/x.png)++}\n",
            "{++Lunch on Friday\nby the water++}\n",
        ] {
            let c = convert(md);
            assert!(!c.loses(), "{md}: {:?} {:?}", c.text_lost, c.addresses_lost);
            assert_eq!(c.held_back(), Some(block_move::SUGGESTION_HELD), "{md}");
            assert_eq!(c.suggestions, 0, "{md}");
            let notes: Vec<&str> = c.notes.iter().map(|n| n.message.as_str()).collect();
            assert!(notes.iter().any(|n| n.contains("holds a mention, an image or a line break")), "{md}: {notes:?}");
        }
        let c = convert("Plan: {++Lunch on Friday++} with [@Nick](/person/p_1).\n");
        assert_eq!(c.held_back(), None, "{:?}", c.notes);
        assert_eq!(c.suggestions, 1);
    }

    /// Markdown whose blocks would drop an address with no note saying so.
    const LOSSY: &str = "Before.\n\n![see [the plan](/page/page_1)](https://example.com/harbour.png)\n\nAfter.\n";

    /// A page moves as its markdown editor showed it: what its owner typed
    /// between angle brackets, their comments and every line of a
    /// paragraph come through character for character, and put back from
    /// History the version from before reads as the blocks do.
    #[sqlx::test]
    async fn a_page_moves_as_its_editor_showed_it(pool: PgPool) {
        let markdown = "Check if a<b and c>d holds.\n\n\
            Dear <name>,\nThanks for <thing you liked>.\n\n\
            <!-- remember to call the bank -->\n\n\
            Groceries\nMilk\nEggs\n";
        let id = markdown_page(&pool, markdown).await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        let dry = dry_run_into(&pool, Which::Pages, dir.path(), None).await.unwrap();
        assert_eq!(result(&dry, &id).outcome, Outcome::WouldConvert, "{dry:?}");
        let report = apply_into(&yjs, dir.path()).await.unwrap();
        let page = result(&report, &id);
        assert_eq!(page.outcome, Outcome::Converted, "{page:?}");
        assert_eq!(page.notes, []);

        let tree = yjs.read_tree(&id).await.unwrap();
        let lines = |n: &Node| -> Vec<String> {
            let mut lines = vec![String::new()];
            for c in &n.content {
                match (&c.text, c.kind.as_str()) {
                    (Some(t), _) => lines.last_mut().unwrap().push_str(t),
                    (None, "hardBreak") => lines.push(String::new()),
                    _ => {}
                }
            }
            lines
        };
        assert_eq!(lines(&tree[0]), ["Check if a<b and c>d holds."]);
        assert_eq!(lines(&tree[1]), ["Dear <name>,", "Thanks for <thing you liked>."]);
        assert_eq!(lines(&tree[2]), ["<!-- remember to call the bank -->"]);
        assert_eq!(lines(&tree[3]), ["Groceries", "Milk", "Eggs"]);

        let kept = versions(&pool, &id).await.last().unwrap().clone();
        let restored = pages::restore_version(&pool, &yjs, &id, &kept.id).await.unwrap();
        assert!(!restored.changed, "the page already reads as the version from before");
    }

    /// The page as it was before the move is the one copy of what the
    /// blocks left out (a code block's info past its language here):
    /// however many versions come after it, it is kept, and History lists
    /// it.
    #[sqlx::test]
    async fn the_page_as_it_was_outlives_every_later_version(pool: PgPool) {
        let id = markdown_page(&pool, "```rust title=main.rs\nfn main() {}\n```\n").await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        let report = apply_into(&yjs, dir.path()).await.unwrap();
        let kept = result(&report, &id).restore_point.expect("the run keeps the page as it was");
        for i in 0..60 {
            pages::cut_version_now(&pool, &yjs, &id, "auto", Some(&format!("Auto-saved (idle) {i}"))).await.unwrap();
        }
        let listed = pages::list_versions(&pool, &id, None).await.unwrap().versions;
        let before = listed.iter().find(|v| v.version_number == kept).expect("History lists the page as it was");
        assert_eq!(before.description.as_deref(), Some(pages::BEFORE_BLOCKS));
        assert_eq!(listed.len(), 21, "the newest twenty, and the page as it was");
        let then = pages::get_version(&pool, &before.id).await.unwrap();
        assert!(then.markdown.contains("title=main.rs"), "{}", then.markdown);
    }

    /// `--apply` names the database the CLI was pointed at: a server that
    /// serves another refuses, and moves no page, block pages on or not. A
    /// copy of a box restored under the box's own name into another Postgres
    /// is another database, and so is one a request does not name.
    #[sqlx::test]
    async fn a_run_meant_for_another_database_moves_nothing(pool: PgPool) {
        let id = markdown_page(&pool, "Lunch.\n").await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        let ours = database_identity(&pool).await.unwrap();
        let renamed = DatabaseIdentity { name: "virtues_doc_dryrun".into(), ..ours.clone() };
        let copy = DatabaseIdentity { started_us: ours.started_us - 1, ..ours.clone() };
        for (other, says) in [(&renamed, "virtues_doc_dryrun"), (&copy, "another database named")] {
            match apply_checked(&yjs, Mover::Cli(other), Which::Pages, ON, dir.path()).await {
                Err(Error::InvalidInput(why)) => assert!(why.contains(says), "{other:?}: {why}"),
                ran => panic!("{other:?}: {ran:?}"),
            }
        }
        match apply_on(&yjs, None, Which::Pages).await {
            Err(Error::InvalidInput(why)) => assert_eq!(why, NO_DATABASE_NAMED),
            ran => panic!("{ran:?}"),
        }
        assert_eq!(stored(&pool, &id).await.format, "markdown");
        assert!(versions(&pool, &id).await.is_empty());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "no run, no report");

        // The database the server serves passes, by any connection to it.
        let again = database_identity(&yjs.pool).await.unwrap();
        assert_eq!(again, ours);
    }

    /// The move is one-way, so what keeps it from running is held here:
    /// the startup switch is on for `1` alone, and every run, by the console
    /// or at startup, moves nothing while block pages are off. Each test of
    /// a move calls `apply_into`, past both.
    #[test]
    fn the_startup_switch_is_on_for_1_alone() {
        for off in [None, Some(""), Some("0"), Some("true"), Some("yes"), Some("01"), Some("1 1")] {
            assert!(!converts_on_startup(off), "{off:?}");
        }
        for on in [Some("1"), Some(" 1\n")] {
            assert!(converts_on_startup(on), "{on:?}");
        }
    }

    #[sqlx::test]
    async fn a_run_while_block_pages_are_off_moves_nothing(pool: PgPool) {
        let id = markdown_page(&pool, "Lunch.\n").await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        let ours = database_identity(&pool).await.unwrap();
        let off = Switches { block_pages: false, block_articles: true };
        for (mover, who) in [(Mover::Server, "the server"), (Mover::Cli(&ours), "the CLI")] {
            match apply_checked(&yjs, mover, Which::Pages, off, dir.path()).await {
                Err(Error::InvalidInput(why)) => assert_eq!(why, BLOCK_PAGES_OFF, "{who}"),
                other => panic!("{who}: {other:?}"),
            }
        }
        assert_eq!(stored(&pool, &id).await.format, "markdown");
        assert!(versions(&pool, &id).await.is_empty());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "no run, no report");
    }

    /// The startup move starts only when switched on, and then moves pages
    /// only while block pages are on as well: tests never set that flag, as
    /// no real box does unasked.
    #[sqlx::test]
    async fn the_startup_move_waits_for_its_switch_and_for_block_pages(pool: PgPool) {
        let id = markdown_page(&pool, "Lunch.\n").await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        assert!(spawn_if(vec![], ON, yjs.clone(), dir.path().into()).is_none());
        let off = Switches { block_pages: false, block_articles: false };
        spawn_if(vec![Which::Pages], off, yjs, dir.path().into())
            .expect("switched on, the move starts")
            .await
            .unwrap();
        assert_eq!(stored(&pool, &id).await.format, "markdown");
        assert!(versions(&pool, &id).await.is_empty());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "no run, no report");
    }

    /// `VIRTUES_CONVERT_ARTICLES` reads as `VIRTUES_CONVERT_PAGES` does, and
    /// the startup move of articles waits on block articles as well as
    /// block pages; switched on with both, it moves the opened articles and
    /// no user page.
    #[sqlx::test]
    async fn startup_switch_moves_articles_only_when_both_switches_are_on(pool: PgPool) {
        let article = opened_article(&pool, "person", "p_1", "Nick", PERSON).await;
        let page = markdown_page(&pool, "Lunch.\n").await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        for (switches, says) in [
            (Switches { block_pages: true, block_articles: false }, "block pages alone"),
            (Switches { block_pages: false, block_articles: true }, "block articles alone"),
        ] {
            spawn_if(vec![Which::Articles], switches, yjs.clone(), dir.path().into())
                .expect("asked for, the run starts")
                .await
                .unwrap();
            assert_eq!(stored(&pool, &article.page_id).await.format, "markdown", "{says}");
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "no run, no report");

        spawn_if(vec![Which::Articles], ON, yjs, dir.path().into()).unwrap().await.unwrap();
        assert_eq!(stored(&pool, &article.page_id).await.format, "tree");
        assert_eq!(stored(&pool, &page).await.format, "markdown", "articles were asked for, not pages");
    }

    /// Pending CriticMarkup proposals move as the contract's suggestion
    /// marks, the owner's to accept or reject, never as text; the export
    /// writes them as CriticMarkup again.
    #[sqlx::test]
    async fn pending_proposals_become_suggestions(pool: PgPool) {
        let markdown = "Lunch {--at noon--}{++on Friday++} by the water.\n\n- Bring {++the map++}\n";
        let id = markdown_page(&pool, markdown).await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        let report = apply_into(&yjs, dir.path()).await.unwrap();
        let page = result(&report, &id);
        assert_eq!(page.outcome, Outcome::Converted, "{page:?}");
        assert_eq!(page.suggestions, 2);

        let tree = yjs.read_tree(&id).await.unwrap();
        let mut marked = vec![];
        let mut text = String::new();
        for n in &tree {
            n.walk(&mut |n| {
                if let Some(t) = &n.text {
                    text.push_str(t);
                    for m in &n.marks {
                        marked.push((m.kind.clone(), t.clone()));
                    }
                }
            });
        }
        assert!(!text.contains("{--") && !text.contains("{++"), "no proposal as text: {text}");
        assert_eq!(
            marked,
            [
                ("proposedDeletion".to_string(), "at noon".to_string()),
                ("proposedInsertion".to_string(), "on Friday".to_string()),
                ("proposedInsertion".to_string(), "the map".to_string()),
            ]
        );
        assert_eq!(stored(&pool, &id).await.content, markdown);
    }

    /// Front matter, raw HTML and media embeds: each converted as the
    /// page's editor showed it (front matter as the rule and the heading it
    /// drew) and the contract holds it, and a version from before the move
    /// put back by converting it again.
    #[sqlx::test]
    async fn front_matter_html_and_media_move_as_the_editor_showed_them(pool: PgPool) {
        let markdown = "---\ntitle: Trip\n---\n# Trip\n\n\
            Lunch <u>by the water</u> with <span style=\"color: red\">red</span> sails.\n\n\
            ![voice.m4a](/drive/df_3)\n\n\
            ![Harbour](https://images.example.com/photo-1)\n\n\
            ![](https://example.com/report.pdf)\n";
        let id = markdown_page(&pool, markdown).await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        let report = apply_into(&yjs, dir.path()).await.unwrap();
        let page = result(&report, &id);
        assert_eq!(page.outcome, Outcome::Converted, "{page:?}");
        assert!(!page.notes.iter().any(|n| n.message.contains("front matter")), "{:?}", page.notes);

        // Underline the editor drew; the span it showed as text.
        let tree = yjs.read_tree(&id).await.unwrap();
        assert_eq!(kinds(&tree), ["horizontalRule", "heading", "heading", "paragraph", "audio", "image", "file"]);
        assert_eq!(tree[1].text_content(), "title: Trip");
        assert_eq!(tree[3].text_content(), SAILS);
        assert!(tree[3].content.iter().any(|n| n.marks.iter().any(|m| m.kind == "underline")));

        // The restore point holds the markdown; put back, it converts again.
        let history = versions(&pool, &id).await;
        let kept = history.last().unwrap();
        let restored = pages::restore_version(&pool, &yjs, &id, &kept.id).await.unwrap();
        assert!(!restored.changed, "the page already reads as the converted version");

        let ids: Vec<String> = tree.iter().filter_map(|n| n.id().map(str::to_string)).collect();
        yjs.edit_tree(&id, None, &[virtues_document::Op::Delete { id: ids[3].clone() }])
            .await
            .unwrap();
        let restored = pages::restore_version(&pool, &yjs, &id, &kept.id).await.unwrap();
        assert!(restored.changed);
        assert_eq!(yjs.read_tree(&id).await.unwrap()[3].text_content(), SAILS);
    }

    const SAILS: &str = "Lunch by the water with <span style=\"color: red\">red</span> sails.";

    /// A wiki article in the writers' shape: a lede, a section, a list and a
    /// table. Fictional.
    const PERSON: &str = "Nick is a friend from the rowing club, met at the R&B night in 2019.\n\n\
        ## Rowing\n\n\
        - Saturday mornings on the river\n- The spring regatta\n\n\
        | Year | Race |\n|---|---|\n| 2024 | Head of the river |\n";

    /// A day page in the narrator's shape: an Abstract, a timed section,
    /// sources after sentences, a table with its own source, a quote and
    /// veiled names. Fictional.
    const DAY: &str = "You met [⟦Nick⟧](/person/p_1) at ⟦the clinic⟧ before the R&B night.[^ev-1][^ev-2]\n\n\
        ## The walk home[^cx-1]\n\n\
        You walked by the water with [David Okafor](/person/p_2).[^ev-3]\n\n\
        | Stop | Time |\n|---|---|\n| Pier | 8:40 PM |\n\n\
        [^ev-4]\n\n\
        ```figure\nkind: quote\ntext: Did you get home safe?\nwho: Nick\nref: data_communication_message:m_4\ntime: 9:44 PM\n```\n\n\
        [^ev-1]: Message · 7:40 PM · data_communication_message:m_1\n\
        [^ev-2]: Recording · 7:52 PM · data_communication_transcription:t_1\n\
        [^cx-1]: 8:10–9:50 PM\n\
        [^ev-3]: Recording · 8:30 PM · data_communication_transcription:t_2\n\
        [^ev-4]: Recording · 8:40 PM · data_communication_transcription:t_3\n";

    /// An article as its writers left it: `content`, and their edition of it
    /// (`machine_text`) when `edition` says so.
    async fn article(pool: &PgPool, kind: &str, subject: &str, title: &str, content: &str) -> crate::api::wiki_articles::Article {
        let article = crate::api::wiki_articles::create_article(pool, kind, subject, title, content).await.unwrap();
        sqlx::query("UPDATE wiki_articles SET machine_text = $1 WHERE id = $2")
            .bind(content)
            .bind(&article.id)
            .execute(pool)
            .await
            .unwrap();
        article
    }

    /// [`article`], opened: its editor has saved a document of it.
    async fn opened_article(pool: &PgPool, kind: &str, subject: &str, title: &str, content: &str) -> crate::api::wiki_articles::Article {
        let article = article(pool, kind, subject, title, content).await;
        sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
            .bind(crate::server::yjs::state_from_text(content))
            .bind(&article.page_id)
            .execute(pool)
            .await
            .unwrap();
        article
    }

    #[derive(Debug, PartialEq, sqlx::FromRow)]
    struct Edition {
        machine_text: Option<String>,
        theirs: serde_json::Value,
        last_written_at: Option<DateTime<Utc>>,
        last_human_edit_at: Option<DateTime<Utc>>,
    }

    async fn edition(pool: &PgPool, article_id: &str) -> Edition {
        sqlx::query_as("SELECT machine_text, theirs, last_written_at, last_human_edit_at FROM wiki_articles WHERE id = $1")
            .bind(article_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// A run over user pages leaves every article, by its kind and by the
    /// `wiki_articles` row that owns a page whatever its kind says, and so
    /// does a dry run; a run over articles moves them, and leaves the user
    /// pages.
    #[sqlx::test]
    async fn articles_move_only_when_asked_and_pages_stay_the_default(pool: PgPool) {
        let article = opened_article(&pool, "person", "p_1", "Nick", PERSON).await;
        let other = opened_article(&pool, "person", "p_2", "David Okafor", "David Okafor rows on Saturdays.\n").await;
        sqlx::query("UPDATE app_pages SET kind = 'page' WHERE id = $1")
            .bind(&other.page_id)
            .execute(&pool)
            .await
            .unwrap();
        let page = markdown_page(&pool, "Lunch on Friday.\n").await;
        let before = [stored(&pool, &article.page_id).await, stored(&pool, &other.page_id).await];
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();

        let report = apply_into(&yjs, dir.path()).await.unwrap();
        assert_eq!(report.which, Which::Pages);
        assert_eq!(report.pages.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), [page.as_str()]);
        assert_eq!([stored(&pool, &article.page_id).await, stored(&pool, &other.page_id).await], before);
        let dry = dry_run_into(&pool, Which::Pages, dir.path(), None).await.unwrap();
        assert!(dry.pages.is_empty(), "{dry:?}");

        let report = apply_which_into(&yjs, Which::Articles, dir.path()).await.unwrap();
        assert_eq!(report.which, Which::Articles);
        assert_eq!((report.converted, report.failed), (2, 0), "{report:?}");
        for id in [&article.page_id, &other.page_id] {
            assert_eq!(stored(&pool, id).await.format, "tree");
        }
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|f| f.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|n| n.ends_with("-apply-articles.json")), "{names:?}");
        assert!(names.iter().any(|n| n.ends_with("-apply-pages.json")), "{names:?}");
    }

    /// An opened article moves with its sources, figures and veils as the
    /// contract's own nodes, its place in every list, and its edition: one
    /// its writers left untouched is the export now, so it still reads as
    /// theirs; one its owner changed is converted on its own, and the
    /// owner's sentence takes the export's spelling, so it still reads as
    /// the owner's. Neither stamp moves: a move is no one's edit.
    #[sqlx::test]
    async fn an_opened_article_moves_with_its_edition_rebased(pool: PgPool) {
        let untouched = opened_article(&pool, "day", "day_2026-03-14", "Saturday, March 14", DAY).await;
        let edited_md = PERSON.replacen("in 2019.", "in 2019. I owe Nick for the R&B tickets.", 1);
        let edited = opened_article(&pool, "person", "p_1", "Nick", &edited_md).await;
        let (theirs, _) = crate::api::wiki_editor::provenance(PERSON, &edited_md);
        assert_eq!(theirs, ["I owe Nick for the R&B tickets."]);
        sqlx::query("UPDATE wiki_articles SET machine_text = $1, theirs = $2 WHERE id = $3")
            .bind(PERSON)
            .bind(serde_json::json!(theirs))
            .bind(&edited.id)
            .execute(&pool)
            .await
            .unwrap();
        let before = [edition(&pool, &untouched.id).await, edition(&pool, &edited.id).await];
        let rows_before = [stored(&pool, &untouched.page_id).await, stored(&pool, &edited.page_id).await];
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();

        let report = apply_which_into(&yjs, Which::Articles, dir.path()).await.unwrap();
        assert_eq!((report.converted, report.failed), (2, 0), "{report:?}");

        // The untouched day page: its edition is its export.
        let page = result(&report, &untouched.page_id);
        let a = page.article.as_ref().expect("an article's report");
        assert_eq!(a.edition, EditionMove::Kept);
        assert!(a.opened && a.untouched_before && a.untouched_after, "{a:?}");
        assert!(a.counts.holds(), "{:?}", a.counts);
        assert!(a.changed_by.contains(&"escapes"), "R&B is escaped in the export: {:?}", a.changed_by);
        let after = stored(&pool, &untouched.page_id).await;
        assert_eq!(after.format, "tree");
        assert_eq!(after.updated_at, rows_before[0].updated_at, "a move is no edit");
        assert_eq!(yjs.read_text(&untouched.page_id).await.unwrap(), after.content);
        let e = edition(&pool, &untouched.id).await;
        assert_eq!(e.machine_text.as_deref(), Some(after.content.as_str()));
        assert_eq!((e.last_written_at, e.last_human_edit_at), (before[0].last_written_at, before[0].last_human_edit_at));
        let tree = yjs.read_tree(&untouched.page_id).await.unwrap();
        let mut kinds_in = vec![];
        for n in &tree {
            n.walk(&mut |n| {
                kinds_in.push(n.kind.clone());
                kinds_in.extend(n.marks.iter().map(|m| m.kind.clone()));
            });
        }
        for kind in ["footnote", "figure", "veil", "table"] {
            assert!(kinds_in.iter().any(|k| k == kind), "{kind} in {kinds_in:?}");
        }
        assert!(!after.content.contains("\\[^"), "sources stay sources: {}", after.content);

        // The edited wiki article: its edition converted on its own, the
        // owner's sentence in the export's spelling.
        let page = result(&report, &edited.page_id);
        let a = page.article.as_ref().unwrap();
        assert_eq!(a.edition, EditionMove::Rebased);
        assert!(!a.untouched_before && !a.untouched_after, "{a:?}");
        assert_eq!(a.theirs_kept, (1, 1));
        let e = edition(&pool, &edited.id).await;
        assert_eq!(e.machine_text, Some(virtues_document::edition_of(PERSON).unwrap()));
        assert_eq!(e.theirs, serde_json::json!(["I owe Nick for the R\\&B tickets."]));
        let after = stored(&pool, &edited.page_id).await;
        assert_eq!(after.updated_at, rows_before[1].updated_at);

        // History holds the article as it was and its blocks, neither an edit.
        let history = versions(&pool, &untouched.page_id).await;
        let descriptions: Vec<Option<&str>> = history.iter().map(|v| v.description.as_deref()).collect();
        assert!(descriptions.contains(&Some(pages::BEFORE_BLOCKS)), "{descriptions:?}");
        assert!(descriptions.contains(&Some(MOVED_TO_BLOCKS)), "{descriptions:?}");
    }

    /// An article nobody has opened is measured, and left: it moves when it
    /// is first opened, so a run gives it no document, no version and no new
    /// edition.
    #[sqlx::test]
    async fn an_unopened_article_converts_when_opened(pool: PgPool) {
        let unopened = article(&pool, "day", "day_2026-03-14", "Saturday, March 14", DAY).await;
        let before = (stored(&pool, &unopened.page_id).await, edition(&pool, &unopened.id).await);
        let history = versions(&pool, &unopened.page_id).await.len();
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();

        let report = apply_which_into(&yjs, Which::Articles, dir.path()).await.unwrap();
        assert_eq!((report.converts_when_opened, report.converted, report.failed), (1, 0, 0), "{report:?}");
        let page = result(&report, &unopened.page_id);
        assert_eq!(page.outcome, Outcome::ConvertsWhenOpened, "{page:?}");
        assert!(!page.article.as_ref().unwrap().opened);
        assert_eq!((stored(&pool, &unopened.page_id).await, edition(&pool, &unopened.id).await), before);
        assert_eq!(versions(&pool, &unopened.page_id).await.len(), history);
    }

    /// An article whose export says it otherwise than its markdown (a final
    /// newline, an escape), with no step of the converter's noted, changed
    /// on the way in: the run's count says so, as its log does.
    #[sqlx::test]
    async fn an_export_change_counts_as_a_change_on_the_way_in(pool: PgPool) {
        article(&pool, "person", "p_1", "Nick", "You met Nick at the shop.").await;
        article(&pool, "person", "p_2", "Zoe", "You met Zoe at the R&B night.\n").await;
        article(&pool, "person", "p_3", "Sam", "You met Sam at the station.\n").await;
        let dir = tempfile::tempdir().unwrap();

        let report = dry_run_into(&pool, Which::Articles, dir.path(), None).await.unwrap();
        assert_eq!(report.converts_when_opened, 3, "{report:?}");
        assert!(report.pages.iter().all(|p| p.notes.is_empty() && p.export_differs.is_none()), "{report:?}");
        assert_eq!(report.with_notes, 2, "{report:?}");
    }

    /// A dry run over articles measures every one: opened, unopened, and in
    /// the trash, reported as such. It writes no page; asked to, it writes
    /// each day page's markdown, export and notes' anchors to a folder of
    /// its own.
    #[sqlx::test]
    async fn dry_run_reads_unopened_articles_too(pool: PgPool) {
        let opened = opened_article(&pool, "day", "day_2026-03-14", "Saturday, March 14", DAY).await;
        let unopened = article(&pool, "day", "day_2026-03-15", "Sunday, March 15", DAY).await;
        let trashed = opened_article(&pool, "person", "p_1", "Nick", PERSON).await;
        pages::delete_page(&pool, &trashed.page_id).await.unwrap();
        sqlx::query(
            "INSERT INTO wiki_notes (subject_type, subject_id, kind, body, author, anchor)              VALUES ('day', 'day_2026-03-14', 'memo', 'Ask about the walk.', 'human', $1)",
        )
        .bind(serde_json::json!({"quote": "You walked by the water with David Okafor.", "sentence": 0}))
        .execute(&pool)
        .await
        .unwrap();
        let before = [
            stored(&pool, &opened.page_id).await,
            stored(&pool, &unopened.page_id).await,
            stored(&pool, &trashed.page_id).await,
        ];
        let dir = tempfile::tempdir().unwrap();
        let texts = tempfile::tempdir().unwrap();

        let report = dry_run_into(&pool, Which::Articles, dir.path(), Some(texts.path())).await.unwrap();
        assert_eq!(report.run, Run::DryRun);
        let outcome = |id: &str| result(&report, id).outcome;
        assert_eq!(outcome(&opened.page_id), Outcome::WouldConvert, "{report:?}");
        assert_eq!(outcome(&unopened.page_id), Outcome::ConvertsWhenOpened);
        assert_eq!(outcome(&trashed.page_id), Outcome::Skipped);
        assert_eq!(result(&report, &trashed.page_id).reason.as_deref(), Some(TRASHED));
        assert!(result(&report, &trashed.page_id).article.is_some(), "measured all the same");
        assert_eq!(result(&report, &opened.page_id).article.as_ref().unwrap().anchors_landing, (1, 1));
        assert_eq!(
            [
                stored(&pool, &opened.page_id).await,
                stored(&pool, &unopened.page_id).await,
                stored(&pool, &trashed.page_id).await,
            ],
            before
        );

        // The day pages' texts, and no other article's.
        let mut files: Vec<serde_json::Value> = std::fs::read_dir(texts.path())
            .unwrap()
            .map(|f| serde_json::from_slice(&std::fs::read(f.unwrap().path()).unwrap()).unwrap())
            .collect();
        assert_eq!(files.len(), 2);
        files.sort_by_key(|f| f["anchors"].as_array().map_or(0, Vec::len));
        assert_eq!(files[1]["source"], DAY);
        assert_eq!(files[1]["anchors"][0]["quote"], "You walked by the water with David Okafor.");
        assert!(files[1]["export"].as_str().unwrap().contains("R\\&B"));
    }

    /// A note written beside a sentence keeps its place: an article whose
    /// blocks would no longer say that sentence stays markdown. Here an
    /// image written inside the sentence, which blocks hold as a block of
    /// its own, parting the sentence in two.
    #[sqlx::test]
    async fn an_anchor_that_would_stop_landing_holds_the_article_back(pool: PgPool) {
        let md = "A quiet day.\n\nIt rained. You walked to the river ![](/drive/df_1.jpg) and home.\n";
        let article = opened_article(&pool, "day", "day_2026-03-16", "Monday, March 16", md).await;
        sqlx::query(
            "INSERT INTO wiki_notes (subject_type, subject_id, kind, body, author, anchor)              VALUES ('day', 'day_2026-03-16', 'memo', 'Cold, too.', 'human', $1)",
        )
        .bind(serde_json::json!({"quote": "You walked to the river ![](/drive/df_1.jpg) and home.", "sentence": 1}))
        .execute(&pool)
        .await
        .unwrap();
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();

        let report = apply_which_into(&yjs, Which::Articles, dir.path()).await.unwrap();
        let page = result(&report, &article.page_id);
        assert_eq!(page.outcome, Outcome::Failed, "{page:?}");
        assert_eq!(page.reason.as_deref(), Some(block_move::ANCHOR_LOST));
        assert_eq!(page.article.as_ref().unwrap().anchors_landing, (1, 0));
        assert_eq!(stored(&pool, &article.page_id).await.format, "markdown");
        assert!(versions(&pool, &article.page_id).await.iter().all(|v| v.description.as_deref() != Some(pages::BEFORE_BLOCKS)));
    }

    /// An article's pending suggestion is its owner's, made with the
    /// markdown editor's inline writer: the move carries it over as a
    /// suggestion, still theirs to accept or reject, with every word of
    /// both sides on the page. Written as accepted, the words under a
    /// pending deletion went with no loss counted.
    #[sqlx::test]
    async fn an_articles_pending_suggestion_moves_as_a_suggestion(pool: PgPool) {
        let md = "You met Zoe at the shop.\n\nI had lunch {--at noon--}{++on Friday++} with her.\n";
        let article = opened_article(&pool, "person", "p_1", "Zoe", md).await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();

        let report = apply_which_into(&yjs, Which::Articles, dir.path()).await.unwrap();
        let page = result(&report, &article.page_id);
        assert_eq!(page.outcome, Outcome::Converted, "{page:?}");
        assert_eq!(page.suggestions, 1);
        let after = stored(&pool, &article.page_id).await;
        assert_eq!(after.format, "tree");
        assert_eq!(after.content, md, "the export says the suggestion as it was");
        let tree = yjs.read_tree(&article.page_id).await.unwrap();
        let html = virtues_document::to_html(&tree, false);
        assert!(
            html.contains("<virtues-del proposal=") && html.contains(">at noon</virtues-del>") && html.contains(">on Friday</virtues-ins>"),
            "{html}"
        );
    }

    /// A suggestion around a source is one a block page's marks hold only in
    /// part, so accepting or rejecting it there would leave the source
    /// behind: the article stays markdown until its owner decides it.
    #[sqlx::test]
    async fn an_articles_suggestion_around_a_source_holds_it_back(pool: PgPool) {
        let md = "A quiet day.\n\nIt rained. {--You walked to the river.[^ev-1]--}\n\n[^ev-1]: Location · 9:30 AM\n";
        let article = opened_article(&pool, "day", "day_2026-03-16", "Monday, March 16", md).await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();

        let report = apply_which_into(&yjs, Which::Articles, dir.path()).await.unwrap();
        let page = result(&report, &article.page_id);
        assert_eq!(page.outcome, Outcome::Failed, "{page:?}");
        assert_eq!(page.reason.as_deref(), Some(block_move::ARTICLE_SUGGESTION_HELD));
        assert_eq!(stored(&pool, &article.page_id).await.format, "markdown");
    }

    /// Moved to blocks, an article keeps its place in every list ordered by
    /// `updated_at`: the table's trigger keeps it on a change of format.
    #[sqlx::test]
    async fn an_article_keeps_updated_at(pool: PgPool) {
        let article = opened_article(&pool, "person", "p_1", "Nick", PERSON).await;
        let before = stored(&pool, &article.page_id).await.updated_at;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        apply_which_into(&yjs, Which::Articles, dir.path()).await.unwrap();
        let after = stored(&pool, &article.page_id).await;
        assert_eq!((after.format.as_str(), after.updated_at), ("tree", before));
    }

    /// An article with no edition moves with its `wiki_articles` row
    /// unwritten: the move changes nothing of it, so its `updated_at` stays.
    #[sqlx::test]
    async fn an_article_with_no_edition_keeps_its_rows_updated_at(pool: PgPool) {
        let article = opened_article(&pool, "person", "p_1", "Nick", PERSON).await;
        sqlx::query("UPDATE wiki_articles SET machine_text = NULL WHERE id = $1")
            .bind(&article.id)
            .execute(&pool)
            .await
            .unwrap();
        let row_updated_at = |pool: PgPool, id: String| async move {
            sqlx::query_scalar::<_, DateTime<Utc>>("SELECT updated_at FROM wiki_articles WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        };
        let before = row_updated_at(pool.clone(), article.id.clone()).await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        let report = apply_which_into(&yjs, Which::Articles, dir.path()).await.unwrap();
        assert_eq!(report.converted, 1, "{report:?}");
        assert_eq!(stored(&pool, &article.page_id).await.format, "tree");
        assert_eq!(row_updated_at(pool.clone(), article.id.clone()).await, before);
        assert_eq!(edition(&pool, &article.id).await.machine_text, None);
    }

    /// Run again, a run over articles moves nothing and writes nothing: the
    /// moved article is blocks, and the unopened one is still left to its
    /// first open.
    #[sqlx::test]
    async fn an_article_run_is_idempotent(pool: PgPool) {
        let opened = opened_article(&pool, "person", "p_1", "Nick", PERSON).await;
        let unopened = article(&pool, "day", "day_2026-03-14", "Saturday, March 14", DAY).await;
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        let first = apply_which_into(&yjs, Which::Articles, dir.path()).await.unwrap();
        assert_eq!((first.converted, first.converts_when_opened), (1, 1), "{first:?}");
        let moved = (stored(&pool, &opened.page_id).await, edition(&pool, &opened.id).await);
        let history = versions(&pool, &opened.page_id).await.len();

        let again = apply_which_into(&yjs, Which::Articles, dir.path()).await.unwrap();
        assert_eq!((again.converted, again.converts_when_opened, again.failed), (0, 1, 0), "{again:?}");
        assert_eq!(again.pages.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), [unopened.page_id.as_str()]);
        assert_eq!((stored(&pool, &opened.page_id).await, edition(&pool, &opened.id).await), moved);
        assert_eq!(versions(&pool, &opened.page_id).await.len(), history);
        assert!(versions(&pool, &unopened.page_id).await.iter().all(|v| v.description.as_deref() != Some(pages::BEFORE_BLOCKS)));
    }

    /// What a run reports an article's export changed, by kind and never
    /// quoting it.
    #[test]
    fn an_exports_changes_are_named_by_kind() {
        let counts = ArticleCounts::default();
        let none: Vec<&str> = vec![];
        assert_eq!(changed_by("Same.\n", "Same.\n", &counts), none);
        assert_eq!(changed_by("R&B and user_name.\n", "R\\&B and user\\_name.\n", &counts), ["escapes"]);
        assert_eq!(changed_by("* one\n* two\n", "- one\n- two\n", &counts), ["list markers"]);
        assert_eq!(
            changed_by("| a | b |\n|---|---|\n| 1 | 2 |\n", "| a | b |\n| --- | --- |\n| 1 | 2 |\n", &counts),
            ["table rules"]
        );
        assert_eq!(changed_by("Met Nick.\n", "Met Nick!\n", &counts), ["other"]);
        assert_eq!(changed_by("Met Nick.  Then home.\n", "Met Nick. Then home.\n", &counts), ["spaces"]);
        assert_eq!(changed_by("**⟦the clinic⟧**\n", "⟦**the clinic**⟧\n", &counts), ["veil placement"]);
        assert_eq!(
            changed_by("A.[^ev-1] B.[^ev-2]\n\n[^ev-2]: two\n[^ev-1]: one\n", "A.[^ev-1] B.[^ev-2]\n\n[^ev-1]: one\n[^ev-2]: two\n", &counts),
            ["note order"]
        );
        let orphan = ArticleCounts { orphan_notes: vec!["cx-2".into()], ..ArticleCounts::default() };
        assert_eq!(changed_by("A.[^ev-1]\n\n[^ev-1]: one\n[^cx-2]: 9:00 AM\n", "A.[^ev-1]\n\n[^ev-1]: one\n", &orphan), ["orphan notes"]);
        assert_eq!(changed_by("One\nline.\n", "One line.\n", &counts), ["line breaks"]);
        assert_eq!(changed_by("Met Nick.", "Met Nick.\n", &counts), ["final newline"]);
        assert_eq!(changed_by("", "\n", &counts), ["final newline"]);
        assert_eq!(changed_by("Met Nick.\n\n", "Met Nick.\n", &counts), ["blank lines"]);
        assert_eq!(changed_by("Met Nick.\r\n", "Met Nick.\n", &counts), ["spaces"]);
        assert_eq!(first_change("A.\nB.\n", "A.\nC.\n").as_deref(), Some("line 2: C."));
        assert_eq!(first_change("A.\n", "A.\n"), None);
    }

    // ── Migration 0051: articles can be block pages ─────────────────────

    /// The table holds an article as blocks, as it holds a user page.
    #[sqlx::test]
    async fn an_article_can_be_a_tree(pool: PgPool) {
        sqlx::query(
            "INSERT INTO app_pages (id, title, kind, format, yjs_state, tree_lineage) \
             VALUES ('page_article', 'Nick', 'article', 'tree', '\\x00', '0123456789abcdef')",
        )
        .execute(&pool)
        .await
        .expect("an article holds blocks");
        let lineage: Option<String> = sqlx::query_scalar("SELECT tree_lineage FROM app_pages WHERE id = 'page_article'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(lineage.as_deref(), Some("0123456789abcdef"));
        // A markdown page has no lineage.
        let page = markdown_page(&pool, "Lunch.\n").await;
        let lineage: Option<String> = sqlx::query_scalar("SELECT tree_lineage FROM app_pages WHERE id = $1")
            .bind(&page)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(lineage, None);
    }

    /// A change of format is no edit, so it keeps `updated_at`; any other
    /// write moves it, as the shared trigger did.
    #[sqlx::test]
    async fn a_format_change_keeps_updated_at_and_an_edit_moves_it(pool: PgPool) {
        let id = markdown_page(&pool, "Lunch.\n").await;
        let at = |pool: PgPool, id: String| async move {
            sqlx::query_scalar::<_, DateTime<Utc>>("SELECT updated_at FROM app_pages WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        };
        let written = at(pool.clone(), id.clone()).await;

        sqlx::query("UPDATE app_pages SET format = 'tree', yjs_state = '\\x00', content = 'Lunch.' WHERE id = $1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(at(pool.clone(), id.clone()).await, written, "a change of format keeps updated_at");

        sqlx::query("UPDATE app_pages SET title = 'Lunch, later' WHERE id = $1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        let edited = at(pool.clone(), id.clone()).await;
        assert!(edited > written, "an edit moves it: {edited} after {written}");
    }

    /// A page's move takes the page's row and nothing more: another page
    /// saves while a move waits. Turning the table's trigger off for the
    /// move held every other write to the table until it ended.
    #[sqlx::test]
    async fn the_move_holds_no_table_lock(pool: PgPool) {
        let moving = unopened_page(&pool, "Lunch.\n").await;
        let other = unopened_page(&pool, "Dinner.\n").await;
        let row = read_row(&pool, &moving).await.unwrap().unwrap();
        let blocks = convert("Lunch.\n").blocks;
        let yjs = YjsState::new(pool.clone());

        // Hold the moving page's row, so its flip waits partway.
        let mut holder = pool.begin().await.unwrap();
        sqlx::query("SELECT 1 FROM app_pages WHERE id = $1 FOR UPDATE")
            .bind(&moving)
            .execute(&mut *holder)
            .await
            .unwrap();
        let mover = {
            let (yjs, id) = (yjs.clone(), moving.clone());
            tokio::spawn(async move { yjs.move_to_tree(&id, row.updated_at, &blocks.state, &blocks.text, None).await })
        };
        pages::until_a_statement_waits(&pool, "UPDATE app_pages SET format%").await;

        let mut writer = pool.acquire().await.unwrap();
        sqlx::query("SET lock_timeout = '2s'").execute(&mut *writer).await.unwrap();
        sqlx::query("UPDATE app_pages SET content = 'Dinner, later.' WHERE id = $1")
            .bind(&other)
            .execute(&mut *writer)
            .await
            .expect("another page saves while a move waits");

        holder.commit().await.unwrap();
        assert_eq!(mover.await.unwrap(), Ok(TreeMove::Moved));
        assert_eq!(stored(&pool, &moving).await.format, "tree");
    }

    /// A page in the trash is reported and left in markdown: History takes
    /// no version of it, so it moves once it is put back.
    #[sqlx::test]
    async fn a_page_in_the_trash_waits_until_it_is_put_back(pool: PgPool) {
        let id = markdown_page(&pool, "Kept for later.\n").await;
        pages::delete_page(&pool, &id).await.unwrap();
        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        let report = apply_into(&yjs, dir.path()).await.unwrap();
        assert_eq!(result(&report, &id).outcome, Outcome::Skipped);
        assert_eq!(stored(&pool, &id).await.format, "markdown");

        sqlx::query("UPDATE app_pages SET deleted_at = NULL WHERE id = $1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        let report = apply_into(&yjs, dir.path()).await.unwrap();
        assert_eq!(result(&report, &id).outcome, Outcome::Converted);
    }

    /// A dry run converts in memory and writes no page.
    #[sqlx::test]
    async fn a_dry_run_reports_and_writes_no_page(pool: PgPool) {
        let fine = markdown_page(&pool, "Lunch.\n\nTotal:     $5\n").await;
        let lossy = markdown_page(&pool, LOSSY).await;
        let before = [stored(&pool, &fine).await, stored(&pool, &lossy).await];
        let dir = tempfile::tempdir().unwrap();

        let report = dry_run_into(&pool, Which::Pages, dir.path(), None).await.unwrap();
        assert_eq!(report.run, Run::DryRun);
        assert_eq!((report.would_convert, report.failed, report.converted), (1, 1, 0));
        assert_eq!(result(&report, &fine).outcome, Outcome::WouldConvert);
        assert!(result(&report, &fine).notes.iter().any(|n| n.message.contains("Total:     $5")));
        assert_eq!(result(&report, &lossy).outcome, Outcome::Failed);
        assert_eq!([stored(&pool, &fine).await, stored(&pool, &lossy).await], before);
        assert!(versions(&pool, &fine).await.is_empty());
        assert!(report.kept_at.is_some());
    }

    /// What a run logs of each page names the kinds of change it went
    /// through, never the page's words: the notes quote its lines, and the
    /// journal is what an owner shares when their server won't start. The
    /// report on the box keeps the notes whole.
    #[sqlx::test]
    async fn a_runs_log_quotes_no_page(pool: PgPool) {
        use std::io::Write;
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct Buf(Arc<Mutex<Vec<u8>>>);
        impl Write for Buf {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Buf {
            type Writer = Buf;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let noted = markdown_page(
            &pool,
            "```rust ledger-zebra\nfn main() {}\n```\n\n\
             | A | B |\n| --- | --- |\n| 1 | 2 | account 99887766 |\n\n\
             Door code 2290    is in the drawer.\n\n\
             Lunch\twith David Okafor at noon.\n",
        )
        .await;
        let lossy = markdown_page(&pool, LOSSY).await;
        let buf = Buf::default();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(buf.clone())
            .finish();
        let dir = tempfile::tempdir().unwrap();
        let report = {
            let _logging = tracing::subscriber::set_default(subscriber);
            // With one subscriber registered, tracing decides whether a log
            // line is wanted by the subscriber of whichever thread meets it
            // first, and caches that for every thread: another test's thread
            // meeting these lines meanwhile, with no subscriber of its own,
            // would turn them off here. A second subscriber, kept alive,
            // makes it ask every registered one instead.
            let _second = tracing::Dispatch::new(tracing_subscriber::fmt().with_writer(std::io::sink).finish());
            // Lines met before this test began are cached already: ask again.
            tracing::callsite::rebuild_interest_cache();
            dry_run_into(&pool, Which::Pages, dir.path(), None).await.unwrap()
        };
        assert!(result(&report, &noted).notes.len() >= 3, "{report:?}");
        assert_eq!(result(&report, &lossy).outcome, Outcome::Failed);

        let logged = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        assert!(logged.contains(&noted) && logged.contains(&lossy), "each page is logged: {logged}");
        assert!(logged.contains("spaces collapsed"), "{logged}");
        for word in ["ledger", "zebra", "99887766", "2290", "drawer", "Okafor", "Lunch", "harbour", "the plan"] {
            assert!(!logged.contains(word), "the log quotes {word:?}: {logged}");
        }
    }

    /// A device that typed on a markdown page while the server was out of
    /// reach, and was closed before it reconnected, holds that typing in its
    /// own copy, which a block page never reads. Sent once the page is
    /// blocks, the copy is kept as the owner's version when it holds typing
    /// the page never had, the same version when sent again, and nothing
    /// when it holds none.
    #[sqlx::test]
    async fn a_devices_copy_with_typing_the_page_never_had_is_kept_as_a_version(pool: PgPool) {
        use yrs::{updates::decoder::Decode, ReadTxn, StateVector, Text, Transact};
        let id = markdown_page(&pool, "Groceries\n").await;
        let synced = stored(&pool, &id).await.yjs_state.unwrap();
        let device = yrs::Doc::new();
        device.transact_mut().apply_update(yrs::Update::decode_v1(&synced).unwrap()).unwrap();
        let text = device.get_or_insert_text("content");
        text.insert(&mut device.transact_mut(), "Groceries\n".len() as u32, "Eggs\n");
        let unsent = device.transact().encode_state_as_update_v1(&StateVector::default());

        // Still markdown: the page's own editor syncs the copy.
        assert!(pages::keep_device_copy(&pool, &id, &unsent).await.is_err());

        let yjs = YjsState::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(apply_into(&yjs, dir.path()).await.unwrap().converted, 1);

        assert!(pages::keep_device_copy(&pool, &id, &synced).await.unwrap().is_none());
        let kept = pages::keep_device_copy(&pool, &id, &unsent).await.unwrap().expect("the typing is kept");
        assert_eq!(
            (kept.created_by.as_str(), kept.description.as_deref()),
            ("user", Some(pages::TYPED_ON_A_DEVICE))
        );
        let then = pages::get_version(&pool, &kept.id).await.unwrap();
        assert_eq!((then.format, then.markdown.as_str()), (PageFormat::Markdown, "Groceries\nEggs\n"));
        let again = pages::keep_device_copy(&pool, &id, &unsent).await.unwrap().unwrap();
        assert_eq!(again.id, kept.id);
        assert_eq!(versions(&pool, &id).await.len(), 3, "before blocks, the blocks, the typing");
        // The page itself is the blocks, untouched.
        assert_eq!(yjs.read_text(&id).await.unwrap(), "Groceries\n");
    }
}
