//! The setup phrase — the box's one secret that proves ownership.
//!
//! Four words. It is the Bluetooth setup key **and** the recovery key, because
//! those were never two things. See `docs/onboarding-paradigm.md` §1.
//!
//! The whole security argument is *where it is readable and when*:
//!
//! - **Unclaimed** → the phrase is on the panel, and it **rotates** there (15
//!   min + 5 min grace, mirroring the standing pair code). Displaying it costs
//!   nothing while the box is empty, and reading it requires *seeing* the box —
//!   radio range passes through walls, line of sight does not. Rotation is what
//!   stops a box left unclaimed for a week from being a permanent key on
//!   display for every houseguest with a camera.
//! - **Claimed** → the phrase **freezes** and leaves the screen forever. The box
//!   now holds a life, so the phrase exists only where the owner saved it. It
//!   freezes rather than being replaced, so what they saved is exactly what they
//!   typed; there is no second secret to explain.
//!
//! That asymmetry is what makes the reset button safe. Anyone who can open the
//! case can reset a box — a nuisance, since the data survives — but only someone
//! with the phrase can *claim* it, and a screwdriver does not provide one.
//!
//! Verification is rate-limited **globally on the box**, not per caller: a BLE
//! central can change its address between attempts, so per-device throttling is
//! theatre. Only one legitimate setup ever happens at a time, so a global budget
//! costs a real owner nothing.

use chrono::{Duration, Utc};
use sqlx::PgPool;

use crate::codename::{ADJECTIVES, ANIMALS};

/// How often a displayed phrase rotates.
pub const ROTATE_INTERVAL_MIN: i64 = 15;
/// A rotated-out phrase stays valid this long after a newer one appears, so a
/// phrase read mid-rotation never dies under the person typing it.
pub const GRACE_MIN: i64 = 5;
const TTL_MIN: i64 = ROTATE_INTERVAL_MIN + GRACE_MIN;

/// Words per phrase. Four from a ~400-word list is ~2^34.6 — trivially enough
/// against a *throttled* online guess (the only kind possible: the phrase is
/// never transmitted and never stored in the clear once frozen), and short
/// enough to read off a screen and type on a phone.
const WORDS: usize = 4;

/// Attempts allowed inside [`ATTEMPT_WINDOW_MIN`], box-wide.
const MAX_ATTEMPTS: usize = 10;
const ATTEMPT_WINDOW_MIN: i64 = 15;

// ─── phrase generation + normalization ──────────────────────────────────────

/// The wordlist: the codename adjectives and animals, reused deliberately.
/// They are already chosen to be short, common, and unambiguous when read aloud
/// off a screen — which is exactly this job.
fn wordlist() -> impl Iterator<Item = &'static str> {
    ADJECTIVES.iter().copied().chain(ANIMALS.iter().copied())
}

fn word_count() -> usize {
    ADJECTIVES.len() + ANIMALS.len()
}

/// Generate a fresh phrase, e.g. `mango-burly-skull-dough`.
pub fn generate() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let n = word_count();
    (0..WORDS)
        .map(|_| wordlist().nth(rng.random_range(0..n)).unwrap_or("virtues"))
        .collect::<Vec<_>>()
        .join("-")
}

/// Normalize however a human typed it. The words are the secret; the
/// punctuation, spacing and case are not — someone reading four words off a
/// panel will type spaces as often as hyphens, and a phone will capitalize the
/// first one whatever the field says.
pub fn normalize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_sep = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('-');
            }
            pending_sep = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_sep = true;
        }
    }
    out
}

fn hash(phrase: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(normalize(phrase).as_bytes());
    format!("{:x}", h.finalize())
}

// ─── rate limiting ──────────────────────────────────────────────────────────

/// Box-wide attempt log. In memory on purpose: a restart clears it, and forcing
/// a restart already requires the physical access this is not trying to defend
/// against (see the paradigm's note on the disk).
static ATTEMPTS: std::sync::Mutex<Vec<std::time::Instant>> = std::sync::Mutex::new(Vec::new());

/// Record an attempt; `false` means the budget is spent and the caller must
/// refuse without even looking at the phrase.
fn spend_attempt() -> bool {
    let window = std::time::Duration::from_secs((ATTEMPT_WINDOW_MIN * 60) as u64);
    let Ok(mut a) = ATTEMPTS.lock() else {
        return true; // a poisoned lock must not lock the owner out
    };
    a.retain(|t| t.elapsed() < window);
    if a.len() >= MAX_ATTEMPTS {
        return false;
    }
    a.push(std::time::Instant::now());
    true
}

/// Clear the budget after a success, so a fumbled entry never eats into the
/// next legitimate setup.
fn clear_attempts() {
    if let Ok(mut a) = ATTEMPTS.lock() {
        a.clear();
    }
}

// ─── storage ────────────────────────────────────────────────────────────────

/// The frozen phrase's hash, if this box has been claimed.
async fn frozen_hash(pool: &PgPool) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT phrase_hash FROM app_setup_phrase WHERE frozen_at IS NOT NULL LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// Whether this box's phrase is frozen — i.e. it has been claimed at least once
/// and its phrase must never appear on the panel again, including after a reset.
pub async fn is_frozen(pool: &PgPool) -> bool {
    frozen_hash(pool).await.is_some()
}

/// The phrase to DISPLAY, minting or rotating as needed.
///
/// `None` once frozen — that is the point of the whole design, not an error.
/// **Box-local only**: this returns plaintext and must never cross the LAN.
pub async fn display_phrase(pool: &PgPool) -> crate::Result<Option<String>> {
    if is_frozen(pool).await {
        return Ok(None);
    }
    // A live, unfrozen row?
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT display_secret FROM app_setup_phrase \
         WHERE frozen_at IS NULL AND display_secret IS NOT NULL AND expires_at > now() \
         ORDER BY expires_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("read setup phrase: {e}")))?;
    if let Some((ciphertext,)) = row {
        let enc = crate::crypto::TokenEncryptor::from_env()
            .map_err(|e| crate::Error::Other(format!("encryptor: {e}")))?;
        let phrase = enc
            .decrypt(&ciphertext)
            .map_err(|e| crate::Error::Other(format!("decrypt setup phrase: {e}")))?;
        return Ok(Some(phrase));
    }
    Ok(Some(mint(pool).await?))
}

/// Mint a fresh rotating phrase and return its plaintext.
async fn mint(pool: &PgPool) -> crate::Result<String> {
    let phrase = generate();
    let enc = crate::crypto::TokenEncryptor::from_env()
        .map_err(|e| crate::Error::Other(format!("encryptor: {e}")))?;
    let display_secret = enc
        .encrypt(&phrase)
        .map_err(|e| crate::Error::Other(format!("encrypt setup phrase: {e}")))?;
    let id = crate::ids::generate_id("sph", &[&hash(&phrase)[..16]]);
    sqlx::query(
        "INSERT INTO app_setup_phrase (id, phrase_hash, display_secret, expires_at) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(&id)
    .bind(hash(&phrase))
    .bind(&display_secret)
    .bind(Utc::now() + Duration::minutes(TTL_MIN))
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("insert setup phrase: {e}")))?;
    Ok(phrase)
}

/// Does `input` match this box's phrase?
///
/// Frozen box → only the frozen phrase counts. Unclaimed box → any *live*
/// rotating phrase counts, which is what the grace window is for: a phrase read
/// just before a rotation must still work.
///
/// Rate-limited box-wide; a spent budget refuses without comparing anything.
pub async fn verify(pool: &PgPool, input: &str) -> bool {
    if normalize(input).is_empty() || !spend_attempt() {
        return false;
    }
    let want = hash(input);
    let ok = if let Some(frozen) = frozen_hash(pool).await {
        // Constant-time compare: both sides are hex of a SHA-256, so length is
        // fixed and only the content is secret.
        virtues_helpers::crypto::constant_time_eq(frozen.as_bytes(), want.as_bytes())
    } else {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM app_setup_phrase \
             WHERE frozen_at IS NULL AND expires_at > now() AND phrase_hash = $1)",
        )
        .bind(&want)
        .fetch_one(pool)
        .await
        .unwrap_or(false)
    };
    if ok {
        clear_attempts();
    }
    ok
}

/// Freeze the phrase at first claim: this row becomes the box's permanent
/// credential, and its plaintext is destroyed so the box can verify the phrase
/// but never reveal it again.
///
/// Idempotent, and deliberately a no-op once something is frozen — a second
/// freeze would mean a second credential, which the partial unique index in the
/// migration also refuses.
pub async fn freeze(pool: &PgPool, phrase: &str) -> crate::Result<()> {
    if is_frozen(pool).await {
        return Ok(());
    }
    sqlx::query(
        "UPDATE app_setup_phrase SET frozen_at = now(), display_secret = NULL \
         WHERE phrase_hash = $1 AND frozen_at IS NULL",
    )
    .bind(hash(phrase))
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("freeze setup phrase: {e}")))?;
    // Everything that never froze is now noise, and it is plaintext-bearing
    // noise: drop it rather than leave rotated-out phrases decryptable on disk.
    let _ = sqlx::query("DELETE FROM app_setup_phrase WHERE frozen_at IS NULL")
        .execute(pool)
        .await;
    Ok(())
}

/// Freeze whatever phrase is currently displayed — the form the claim path
/// wants, since it knows a claim happened but not which words were on the
/// panel. No-op on a box that is already frozen, or one that somehow never
/// minted a phrase.
pub async fn freeze_current(pool: &PgPool) -> crate::Result<()> {
    if is_frozen(pool).await {
        return Ok(());
    }
    // Freeze the newest live row: that is the one the panel is showing and the
    // one the owner just typed. Older rows are inside their grace window and
    // are deleted with the rest.
    let frozen = sqlx::query(
        "UPDATE app_setup_phrase SET frozen_at = now(), display_secret = NULL \
         WHERE id = ( \
             SELECT id FROM app_setup_phrase \
             WHERE frozen_at IS NULL AND expires_at > now() \
             ORDER BY expires_at DESC LIMIT 1 \
         )",
    )
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("freeze setup phrase: {e}")))?;
    if frozen.rows_affected() == 0 {
        return Err(crate::Error::Other(
            "no live setup phrase to freeze — the panel may still be showing one".into(),
        ));
    }
    let _ = sqlx::query("DELETE FROM app_setup_phrase WHERE frozen_at IS NULL")
        .execute(pool)
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_phrases_have_four_words() {
        for _ in 0..200 {
            let p = generate();
            assert_eq!(p.split('-').count(), WORDS, "not four words: {p}");
            assert!(p.chars().all(|c| c.is_ascii_lowercase() || c == '-'), "odd chars: {p}");
        }
    }

    #[test]
    fn normalize_accepts_however_a_human_types_it() {
        // Someone reading four words off a panel types spaces as often as
        // hyphens, and a phone capitalizes the first word whatever we ask.
        let want = "mango-burly-skull-dough";
        for typed in [
            "mango-burly-skull-dough",
            "Mango Burly Skull Dough",
            "  mango   burly--skull_dough  ",
            "MANGO-BURLY-SKULL-DOUGH",
            "mango, burly, skull, dough",
        ] {
            assert_eq!(normalize(typed), want, "failed on {typed:?}");
        }
    }

    #[test]
    fn normalize_does_not_invent_separators() {
        assert_eq!(normalize("---"), "");
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("one"), "one");
    }

    #[test]
    fn hash_is_stable_across_typing_styles() {
        assert_eq!(hash("Mango Burly Skull Dough"), hash("mango-burly-skull-dough"));
        assert_ne!(hash("mango-burly-skull-dough"), hash("mango-burly-skull-dought"));
    }

    #[test]
    fn the_wordlist_is_large_enough_to_matter() {
        // 4 words from this list, against a throttled online guess. If someone
        // shrinks the list, this is the test that should stop them.
        let n = word_count() as f64;
        let bits = (n.powi(WORDS as i32)).log2();
        assert!(bits > 30.0, "phrase entropy too low: {bits:.1} bits from {n} words");
    }

    #[test]
    fn the_attempt_budget_closes_and_reopens_on_success() {
        clear_attempts();
        for i in 0..MAX_ATTEMPTS {
            assert!(spend_attempt(), "budget closed early at {i}");
        }
        assert!(!spend_attempt(), "budget should be spent");
        clear_attempts();
        assert!(spend_attempt(), "a success must restore the budget");
        clear_attempts();
    }
}
