//! The Settled shelf: harlo puts a worktree's sessions on their project's
//! shelf (in the sidebar, under the project's sessions) once their work
//! is done: the branch's pull request merged or closed after the last
//! activity in them, or nothing happened in them for a week. Newer work in
//! one brings it back; one put back by hand stays until there's newer
//! work and a reason again. Pinned sessions, and those running or waiting
//! on you, stay where they are.

use serde_json::Value;

/// A week of quiet, in seconds.
pub const QUIET: i64 = 7 * 24 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewState {
    Open,
    Merged,
    Closed,
}

/// A branch's pull (merge) request, as far as the shelf cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Review {
    pub state: ReviewState,
    /// When it merged or closed (unix seconds; 0 while open).
    pub changed_at: i64,
}

/// Why a session goes on the shelf now (`modified`: its last activity,
/// unix seconds), if it does: "merged" / "closed" (its pull request, after
/// that activity), "quiet" (a week without any).
pub fn why(modified: i64, now: i64, review: Option<Review>) -> Option<&'static str> {
    match review {
        Some(Review { state: ReviewState::Merged, changed_at }) if changed_at >= modified => Some("merged"),
        Some(Review { state: ReviewState::Closed, changed_at }) if changed_at >= modified => Some("closed"),
        _ if now - modified >= QUIET => Some("quiet"),
        _ => None,
    }
}

/// How the sidebar says why.
pub fn why_text(why: &str) -> &'static str {
    match why {
        "merged" => "Pull request merged",
        "closed" => "Pull request closed",
        "quiet" => "No activity for a week",
        "helper" => "Done as a helper",
        _ => "",
    }
}

fn unix(iso: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(iso).map(|t| t.timestamp()).unwrap_or(0)
}

/// The branch's request from a list of them, newest first or not: an open
/// one if there is one (an older open request outlives newer closed
/// ones), else the one merged or closed last.
fn pick(list: impl Iterator<Item = Review>) -> Option<Review> {
    let all: Vec<Review> = list.collect();
    all.iter().find(|r| r.state == ReviewState::Open).copied().or_else(|| all.into_iter().filter(|r| r.changed_at > 0).max_by_key(|r| r.changed_at))
}

/// From `gh pr list --head <branch> --state all --json state,mergedAt,closedAt`.
pub fn gh_review(json: &str) -> Option<Review> {
    let v: Value = serde_json::from_str(json).ok()?;
    pick(v.as_array()?.iter().filter_map(|p| {
        Some(match p["state"].as_str()? {
            "OPEN" => Review { state: ReviewState::Open, changed_at: 0 },
            "MERGED" => Review { state: ReviewState::Merged, changed_at: unix(p["mergedAt"].as_str()?) },
            "CLOSED" => Review { state: ReviewState::Closed, changed_at: unix(p["closedAt"].as_str()?) },
            _ => return None,
        })
    }))
}

/// From `glab mr list --source-branch <branch> --all -F json`.
pub fn glab_review(json: &str) -> Option<Review> {
    let v: Value = serde_json::from_str(json).ok()?;
    pick(v.as_array()?.iter().filter_map(|m| {
        Some(match m["state"].as_str()? {
            "opened" => Review { state: ReviewState::Open, changed_at: 0 },
            "merged" => Review { state: ReviewState::Merged, changed_at: unix(m["merged_at"].as_str()?) },
            "closed" => Review { state: ReviewState::Closed, changed_at: unix(m["closed_at"].as_str()?) },
            _ => return None,
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 24 * 3600;

    #[test]
    fn settles_done_and_quiet_sessions() {
        let now = 100 * DAY;
        let merged = |at| Some(Review { state: ReviewState::Merged, changed_at: at });
        let closed = |at| Some(Review { state: ReviewState::Closed, changed_at: at });
        let open = Some(Review { state: ReviewState::Open, changed_at: 0 });
        assert_eq!(why(now - DAY, now, merged(now - 3600)), Some("merged"));
        assert_eq!(why(now - DAY, now, closed(now - 3600)), Some("closed"));
        // Work after the merge: not done.
        assert_eq!(why(now - 3600, now, merged(now - DAY)), None);
        // A week quiet, whatever its request.
        assert_eq!(why(now - 8 * DAY, now, open), Some("quiet"));
        assert_eq!(why(now - 8 * DAY, now, merged(now - 9 * DAY)), Some("quiet"));
        assert_eq!(why(now - 8 * DAY, now, None), Some("quiet"));
        assert_eq!(why(now - 6 * DAY, now, open), None);
        assert_eq!(why(now - 6 * DAY, now, None), None);
    }

    #[test]
    fn reads_the_branchs_request() {
        let gh = r#"[{"state":"MERGED","mergedAt":"2026-10-30T12:00:00Z","closedAt":"2026-10-30T12:00:00Z"},
                     {"state":"CLOSED","mergedAt":null,"closedAt":"2026-10-01T08:00:00Z"}]"#;
        assert_eq!(gh_review(gh), Some(Review { state: ReviewState::Merged, changed_at: unix("2026-10-30T12:00:00Z") }));
        // An older open one wins over newer closed ones.
        let gh = r#"[{"state":"CLOSED","mergedAt":null,"closedAt":"2026-10-30T12:00:00Z"},{"state":"OPEN","mergedAt":null,"closedAt":null}]"#;
        assert_eq!(gh_review(gh).unwrap().state, ReviewState::Open);
        assert_eq!(gh_review("[]"), None);
        assert_eq!(gh_review("no pull requests found"), None);

        let glab = r#"[{"state":"closed","merged_at":null,"closed_at":"2026-10-02T08:00:00.000Z"},{"state":"merged","merged_at":"2026-10-03T09:30:00.123+02:00","closed_at":null}]"#;
        assert_eq!(glab_review(glab), Some(Review { state: ReviewState::Merged, changed_at: unix("2026-10-03T07:30:00Z") }));
        assert_eq!(why_text("quiet"), "No activity for a week");
    }
}
