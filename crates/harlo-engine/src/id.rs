//! IDs: a prefix, twelve hex digits of time and a
//! counter, then random letters.
//!
//! Ascending IDs (messages, parts, permissions) sort oldest first;
//! descending ones (sessions) sort newest first. Clients compare them as
//! strings, so the order is all that matters.

// Rust guideline compliant 2026-10-09

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// The letters the random part is drawn from.
const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
/// Characters after the prefix and underscore.
const BODY_LEN: usize = 26;
/// Hex digits of the time-and-counter part.
const TIME_DIGITS: usize = 12;
/// The last millisecond an ID was made in, and how many were made in it.
static LAST: Mutex<(u64, u64)> = Mutex::new((0, 0));

/// A new ID that sorts after every earlier one, such as `msg_20c7…`.
#[must_use]
pub fn ascending_id(prefix: &str) -> String {
    make(prefix, false)
}

/// A new ID that sorts before every earlier one, such as `ses_df38…`.
#[must_use]
pub fn descending_id(prefix: &str) -> String {
    make(prefix, true)
}

/// A new ascending ID that sorts just before `next`, with the same prefix;
/// none when `next` isn't an ascending ID or nothing sorts before it.
///
/// IDs made in the same millisecond by different processes don't sort by
/// when they were made; this puts one the server made where it belongs.
#[must_use]
pub fn just_before(next: &str) -> Option<String> {
    let (prefix, body) = next.split_once('_')?;
    let time = u64::from_str_radix(body.get(..TIME_DIGITS)?, 16).ok()?;
    let value = time.checked_sub(1)?;
    Some(with_value(prefix, value))
}

/// A mark that sorts before every ascending ID with `prefix` made from
/// this millisecond on, by this process or another.
///
/// An ID of [`ascending_id`] can't be such a mark: one that another
/// process makes in the same millisecond may sort before it.
#[must_use]
pub fn from_now(prefix: &str) -> String {
    // (The counter of an ID starts at one, so none of this millisecond
    // has a time-and-counter part this small.)
    let value = now_millis().wrapping_mul(0x1000) & 0xffff_ffff_ffff;
    format!("{prefix}_{value:0TIME_DIGITS$x}")
}

/// The time, in milliseconds since 1970.
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn make(prefix: &str, descending: bool) -> String {
    let millis = now_millis();
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
    let mut value = millis.wrapping_mul(0x1000).wrapping_add(counter) & 0xffff_ffff_ffff;
    if descending {
        value = !value & 0xffff_ffff_ffff;
    }
    with_value(prefix, value)
}

/// An ID with time-and-counter `value`, then random letters.
fn with_value(prefix: &str, value: u64) -> String {
    let mut out = format!("{prefix}_{value:0TIME_DIGITS$x}");
    let mut random = [0_u8; BODY_LEN - TIME_DIGITS];
    if getrandom::fill(&mut random).is_err() {
        tracing::warn!(name: "id.random.unavailable", "random source unavailable for IDs");
    }
    out.extend(
        random
            .iter()
            .map(|b| char::from(ALPHABET[usize::from(*b) % ALPHABET.len()])),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascending_ids_sort_in_order_and_descending_ones_the_other_way() {
        let (a, b) = (ascending_id("msg"), ascending_id("msg"));
        assert!(a < b, "{a} then {b}");
        let (c, d) = (descending_id("ses"), descending_id("ses"));
        assert!(c > d, "{c} then {d}");
        assert_eq!(a.len(), "msg_".len() + BODY_LEN);
        let before = just_before(&b).expect("before");
        assert!(before < b && before.len() == b.len(), "{before} before {b}");
        assert_eq!(just_before("msg_000000000000abc"), None);
    }

    #[test]
    fn a_mark_sorts_before_what_is_made_from_then_on_and_after_what_was() {
        let earlier = ascending_id("msg");
        std::thread::sleep(std::time::Duration::from_millis(2));
        let mark = from_now("msg");
        let later = ascending_id("msg");
        assert!(earlier < mark && mark < later, "{earlier} {mark} {later}");
        // Whatever another process draws in the same millisecond, with
        // its own counter at one: the mark is its ID's beginning.
        let same_time = format!("{}1{}", &mark[..mark.len() - 1], "0".repeat(14));
        assert!(mark < same_time, "{mark} {same_time}");
    }
}
