//! The date and time, local (the system's zone, or `TZ`) and UTC. The
//! instructions carry only the date the session started (so they stay the
//! same all day, and the cache serves them); `current_time` is the clock.

// Rust guideline compliant 2026-10-09

use std::ffi::CStr;

/// A moment, as the local clock shows it.
pub struct Local {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub weekday: &'static str,
    /// Seconds east of UTC.
    pub offset: i64,
    pub zone: String,
    pub unix: i64,
}

const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

#[expect(trivial_numeric_casts, reason = "time_t and c_long are i64 on the 64-bit targets harlo is built for, but not on every target")]
#[cfg_attr(target_env = "musl", expect(deprecated, reason = "libc deprecates time_t on musl ahead of its move to 64 bits; it is 64-bit there already"))]
fn at(unix: i64, utc: bool) -> Local {
    let t: libc::time_t = unix as libc::time_t;
    // SAFETY: libc::tm is integers and one pointer, for which all zeros
    // (tm_zone null) is a valid value.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are to locals alive for the whole call; the _r
    // forms are thread-safe and write their result only into `tm`. A time
    // they can't convert leaves `tm` zeroed, which is still valid.
    unsafe {
        if utc {
            libc::gmtime_r(&raw const t, &raw mut tm);
        } else {
            libc::localtime_r(&raw const t, &raw mut tm);
        }
    }
    let zone = if tm.tm_zone.is_null() {
        String::new()
    } else {
        // SAFETY: tm_zone points at a static zone name.
        unsafe { CStr::from_ptr(tm.tm_zone) }.to_string_lossy().into_owned()
    };
    Local {
        year: tm.tm_year + 1900,
        month: u32::try_from(tm.tm_mon + 1).unwrap_or(0),
        day: u32::try_from(tm.tm_mday).unwrap_or(0),
        hour: u32::try_from(tm.tm_hour).unwrap_or(0),
        minute: u32::try_from(tm.tm_min).unwrap_or(0),
        second: u32::try_from(tm.tm_sec).unwrap_or(0),
        weekday: DAYS[usize::try_from(tm.tm_wday.clamp(0, 6)).unwrap_or(0)],
        offset: tm.tm_gmtoff as i64,
        zone,
        unix,
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

pub fn now() -> Local {
    at(unix_now(), false)
}

/// `unix` on the local clock.
pub fn local(unix: i64) -> Local {
    at(unix, false)
}

/// Now in UTC, as ISO 8601 (`2026-11-18T04:05:06Z`).
pub fn utc_now() -> String {
    let t = at(unix_now(), true);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", t.year, t.month, t.day, t.hour, t.minute, t.second)
}

impl Local {
    /// `2026-11-07 (Saturday)`.
    pub fn date(&self) -> String {
        format!("{:04}-{:02}-{:02} ({})", self.year, self.month, self.day, self.weekday)
    }

    /// `+03:00`.
    fn offset(&self) -> String {
        let (sign, o) = if self.offset < 0 { ('-', -self.offset) } else { ('+', self.offset) };
        format!("{sign}{:02}:{:02}", o / 3600, (o % 3600) / 60)
    }
}

/// What `current_time` answers.
pub fn describe(l: &Local) -> String {
    let u = at(l.unix, true);
    let zone = if l.zone.is_empty() { String::new() } else { format!(" {}", l.zone) };
    format!(
        "Local: {:04}-{:02}-{:02}T{:02}:{:02}:{:02}{} ({}, UTC{}{zone})\nUTC: {:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z\nUnix: {}",
        l.year, l.month, l.day, l.hour, l.minute, l.second, l.offset(), l.weekday, l.offset(),
        u.year, u.month, u.day, u.hour, u.minute, u.second, l.unix
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_reads_right() {
        // 2026-11-07 15:04:05 UTC, a Saturday.
        let u = at(1_794_063_845, true);
        assert_eq!((u.year, u.month, u.day, u.hour, u.minute, u.second, u.weekday, u.offset), (2026, 11, 7, 15, 4, 5, "Saturday", 0));
        assert_eq!(u.date(), "2026-11-07 (Saturday)");
        let text = describe(&u);
        assert!(text.contains("UTC: 2026-11-07T15:04:05Z") && text.contains("Unix: 1794063845") && text.contains("UTC+00:00"), "{text}");
    }
}
