//! The sessions' shared plans (`harlo_core::plan`), kept in harlo's data
//! folder (`plans/`, a file per session). Your edits (the UI thread) and
//! the agents' plan tools (the orchestration threads) both go through
//! here, one at a time.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use harlo_core::plan::Plan;
use serde_json::{Value, json};

static PLANS: Mutex<Option<HashMap<String, Plan>>> = Mutex::new(None);

fn dir() -> PathBuf {
    harlo_core::db::default_path().parent().map_or_else(|| PathBuf::from("plans"), |d| d.join("plans"))
}

fn file_of(id: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    dir().join(format!("{:016x}.json", h.finish()))
}

/// A session's plan's id: its file once known, else its live key.
pub fn id_for(file: Option<&Path>, key: &str) -> String {
    file.map_or_else(|| key.to_owned(), |f| f.to_string_lossy().into_owned())
}

fn load(map: &mut HashMap<String, Plan>, id: &str) {
    if !map.contains_key(id) {
        let plan = std::fs::read_to_string(file_of(id)).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).map(|v| Plan::parse(&v["plan"].to_string())).unwrap_or_default();
        map.insert(id.to_owned(), plan);
    }
}

fn save(id: &str, plan: &Plan) {
    let file = file_of(id);
    if plan.steps.is_empty() {
        let _ = std::fs::remove_file(&file);
        return;
    }
    let _ = std::fs::create_dir_all(dir());
    let text = json!({ "session": id, "plan": serde_json::from_str::<Value>(&plan.to_text()).unwrap_or_default() }).to_string();
    if let Err(e) = std::fs::write(&file, text) {
        tracing::event!(name: "plan.save.failed", tracing::Level::WARN, session.key = %id, error.message = %e, "saving the plan of {{session.key}}: {{error.message}}");
    }
}

/// A session's plan (`file`: its session file, when known; a plan made
/// before it was moves to it).
pub fn get(file: Option<&Path>, key: &str) -> Plan {
    let mut guard = PLANS.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    let id = id_for(file, key);
    load(map, &id);
    if file.is_some() && map[&id].steps.is_empty() {
        load(map, key);
        if let Some(early) = map.remove(key).filter(|p| !p.steps.is_empty()) {
            save(&id, &early);
            save(key, &Plan::default());
            map.insert(id.clone(), early);
        }
    }
    map[&id].clone()
}

/// Changes a session's plan and saves it.
pub fn update<R>(file: Option<&Path>, key: &str, f: impl FnOnce(&mut Plan) -> R) -> R {
    get(file, key);
    let id = id_for(file, key);
    let mut guard = PLANS.lock().unwrap();
    let plan = guard.get_or_insert_with(HashMap::new).entry(id.clone()).or_default();
    let r = f(plan);
    save(&id, plan);
    r
}

/// An agent's plan tool call (off the UI thread). `ctx`: the session
/// (`key`, `file`), its folder (`cwd`), whether steps are committed
/// (`commits`), and the other sessions' last work (`others: [{ cwd, last
/// }]`).
pub fn tool(tool: &str, args: &Value, ctx: &Value) -> anyhow::Result<Value> {
    use harlo_core::plan;
    let key = ctx["key"].as_str().unwrap_or_default();
    let file = ctx["file"].as_str().map(PathBuf::from);
    let file = file.as_deref();
    let cwd = PathBuf::from(ctx["cwd"].as_str().unwrap_or_default());
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let id = u32::try_from(args["id"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
    match tool {
        "plan_read" => Ok(get(file, key).json()),
        "plan_set" => {
            let steps: Vec<(String, String, bool)> = args["steps"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|s| match s {
                    Value::String(t) => (t.clone(), String::new(), false),
                    _ => (s["title"].as_str().unwrap_or_default().to_owned(), s["detail"].as_str().unwrap_or_default().to_owned(), s["alongside"] == true),
                })
                .collect();
            update(file, key, |p| p.set(&steps).map(|()| p.json()))
        }
        "plan_start" => {
            // The repository as the step finds it (outside the lock: git).
            let snapshot = plan::snapshot(&cwd, now);
            update(file, key, |p| p.start(id, snapshot).map(|()| json!({ "started": id })))
        }
        "plan_done" => {
            let skipped = args["skipped"].as_bool().unwrap_or(false);
            let summary = args["summary"].as_str().unwrap_or_default().to_owned();
            let step = update(file, key, |p| p.finish(id, &summary, skipped).map(|s| s.clone()))?;
            if skipped || ctx["commits"] != true {
                return Ok(json!({ "step": id, "status": step.status }));
            }
            // Its own commit, when its changes are provably its own.
            let outcome: anyhow::Result<Option<String>> = match &step.start {
                None => Err(anyhow::Error::msg("plan_start wasn't called for it, so harlo doesn't know what it changed.")),
                Some(start) => {
                    let root = harlo_core::git::repo_root(&cwd).unwrap_or_else(|| cwd.clone());
                    let busy_elsewhere = ctx["others"].as_array().into_iter().flatten().any(|o| {
                        Path::new(o["cwd"].as_str().unwrap_or_default()).starts_with(&root) && o["last"].as_i64().unwrap_or(0) >= start.at
                    });
                    if busy_elsewhere {
                        Err(anyhow::Error::msg("Another session worked in this repository during the step, so harlo can't tell the changes apart."))
                    } else {
                        match plan::changed_since(&cwd, start) {
                            Ok(files) if files.is_empty() => Ok(None),
                            Ok(files) => plan::commit(&cwd, &files, &step.title, &step.summary).map(Some).map_err(|e| anyhow::anyhow!("git couldn't commit it: {e}")),
                            Err(why) => Err(why),
                        }
                    }
                }
            };
            update(file, key, |p| {
                if let Some(s) = p.steps.iter_mut().find(|s| s.id == id) {
                    s.start = None;
                    match &outcome {
                        Ok(Some(sha)) => s.commit.clone_from(sha),
                        Ok(None) => s.commit_note = "Nothing changed in it.".into(),
                        Err(why) => s.commit_note = format!("Not committed: {why} The changes are left for you."),
                    }
                }
            });
            Ok(match outcome {
                Ok(Some(sha)) => json!({ "step": id, "status": "done", "commit": sha, "note": "harlo committed the step's changes." }),
                Ok(None) => json!({ "step": id, "status": "done", "note": "Nothing changed in it, so there's nothing to commit." }),
                Err(why) => json!({ "step": id, "status": "done", "note": format!("harlo didn't commit it: {why} Leave those changes as they are.") }),
            })
        }
        _ => Err(anyhow::anyhow!("harlo has no tool {tool}.")),
    }
}
