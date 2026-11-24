//! Client-made identifiers in the server's format.

// Rust guideline compliant 2026-10-09

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Characters of the random suffix.
const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Length of the part after the prefix: 12 hex digits of time, then random characters.
const BODY_LEN: usize = 26;

/// Hex digits taken from the time and counter.
const TIME_DIGITS: usize = 12;

/// The last timestamp used and how many IDs were made in that millisecond.
static LAST: Mutex<(u64, u64)> = Mutex::new((0, 0));

/// Creates a new message ID (`msg_…`) that sorts after earlier ones.
///
/// IDs are `<prefix>_`, then 12 hex digits of `millis * 0x1000 + counter`,
/// then 14 random characters, so IDs made later sort later. Sending a prompt
/// with an ID made here lets a retry reuse it: the server stores the message once.
#[must_use]
pub fn message_id() -> String {
    ascending("msg")
}

fn ascending(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    let counter = {
        let mut last = LAST
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if last.0 == millis {
            last.1 += 1;
        } else {
            *last = (millis, 1);
        }
        last.1
    };
    let value = millis.wrapping_mul(0x1000).wrapping_add(counter) & 0xffff_ffff_ffff;
    let mut out = format!("{prefix}_{value:0TIME_DIGITS$x}");
    let mut random = [0_u8; BODY_LEN - TIME_DIGITS];
    // An unavailable entropy source leaves zeros; the time part still keeps IDs apart.
    if getrandom::fill(&mut random).is_err() {
        tracing::warn!(name: "id.random.unavailable", "random source unavailable for message IDs");
    }
    out.extend(
        random
            .iter()
            .map(|b| char::from(ALPHABET[usize::from(*b) % ALPHABET.len()])),
    );
    out
}
