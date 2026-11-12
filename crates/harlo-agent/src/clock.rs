//! The date and time, local (the system's zone, or `TZ`) and UTC. The
//! instructions carry only the date the session started (so they stay the
//! same all day, and the cache serves them); `current_time` is the clock.

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

#[allow(deprecated)] // (libc's time_t on musl: 64-bit there too, as it says it will be.)
fn at(unix: i64, utc: bool) -> Local {
    let t: libc::time_t = unix as libc::time_t;
    // SAFETY: tm is plain data, filled by localtime_r/gmtime_r from t.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        if utc {
            libc::gmtime_r(&t, &mut tm);
        } else {
            libc::localtime_r(&t, &mut tm);
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
        month: (tm.tm_mon + 1) as u32,
        day: tm.tm_mday as u32,
        hour: tm.tm_hour as u32,
        minute: tm.tm_min as u32,
        second: tm.tm_sec as u32,
        weekday: DAYS[(tm.tm_wday.clamp(0, 6)) as usize],
        offset: tm.tm_gmtoff as i64,
        zone,
        unix,
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

pub fn now() -> Local {
    at(unix_now(), false)
}

/// `unix` on the local clock.
pub fn local(unix: i64) -> Local {
    at(unix, false)
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
