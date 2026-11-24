//! Which models the model menus offer.
//!
//! Each model can be shown or hidden in Settings → Models. One nobody chose
//! for shows when it's the newest of its family from the last six months,
//! one of OpenCode's free models, or undated; older and superseded models
//! wait in Settings until they're wanted.

// Rust guideline compliant 2026-10-09

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::server::Model;

/// How recent a model must be to show without being chosen.
pub const RECENT: Duration = Duration::from_hours(24 * 183);

/// The provider whose free models always show.
const OPENCODE: &str = "opencode";

/// A model's key in [`crate::layout::Preferences::models`]:
/// `provider/model`.
#[must_use]
pub fn key(provider_id: &str, id: &str) -> String {
    format!("{provider_id}/{id}")
}

/// Returns the keys of the models that show until someone chooses.
///
/// Out of `models`, at `now`: per provider, the newest of each family
/// released within [`RECENT`] (free and paid ones apart), OpenCode's free
/// models, and those without a release date.
#[must_use]
pub fn shown_by_default(models: &[Model], now: SystemTime) -> HashSet<String> {
    let now_ms = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    let recent_ms = i64::try_from(RECENT.as_millis()).unwrap_or(i64::MAX);
    let mut newest: HashMap<(&str, Option<&str>, bool), &Model> = HashMap::new();
    let mut shown = HashSet::new();
    for model in models {
        let opencode_free = model.provider_id == OPENCODE && model.free;
        match model.released {
            None => {
                shown.insert(key(&model.provider_id, &model.id));
            }
            Some(_) if opencode_free => {
                shown.insert(key(&model.provider_id, &model.id));
            }
            Some(released) if now_ms.abs_diff(released) < recent_ms.unsigned_abs() => {
                let group = (
                    model.provider_id.as_str(),
                    model.family.as_deref(),
                    model.free,
                );
                // A tie keeps the first listed.
                let entry = newest.entry(group).or_insert(model);
                if released > entry.released.unwrap_or_default() {
                    *entry = model;
                }
            }
            Some(_) => {}
        }
    }
    shown.extend(newest.values().map(|m| key(&m.provider_id, &m.id)));
    shown
}

/// Whether the model with key `key` shows: as chosen in `choices`, else as
/// `defaults` (from [`shown_by_default`]) say.
#[must_use]
pub fn shown<S: std::hash::BuildHasher>(
    choices: &BTreeMap<String, bool>,
    defaults: &HashSet<String, S>,
    key: &str,
) -> bool {
    choices
        .get(key)
        .copied()
        .unwrap_or_else(|| defaults.contains(key))
}
