//! Keep the box's map files in step with where its owner has lived.
//!
//! Once a day: read the index of the current map build, score every square
//! the location history touches, pick what fits the budget, download what is
//! missing, delete what is no longer picked, reload the readers. See
//! agents/record/map-tiles.md for why each rule is what it is.
//!
//! **Privacy.** Only fixed, pre-cut files are ever requested, identical for
//! every box that takes them: a download names a ~300 km or ~1,000 km square,
//! never a place inside it. Nothing here logs a square or a coordinate.
//!
//! **Importance.** A day counts for a square when the owner spent at least an
//! hour in it (six distinct ten-minute buckets of location points, which does
//! not depend on how often the phone samples). A day's weight halves every
//! year, so a new city overtakes an old one within weeks while a place lived
//! in for months keeps its detail.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::io::AsyncWriteExt;

use super::maps_root;

/// Bytes allowed for home and visited files together; the world file and the
/// fonts do not count.
const BUDGET_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Of that, bytes allowed for home files.
const HOME_BUDGET_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Days in a square before it earns home detail.
const HOME_MIN_DAYS: u32 = 7;
/// Days in a square that make it a home however long ago they were.
const LIVED_THERE_DAYS: u32 = 90;
/// Days in a z5 square before it earns street-level detail. One day is a
/// drive-through.
const VISITED_MIN_DAYS: u32 = 2;
/// A day's weight halves every this many days.
const HALF_LIFE_DAYS: f64 = 365.0;
/// A file from an older build is replaced once the published build is this
/// much newer; OpenStreetMap changes slowly.
const REFRESH_AFTER_DAYS: i64 = 90;
/// Free space to leave on the disk after a download.
const MIN_FREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct IndexFile {
    pub name: String,
    pub tier: String,
    pub z: u8,
    pub x: u32,
    pub y: u32,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Index {
    pub build: String,
    /// Only a full monthly cut says `true`. A box deletes nothing against an
    /// index that does not: a partial build treated as the whole map would
    /// delete every square it does not list (deploy/maps/cut.py).
    #[serde(default)]
    pub complete: bool,
    pub files: Vec<IndexFile>,
}

/// What the box holds and from which build, beside the files.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    /// file name → (build it came from, sha256)
    files: HashMap<String, (String, String)>,
}

/// Days with an hour or more in one z7 square.
#[derive(Debug, Clone, Default)]
pub struct SquareDays {
    /// Age of each day in days (0 = today).
    ages: Vec<i64>,
}

impl SquareDays {
    fn days(&self) -> u32 {
        self.ages.len() as u32
    }
    fn score(&self) -> f64 {
        self.ages.iter().map(|&a| 0.5f64.powf(a as f64 / HALF_LIFE_DAYS)).sum()
    }
}

/// z7 square → the days spent in it.
pub type Presence = HashMap<(u32, u32), SquareDays>;

/// Read presence from the location history: per UTC day, the z7 squares with
/// six or more distinct ten-minute buckets of points.
pub async fn load_presence(pool: &PgPool) -> Result<Presence> {
    let rows: Vec<(i32, i32, i32)> = sqlx::query_as(
        r#"
        WITH p AS (
            SELECT (occurred_at AT TIME ZONE 'UTC')::date AS day,
                   floor((longitude + 180) / 360 * 128)::int AS x,
                   floor((1 - ln(tan(radians(latitude)) + 1 / cos(radians(latitude))) / pi()) / 2 * 128)::int AS y,
                   floor(extract(epoch FROM occurred_at) / 600)::bigint AS bucket
            FROM data_location_point
            WHERE latitude BETWEEN -85 AND 85 AND longitude BETWEEN -180 AND 180
              AND deleted_at_source IS NULL AND NOT is_archived
        )
        SELECT x, y, ((now() AT TIME ZONE 'UTC')::date - day)::int AS age
        FROM p
        GROUP BY day, x, y
        HAVING count(DISTINCT bucket) >= 6
        "#,
    )
    .fetch_all(pool)
    .await
    .context("reading location presence")?;

    let mut presence: Presence = HashMap::new();
    for (x, y, age) in rows {
        if !(0..128).contains(&x) || !(0..128).contains(&y) {
            continue;
        }
        presence
            .entry((x as u32, y as u32))
            .or_default()
            .ages
            .push(i64::from(age.max(0)));
    }
    Ok(presence)
}

/// The files to hold: the world file and assets always, then home and
/// visited squares by importance within their budgets.
pub fn select(index: &Index, presence: &Presence) -> Vec<IndexFile> {
    let by_name: HashMap<&str, &IndexFile> = index.files.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut picked: Vec<IndexFile> = index
        .files
        .iter()
        // By exact name: an index entry's name becomes a path on this disk.
        .filter(|f| (f.tier == "world" && f.name == "world.pmtiles") || (f.tier == "assets" && f.name == "assets.tar"))
        .cloned()
        .collect();

    // Home: squares with a week or more, places lived in first, then by score.
    let mut home: Vec<(&(u32, u32), &SquareDays)> =
        presence.iter().filter(|(_, d)| d.days() >= HOME_MIN_DAYS).collect();
    home.sort_by(|a, b| {
        let lived = |d: &SquareDays| d.days() >= LIVED_THERE_DAYS;
        lived(b.1)
            .cmp(&lived(a.1))
            .then(b.1.score().total_cmp(&a.1.score()))
            .then(a.0.cmp(b.0))
    });
    let mut used = 0u64;
    let mut home_used = 0u64;
    for ((x, y), _) in home {
        let Some(f) = by_name.get(format!("home-z7-{x}-{y}.pmtiles").as_str()) else {
            continue; // open sea, or not in this build
        };
        if home_used + f.bytes > HOME_BUDGET_BYTES {
            continue;
        }
        home_used += f.bytes;
        used += f.bytes;
        picked.push((*f).clone());
    }

    // Visited: z5 squares with two or more distinct days, by score.
    let mut visited: HashMap<(u32, u32), SquareDays> = HashMap::new();
    for ((x, y), d) in presence {
        let v = visited.entry((x >> 2, y >> 2)).or_default();
        v.ages.extend(d.ages.iter().copied());
    }
    for v in visited.values_mut() {
        // A day spent in two z7 squares of one z5 square is one day.
        v.ages.sort_unstable();
        v.ages.dedup();
    }
    let mut visited: Vec<_> = visited.into_iter().filter(|(_, d)| d.days() >= VISITED_MIN_DAYS).collect();
    visited.sort_by(|a, b| b.1.score().total_cmp(&a.1.score()).then(a.0.cmp(&b.0)));
    for ((x, y), _) in visited {
        let Some(f) = by_name.get(format!("visited-z5-{x}-{y}.pmtiles").as_str()) else {
            continue;
        };
        if used + f.bytes > BUDGET_BYTES {
            continue;
        }
        used += f.bytes;
        picked.push((*f).clone());
    }
    picked
}

/// Where maps come from: `VIRTUES_MAPS_SOURCE` (a self-hosted copy with the
/// same `index` and `<build>/<file>` layout), else virtues-api.
fn source_base() -> String {
    match std::env::var("VIRTUES_MAPS_SOURCE") {
        Ok(s) if !s.is_empty() => s.trim_end_matches('/').to_string(),
        _ => format!("{}/v1/maps", crate::virtues_api::api_url().trim_end_matches('/')),
    }
}

async fn bearer(pool: &PgPool) -> Result<Option<String>> {
    if let Ok(key) = std::env::var("VIRTUES_API_KEY") {
        if !key.is_empty() {
            return Ok(Some(key));
        }
    }
    crate::virtues_api::renew::read_api_key(pool).await
}

fn build_date(build: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(build, "%Y%m%d").ok()
}

/// Does the file on disk still serve? Same bytes as the index, or from a
/// build recent enough that refreshing it is not worth a download.
fn is_current(manifest: &Manifest, dir: &Path, f: &IndexFile, build: &str) -> bool {
    if !dir.join(&f.name).exists() {
        return false;
    }
    let Some((held_build, held_sha)) = manifest.files.get(&f.name) else {
        return false;
    };
    if *held_sha == f.sha256 {
        return true;
    }
    // An older fonts archive that never unpacked serves nothing, so the
    // refresh window must not keep it: the newer build's archive may unpack.
    if f.tier == "assets" && !dir.join("assets").exists() {
        return false;
    }
    match (build_date(held_build), build_date(build)) {
        (Some(held), Some(now)) => (now - held).num_days() < REFRESH_AFTER_DAYS,
        _ => false,
    }
}

/// Download one file into `<name>.tmp`, resuming a partial one, check its
/// sha256, and rename it into place. A rename swaps the inode, so a reader
/// holding the old file's mmap keeps working until the next reload.
async fn download(http: &reqwest::Client, url: &str, bearer: Option<&str>, dir: &Path, f: &IndexFile) -> Result<()> {
    let tmp = dir.join(format!("{}.tmp", f.name));
    let have = tokio::fs::metadata(&tmp).await.map(|m| m.len()).unwrap_or(0);
    let mut req = http.get(url);
    if let Some(b) = bearer {
        req = req.bearer_auth(b);
    }
    if have > 0 && have < f.bytes {
        req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    // No `{e}` of a reqwest error reaches a log with its URL: that URL names a square.
    let resp = req.send().await.map_err(|e| anyhow::anyhow!("map download request: {}", e.without_url()))?;
    let status = resp.status();
    let mut resuming = status == reqwest::StatusCode::PARTIAL_CONTENT;
    if !status.is_success() {
        bail!("map download answered {status}");
    }
    if resuming {
        // Append only where we asked to start; anything else starts over.
        let starts_at_have = resp
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with(&format!("bytes {have}-")));
        if !starts_at_have {
            let _ = tokio::fs::remove_file(&tmp).await;
            bail!("map download resumed at the wrong offset; starting over next pass");
        }
    } else {
        resuming = false;
    }
    let mut out = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resuming)
        .truncate(!resuming)
        .open(&tmp)
        .await?;
    let mut written = if resuming { have } else { 0 };
    let mut body = resp.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| anyhow::anyhow!("map download interrupted: {}", e.without_url()))?;
        written += chunk.len() as u64;
        if written > f.bytes {
            // The index says how big the file is; more than that is not it.
            drop(out);
            let _ = tokio::fs::remove_file(&tmp).await;
            bail!("map download ran past its expected size");
        }
        out.write_all(&chunk).await?;
    }
    out.flush().await?;
    drop(out);

    let sha = {
        let path = tmp.clone();
        tokio::task::spawn_blocking(move || -> Result<String> {
            let mut h = Sha256::new();
            let mut file = std::fs::File::open(path)?;
            std::io::copy(&mut file, &mut h)?;
            Ok(hex::encode(h.finalize()))
        })
        .await??
    };
    if sha != f.sha256 {
        // Start over next time rather than resuming onto bad bytes.
        let _ = tokio::fs::remove_file(&tmp).await;
        bail!("map file failed its checksum");
    }
    tokio::fs::rename(&tmp, dir.join(&f.name)).await?;
    Ok(())
}

fn unpack_assets(archive: PathBuf, dir: PathBuf) -> Result<()> {
    let staging = dir.join("assets.new");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    // Entry by entry, refusing links: the glyph and sprite routes read files
    // under assets/, and a symlink there would let them read any file.
    let mut ar = tar::Archive::new(std::fs::File::open(&archive)?);
    for entry in ar.entries()? {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            anyhow::bail!("the assets archive holds a link; refusing it");
        }
        entry.unpack_in(&staging)?;
    }
    let live = dir.join("assets");
    let old = dir.join("assets.old");
    let _ = std::fs::remove_dir_all(&old);
    if live.exists() {
        std::fs::rename(&live, &old)?;
    }
    std::fs::rename(&staging, &live)?;
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub downloaded: usize,
    pub removed: usize,
    pub held_bytes: u64,
}

/// One pass. Stops quietly (Ok) when there is nothing to sync against: no
/// subscription, a lapsed one, or no build published yet. Files already on
/// disk are kept in every such case.
pub async fn sync_once(pool: &PgPool) -> Result<SyncReport> {
    let dir = maps_root();
    tokio::fs::create_dir_all(&dir).await?;
    let base = source_base();
    let self_hosted = std::env::var("VIRTUES_MAPS_SOURCE").is_ok_and(|s| !s.is_empty());
    // The subscription key goes to virtues-api and nowhere else.
    let key = if self_hosted { None } else { bearer(pool).await? };
    if key.is_none() && !self_hosted {
        tracing::info!("maps: no subscription linked; keeping what is on disk");
        return Ok(SyncReport::default());
    }

    let http = crate::http_client::base_builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(6 * 3600))
        .build()
        .context("maps http client")?;
    let mut req = http.get(format!("{base}/index"));
    if let Some(k) = &key {
        req = req.bearer_auth(k);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("reading the map index: {}", e.without_url()))?;
    match resp.status().as_u16() {
        200 => {}
        401 | 402 | 403 => {
            tracing::info!("maps: subscription not active; keeping what is on disk");
            return Ok(SyncReport::default());
        }
        503 => {
            tracing::info!("maps: no map build published yet");
            return Ok(SyncReport::default());
        }
        s => bail!("map index answered {s}"),
    }
    let index: Index = resp.json().await.context("parsing the map index")?;

    let presence = load_presence(pool).await?;
    let want = select(&index, &presence);

    let manifest_path = dir.join("manifest.json");
    let mut manifest: Manifest = tokio::fs::read(&manifest_path)
        .await
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default(); // absent or unreadable: files on disk are re-adopted by hash below

    let mut report = SyncReport::default();
    let mut changed = false;
    for f in &want {
        if is_current(&manifest, &dir, f, &index.build) {
            continue;
        }
        // On disk but not in the manifest: a pass that was cut short after the
        // rename, or a lost manifest. Keep it if its bytes are the index's.
        if !manifest.files.contains_key(&f.name) && sha256_of(dir.join(&f.name)).await.as_deref() == Some(&f.sha256) {
            manifest.files.insert(f.name.clone(), (index.build.clone(), f.sha256.clone()));
            save_manifest(&dir, &manifest).await?;
            changed = true;
            if f.tier == "assets" && !dir.join("assets").exists() {
                unpack_logged(&dir, f).await;
            }
            continue;
        }
        let free = crate::storage::lake::free_bytes_at(&dir).unwrap_or(u64::MAX);
        if free < f.bytes + MIN_FREE_BYTES {
            tracing::warn!("maps: not enough free disk for the next map file; stopping this pass");
            break;
        }
        let url = format!("{base}/{}/{}", index.build, f.name);
        match download(&http, &url, key.as_deref(), &dir, f).await {
            Ok(()) => {
                // A fonts bundle that will not unpack is not recorded, so the
                // next pass tries again; it does not stop this one.
                if f.tier == "assets" && !unpack_logged(&dir, f).await {
                    continue;
                }
                manifest.files.insert(f.name.clone(), (index.build.clone(), f.sha256.clone()));
                // Saved per file: a restart mid-pass must not cost the files
                // already in place.
                save_manifest(&dir, &manifest).await?;
                report.downloaded += 1;
                changed = true;
            }
            // One bad file must not stop the rest; the next pass retries it.
            Err(e) => tracing::warn!("maps: a map file did not download: {e:#}"),
        }
    }

    for name in removals(&manifest, &want, index.complete, presence.is_empty()) {
        match tokio::fs::remove_file(dir.join(&name)).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                // Still on disk, so still held: keep it in the manifest.
                tracing::warn!("maps: could not remove a map file: {e}");
                continue;
            }
        }
        manifest.files.remove(&name);
        report.removed += 1;
        changed = true;
    }

    report.held_bytes = want
        .iter()
        .filter(|f| manifest.files.contains_key(&f.name))
        .map(|f| f.bytes)
        .sum();
    save_manifest(&dir, &manifest).await?;
    if changed {
        super::reload().await;
    }
    Ok(report)
}

/// The held files to delete this pass: those the selection no longer picks.
/// None at all unless the index is a complete cut AND the box has location
/// history. A partial index, or a history that is momentarily empty (a
/// restore, a re-import), would otherwise read as "delete every square".
fn removals(manifest: &Manifest, want: &[IndexFile], index_complete: bool, presence_empty: bool) -> Vec<String> {
    if !index_complete || presence_empty {
        return Vec::new();
    }
    let keep: HashSet<&str> = want.iter().map(|f| f.name.as_str()).collect();
    let mut gone: Vec<String> = manifest.files.keys().filter(|n| !keep.contains(n.as_str())).cloned().collect();
    gone.sort();
    gone
}

async fn save_manifest(dir: &Path, manifest: &Manifest) -> Result<()> {
    let tmp = dir.join("manifest.json.tmp");
    tokio::fs::write(&tmp, serde_json::to_vec_pretty(manifest)?).await?;
    tokio::fs::rename(&tmp, dir.join("manifest.json")).await?;
    Ok(())
}

async fn sha256_of(path: PathBuf) -> Option<String> {
    tokio::task::spawn_blocking(move || -> Option<String> {
        let mut h = Sha256::new();
        let mut file = std::fs::File::open(path).ok()?; // absent: nothing to adopt
        std::io::copy(&mut file, &mut h).ok()?;
        Some(hex::encode(h.finalize()))
    })
    .await
    .ok()
    .flatten()
}

/// Unpack the fonts bundle; false (logged) when it will not.
async fn unpack_logged(dir: &Path, f: &IndexFile) -> bool {
    let (a, d) = (dir.join(&f.name), dir.to_path_buf());
    match tokio::task::spawn_blocking(move || unpack_assets(a, d)).await {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            tracing::warn!("maps: the fonts bundle did not unpack: {e:#}");
            false
        }
        Err(e) => {
            tracing::warn!("maps: the fonts unpack task failed: {e}");
            false
        }
    }
}

/// Run a pass a few minutes after start, then daily. Off in a dev checkout
/// unless `VIRTUES_MAPS_SYNC=1`: a dev box reads `data/maps`, which
/// `tools/maps-dev.sh` fills by hand, and a sync against dev data would delete
/// those files.
pub fn spawn(pool: PgPool) {
    let dev = std::env::var("ENVIRONMENT").is_ok_and(|e| e == "dev");
    let forced = std::env::var("VIRTUES_MAPS_SYNC").is_ok_and(|v| v == "1");
    if dev && !forced {
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(if forced { 5 } else { 180 })).await;
        let mut tick = tokio::time::interval(Duration::from_secs(24 * 3600));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            match sync_once(&pool).await {
                Ok(r) if r.downloaded + r.removed > 0 => tracing::info!(
                    "maps: {} file(s) downloaded, {} removed, {:.1} GB held",
                    r.downloaded,
                    r.removed,
                    r.held_bytes as f64 / 1e9
                ),
                Ok(_) => {}
                Err(e) => tracing::warn!("maps: sync failed: {e:#}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, tier: &str, z: u8, x: u32, y: u32, bytes: u64) -> IndexFile {
        IndexFile { name: name.into(), tier: tier.into(), z, x, y, bytes, sha256: "s".into() }
    }

    fn days(ages: &[i64]) -> SquareDays {
        SquareDays { ages: ages.to_vec() }
    }

    fn index() -> Index {
        const MB: u64 = 1024 * 1024;
        Index {
            build: "20260927".into(),
            complete: true,
            files: vec![
                file("world.pmtiles", "world", 0, 0, 0, 188 * MB),
                file("assets.tar", "assets", 0, 0, 0, 14 * MB),
                file("home-z7-29-52.pmtiles", "home", 7, 29, 52, 187 * MB),
                file("home-z7-37-48.pmtiles", "home", 7, 37, 48, 434 * MB),
                file("home-z7-64-42.pmtiles", "home", 7, 64, 42, 1_800 * MB),
                file("visited-z5-7-13.pmtiles", "visited", 5, 7, 13, 160 * MB),
                file("visited-z5-9-12.pmtiles", "visited", 5, 9, 12, 266 * MB),
                file("visited-z5-16-10.pmtiles", "visited", 5, 16, 10, 1_500 * MB),
            ],
        }
    }

    fn names(picked: &[IndexFile]) -> Vec<&str> {
        let mut n: Vec<&str> = picked.iter().map(|f| f.name.as_str()).collect();
        n.sort_unstable();
        n
    }

    #[test]
    fn an_older_fonts_archive_that_never_unpacked_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("assets.tar"), b"old").unwrap();
        let mut manifest = Manifest::default();
        manifest.files.insert("assets.tar".into(), ("20260928".into(), "old".into()));
        let f = file("assets.tar", "assets", 0, 0, 0, 1);
        // Four days newer is inside the refresh window, but nothing unpacked.
        assert!(!is_current(&manifest, dir.path(), &f, "20261002"));
        // Once the fonts are on disk, the window applies as to any other file.
        std::fs::create_dir(dir.path().join("assets")).unwrap();
        assert!(is_current(&manifest, dir.path(), &f, "20261002"));
    }

    #[test]
    fn a_new_box_gets_the_world_and_fonts_only() {
        assert_eq!(names(&select(&index(), &Presence::new())), ["assets.tar", "world.pmtiles"]);
    }

    #[test]
    fn a_drive_through_downloads_nothing() {
        let presence = Presence::from([((37, 48), days(&[3]))]);
        assert_eq!(names(&select(&index(), &presence)), ["assets.tar", "world.pmtiles"]);
    }

    #[test]
    fn a_week_somewhere_earns_home_detail_and_its_region() {
        let presence = Presence::from([((29, 52), days(&[0, 1, 2, 3, 4, 5, 6]))]);
        assert_eq!(
            names(&select(&index(), &presence)),
            ["assets.tar", "home-z7-29-52.pmtiles", "visited-z5-7-13.pmtiles", "world.pmtiles"]
        );
    }

    #[test]
    fn a_short_trip_gets_street_level_only() {
        let presence = Presence::from([((37, 48), days(&[40, 41]))]);
        assert_eq!(names(&select(&index(), &presence)), ["assets.tar", "visited-z5-9-12.pmtiles", "world.pmtiles"]);
    }

    #[test]
    fn a_place_lived_in_keeps_home_detail_after_moving() {
        // Five years in one city ending two years ago, and three weeks in a new one.
        let old: Vec<i64> = (730..730 + 5 * 365).collect();
        let new: Vec<i64> = (0..21).collect();
        let presence = Presence::from([((37, 48), days(&old)), ((29, 52), days(&new))]);
        let selected = select(&index(), &presence);
        let picked = names(&selected);
        assert!(picked.contains(&"home-z7-37-48.pmtiles"), "{picked:?}");
        assert!(picked.contains(&"home-z7-29-52.pmtiles"), "{picked:?}");
    }

    #[test]
    fn the_budget_is_never_exceeded() {
        let every: Vec<i64> = (0..400).collect();
        let presence = Presence::from([
            ((29, 52), days(&every)),
            ((37, 48), days(&every)),
            ((64, 42), days(&every)),
        ]);
        let picked = select(&index(), &presence);
        let home: u64 = picked.iter().filter(|f| f.tier == "home").map(|f| f.bytes).sum();
        let both: u64 = picked.iter().filter(|f| f.tier == "home" || f.tier == "visited").map(|f| f.bytes).sum();
        assert!(home <= HOME_BUDGET_BYTES, "home {home}");
        assert!(both <= BUDGET_BYTES, "both {both}");
    }

    #[test]
    fn recent_days_outweigh_old_ones() {
        assert!(days(&[0, 1, 2]).score() > days(&[400, 401, 402, 403]).score());
    }

    fn held(names: &[&str]) -> Manifest {
        Manifest { files: names.iter().map(|n| (n.to_string(), ("20260927".into(), "s".into()))).collect() }
    }

    #[test]
    fn nothing_is_deleted_against_a_partial_index() {
        // A trial cut that lists only a few squares must not read as "delete the rest".
        let m = held(&["home-z7-29-52.pmtiles", "visited-z5-7-13.pmtiles"]);
        let want = vec![file("world.pmtiles", "world", 0, 0, 0, 1)];
        assert!(removals(&m, &want, false, false).is_empty());
    }

    #[test]
    fn nothing_is_deleted_while_the_history_is_empty() {
        // A restore or re-import in progress is not "the owner has been nowhere".
        let m = held(&["home-z7-29-52.pmtiles"]);
        let want = vec![file("world.pmtiles", "world", 0, 0, 0, 1)];
        assert!(removals(&m, &want, true, true).is_empty());
    }

    #[test]
    fn a_square_no_longer_picked_is_deleted_against_a_complete_index() {
        let m = held(&["home-z7-29-52.pmtiles", "world.pmtiles"]);
        let want = vec![file("world.pmtiles", "world", 0, 0, 0, 1)];
        assert_eq!(removals(&m, &want, true, false), ["home-z7-29-52.pmtiles"]);
    }

    #[test]
    fn index_names_that_would_leave_the_maps_directory_are_ignored() {
        let mut idx = index();
        idx.files.push(file("../../etc/passwd", "world", 0, 0, 0, 1));
        idx.files.push(file("/tmp/x", "assets", 0, 0, 0, 1));
        let selected = select(&idx, &Presence::new());
        assert_eq!(names(&selected), ["assets.tar", "world.pmtiles"]);
    }

    #[test]
    fn a_link_in_the_fonts_bundle_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("assets.tar");
        {
            let mut b = tar::Builder::new(std::fs::File::create(&archive).unwrap());
            let mut h = tar::Header::new_gnu();
            h.set_entry_type(tar::EntryType::Symlink);
            h.set_size(0);
            b.append_link(&mut h, "fonts/evil", "/etc/passwd").unwrap();
            b.finish().unwrap();
        }
        assert!(unpack_assets(archive, dir.path().to_path_buf()).is_err());
        assert!(!dir.path().join("assets").exists());
    }

    #[test]
    fn a_file_from_a_recent_build_is_not_downloaded_again() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("world.pmtiles"), b"x").unwrap();
        let f = file("world.pmtiles", "world", 0, 0, 0, 1);
        let held = |build: &str| Manifest {
            files: HashMap::from([("world.pmtiles".into(), (build.into(), "old".into()))]),
        };
        assert!(is_current(&held("20260901"), dir.path(), &f, "20260927"));
        assert!(!is_current(&held("20260601"), dir.path(), &f, "20260927"));
        assert!(!is_current(&Manifest::default(), dir.path(), &f, "20260927"));
    }
}
