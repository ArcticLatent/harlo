//! Usage statistics across every pi/omp session on disk: sessions, prompts,
//! tokens, cost, activity by day, models and tool calls.
//!
//! Each session file is summarised once and cached by (modified time, size);
//! a report then aggregates the summaries for a period and set of folders.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use chrono::{Datelike, Local, NaiveDate, TimeZone};
use harlo_harness::{HarnessKind, store};
use serde_json::{Value, json};

/// One model call (an assistant message).
#[derive(Debug, Clone, Default)]
struct Step {
    ts: i64,
    model: String,
    provider: String,
    thinking: String,
    input: u64,
    output: u64,
    reasoning: u64,
    cache_read: u64,
    cache_write: u64,
    cost: f64,
}

#[derive(Debug, Default)]
pub struct FileStats {
    pub path: PathBuf,
    pub cwd: PathBuf,
    pub harness: String,
    /// Started by another session (omp subagents).
    pub subagent: bool,
    steps: Vec<Step>,
    prompts: Vec<i64>,
    tools: Vec<(i64, String)>,
}

/// Models used in these sessions: `(harness, "provider/model", last used)`,
/// most recently used first, one entry per harness and model.
pub fn known_models(files: &[Arc<FileStats>]) -> Vec<(String, String, i64)> {
    let mut seen: HashMap<(String, String), i64> = HashMap::new();
    for f in files {
        for s in &f.steps {
            if s.model.is_empty() || s.model == "unknown" {
                continue;
            }
            let id = if s.provider.is_empty() { s.model.clone() } else { format!("{}/{}", s.provider, s.model) };
            let last = seen.entry((f.harness.clone(), id)).or_insert(0);
            *last = (*last).max(s.ts);
        }
    }
    let mut out: Vec<(String, String, i64)> = seen.into_iter().map(|((h, m), t)| (h, m, t)).collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.2));
    out
}

/// Summarises one session file (blocking).
pub fn summarize(kind: HarnessKind, path: &Path) -> Option<FileStats> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut fs = FileStats { path: path.to_owned(), harness: kind.as_str().to_owned(), ..Default::default() };
    let mut thinking = String::new();
    let mut have_header = false;
    for line in text.lines() {
        // Cheap prefilter: only a few entry types matter.
        if !(line.contains("\"message\"") || line.contains("\"session\"") || line.contains("thinking_level_change")) {
            continue;
        }
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        match e["type"].as_str() {
            Some("session") if !have_header => {
                have_header = true;
                fs.cwd = PathBuf::from(e["cwd"].as_str().unwrap_or_default());
                fs.subagent = e["parentSession"].as_str().is_some();
            }
            Some("thinking_level_change") => {
                thinking = e["thinkingLevel"].as_str().unwrap_or_default().to_owned();
            }
            Some("message") => {
                let m = &e["message"];
                let ts = m["timestamp"].as_i64().or_else(|| e["timestamp"].as_str().and_then(parse_iso_ms)).unwrap_or(0);
                match m["role"].as_str() {
                    Some("user") => fs.prompts.push(ts),
                    Some("assistant") => {
                        let u = &m["usage"];
                        let n = |k: &str| u[k].as_u64().unwrap_or(0);
                        fs.steps.push(Step {
                            ts,
                            model: m["model"].as_str().unwrap_or("unknown").to_owned(),
                            provider: m["provider"].as_str().unwrap_or_default().to_owned(),
                            thinking: thinking.clone(),
                            input: n("input"),
                            output: n("output"),
                            reasoning: n("reasoning"),
                            cache_read: n("cacheRead"),
                            cache_write: n("cacheWrite"),
                            cost: u["cost"]["total"].as_f64().unwrap_or(0.0),
                        });
                        for b in m["content"].as_array().into_iter().flatten() {
                            if b["type"] == "toolCall" {
                                fs.tools.push((ts, b["name"].as_str().unwrap_or("tool").to_owned()));
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    have_header.then_some(fs)
}

fn parse_iso_ms(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}

type CacheEntry = (SystemTime, u64, Arc<FileStats>);

fn cache() -> &'static Mutex<HashMap<PathBuf, CacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Summaries of every session file, re-reading only new or changed ones (blocking).
pub fn all_sessions() -> Vec<Arc<FileStats>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for kind in [HarnessKind::Pi, HarnessKind::Omp] {
        for (path, modified, size) in store::list_files(kind) {
            seen.insert(path.clone());
            let cached = cache().lock().unwrap().get(&path).filter(|(m, s, _)| *m == modified && *s == size).map(|(_, _, f)| f.clone());
            let stats = match cached {
                Some(f) => f,
                None => match summarize(kind, &path) {
                    Some(f) => {
                        let f = Arc::new(f);
                        cache().lock().unwrap().insert(path.clone(), (modified, size, f.clone()));
                        f
                    }
                    None => continue,
                },
            };
            out.push(stats);
        }
    }
    cache().lock().unwrap().retain(|p, _| seen.contains(p));
    out
}

fn local_day(ts_ms: i64) -> Option<NaiveDate> {
    Local.timestamp_millis_opt(ts_ms).single().map(|d| d.date_naive())
}

#[derive(Default)]
struct ModelAgg {
    tokens: u64,
    input: u64,
    cost: f64,
    steps: u64,
}

/// Aggregates `files` over the last `days` days (0 = all time), keeping
/// sessions whose folder is inside one of `roots` (empty = all).
pub fn report(files: &[Arc<FileStats>], days: u32, roots: &[PathBuf]) -> Value {
    let today = Local::now().date_naive();
    let since = (days > 0).then(|| today - chrono::Duration::days(days as i64 - 1));
    let in_period = |ts: i64| match (since, local_day(ts)) {
        (Some(s), Some(d)) => d >= s,
        (None, _) => true,
        (Some(_), None) => false,
    };
    let in_roots = |cwd: &Path| roots.is_empty() || roots.iter().any(|r| cwd.starts_with(r));

    let (mut sessions, mut subagents, mut prompts) = (0u64, 0u64, 0u64);
    let (mut input, mut output, mut reasoning, mut cache_read, mut cache_write) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut cost = 0.0f64;
    let mut steps_total = 0u64;
    let mut active: HashSet<NaiveDate> = HashSet::new();
    let mut per_day: BTreeMap<NaiveDate, u64> = BTreeMap::new();
    let mut models: HashMap<(String, String, String), ModelAgg> = HashMap::new();
    let mut tools: HashMap<String, u64> = HashMap::new();
    let mut first_day: Option<NaiveDate> = None;

    for f in files.iter().filter(|f| in_roots(&f.cwd)) {
        let mut touched = false;
        for &ts in f.prompts.iter().filter(|&&t| in_period(t)) {
            prompts += 1;
            touched = true;
            if let Some(d) = local_day(ts) {
                active.insert(d);
                first_day = Some(first_day.map_or(d, |f| f.min(d)));
            }
        }
        for s in f.steps.iter().filter(|s| in_period(s.ts)) {
            touched = true;
            steps_total += 1;
            input += s.input;
            output += s.output;
            reasoning += s.reasoning;
            cache_read += s.cache_read;
            cache_write += s.cache_write;
            cost += s.cost;
            if let Some(d) = local_day(s.ts) {
                active.insert(d);
                *per_day.entry(d).or_default() += 1;
                first_day = Some(first_day.map_or(d, |f| f.min(d)));
            }
            let m = models.entry((s.model.clone(), s.thinking.clone(), s.provider.clone())).or_default();
            m.tokens += s.input + s.output + s.cache_read + s.cache_write;
            m.input += s.input;
            m.cost += s.cost;
            m.steps += 1;
        }
        for (ts, name) in &f.tools {
            if in_period(*ts) {
                *tools.entry(name.clone()).or_default() += 1;
            }
        }
        if touched {
            sessions += 1;
            if f.subagent {
                subagents += 1;
            }
        }
    }

    // Longest run of consecutive active days.
    let mut sorted: Vec<NaiveDate> = active.iter().copied().collect();
    sorted.sort();
    let (mut streak, mut best) = (0u32, 0u32);
    let mut prev: Option<NaiveDate> = None;
    for d in &sorted {
        streak = if prev.is_some_and(|p| *d - p == chrono::Duration::days(1)) { streak + 1 } else { 1 };
        best = best.max(streak);
        prev = Some(*d);
    }

    // One bar per day from the start of the period (or first activity) to today.
    let start = since.or(first_day).unwrap_or(today);
    let mut daily = Vec::new();
    let mut d = start;
    while d <= today {
        daily.push(json!({ "date": format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()), "steps": per_day.get(&d).copied().unwrap_or(0) }));
        d += chrono::Duration::days(1);
    }

    let total_tokens = input + output + cache_read + cache_write;
    let mut model_rows: Vec<Value> = models
        .into_iter()
        .map(|((model, thinking, provider), m)| {
            json!({ "model": model, "thinking": thinking, "provider": provider, "tokens": m.tokens, "input": m.input, "cost": m.cost, "steps": m.steps })
        })
        .collect();
    model_rows.sort_by(|a, b| b["tokens"].as_u64().cmp(&a["tokens"].as_u64()));
    let mut tool_rows: Vec<Value> = tools.into_iter().map(|(name, count)| json!({ "name": name, "count": count })).collect();
    tool_rows.sort_by(|a, b| b["count"].as_u64().cmp(&a["count"].as_u64()));
    let per = |x: f64| if sessions > 0 { x / sessions as f64 } else { 0.0 };
    let cache_denom = input + cache_read + cache_write;

    json!({
        "sessions": sessions,
        "subagents": subagents,
        "prompts": prompts,
        "steps": steps_total,
        "tokens": { "total": total_tokens, "input": input, "output": output, "reasoning": reasoning, "cacheRead": cache_read, "cacheWrite": cache_write },
        "cost": cost,
        "activeDays": active.len(),
        "longestStreak": best,
        "daily": daily,
        "efficiency": {
            "cacheHit": if cache_denom > 0 { cache_read as f64 / cache_denom as f64 * 100.0 } else { 0.0 },
            "costPerSession": per(cost),
            "costPerMillion": if total_tokens > 0 { cost / (total_tokens as f64 / 1e6) } else { 0.0 },
            "tokensPerSession": per(total_tokens as f64),
            "stepsPerSession": per(steps_total as f64),
            "reasoningShare": if output > 0 { reasoning as f64 / output as f64 * 100.0 } else { 0.0 },
        },
        "models": model_rows,
        "tools": tool_rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(lines: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("harlo-stats-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(format!("{}.jsonl", lines.len()));
        std::fs::write(&p, lines.join("\n")).unwrap();
        p
    }

    #[test]
    fn summarizes_and_reports() {
        let now = Local::now().timestamp_millis();
        let day = 86_400_000i64;
        let a = format!(r#"{{"type":"message","message":{{"role":"assistant","model":"gpt","provider":"openai","timestamp":{},"usage":{{"input":100,"output":50,"reasoning":20,"cacheRead":850,"cacheWrite":0,"cost":{{"total":0.5}}}},"content":[{{"type":"toolCall","name":"bash"}},{{"type":"toolCall","name":"read"}}]}}}}"#, now);
        let b = format!(r#"{{"type":"message","message":{{"role":"assistant","model":"gpt","provider":"openai","timestamp":{},"usage":{{"input":10,"output":5,"cacheRead":85,"cost":{{"total":0.1}}}},"content":[{{"type":"toolCall","name":"bash"}}]}}}}"#, now - day);
        let old = format!(r#"{{"type":"message","message":{{"role":"assistant","model":"old","timestamp":{},"usage":{{"input":1}}}}}}"#, now - 100 * day);
        let lines = [
            r#"{"type":"session","id":"s","cwd":"/w/app"}"#.to_owned(),
            r#"{"type":"thinking_level_change","thinkingLevel":"high"}"#.to_owned(),
            format!(r#"{{"type":"message","message":{{"role":"user","content":"hi","timestamp":{}}}}}"#, now),
            a,
            b,
            old,
        ];
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let fs = Arc::new(summarize(HarnessKind::Pi, &file(&refs)).unwrap());
        assert_eq!((fs.steps.len(), fs.prompts.len(), fs.tools.len()), (3, 1, 3));
        assert_eq!(fs.steps[0].thinking, "high");

        let r = report(std::slice::from_ref(&fs), 30, &[]);
        assert_eq!((r["sessions"].as_u64(), r["prompts"].as_u64(), r["steps"].as_u64()), (Some(1), Some(1), Some(2)));
        assert_eq!(r["tokens"]["total"], 100 + 50 + 850 + 10 + 5 + 85);
        assert!((r["cost"].as_f64().unwrap() - 0.6).abs() < 1e-9);
        assert_eq!((r["activeDays"].as_u64(), r["longestStreak"].as_u64()), (Some(2), Some(2)));
        assert_eq!(r["daily"].as_array().unwrap().len(), 30);
        assert_eq!(r["tools"][0]["name"], "bash");
        assert_eq!(r["models"][0]["model"], "gpt");
        // All time includes the 100-day-old step; another folder excludes everything.
        assert_eq!(report(std::slice::from_ref(&fs), 0, &[])["steps"], 3);
        assert_eq!(report(&[fs], 30, &[PathBuf::from("/other")])["sessions"], 0);
    }
}
