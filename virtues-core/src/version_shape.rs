// The shape of a baked version string.
//
// INCLUDED BY `build.rs` (`include!`) and declared as a module by `lib.rs`, so
// one copy of the logic is both used at build time and tested at test time. A
// build script cannot import from the crate it builds, and duplicating it would
// let the tested copy and the used copy drift — which is the failure this file
// exists to avoid, not a style preference.
//
// Plain `//` comments and OUTER attributes only: `include!` splices this into
// the middle of build.rs, where `//!` and `#![...]` are errors.

/// Reshape `git describe`'s suffix into semver BUILD METADATA.
///
/// `git describe` joins its commit offset and sha to the tag with HYPHENS:
///
/// ```text
/// v0.1.7-staging.78-13-ga2ee5292-dirty
/// ```
///
/// Semver splits a prerelease on DOTS, so that trailing run is not a separate
/// field — it is swallowed into the second prerelease identifier, making it
/// the string `78-13-ga2ee5292-dirty`. And semver's rule for comparing two
/// identifiers checks their TYPE before their value:
///
/// ```text
/// "Numeric identifiers always have lower precedence than non-numeric
///  identifiers." — semver.org §11.4.3
/// ```
///
/// `79` is numeric; `78-13-ga2ee5292-dirty` is alphanumeric. So the local
/// build outranks `staging.79` — and `.80`, and `.150`, because the rule is
/// about type, not magnitude. `cli/upgrade.rs` compares with `semver::Version`
/// and correctly reports the real release as a DOWNGRADE, so a box that was
/// once hot-patched with a local binary can never take another prerelease
/// without `--force`. That is what happened to the dev board on 2026-09-16.
///
/// Semver has a field for exactly this data, and is explicit that it does not
/// count:
///
/// ```text
/// "Build metadata MUST be ignored when determining version precedence."
/// ```
///
/// So `+` instead of `-`:
///
/// ```text
/// v0.1.7-staging.78+13.ga2ee5292.dirty
/// ```
///
/// which compares EQUAL to `v0.1.7-staging.78` — the truth about what the
/// binary was built from — and upgrades to `.79` with nothing overridden.
///
/// Only CI's own builds avoid all of this, because CI sets
/// `VIRTUES_BUILD_VERSION` to the exact tag and never reaches `git describe`.
/// A released box has never been affected; only a hand-built one.
#[allow(dead_code)] // Used by build.rs; the library carries it to test it.
fn semver_safe(describe: &str) -> String {
    let (head, dirty) = match describe.strip_suffix("-dirty") {
        Some(h) => (h, true),
        None => (describe, false),
    };

    let mut meta: Vec<&str> = Vec::new();
    let base = match split_offset_sha(head) {
        Some((base, offset, sha)) => {
            meta.push(offset);
            meta.push(sha);
            base
        }
        None => head,
    };
    if dirty {
        meta.push("dirty");
    }

    if meta.is_empty() {
        base.to_string()
    } else {
        format!("{base}+{}", meta.join("."))
    }
}

/// `v1.2.3-rc.1-13-gabc1234` -> `("v1.2.3-rc.1", "13", "gabc1234")`.
///
/// Matched from the RIGHT and checked for shape, so a tag that merely contains
/// a hyphen — `v1.0.0-golden`, `v0.1.7-staging.78` — is left alone: `golden`
/// is not `g` + hex, and there is no numeric offset before it.
#[allow(dead_code)]
fn split_offset_sha(s: &str) -> Option<(&str, &str, &str)> {
    let g = s.rfind("-g")?;
    let sha = &s[g + 1..];
    if sha.len() < 2 || !sha[1..].chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let left = &s[..g];
    let d = left.rfind('-')?;
    let offset = &left[d + 1..];
    if offset.is_empty() || !offset.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((&left[..d], offset, sha))
}

#[cfg(test)]
mod tests {
    use super::semver_safe;

    #[test]
    fn describe_suffixes_become_build_metadata() {
        assert_eq!(
            semver_safe("v0.1.7-staging.78-13-ga2ee5292-dirty"),
            "v0.1.7-staging.78+13.ga2ee5292.dirty"
        );
        assert_eq!(
            semver_safe("v0.1.7-staging.78-13-ga2ee5292"),
            "v0.1.7-staging.78+13.ga2ee5292"
        );
        assert_eq!(semver_safe("v0.1.7-staging.78-dirty"), "v0.1.7-staging.78+dirty");
    }

    #[test]
    fn a_clean_tag_is_untouched() {
        assert_eq!(semver_safe("v0.1.7-staging.78"), "v0.1.7-staging.78");
        assert_eq!(semver_safe("v0.1.7"), "v0.1.7");
        // `--always` with no tag in reach: a bare sha, not semver at all.
        assert_eq!(semver_safe("a2ee5292"), "a2ee5292");
    }

    #[test]
    fn a_hyphen_in_the_tag_is_not_an_offset() {
        // `golden` is not `g` + hex, and nothing numeric precedes it.
        assert_eq!(semver_safe("v1.0.0-golden"), "v1.0.0-golden");
        assert_eq!(semver_safe("v1.0.0-rc.1"), "v1.0.0-rc.1");
    }
}
