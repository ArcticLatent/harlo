//! Where an agent compacts its context on its own: the context size, in
//! tokens, that sets it off, worked out from the agent's settings the way it
//! reads them, so harlo can say it's coming. `None`: it won't (turned off),
//! or it isn't known. Claude Code says its own (`get_context_usage`).

use std::path::{Path, PathBuf};

use serde_json::Value;
use yaml_rust::{Yaml, YamlLoader};

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn read_json(path: &Path) -> Value {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null)
}

fn read_yaml(path: &Path) -> Yaml {
    std::fs::read_to_string(path).ok().and_then(|t| YamlLoader::load_from_str(&t).ok()).and_then(|d| d.into_iter().next()).unwrap_or(Yaml::Null)
}

/// pi: past the window less `compaction.reserveTokens` (16,384 unless set,
/// or set for the model under `modelOverrides`), from its settings and the
/// project's (`.pi/settings.json`, which wins).
/// `agent_dir`: the session's own `PI_CODING_AGENT_DIR`, if it has one.
pub fn pi(agent_dir: Option<&Path>, cwd: &Path, model: &str, window: u64) -> Option<u64> {
    let dir = agent_dir
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("PI_CODING_AGENT_DIR").map(PathBuf::from))
        .unwrap_or_else(|| home().join(".pi/agent"));
    pi_at(&[read_json(&dir.join("settings.json")), read_json(&cwd.join(".pi/settings.json"))], model, window)
}

/// The same on another machine, from its settings as last read there (pi's
/// own `~/.pi/agent`, and the project's): None until they have been.
pub fn pi_on(host: &str, cwd: &str, model: &str, window: u64) -> Option<u64> {
    let own = crate::remote::home_file_soon(host, ".pi/agent/settings.json", REREAD)?;
    let project = crate::remote::home_file_soon(host, &format!("{}/.pi/settings.json", cwd.trim_end_matches('/')), REREAD).unwrap_or_default();
    let json = |t: &str| serde_json::from_str(t).unwrap_or(Value::Null);
    pi_at(&[json(&own), json(&project)], model, window)
}

/// How often another machine's settings are read again for this.
const REREAD: std::time::Duration = std::time::Duration::from_secs(60);

fn yaml(text: &str) -> Yaml {
    YamlLoader::load_from_str(text).ok().and_then(|d| d.into_iter().next()).unwrap_or(Yaml::Null)
}

fn pi_at(layers: &[Value], model: &str, window: u64) -> Option<u64> {
    let setting = |pick: &dyn Fn(&Value) -> &Value| layers.iter().rev().map(|l| pick(&l["compaction"])).find(|v| !v.is_null()).cloned();
    if window == 0 || setting(&|c| &c["enabled"]).and_then(|v| v.as_bool()) == Some(false) {
        return None;
    }
    let reserve = setting(&|c| &c["modelOverrides"][model]["reserveTokens"])
        .or_else(|| setting(&|c| &c["reserveTokens"]))
        .and_then(|v| v.as_u64())
        .unwrap_or(16_384);
    (window > reserve).then(|| window - reserve)
}

/// omp: at `compaction.thresholdTokens` or `thresholdPercent` of the window
/// when set (or a model's `modelThresholds`), else the window less the larger
/// of 15% of it and `reserveTokens` (16,384), from `~/.omp/agent/config.yml`.
pub fn omp(model: &str, window: u64) -> Option<u64> {
    omp_at(&read_yaml(&home().join(".omp/agent/config.yml")), model, window)
}

/// The same on another machine (its `~/.omp/agent/config.yml` as last read).
pub fn omp_on(host: &str, model: &str, window: u64) -> Option<u64> {
    omp_at(&yaml(&crate::remote::home_file_soon(host, ".omp/agent/config.yml", REREAD)?), model, window)
}

fn omp_at(config: &Yaml, model: &str, window: u64) -> Option<u64> {
    let c = &config["compaction"];
    if window == 0 || c["enabled"].as_bool() == Some(false) || c["strategy"].as_str() == Some("off") {
        return None;
    }
    let number = |y: &Yaml| y.as_i64().map(|n| n as f64).or_else(|| y.as_f64()).filter(|n| *n > 0.0);
    let reserve = c["reserveTokens"].as_i64().filter(|n| *n >= 0).map(|n| n as u64);
    let mut tokens = number(&c["thresholdTokens"]).map(|n| n as u64);
    let mut percent = number(&c["thresholdPercent"]);
    let mut base = None;
    // The model's own: "f90000" fires there, "80%" is a percent, a bare
    // count is the window to work from.
    if let Some(entry) = omp_model_threshold(&c["modelThresholds"], model) {
        let text = match entry {
            Yaml::Integer(n) => n.to_string(),
            Yaml::String(s) => s.trim().to_owned(),
            _ => String::new(),
        };
        if let Some(n) = text.strip_prefix(['f', 'F']).and_then(|n| n.parse::<u64>().ok()).filter(|n| *n > 0) {
            (tokens, percent) = (Some(n), None);
        } else if let Some(p) = text.strip_suffix('%').and_then(|p| p.parse::<f64>().ok()).filter(|p| *p > 0.0 && *p <= 100.0) {
            (tokens, percent) = (None, Some(p));
        } else if let Some(n) = text.parse::<u64>().ok().filter(|n| *n > 0) {
            (tokens, base) = (None, Some(n));
        }
    }
    // The window less what it keeps free.
    let free = |w: u64| {
        let least = (w as f64 * 0.15).floor().max(1.0) as u64;
        let kept = least.max(reserve.unwrap_or(16_384));
        let kept = if (reserve.is_none() && kept >= w - least) || kept >= w { least } else { kept };
        w.saturating_sub(kept).min(w - 1)
    };
    if let Some(t) = tokens {
        return Some(if t >= window { free(window) } else { t.max(1) });
    }
    let window = base.map_or(window, |b| b.min(window));
    Some(match percent {
        Some(p) => (window as f64 * p.clamp(1.0, 99.0) / 100.0).floor() as u64,
        None => free(window),
    })
}

/// The model's entry in omp's `modelThresholds`: `provider/id`, else the
/// longest `provider/…*` prefix that matches.
fn omp_model_threshold<'a>(map: &'a Yaml, model: &str) -> Option<&'a Yaml> {
    let map = map.as_hash()?;
    if let Some(v) = map.get(&Yaml::String(model.to_owned())) {
        return Some(v);
    }
    map.iter()
        .filter_map(|(k, v)| Some((k.as_str()?.strip_suffix('*')?, v)))
        .filter(|(prefix, _)| model.starts_with(prefix))
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn yaml(text: &str) -> Yaml {
        YamlLoader::load_from_str(text).unwrap().into_iter().next().unwrap_or(Yaml::Null)
    }

    #[test]
    fn pi_keeps_its_reserve_free() {
        assert_eq!(pi_at(&[Value::Null, Value::Null], "a/b", 200_000), Some(200_000 - 16_384));
        let global = json!({ "compaction": { "reserveTokens": 20000, "modelOverrides": { "l/q": { "reserveTokens": 8000 } } } });
        assert_eq!(pi_at(&[global.clone(), Value::Null], "a/b", 200_000), Some(180_000));
        assert_eq!(pi_at(&[global.clone(), Value::Null], "l/q", 128_000), Some(120_000));
        let project = json!({ "compaction": { "enabled": false } });
        assert_eq!(pi_at(&[global, project], "a/b", 200_000), None, "the project's turns it off");
        assert_eq!(pi_at(&[Value::Null], "a/b", 0), None, "no window known");
    }

    #[test]
    fn omp_fires_at_its_threshold() {
        let none = Yaml::Null;
        assert_eq!(omp_at(&none, "a/b", 200_000), Some(170_000), "15% kept free");
        assert_eq!(omp_at(&none, "a/b", 64_000), Some(64_000 - 16_384), "or its reserve");
        assert_eq!(omp_at(&yaml("compaction:\n  thresholdPercent: 80\n"), "a/b", 200_000), Some(160_000));
        assert_eq!(omp_at(&yaml("compaction:\n  thresholdTokens: 90000\n"), "a/b", 200_000), Some(90_000));
        let models = yaml("compaction:\n  thresholdPercent: 80\n  modelThresholds:\n    a/b: f50000\n    a/*: \"60%\"\n    l/q: 100000\n");
        assert_eq!(omp_at(&models, "a/b", 200_000), Some(50_000));
        assert_eq!(omp_at(&models, "a/c", 200_000), Some(120_000));
        assert_eq!(omp_at(&models, "l/q", 200_000), Some(80_000), "80% of its 100k");
        assert_eq!(omp_at(&yaml("compaction:\n  strategy: \"off\"\n"), "a/b", 200_000), None);
    }
}
