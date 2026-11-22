//! What the one-time import applets share: Spotify, Amazon orders and
//! Discord.
//!
//! The `/api/imports/:source/upload` route streams a download into a staging
//! folder and starts the source's applet with `payload = { "file_path": ... }`.
//! Each applet reads that file, a zip or a bare file, writes rows in batches
//! that dedup on `source_stream_id`, removes the file, and ends with one
//! sentence naming what arrived and the months it spans. The sentence never
//! carries a count; `records_processed` does.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Utc};
use serde::de::{Deserializer as _, SeqAccess, Visitor};
use serde_json::Value;
use uuid::Uuid;
use virtues_helpers::AppletInput;

/// Rows per write. Large enough that inserts stay efficient, small enough
/// that a reader never holds much in memory and a batch stays well under
/// Postgres's limit of 65,535 bound parameters.
pub const BATCH: usize = 1_000;

/// The folders the upload route stages into (`server::api::imports`): under
/// the state root on a box, under the temp folder in a dev checkout.
pub const STAGING_FOLDERS: [&str; 2] = ["imports", "virtues-imports"];

/// Namespace for the ids derived here, so an import's row ids never collide
/// with ids another writer derives from the same text.
const IMPORT_NS: Uuid = Uuid::from_u128(0x6b1e_42c9_8d3f_5a70_b2e4_19c8_d05f_7a31);

/// A file inside one of the imports staging folders, ending `.upload`, and a
/// regular file rather than a link. An import removes the file it reads, so
/// it reads nothing else, including a path someone typed into a manual run.
pub fn is_staged_upload(path: &Path) -> bool {
    let in_staging = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .is_some_and(|n| STAGING_FOLDERS.contains(&n));
    let named = path.extension().is_some_and(|e| e == "upload");
    let regular = std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file());
    in_staging && named && regular
}

/// `payload.file_path`, checked to be a staged upload.
pub fn staged_file(input: &AppletInput, applet: &str) -> Result<PathBuf> {
    let path = input
        .payload
        .as_ref()
        .and_then(|p| p.get("file_path"))
        .and_then(|v| v.as_str())
        .with_context(|| format!("{applet}: payload.file_path is required"))?;
    let path = PathBuf::from(path);
    if !is_staged_upload(&path) {
        bail!("{applet} reads only files the upload route staged, not {}", path.display());
    }
    Ok(path)
}

/// Remove the upload. It is input, not a kept file, so this runs however the
/// import ends.
pub fn remove_upload(path: &Path, applet: &str) {
    if let Err(e) = std::fs::remove_file(path) {
        tracing::warn!("{applet}: could not remove the upload {}: {e}", path.display());
    }
}

/// A row id derived from its `source_stream_id`, so importing the same
/// download again produces the same ids.
pub fn row_id(source_stream_id: &str) -> String {
    Uuid::new_v5(&IMPORT_NS, source_stream_id.as_bytes()).to_string()
}

/// A short stable key for text that has no id of its own, such as a song
/// with no Spotify URI. The parts are separated so ("ab", "c") and ("a",
/// "bc") differ.
pub fn content_key(parts: &[&str]) -> String {
    let joined = parts.join("\u{1f}");
    Uuid::new_v5(&IMPORT_NS, joined.as_bytes()).simple().to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// The uploaded file: a zip or a bare file
// ─────────────────────────────────────────────────────────────────────────────

/// An opened upload. The route names every upload `<uuid>.upload`, so the
/// first bytes say whether it is a zip.
pub enum Upload {
    Zip(zip::ZipArchive<File>),
    Bare(File),
}

impl Upload {
    pub fn open(path: &Path) -> Result<Upload> {
        let mut file = File::open(path).with_context(|| format!("could not open {}", path.display()))?;
        let mut magic = [0u8; 4];
        let is_zip = file.read(&mut magic)? == 4 && magic == *b"PK\x03\x04";
        file.seek(SeekFrom::Start(0))?;
        if !is_zip {
            return Ok(Upload::Bare(file));
        }
        let archive = zip::ZipArchive::new(file).context(
            "This zip could not be opened. It may not have finished downloading. \
             Download it again and retry.",
        )?;
        Ok(Upload::Zip(archive))
    }

    /// The names of the zip's files for which `wanted` is true, sorted so a
    /// re-import reads them in the same order. Empty for a bare file.
    pub fn names(&self, wanted: impl Fn(&str) -> bool) -> Vec<String> {
        let Upload::Zip(archive) = self else {
            return Vec::new();
        };
        let mut names: Vec<String> = archive
            .file_names()
            .filter(|n| !n.ends_with('/') && wanted(n))
            .map(str::to_string)
            .collect();
        names.sort();
        names
    }

    /// Hand the zip file `name` to `read`. `Ok(None)` when the zip has no
    /// file of that name or this is a bare file.
    pub fn with_entry<T>(&mut self, name: &str, read: impl FnOnce(&mut dyn Read) -> Result<T>) -> Result<Option<T>> {
        let Upload::Zip(archive) = self else {
            return Ok(None);
        };
        let mut entry = match archive.by_name(name) {
            Ok(e) => e,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(e) => return Err(e).context("Part of this zip is damaged. Download it again and retry."),
        };
        read(&mut entry).map(Some)
    }
}

/// Hand each file in the upload to `read`: every zip file for which `wanted`
/// is true, or the bare file itself with an empty name. Returns how many
/// files were read.
pub fn for_each_file(
    path: &Path,
    wanted: impl Fn(&str) -> bool,
    read: &mut dyn FnMut(&str, &mut dyn Read) -> Result<()>,
) -> Result<usize> {
    let mut upload = Upload::open(path)?;
    if let Upload::Bare(file) = &mut upload {
        read("", file)?;
        return Ok(1);
    }
    let names = upload.names(wanted);
    for name in &names {
        upload.with_entry(name, |r| read(name, r))?;
    }
    Ok(names.len())
}

/// The last part of a zip path, `b.json` in `a/b.json`.
pub fn base_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

// ─────────────────────────────────────────────────────────────────────────────
// Reading on a blocking thread, writing in batches
// ─────────────────────────────────────────────────────────────────────────────

/// Collects rows and hands them to a sink in batches of [`BATCH`].
pub struct Batches<'a, T> {
    buf: Vec<T>,
    sink: &'a mut dyn FnMut(Vec<T>) -> Result<()>,
}

impl<'a, T> Batches<'a, T> {
    pub fn new(sink: &'a mut dyn FnMut(Vec<T>) -> Result<()>) -> Self {
        Batches { buf: Vec::new(), sink }
    }

    pub fn push(&mut self, row: T) -> Result<()> {
        self.buf.push(row);
        if self.buf.len() >= BATCH {
            self.flush()?;
        }
        Ok(())
    }

    /// Hand on what has been collected so far. A reader calls this before
    /// returning an error, so the rows before a damaged part are kept.
    pub fn flush(&mut self) -> Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        (self.sink)(std::mem::take(&mut self.buf))
    }
}

/// Run `read` on a blocking thread, receiving its batches on the returned
/// channel. The caller writes each batch as it arrives and then awaits the
/// handle for the reader's own result. Memory stays flat because the channel
/// holds at most two batches.
pub fn spawn_reader<T, F>(path: &Path, read: F) -> (tokio::sync::mpsc::Receiver<Vec<T>>, tokio::task::JoinHandle<Result<()>>)
where
    T: Send + 'static,
    F: FnOnce(&Path, &mut dyn FnMut(Vec<T>) -> Result<()>) -> Result<()> + Send + 'static,
{
    let (tx, rx) = tokio::sync::mpsc::channel::<Vec<T>>(2);
    let owned = path.to_path_buf();
    let handle = tokio::task::spawn_blocking(move || {
        read(&owned, &mut |batch| tx.blocking_send(batch).map_err(|_| anyhow::anyhow!("the import stopped writing")))
    });
    (rx, handle)
}

/// Await a reader started by [`spawn_reader`].
pub async fn join_reader(handle: tokio::task::JoinHandle<Result<()>>) -> Result<()> {
    handle.await.context("the download reader stopped unexpectedly")?
}

// ─────────────────────────────────────────────────────────────────────────────
// JSON arrays, one element at a time
// ─────────────────────────────────────────────────────────────────────────────

/// Whether the next non-blank character is `[`, without consuming it. Skips
/// a UTF-8 byte order mark, which some exports start with.
pub fn starts_with_array<R: Read>(reader: &mut BufReader<R>) -> Result<bool> {
    let mut checked_bom = false;
    loop {
        let buf = reader.fill_buf()?;
        if !checked_bom {
            checked_bom = true;
            if buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
                reader.consume(3);
                continue;
            }
        }
        let Some(&first) = buf.first() else {
            return Ok(false);
        };
        let blank = buf.iter().take_while(|b| b.is_ascii_whitespace()).count();
        if blank == 0 {
            return Ok(first == b'[');
        }
        reader.consume(blank);
    }
}

/// Walk a top-level JSON array one element at a time, so the whole document
/// never sits in memory as one value. An error from `each` stops the walk and
/// is returned as is.
pub fn for_each_element<R: Read>(reader: R, each: &mut dyn FnMut(Value) -> Result<()>) -> Result<()> {
    struct Each<'a> {
        each: &'a mut dyn FnMut(Value) -> Result<()>,
        failed: &'a mut Option<anyhow::Error>,
    }
    impl<'de> Visitor<'de> for Each<'_> {
        type Value = ();
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a JSON array")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
            while let Some(element) = seq.next_element::<Value>()? {
                if let Err(e) = (self.each)(element) {
                    *self.failed = Some(e);
                    return Err(serde::de::Error::custom("the import stopped"));
                }
            }
            Ok(())
        }
    }
    let mut failed = None;
    let mut de = serde_json::Deserializer::from_reader(reader);
    let walked = de.deserialize_seq(Each { each, failed: &mut failed });
    if let Some(e) = failed {
        return Err(e);
    }
    walked?;
    de.end()?;
    Ok(())
}

/// A JSON field as text: a string as is, a number written out. Discord wrote
/// ids as numbers in some years and as strings in others.
pub fn text(v: &Value, key: &str) -> Option<String> {
    match v.get(key)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CSV
// ─────────────────────────────────────────────────────────────────────────────

/// A CSV reader for the exports' files: comma separated, fields optionally
/// in double quotes, a quoted field may hold commas, newlines and `""` for a
/// quote. Reads one record at a time.
pub struct Csv<R: Read> {
    reader: BufReader<R>,
    first: bool,
}

impl<R: Read> Csv<R> {
    pub fn new(reader: R) -> Self {
        Csv { reader: BufReader::new(reader), first: true }
    }

    /// The next record's fields, or `None` at the end of the file. A blank
    /// line is passed over.
    pub fn next_record(&mut self) -> Result<Option<Vec<String>>> {
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line)? == 0 {
                return Ok(None);
            }
            if self.first {
                self.first = false;
                if let Some(rest) = line.strip_prefix('\u{feff}') {
                    line = rest.to_string();
                }
            }
            let mut fields = Vec::new();
            let mut field = String::new();
            let mut quoted = false;
            let mut chars = line.chars().peekable();
            loop {
                let Some(c) = chars.next() else {
                    if quoted {
                        // The quoted field goes on to the next line.
                        line.clear();
                        if self.reader.read_line(&mut line)? == 0 {
                            bail!("this file ends inside a quoted field");
                        }
                        chars = line.chars().peekable();
                        continue;
                    }
                    break;
                };
                match (quoted, c) {
                    (true, '"') if chars.peek() == Some(&'"') => {
                        chars.next();
                        field.push('"');
                    }
                    (true, '"') => quoted = false,
                    (true, c) => field.push(c),
                    (false, '"') if field.is_empty() => quoted = true,
                    (false, ',') => fields.push(std::mem::take(&mut field)),
                    (false, '\r') | (false, '\n') => {}
                    (false, c) => field.push(c),
                }
            }
            fields.push(field);
            if fields.len() == 1 && fields[0].is_empty() {
                continue;
            }
            return Ok(Some(fields));
        }
    }
}

/// Where each named column sits in a header row. Headers match by name,
/// ignoring case and surrounding spaces, so a column moving or a vendor
/// changing a name's capitals does not break the import.
pub struct Columns {
    headers: Vec<String>,
}

impl Columns {
    pub fn new(header: &[String]) -> Self {
        Columns { headers: header.iter().map(|h| h.trim().to_ascii_lowercase()).collect() }
    }

    /// The index of the first of `names` the header has.
    pub fn find(&self, names: &[&str]) -> Option<usize> {
        names.iter().find_map(|n| {
            let n = n.to_ascii_lowercase();
            self.headers.iter().position(|h| *h == n)
        })
    }
}

/// The trimmed field at `index`, `None` when absent or blank.
pub fn field(record: &[String], index: Option<usize>) -> Option<&str> {
    let v = record.get(index?)?.trim();
    (!v.is_empty()).then_some(v)
}

// ─────────────────────────────────────────────────────────────────────────────
// Time
// ─────────────────────────────────────────────────────────────────────────────

/// A timestamp in any of the forms these exports have used. A time with no
/// zone is UTC, which is what each of these services documents. A bare date
/// is midnight UTC.
///
/// chrono's `%Y` accepts one to four digits, so "01/15/19" parses as the year
/// 19 under `%m/%d/%Y`. Every format's result is therefore checked for a year
/// of 1900 or later before it is taken, which lets the `%y` form that follows
/// read the two-digit year.
pub fn parse_time(raw: &str) -> Option<DateTime<Utc>> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let plausible = |t: DateTime<Utc>| (t.year() >= 1900).then_some(t);
    if let Some(t) = DateTime::parse_from_rfc3339(s).ok().and_then(|t| plausible(t.with_timezone(&Utc))) {
        return Some(t);
    }
    for f in ["%Y-%m-%d %H:%M:%S%.f%:z", "%Y-%m-%d %H:%M:%S%.f%z", "%Y-%m-%dT%H:%M:%S%.f%z"] {
        if let Some(t) = DateTime::parse_from_str(s, f).ok().and_then(|t| plausible(t.with_timezone(&Utc))) {
            return Some(t);
        }
    }
    let naive = s.strip_suffix(" UTC").or_else(|| s.strip_suffix('Z')).unwrap_or(s);
    for f in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%m/%d/%Y %H:%M:%S",
        "%m/%d/%Y %H:%M",
        "%m/%d/%y %H:%M:%S",
        "%m/%d/%y %H:%M",
    ] {
        if let Some(t) = NaiveDateTime::parse_from_str(naive, f).ok().and_then(|t| plausible(t.and_utc())) {
            return Some(t);
        }
    }
    for f in ["%Y-%m-%d", "%m/%d/%Y", "%m/%d/%y"] {
        if let Some(t) = NaiveDate::parse_from_str(naive, f)
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .and_then(|t| plausible(t.and_utc()))
        {
            return Some(t);
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// The summary
// ─────────────────────────────────────────────────────────────────────────────

/// What one import read and wrote, for its summary.
#[derive(Default)]
pub struct Tally {
    pub parsed: usize,
    pub written: u64,
    first: Option<DateTime<Utc>>,
    last: Option<DateTime<Utc>>,
}

impl Tally {
    /// Count one parsed row at time `t`.
    pub fn saw(&mut self, t: DateTime<Utc>) {
        self.parsed += 1;
        self.first = Some(self.first.map_or(t, |f| f.min(t)));
        self.last = Some(self.last.map_or(t, |l| l.max(t)));
    }

    /// " from March 2023 to November 2026", " from March 2023", or nothing.
    pub fn span(&self) -> String {
        match (self.first, self.last) {
            (Some(a), Some(b)) if month(a) == month(b) => format!(" from {}", month(a)),
            (Some(a), Some(b)) => format!(" from {} to {}", month(a), month(b)),
            _ => String::new(),
        }
    }

    /// "Brought in your Spotify listening from March 2019 to November 2026."
    /// When the download added nothing new: "Your Spotify listening from
    /// March 2019 to November 2026 was already here." `already` is "was" or
    /// "were", to agree with `what`.
    pub fn summary(&self, what: &str, already: &str) -> String {
        let span = self.span();
        if self.written == 0 {
            format!("Your {what}{span} {already} already here.")
        } else {
            format!("Brought in your {what}{span}.")
        }
    }
}

fn month(t: DateTime<Utc>) -> String {
    t.format("%B %Y").to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// A path in the temp folder no other test uses.
    pub fn temp_path(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("virtues_import_test_{unique}_{name}"))
    }

    /// Write a zip holding `entries` at `path`.
    pub fn zip_with(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn only_a_staged_upload_is_read() {
        let dir = temp_path("staging");
        for folder in STAGING_FOLDERS {
            let staging = dir.join(folder);
            std::fs::create_dir_all(&staging).unwrap();
            let staged = staging.join("1.upload");
            std::fs::write(&staged, "[]").unwrap();
            assert!(is_staged_upload(&staged), "{folder}");
            let unnamed = staging.join("notes.json");
            std::fs::write(&unnamed, "[]").unwrap();
            assert!(!is_staged_upload(&unnamed));
            assert!(!is_staged_upload(&staging.join("missing.upload")));
        }
        let other = dir.join("chat-import");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("2.upload"), "[]").unwrap();
        assert!(!is_staged_upload(&other.join("2.upload")));
        let elsewhere = dir.join("notes.upload");
        std::fs::write(&elsewhere, "[]").unwrap();
        assert!(!is_staged_upload(&elsewhere));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_matching_files_in_a_zip_and_a_bare_file_whole() {
        let path = temp_path("bundle.zip");
        zip_with(
            &path,
            &[("b/two.json", b"2"), ("a/one.json", b"1"), ("a/skip.txt", b"x"), ("a/", b"")],
        );
        let mut seen = Vec::new();
        let n = for_each_file(&path, |n| n.ends_with(".json"), &mut |name, r| {
            let mut s = String::new();
            r.read_to_string(&mut s)?;
            seen.push((name.to_string(), s));
            Ok(())
        })
        .unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(n, 2);
        assert_eq!(seen, vec![("a/one.json".into(), "1".into()), ("b/two.json".into(), "2".into())]);

        let bare = temp_path("bare.csv");
        std::fs::write(&bare, "a,b\n").unwrap();
        let mut seen = Vec::new();
        for_each_file(&bare, |_| false, &mut |name, r| {
            let mut s = String::new();
            r.read_to_string(&mut s)?;
            seen.push((name.to_string(), s));
            Ok(())
        })
        .unwrap();
        std::fs::remove_file(&bare).unwrap();
        assert_eq!(seen, vec![(String::new(), "a,b\n".into())]);
    }

    #[test]
    fn a_missing_zip_entry_is_none() {
        let path = temp_path("one.zip");
        zip_with(&path, &[("a.json", b"[]")]);
        let mut upload = Upload::open(&path).unwrap();
        assert!(upload.with_entry("b.json", |_| Ok(())).unwrap().is_none());
        assert!(upload.with_entry("a.json", |_| Ok(())).unwrap().is_some());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn walks_a_json_array_one_element_at_a_time() {
        let mut reader = BufReader::new("\u{feff}\n [1, {\"a\": 2}, \"x\"]".as_bytes());
        assert!(starts_with_array(&mut reader).unwrap());
        let mut seen = Vec::new();
        for_each_element(reader, &mut |v| {
            seen.push(v);
            Ok(())
        })
        .unwrap();
        assert_eq!(seen.len(), 3);
        assert!(!starts_with_array(&mut BufReader::new("{}".as_bytes())).unwrap());
    }

    #[test]
    fn csv_handles_quotes_commas_and_newlines() {
        let data = "\u{feff}Order ID,Product Name,Total\r\n\
                    111-1,\"Paper Lanterns, set of 4\",\"$1,299.00\"\r\n\
                    \r\n\
                    111-2,\"A \"\"quoted\"\" name\nover two lines\",\r\n";
        let mut csv = Csv::new(data.as_bytes());
        assert_eq!(csv.next_record().unwrap().unwrap(), vec!["Order ID", "Product Name", "Total"]);
        assert_eq!(csv.next_record().unwrap().unwrap(), vec!["111-1", "Paper Lanterns, set of 4", "$1,299.00"]);
        assert_eq!(
            csv.next_record().unwrap().unwrap(),
            vec!["111-2", "A \"quoted\" name\nover two lines", ""]
        );
        assert!(csv.next_record().unwrap().is_none());
        assert!(Csv::new("a,\"open".as_bytes()).next_record().is_err());
    }

    #[test]
    fn columns_match_by_name_ignoring_case() {
        let header: Vec<String> = ["Order ID", " ASIN/ISBN ", "title"].iter().map(|s| s.to_string()).collect();
        let cols = Columns::new(&header);
        assert_eq!(cols.find(&["order id"]), Some(0));
        assert_eq!(cols.find(&["ASIN", "ASIN/ISBN"]), Some(1));
        assert_eq!(cols.find(&["Product Name", "Title"]), Some(2));
        assert_eq!(cols.find(&["Currency"]), None);
    }

    #[test]
    fn parses_the_timestamp_forms_the_exports_use() {
        let want = |s: &str| parse_time(s).unwrap().to_rfc3339();
        assert_eq!(want("2023-05-01T18:23:45Z"), "2023-05-01T18:23:45+00:00");
        assert_eq!(want("2023-05-01T18:23:45.120Z"), "2023-05-01T18:23:45.120+00:00");
        assert_eq!(want("2023-05-01 18:23:45"), "2023-05-01T18:23:45+00:00");
        assert_eq!(want("2020-01-02 03:04:05.678000+00:00"), "2020-01-02T03:04:05.678+00:00");
        assert_eq!(want("2023-05-01 18:23"), "2023-05-01T18:23:00+00:00");
        assert_eq!(want("2023-05-01 18:23:45 UTC"), "2023-05-01T18:23:45+00:00");
        assert_eq!(want("05/01/2023"), "2023-05-01T00:00:00+00:00");
        // A two-digit year is read as %y, not as the year 19.
        assert_eq!(want("01/15/19"), "2019-01-15T00:00:00+00:00");
        assert_eq!(want("01/15/2019"), "2019-01-15T00:00:00+00:00");
        assert_eq!(want("01/15/19 10:30:00"), "2019-01-15T10:30:00+00:00");
        assert!(parse_time("Not Available").is_none());
        assert!(parse_time("").is_none());
    }

    #[test]
    fn summary_names_the_span_not_a_count() {
        let mut t = Tally::default();
        assert_eq!(t.summary("Amazon orders", "were"), "Your Amazon orders were already here.");
        t.saw(parse_time("2023-03-02T00:00:00Z").unwrap());
        t.saw(parse_time("2023-03-20T00:00:00Z").unwrap());
        t.written = 2;
        assert_eq!(t.summary("Amazon orders", "were"), "Brought in your Amazon orders from March 2023.");
        t.saw(parse_time("2026-11-01T00:00:00Z").unwrap());
        t.written = 0;
        assert_eq!(
            t.summary("Spotify listening", "was"),
            "Your Spotify listening from March 2023 to November 2026 was already here."
        );
    }

    #[test]
    fn ids_are_stable() {
        assert_eq!(row_id("spotify:a"), row_id("spotify:a"));
        assert_ne!(row_id("spotify:a"), row_id("spotify:b"));
        assert_ne!(content_key(&["ab", "c"]), content_key(&["a", "bc"]));
    }
}
