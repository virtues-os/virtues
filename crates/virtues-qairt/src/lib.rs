//! Pinned Qualcomm QAIRT runtime files, fetched from Qualcomm's own SDK zip.
//!
//! Two things on a Dragon load QAIRT at runtime: `virtues-qnnd` (embed and
//! rerank) and the local model (`genie-t2t-run`). Neither can ship the files.
//!
//! ## Why we fetch instead of re-hosting
//!
//! The SDK's licence grants distribution of the Software in object code as
//! incorporated in your own application, then withholds any licence to
//! distribute it **on a standalone basis**, which is precisely what a bare `.so`
//! release asset is. So the box fetches from Qualcomm's own public distribution
//! and we re-host nothing.
//!
//! ## Why a Range read rather than a download
//!
//! The zip is 1.4 to 1.7 GB and we need a few tens of MB of it. Qualcomm's CDN
//! honours Range requests, so we read the central directory out of the tail and
//! then pull only the members we want. (`HEAD` is refused with 403; a one-byte
//! Range GET is how you learn the length.) See `Fetcher` for the measurement
//! that forced hand-rolled zip parsing.
//!
//! ## Why two runtimes
//!
//! `qnnd`'s context binaries are compiled against 2.42 and CI builds it against
//! 2.42 headers. The local model's binaries come from Qualcomm AI Hub, which
//! compiles with 2.45, and a 2.42 runtime cannot load them. Separate processes
//! with separate library paths, so the two never meet. Each set is exactly what
//! its process maps, measured on a Dragon: `libQnnHtpPrepare.so` (the 90 MB
//! offline graph compiler) is absent from both because we only load
//! precompiled context binaries.

use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A pinned set of files from one QAIRT SDK release. (file name, sha256)
pub struct Runtime {
    pub version: &'static str,
    /// `lib/<HOST_TRIPLE>/` → `LD_LIBRARY_PATH`.
    pub host: &'static [(&'static str, &'static str)],
    /// `lib/hexagon-v68/unsigned/` → `ADSP_LIBRARY_PATH`. Loads onto the DSP.
    pub dsp: &'static [(&'static str, &'static str)],
    /// `bin/<HOST_TRIPLE>/` executables.
    pub bin: &'static [(&'static str, &'static str)],
}

/// The SDK subdirectory holding the host-side libs. "oe" reads like Yocto, but
/// this is the variant the lab Dragon runs on Ubuntu 24.04 / glibc 2.39. Don't
/// "correct" it to `aarch64-ubuntu-gcc9.4` without measuring.
pub const HOST_TRIPLE: &str = "aarch64-oe-linux-gcc11.2";

/// `virtues-qnnd`. The version must match `QAIRT_VERSION` in
/// `.github/workflows/release-linux.yml` (the headers qnnd compiles against)
/// and the QAIRT its `.bin` context binaries were compiled with. The digests
/// are the bytes the lab Dragon is validated against (3.8 ms/call embed).
pub const QNND: Runtime = Runtime {
    version: "2.42.0.251225",
    host: &[
        ("libQnnHtp.so", "6a04a4b8e276b863eb1ea8550f6855a3f8345412c5548f5679d26d7c2609b02a"),
        ("libQnnSystem.so", "9c2692c5cbc5d062beede749480cf3448fa9aa0267dfcb94933ad63078cf356d"),
        ("libQnnHtpV68Stub.so", "1436f5337f6d469cb6aca1095fc7426ab1ac61d030c4a39b2cdf013fe17d9ec4"),
        (
            "libQnnHtpV68CalculatorStub.so",
            "3e8fa1c13e51e9e0d5df65ef4a6b3ced646459ebd09dd8a2079b55f07001886b",
        ),
    ],
    dsp: &[("libQnnHtpV68Skel.so", "2cf8b6662cd9c98049c6ac0285d83c4b6966e6a1c9abb00aa843cb8e7e706b3c")],
    bin: &[],
};

/// The local model: Qualcomm's `genie-t2t-run` and what it maps. Without
/// `libQnnHtpNetRunExtensions.so`, Genie logs "Unable to open backend extension
/// library" and fails with device error 14001. The set was confirmed on a
/// Dragon by removing everything else and running a turn.
pub const GENIE: Runtime = Runtime {
    version: "2.45.0.260326",
    host: &[
        ("libGenie.so", "ab963849cad7018b90d3d0a0b1dc414b38297acf900f1f6720c912f9748cab7f"),
        ("libQnnHtp.so", "821f0aee4708cb8012324d5c5cb75d22d7dc73aeb2543d1e1b5a2a7159be5196"),
        ("libQnnSystem.so", "ced8b05daa0dea696789ec6b60a773d534a54d3b2ed5adf1abf5a5cd1bfbdfb4"),
        ("libQnnHtpV68Stub.so", "7afe7b26655868fbda44b7bc4c6bd5123d24fbb603eba11d8a1d69d9446d323d"),
        (
            "libQnnHtpV68CalculatorStub.so",
            "fe06c3d257eea0c7dcecc806afe6a068352d7a9af3c8dd53d6562f9373735389",
        ),
        (
            "libQnnHtpNetRunExtensions.so",
            "d33e327f7c5b488a16e868ee69e75ee0acc613ec3ed83b7f0999b850990cc580",
        ),
    ],
    dsp: &[("libQnnHtpV68Skel.so", "4d929ec3d7311c19b602670a62e54a668937ab68328d61829c40c7fff1645554")],
    bin: &[("genie-t2t-run", "b9f30621c7df47263ad7c07fecb23ed7db5022e809b47c2245c4b617b9cd9ba3")],
};

/// Where one runtime's files live on the box: `host/`, `dsp/` and `bin/` under
/// a root the caller owns.
#[derive(Clone, Debug)]
pub struct Dirs {
    pub host: PathBuf,
    pub dsp: PathBuf,
    pub bin: PathBuf,
}

impl Dirs {
    pub fn under(root: &Path) -> Self {
        Self { host: root.join("host"), dsp: root.join("dsp"), bin: root.join("bin") }
    }
}

impl Runtime {
    fn sdk_url(&self) -> String {
        format!(
            "https://softwarecenter.qualcomm.com/api/download/software/sdks/\
             Qualcomm_AI_Runtime_Community/All/{v}/v{v}.zip",
            v = self.version
        )
    }

    /// Every file present with its pinned digest. Digest-checked rather than
    /// existence-checked: a half-written lib from an interrupted fetch would
    /// otherwise pass as "already there" and fail later as an opaque `dlopen`
    /// error.
    pub fn is_installed(&self, dirs: &Dirs) -> bool {
        verify_dir(&dirs.host, self.host)
            && verify_dir(&dirs.dsp, self.dsp)
            && verify_dir(&dirs.bin, self.bin)
    }

    /// Fetch every file from Qualcomm's zip, check its digest, and write it
    /// into place. Blocking: each seek is an HTTP range request. `on_file` is
    /// called with each file name before it is fetched, for progress.
    pub fn fetch_blocking(&self, dirs: &Dirs, mut on_file: impl FnMut(&str)) -> Result<()> {
        for d in [&dirs.host, &dirs.dsp, &dirs.bin] {
            fs::create_dir_all(d).with_context(|| format!("creating {}", d.display()))?;
        }
        let fetcher = Fetcher::new(self.sdk_url())?;
        let index = fetcher.central_directory()?;
        let base = format!("qairt/{}", self.version);
        let wanted = self
            .host
            .iter()
            .map(|(n, s)| (format!("{base}/lib/{HOST_TRIPLE}/{n}"), dirs.host.join(n), *s, 0o644))
            .chain(self.dsp.iter().map(|(n, s)| {
                (format!("{base}/lib/hexagon-v68/unsigned/{n}"), dirs.dsp.join(n), *s, 0o644)
            }))
            .chain(
                self.bin
                    .iter()
                    .map(|(n, s)| (format!("{base}/bin/{HOST_TRIPLE}/{n}"), dirs.bin.join(n), *s, 0o755)),
            );

        for (member, dest, sha, mode) in wanted {
            on_file(member.rsplit('/').next().unwrap_or(&member));
            let entry = index
                .get(&member)
                .ok_or_else(|| anyhow!("{member} not found in QAIRT {}", self.version))?;
            let bytes = fetcher.member(entry).with_context(|| format!("extracting {member}"))?;
            let got = hex::encode(Sha256::digest(&bytes));
            if got != sha {
                bail!("{member}: sha256 {got}, expected {sha}; did QAIRT {} change?", self.version);
            }
            // Same-dir temp + rename: an interrupted fetch must not leave a
            // truncated file that later passes an existence check. The mode is
            // set explicitly because the processes run as `virtues`, not with
            // whatever umask the fetching process happened to carry.
            let tmp = dest.with_extension("part");
            fs::write(&tmp, &bytes).with_context(|| format!("writing {}", tmp.display()))?;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))
                .with_context(|| format!("chmod {}", tmp.display()))?;
            fs::rename(&tmp, &dest).with_context(|| format!("renaming into {}", dest.display()))?;
        }
        Ok(())
    }
}

/// Give QNN the unversioned `libcdsprpc.so` name it dlopens.
///
/// The Radxa image ships `libcdsprpc1`, which installs ONLY the soname
/// (`libcdsprpc.so.1`); the bare `.so` symlink lives in a `-dev` package
/// nobody has. QNN's HTP stub dlopens the bare name, walks every loader path,
/// gets ENOENT, and reports it as `Transport layer setup failed: 14001`, three
/// layers away from the cause. Linked inside the runtime's own host dir (which
/// is on the process's `LD_LIBRARY_PATH`), so no system directory is touched.
///
/// Where the package puts the soname depends on its version: 1.0.7 uses the
/// multiarch dir, 1.0.4 plain `/usr/lib`. Both are on Dragons in the field.
pub fn link_cdsprpc(host_dir: &Path) -> Result<()> {
    const SONAMES: [&str; 2] = [
        "/usr/lib/aarch64-linux-gnu/libcdsprpc.so.1",
        "/usr/lib/libcdsprpc.so.1",
    ];
    let Some(target) = SONAMES.iter().map(Path::new).find(|p| p.exists()) else {
        bail!(
            "libcdsprpc.so.1 not found. QNN needs the cdsp FastRPC library \
             (Radxa: apt install libcdsprpc1)"
        );
    };
    let link = host_dir.join("libcdsprpc.so");
    let _ = fs::remove_file(&link);
    std::os::unix::fs::symlink(target, &link)
        .with_context(|| format!("linking {}", link.display()))
}

fn verify_dir(dir: &Path, want: &[(&str, &str)]) -> bool {
    want.iter().all(|(name, sha)| match fs::read(dir.join(name)) {
        Ok(bytes) => hex::encode(Sha256::digest(&bytes)) == *sha,
        Err(_) => false,
    })
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

    /// Live-network test: pull each runtime's members out of Qualcomm's zip
    /// over Range and check them against the pinned digests. `#[ignore]` so
    /// ordinary `cargo test` and CI stay offline. Run it when bumping a
    /// version, because it verifies the member paths still exist, the CDN
    /// still honours Range, and the bytes are what we pinned:
    ///
    ///   cargo test -p virtues-qairt -- --ignored --nocapture
    #[test]
    #[ignore = "hits Qualcomm's CDN; run when bumping a QAIRT version"]
    fn range_extract_matches_pinned_digests() {
        for rt in [&QNND, &GENIE] {
            let tmp = tempfile::tempdir().expect("tempdir");
            let dirs = Dirs::under(tmp.path());
            rt.fetch_blocking(&dirs, |_| {}).expect("extract from the live SDK");
            assert!(rt.is_installed(&dirs), "{} must match pinned digests", rt.version);
        }
    }

    #[test]
    fn a_missing_file_is_not_installed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(!GENIE.is_installed(&Dirs::under(tmp.path())));
    }
}
