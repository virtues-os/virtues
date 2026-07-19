//! Normalizing the identifiers a person answers to.
//!
//! This is the join that never happened. `wiki_people` holds 525 contacts, and
//! `data_communication_message` holds thousands of messages, and NONE of them are
//! connected — because the two sides spell the same person differently:
//!
//! ```text
//! iOS Contacts →  "(512) 555-0142"     (however you typed it)
//! chat.db      →  "+15125550142"       (E.164)
//! ```
//!
//! As strings those never match, so every message says `+15125550100` and none say
//! "Nick". Contacts were stored RAW, and the only matcher was a `LIKE '%digits%'`
//! substring scan over JSONB — which is both unindexed and wrong: a 7-digit number
//! matches inside a different country's 11-digit number.
//!
//! So: one normal form, computed once, on both sides. Phones become E.164; emails
//! become lowercase. Everything else (a short code like `22395`, a service handle)
//! passes through as-is — those aren't people, and pretending otherwise would
//! invent relationships.

/// The normal form of a phone number or email, or `None` if it isn't one we can
/// meaningfully key on.
pub fn normalize_handle(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('@') {
        return Some(trimmed.to_ascii_lowercase());
    }
    normalize_phone(trimmed)
}

/// A phone number in E.164 (`+15125550142`), or `None` if it isn't one.
///
/// The country-code inference is deliberately narrow: a bare 10-digit number is
/// assumed North American, because that is what an iPhone in the US actually hands
/// you, and guessing wider would silently merge strangers. Anything shorter than 7
/// digits is a short code (`22395`, `692639`) — a bank, a 2FA robot, not a person —
/// and gets no handle at all, so it can never resolve to a human.
pub fn normalize_phone(raw: &str) -> Option<String> {
    let had_plus = raw.trim_start().starts_with('+');
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();

    if digits.len() < 7 {
        return None; // short code / junk — not a person
    }

    Some(match digits.len() {
        // Already international, or explicitly written as such.
        _ if had_plus => format!("+{digits}"),
        // 15125550142 → +15125550142
        11 if digits.starts_with('1') => format!("+{digits}"),
        // 5125550142 → +15125550142  (NANP, the common iPhone case)
        10 => format!("+1{digits}"),
        // Anything else: keep the digits, flagged international, rather than
        // guessing a country and inventing a match.
        _ => format!("+{digits}"),
    })
}

/// Every normalized handle for a contact — its emails and phones, in one set to be
/// stored on the person and indexed. Resolution is then a single containment check.
pub fn normalized_handles<'a>(
    emails: impl IntoIterator<Item = &'a str>,
    phones: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut out: Vec<String> = emails
        .into_iter()
        .chain(phones)
        .filter_map(normalize_handle)
        .collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_join_that_never_happened() {
        // The exact pair that left 525 contacts and 0 resolved messages.
        assert_eq!(
            normalize_handle("(512) 555-0142"),
            normalize_handle("+15125550142")
        );
        assert_eq!(normalize_handle("512-555-0142").unwrap(), "+15125550142");
        assert_eq!(normalize_handle("+1 (512) 555-0142").unwrap(), "+15125550142");
        assert_eq!(normalize_handle("15125550142").unwrap(), "+15125550142");
    }

    #[test]
    fn emails_are_lowercased() {
        assert_eq!(normalize_handle("Nick@Gmail.com").unwrap(), "nick@gmail.com");
    }

    #[test]
    fn short_codes_are_not_people() {
        // A bank's 2FA sender must never resolve to a human.
        assert_eq!(normalize_phone("22395"), None);
        assert_eq!(normalize_phone("692639"), None);
    }

    #[test]
    fn distinct_numbers_stay_distinct() {
        // The old matcher was a substring scan: a 7-digit number matched INSIDE an
        // 11-digit one, silently attributing messages to the wrong person.
        let a = normalize_phone("+15125550142").unwrap();
        let b = normalize_phone("+445125550142").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn international_is_preserved() {
        assert_eq!(normalize_phone("+44 20 7946 0958").unwrap(), "+442079460958");
    }
}
