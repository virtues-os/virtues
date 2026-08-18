//! Fetching the QAIRT runtime libs that `virtues-qnnd` `dlopen`s.
//!
//! The Dragon's NPU daemon is built against QAIRT *headers* in CI, but at
//! runtime it opens `libQnnHtp.so` + friends on the box. Nothing put them
//! there. The lab Dragon works only because someone hand-unpacked a 1.44 GB SDK
//! into `/qairt-extract/`, which no DIY owner will ever reproduce — so a Dragon
//! install produced a daemon that could not start, and (until the gate in
//! `install_qnn`) a crash loop that ran for ten days on one box.
//!
//! ## Why we fetch instead of re-hosting
//!
//! The obvious fix is to publish the five `.so` files to our `models-*` release
//! bucket beside the GGUFs and context binaries. The SDK's licence forecloses
//! exactly that: it grants distribution of the Software in object code as
//! incorporated in your own application, then withholds any licence to
//! distribute it **on a standalone basis**, which is precisely what a bare `.so`
//! release asset is. So the box fetches from Qualcomm's own public distribution
//! and we re-host nothing.
//!
//! ## Why a Range read rather than a download
//!
//! The zip is 1.44 GB and we need 15.36 MB of it. Qualcomm's CDN honours Range
//! requests, so we read the central directory out of the tail and then pull only
//! the members we want — about 5.6 MB on the wire, since they are Deflate'd.
//! (`HEAD` is refused with 403; a one-byte Range GET is how you learn the
//! length.) Six requests, ~6 s measured end to end.
//!
//! The zip parsing is hand-rolled rather than delegated — see `Fetcher` for the
//! measurement that forced it.
//!
//! ## Why these five files
//!
//! Measured, not guessed. The live daemon on the lab Dragon maps exactly three
//! host libs — `libQnnHtp.so`, `libQnnSystem.so`, `libQnnHtpV68Stub.so`. The
//! calculator stub is the documented companion of the V68 stub and costs 10 KB,
//! so it comes along rather than being discovered missing on some other op path.
//! `libQnnHtpPrepare.so` is deliberately absent: it is 90 MB, it is the offline
//! graph compiler, and it never appears in the running process — we load
//! precompiled context binaries. The skel is the Hexagon-side half and loads
//! onto the DSP, which is why it lives in its own directory pointed at by
//! `ADSP_LIBRARY_PATH` rather than `LD_LIBRARY_PATH`.
//!
//! The pinned digests are the bytes the lab Dragon is validated against — they
//! were taken from the remote zip and then confirmed equal to the files that
//! box is measurably running (3.8 ms/call embed). Version has to stay in step
//! with `QAIRT_VERSION` in `release-linux.yml` (headers the daemon compiles
//! against) and with the QAIRT the `.bin` context binaries were compiled with.

use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::InstallConfig;
use crate::ui;

/// Must match `QAIRT_VERSION` in `.github/workflows/release-linux.yml`.
pub const QAIRT_VERSION: &str = "2.42.0.251225";

/// The SDK subdirectory holding the host-side libs. "oe" reads like Yocto, but
/// this is the variant the lab Dragon is actually running on Ubuntu 24.04 /
/// glibc 2.39 — don't "correct" it to `aarch64-ubuntu-gcc9.4` without measuring.
const HOST_TRIPLE: &str = "aarch64-oe-linux-gcc11.2";

/// Host-side libs → `LD_LIBRARY_PATH`. (file name, sha256)
const HOST_LIBS: &[(&str, &str)] = &[
    ("libQnnHtp.so", "6a04a4b8e276b863eb1ea8550f6855a3f8345412c5548f5679d26d7c2609b02a"),
    ("libQnnSystem.so", "9c2692c5cbc5d062beede749480cf3448fa9aa0267dfcb94933ad63078cf356d"),
    ("libQnnHtpV68Stub.so", "1436f5337f6d469cb6aca1095fc7426ab1ac61d030c4a39b2cdf013fe17d9ec4"),
    (
        "libQnnHtpV68CalculatorStub.so",
        "3e8fa1c13e51e9e0d5df65ef4a6b3ced646459ebd09dd8a2079b55f07001886b",
    ),
];

/// Hexagon-side skel → `ADSP_LIBRARY_PATH`. (file name, sha256)
const DSP_LIBS: &[(&str, &str)] =
    &[("libQnnHtpV68Skel.so", "2cf8b6662cd9c98049c6ac0285d83c4b6966e6a1c9abb00aa843cb8e7e706b3c")];

fn sdk_url() -> String {
    format!(
        "https://softwarecenter.qualcomm.com/api/download/software/sdks/\
         Qualcomm_AI_Runtime_Community/All/{QAIRT_VERSION}/v{QAIRT_VERSION}.zip"
    )
}

/// Ensure the runtime libs are on the box, fetching them if they are not.
/// Returns `(host_dir, dsp_dir)` for the unit's two library paths.
pub async fn ensure_libs(cfg: &InstallConfig) -> Result<(PathBuf, PathBuf)> {
    let root = cfg.qnn_managed_lib_dir();
    let host_dir = root.join("host");
    let dsp_dir = root.join("dsp");

    if verify_dir(&host_dir, HOST_LIBS) && verify_dir(&dsp_dir, DSP_LIBS) {
        ui::skip("QAIRT runtime libs already present");
        return Ok((host_dir, dsp_dir));
    }

    ui::info(&format!(
        "Fetching QAIRT {QAIRT_VERSION} runtime libs from Qualcomm (~6 MB of a 1.4 GB SDK)"
    ));

    let (h, d) = (host_dir.clone(), dsp_dir.clone());
    // Blocking: the `zip` reader wants Read+Seek, and each seek is an HTTP
    // range request. Confining that to one blocking task keeps it out of the
    // async executor rather than sprinkling block_on through a Read impl.
    tokio::task::spawn_blocking(move || extract_blocking(&h, &d))
        .await
        .context("QAIRT extraction task panicked")??;

    if !verify_dir(&host_dir, HOST_LIBS) || !verify_dir(&dsp_dir, DSP_LIBS) {
        bail!("QAIRT libs failed verification after fetch");
    }
    ui::ok(&format!("QAIRT runtime libs installed ({})", host_dir.display()));
    link_cdsprpc(&host_dir);
    Ok((host_dir, dsp_dir))
}

/// Give QNN the unversioned `libcdsprpc.so` name it dlopens.
///
/// The Radxa image ships `libcdsprpc1`, which installs ONLY the soname
/// (`libcdsprpc.so.1`); the bare `.so` symlink lives in a `-dev` package
/// nobody has. QNN's HTP stub dlopens the bare name, walks every loader
/// path, gets ENOENT — and reports it as `Transport layer setup failed:
/// 14001`, three layers away from the cause. The lab box worked because
/// someone left a hand-made symlink during NPU bring-up that never became
/// an install step; the first fresh master build, 2026-08-18, is where it
/// finally failed. Symlinked HERE, inside our own managed lib dir (already
/// on the unit's LD_LIBRARY_PATH), so no system directory is touched.
fn link_cdsprpc(host_dir: &Path) {
    let target = Path::new("/usr/lib/aarch64-linux-gnu/libcdsprpc.so.1");
    if !target.exists() {
        ui::warn(
            "libcdsprpc.so.1 not found — QNN needs the cdsp FastRPC lib \
             (Radxa: apt install libcdsprpc1), the NPU will not serve without it",
        );
        return;
    }
    let link = host_dir.join("libcdsprpc.so");
    let _ = fs::remove_file(&link);
    if let Err(e) = std::os::unix::fs::symlink(target, &link) {
        ui::warn(&format!("could not link {}: {e}", link.display()));
    }
}

/// Every expected file present with the pinned digest.
///
/// Digest-checked rather than existence-checked, unlike the GGUF path: these
/// are 15 MB total so hashing is free, and a half-written lib from an install
/// interrupted mid-extract would otherwise present as "already there" and fail
/// later as an opaque `dlopen` error.
fn verify_dir(dir: &Path, want: &[(&str, &str)]) -> bool {
    want.iter().all(|(name, sha)| {
        let p = dir.join(name);
        match fs::read(&p) {
            Ok(bytes) => hex::encode(Sha256::digest(&bytes)) == *sha,
            Err(_) => false,
        }
    })
}

fn extract_blocking(host_dir: &Path, dsp_dir: &Path) -> Result<()> {
    fs::create_dir_all(host_dir).with_context(|| format!("creating {}", host_dir.display()))?;
    fs::create_dir_all(dsp_dir).with_context(|| format!("creating {}", dsp_dir.display()))?;

    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_style(
        indicatif::ProgressStyle::with_template("  {spinner:.dim} {msg}")
            .unwrap()
            .tick_strings(&["\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}", "\u{2827}", "\u{2807}", "\u{280f}"]),
    );
    spinner.enable_steady_tick(Duration::from_millis(80));
    spinner.set_message("reading QAIRT SDK index");

    let fetcher = Fetcher::new(sdk_url())?;
    let dir = fetcher.central_directory()?;

    let base = format!("qairt/{QAIRT_VERSION}/lib");
    let wanted = HOST_LIBS
        .iter()
        .map(|(n, s)| (format!("{base}/{HOST_TRIPLE}/{n}"), host_dir.join(n), *s))
        .chain(
            DSP_LIBS
                .iter()
                .map(|(n, s)| (format!("{base}/hexagon-v68/unsigned/{n}"), dsp_dir.join(n), *s)),
        );

    for (member, dest, sha) in wanted {
        let leaf = member.rsplit('/').next().unwrap_or(&member).to_string();
        spinner.set_message(format!("fetching {leaf}"));

        let entry = dir
            .get(&member)
            .ok_or_else(|| anyhow!("{member} not found in QAIRT {QAIRT_VERSION}"))?;
        let bytes = fetcher.member(entry).with_context(|| format!("extracting {member}"))?;

        let got = hex::encode(Sha256::digest(&bytes));
        if got != sha {
            bail!("{member}: sha256 {got}, expected {sha} \u{2014} QAIRT {QAIRT_VERSION} changed?");
        }
        // Same-dir temp + rename: an interrupted install must not leave a
        // truncated .so that later passes an existence check.
        let tmp = dest.with_extension("part");
        fs::write(&tmp, &bytes).with_context(|| format!("writing {}", tmp.display()))?;
        // The daemon runs as `virtues`, not root. Set the mode explicitly rather
        // than inheriting whatever umask the install shell happened to carry.
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o644))
            .with_context(|| format!("chmod {}", tmp.display()))?;
        fs::rename(&tmp, &dest).with_context(|| format!("renaming into {}", dest.display()))?;
    }
    spinner.finish_and_clear();
    Ok(())
}

// \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}
// Minimal zip reader over HTTP Range
// \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}

/// Why this exists instead of handing a Read+Seek to the `zip` crate: measured,
/// that combination issued 343 range requests and pulled 343 MB to extract 15 —
/// it walks the archive rather than jumping via the central directory, and over
/// a network each of those steps is a round trip. Four minutes for five files.
///
/// Parsing the central directory ourselves makes the cost deterministic: one
/// request for the tail, then exactly one per member. Same 15 MB out, ~10 s.
/// We only support what this archive is — classic (non-zip64) central directory
/// entries, Deflate or Store — and fail loudly on anything else rather than
/// guessing.
const TAIL_BYTES: u64 = 4 * 1024 * 1024;

/// Slack fetched past a member's compressed size to cover its local file
/// header, whose name/extra lengths can differ from the central directory's and
/// so aren't known until we read it.
const LOCAL_HEADER_SLACK: u64 = 4096;

struct CdEntry {
    /// Offset of the local file header.
    local_offset: u64,
    compressed_size: u64,
    uncompressed_size: u64,
    /// 0 = stored, 8 = deflate.
    method: u16,
}

struct Fetcher {
    client: reqwest::blocking::Client,
    url: String,
    len: u64,
}

impl Fetcher {
    fn new(url: String) -> Result<Self> {
        // main() installs the ring provider process-wide, but this module is
        // also reachable from tests, where main() never runs and rustls panics
        // with a bare "No provider set". Idempotent, so it costs nothing here.
        let _ = rustls::crypto::ring::default_provider().install_default();

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .user_agent("Mozilla/5.0")
            .build()?;
        let me = Self { client, url, len: 0 };

        // The CDN answers HEAD with 403; a one-byte Range GET gets us the total
        // out of Content-Range.
        let resp = me
            .client
            .get(&me.url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .send()
            .context("probing QAIRT SDK (is Qualcomm's software centre reachable?)")?
            .error_for_status()?;
        let cr = resp
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| anyhow!("QAIRT download did not honour a Range request"))?;
        let len: u64 = cr
            .rsplit('/')
            .next()
            .and_then(|s| s.trim().parse().ok())
            .ok_or_else(|| anyhow!("unparseable Content-Range: {cr}"))?;
        if len == 0 {
            bail!("QAIRT download reported zero length");
        }
        Ok(Self { len, ..me })
    }

    fn fetch(&self, start: u64, end_inclusive: u64) -> Result<Vec<u8>> {
        let resp = self
            .client
            .get(&self.url)
            .header(reqwest::header::RANGE, format!("bytes={start}-{end_inclusive}"))
            .send()
            .with_context(|| format!("QAIRT range {start}-{end_inclusive}"))?
            .error_for_status()?;
        Ok(resp.bytes()?.to_vec())
    }

    /// Read the tail, locate the end-of-central-directory record, and parse the
    /// central directory into a name -> entry map.
    fn central_directory(&self) -> Result<std::collections::HashMap<String, CdEntry>> {
        let tail_len = TAIL_BYTES.min(self.len);
        let tail_start = self.len - tail_len;
        let tail = self.fetch(tail_start, self.len - 1)?;

        // EOCD: signature, then at +16 the central directory offset, at +12 its
        // size. Scanned from the end because a trailing comment may follow it.
        let eocd = tail
            .windows(4)
            .rposition(|w| w == [0x50, 0x4b, 0x05, 0x06])
            .ok_or_else(|| anyhow!("no end-of-central-directory record in the QAIRT zip tail"))?;
        let cd_size = u32::from_le_bytes(tail[eocd + 12..eocd + 16].try_into()?) as u64;
        let cd_off = u32::from_le_bytes(tail[eocd + 16..eocd + 20].try_into()?) as u64;
        if cd_off == u32::MAX as u64 || cd_size == u32::MAX as u64 {
            bail!("QAIRT zip uses zip64 offsets; this reader only handles classic entries");
        }

        // Usually already in the tail; fetch it if the archive grew past it.
        let cd = if cd_off >= tail_start {
            let from = (cd_off - tail_start) as usize;
            tail[from..from + cd_size as usize].to_vec()
        } else {
            self.fetch(cd_off, cd_off + cd_size - 1)?
        };

        let mut out = std::collections::HashMap::new();
        let mut p = 0usize;
        while p + 46 <= cd.len() && cd[p..p + 4] == [0x50, 0x4b, 0x01, 0x02] {
            let method = u16::from_le_bytes(cd[p + 10..p + 12].try_into()?);
            let compressed_size = u32::from_le_bytes(cd[p + 20..p + 24].try_into()?) as u64;
            let uncompressed_size = u32::from_le_bytes(cd[p + 24..p + 28].try_into()?) as u64;
            let name_len = u16::from_le_bytes(cd[p + 28..p + 30].try_into()?) as usize;
            let extra_len = u16::from_le_bytes(cd[p + 30..p + 32].try_into()?) as usize;
            let comment_len = u16::from_le_bytes(cd[p + 32..p + 34].try_into()?) as usize;
            let local_offset = u32::from_le_bytes(cd[p + 42..p + 46].try_into()?) as u64;
            let name = String::from_utf8_lossy(&cd[p + 46..p + 46 + name_len]).into_owned();
            out.insert(
                name,
                CdEntry { local_offset, compressed_size, uncompressed_size, method },
            );
            p += 46 + name_len + extra_len + comment_len;
        }
        if out.is_empty() {
            bail!("QAIRT zip central directory parsed to zero entries");
        }
        Ok(out)
    }

    /// One range request covering the member's local header and body, then
    /// inflate. The header's name/extra lengths are only knowable from the
    /// header itself, hence the slack.
    fn member(&self, e: &CdEntry) -> Result<Vec<u8>> {
        let end = (e.local_offset + 30 + LOCAL_HEADER_SLACK + e.compressed_size).min(self.len) - 1;
        let buf = self.fetch(e.local_offset, end)?;
        if buf.len() < 30 || buf[..4] != [0x50, 0x4b, 0x03, 0x04] {
            bail!("member local header missing at offset {}", e.local_offset);
        }
        let name_len = u16::from_le_bytes(buf[26..28].try_into()?) as usize;
        let extra_len = u16::from_le_bytes(buf[28..30].try_into()?) as usize;
        let data_at = 30 + name_len + extra_len;
        let want = e.compressed_size as usize;

        // The slack is generous, but a pathological extra field could still
        // push the body past what we pulled; fetch the exact range instead of
        // silently truncating.
        let data = if buf.len() >= data_at + want {
            buf[data_at..data_at + want].to_vec()
        } else {
            let start = e.local_offset + data_at as u64;
            self.fetch(start, start + e.compressed_size - 1)?
        };

        let out = match e.method {
            0 => data,
            8 => {
                let mut out = Vec::with_capacity(e.uncompressed_size as usize);
                flate2::read::DeflateDecoder::new(&data[..]).read_to_end(&mut out)?;
                out
            }
            m => bail!("unsupported zip compression method {m}"),
        };
        if out.len() as u64 != e.uncompressed_size {
            bail!("member inflated to {} bytes, expected {}", out.len(), e.uncompressed_size);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live-network test: pull the five members out of Qualcomm's zip over
    /// Range and check them against the pinned digests. `#[ignore]` so ordinary
    /// `cargo test` and CI stay offline — this is the thing to run when bumping
    /// `QAIRT_VERSION`, because it verifies the member paths still exist, the
    /// CDN still honours Range, and the bytes are what we pinned:
    ///
    ///   cargo test -p virtues-installer qairt -- --ignored --nocapture
    #[test]
    #[ignore = "hits Qualcomm's CDN; run when bumping QAIRT_VERSION"]
    fn range_extract_matches_pinned_digests() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let host = tmp.path().join("host");
        let dsp = tmp.path().join("dsp");

        extract_blocking(&host, &dsp).expect("extract from the live SDK");

        assert!(verify_dir(&host, HOST_LIBS), "host libs must match pinned digests");
        assert!(verify_dir(&dsp, DSP_LIBS), "skel must match pinned digest");

        let total: u64 = HOST_LIBS
            .iter()
            .map(|(n, _)| host.join(n))
            .chain(DSP_LIBS.iter().map(|(n, _)| dsp.join(n)))
            .map(|p| fs::metadata(&p).expect("extracted file").len())
            .sum();
        println!("extracted {:.2} MB", total as f64 / 1e6);
        // Guards against a future SDK layout where a member resolves to
        // something small and wrong (a symlink stub, say) yet still hashes.
        assert!(total > 14_000_000, "expected ~15.4 MB of libs, got {total}");
    }
}
