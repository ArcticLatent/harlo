//! Coding plans for Claude Code: subscriptions (GLM Coding Plan, Kimi
//! Code, MiniMax M Plan) whose makers support Claude Code pointed at their
//! Anthropic-format endpoint.
//!
//! Their terms allow listed tools only, so harlo never calls these
//! endpoints itself: a Claude Code session on a plan runs `claude` with the
//! plan's endpoint, key and models in a settings file only the user can
//! read (`--settings`), and the key's check is one short Claude Code
//! request. The plans are a built-in list (`coding_plans.json`, from each
//! plan's own Claude Code page); the keys are in
//! `$XDG_CONFIG_HOME/harlo/coding-plans.json`, readable by the user alone.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow, bail};
use serde::Deserialize;
use serde_json::{Value, json};

/// A coding plan Claude Code can use.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// harlo's name for it ("glm-coding").
    pub id: String,
    /// As its maker names it ("GLM Coding Plan").
    pub name: String,
    /// Whose models it serves, for its logo ("zai").
    pub maker: String,
    /// Its Anthropic-format endpoint (`ANTHROPIC_BASE_URL`).
    pub base_url: String,
    /// The variable its key goes in (`ANTHROPIC_AUTH_TOKEN` or `ANTHROPIC_API_KEY`).
    pub key_var: String,
    /// Where a key is made.
    pub key_page: String,
    /// Its plans and what they include.
    pub plan_page: String,
    /// What can be picked, first the one to start with.
    pub models: Vec<PlanModel>,
    /// The models Claude Code's own choices (Opus, Sonnet, Haiku, its
    /// subagents) map to.
    pub tiers: Tiers,
    /// The rest of what its Claude Code page sets (context window, timeout).
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// One of a plan's models.
#[derive(Clone, Debug, Deserialize)]
pub struct PlanModel {
    /// As Claude Code asks for it ("glm-5.3[1m]").
    pub id: String,
    /// As shown ("GLM 5.3").
    pub name: String,
}

/// Claude Code's model choices, mapped to a plan's models.
#[derive(Clone, Debug, Deserialize)]
pub struct Tiers {
    pub opus: String,
    pub sonnet: String,
    pub haiku: String,
    /// Its subagents' model, when the plan names one.
    #[serde(default)]
    pub subagent: Option<String>,
}

static PLANS: LazyLock<Vec<Plan>> = LazyLock::new(|| serde_json::from_str(include_str!("coding_plans.json")).expect("coding_plans.json is a list of plans"));

/// The coding plans harlo knows.
#[must_use]
pub fn plans() -> &'static [Plan] {
    &PLANS
}

/// The plan named `id`.
#[must_use]
pub fn find(id: &str) -> Option<&'static Plan> {
    PLANS.iter().find(|p| p.id == id)
}

/// The plan a `<plan>/<model>` choice is on, and the model.
#[must_use]
pub fn of_model(choice: &str) -> Option<(&'static Plan, &'static PlanModel)> {
    let (plan, model) = choice.split_once('/')?;
    let plan = find(plan)?;
    Some((plan, plan.models.iter().find(|m| m.id == model)?))
}

/// The keys' file: `$XDG_CONFIG_HOME/harlo/coding-plans.json`.
fn keys_file() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    config.join("harlo").join("coding-plans.json")
}

fn read_keys() -> BTreeMap<String, String> {
    read_keys_at(&keys_file())
}

fn read_keys_at(file: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Writes the keys where only the user can read them (folder 0700, file
/// 0600, replaced whole).
fn write_keys_at(file: &Path, keys: &BTreeMap<String, String>) -> Result<()> {
    let dir = file.parent().context("no folder for the keys")?;
    std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    private(dir, 0o700)?;
    let next = file.with_extension("json.new");
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode_0600().open(&next).with_context(|| format!("writing {}", next.display()))?;
    f.write_all(serde_json::to_string_pretty(keys)?.as_bytes())?;
    f.sync_all()?;
    drop(f);
    private(&next, 0o600)?;
    std::fs::rename(&next, file).with_context(|| format!("writing {}", file.display()))
}

/// Opening a file only its owner can read (where that applies).
trait Mode0600 {
    fn mode_0600(&mut self) -> &mut Self;
}

impl Mode0600 for std::fs::OpenOptions {
    #[cfg(unix)]
    fn mode_0600(&mut self) -> &mut Self {
        use std::os::unix::fs::OpenOptionsExt as _;
        self.mode(0o600)
    }
    #[cfg(not(unix))]
    fn mode_0600(&mut self) -> &mut Self {
        self
    }
}

#[cfg(unix)]
fn private(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).with_context(|| format!("making {} private", path.display()))
}

#[cfg(not(unix))]
fn private(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

/// The key saved for plan `id`.
#[must_use]
pub fn key(id: &str) -> Option<String> {
    read_keys().remove(id).filter(|k| !k.is_empty())
}

/// Saves plan `id`'s key.
///
/// # Errors
///
/// An unknown plan, an empty key, or the file not written.
pub fn set_key(id: &str, key: &str) -> Result<()> {
    set_key_at(&keys_file(), id, key)
}

fn set_key_at(file: &Path, id: &str, key: &str) -> Result<()> {
    if find(id).is_none() {
        bail!("harlo doesn't know the plan {id}");
    }
    let key = key.trim();
    if key.is_empty() {
        bail!("the key is empty");
    }
    let mut keys = read_keys_at(file);
    keys.insert(id.to_owned(), key.to_owned());
    write_keys_at(file, &keys)
}

/// Forgets plan `id`'s key.
///
/// # Errors
///
/// The file not written.
pub fn remove_key(id: &str) -> Result<()> {
    remove_key_at(&keys_file(), id)
}

fn remove_key_at(file: &Path, id: &str) -> Result<()> {
    let mut keys = read_keys_at(file);
    if keys.remove(id).is_some() {
        write_keys_at(file, &keys)?;
    }
    Ok(())
}

/// The plans as Settings lists them (the cloud providers' shape): each
/// with whether it has a key, its pages and its models.
#[must_use]
pub fn list_json() -> Value {
    let keys = read_keys();
    Value::Array(
        PLANS
            .iter()
            .map(|p| {
                json!({
                    "id": p.id, "name": p.name, "maker": p.maker, "keyOf": p.id,
                    "key": if keys.get(&p.id).is_some_and(|k| !k.is_empty()) { "saved" } else { "none" },
                    "keyPage": p.key_page, "planPage": p.plan_page,
                    "models": p.models.iter().map(|m| json!({ "id": m.id, "name": m.name })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// The models of the plans with a key, as a Claude Code session's picker
/// lists them: `{ provider: <plan id>, providerName, id, name, maker, plan }`.
#[must_use]
pub fn models_json() -> Vec<Value> {
    let keys = read_keys();
    PLANS
        .iter()
        .filter(|p| keys.get(&p.id).is_some_and(|k| !k.is_empty()))
        .flat_map(|p| p.models.iter().map(move |m| json!({ "provider": p.id, "providerName": p.name, "id": m.id, "name": m.name, "maker": p.maker, "plan": true, "reasoning": true })))
        .collect()
}

/// What Claude Code needs to run `model` on `plan` with `key`: its
/// settings' `env` (the endpoint, the key, the model and what Claude
/// Code's own choices map to, and the plan's other settings).
#[must_use]
pub fn settings(plan: &Plan, model: &str, key: &str) -> Value {
    let mut env: BTreeMap<String, String> = plan.env.clone();
    env.insert("ANTHROPIC_BASE_URL".into(), plan.base_url.clone());
    // (Only the plan's own variable: the other would win if set.)
    for var in ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"] {
        env.insert(var.into(), if var == plan.key_var { key.to_owned() } else { String::new() });
    }
    env.insert("ANTHROPIC_MODEL".into(), model.to_owned());
    env.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".into(), plan.tiers.opus.clone());
    env.insert("ANTHROPIC_DEFAULT_SONNET_MODEL".into(), plan.tiers.sonnet.clone());
    env.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), plan.tiers.haiku.clone());
    if let Some(sub) = &plan.tiers.subagent {
        env.insert("CLAUDE_CODE_SUBAGENT_MODEL".into(), sub.clone());
    }
    json!({ "env": env })
}

/// Writes `settings` to a private file in a folder of its own under `dir`.
///
/// Only the user can read it, and it's removed when the returned guard is
/// dropped. Folders left there by a harlo that has ended (one that quit with
/// a session on a plan) go first.
///
/// # Errors
///
/// The folder or file not written.
pub fn write_settings(dir: &Path, settings: &Value) -> Result<SettingsFile> {
    sweep_ended(dir);
    let folder = dir.join(format!("harlo-plan-{}-{:x}", std::process::id(), rand_word()));
    std::fs::create_dir_all(&folder).with_context(|| format!("making {}", folder.display()))?;
    private(&folder, 0o700)?;
    let path = folder.join("settings.json");
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode_0600().open(&path).with_context(|| format!("writing {}", path.display()))?;
    f.write_all(settings.to_string().as_bytes())?;
    Ok(SettingsFile { folder, path })
}

/// Removes the `harlo-plan-<pid>-…` folders in `dir` whose harlo has ended.
fn sweep_ended(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.strip_prefix("harlo-plan-")).and_then(|r| r.split('-').next()) else { continue };
        if pid.parse::<u32>().is_ok() && !Path::new("/proc").join(pid).exists() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// A word that differs call to call (the clock and a count), for folder names.
fn rand_word() -> u64 {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    (now.as_secs() << 20) ^ u64::from(now.subsec_nanos()) ^ COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed).rotate_left(40)
}

/// A plan's settings file, removed with its folder when dropped.
#[derive(Debug)]
pub struct SettingsFile {
    folder: PathBuf,
    /// The file, for `claude --settings`.
    pub path: PathBuf,
}

impl Drop for SettingsFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

/// Checks plan `id`'s key with one short Claude Code request on the plan
/// (`claude` is the program): what the model said, or why it failed.
///
/// # Errors
///
/// No key, `claude` not started, or the request refused or timed out
/// (Claude Code's message, which says what the plan answered).
pub fn check(id: &str, claude: &Path) -> Result<String> {
    let plan = find(id).ok_or_else(|| anyhow!("harlo doesn't know the plan {id}"))?;
    let key = key(id).ok_or_else(|| anyhow!("no key saved for {}", plan.name))?;
    let file = write_settings(&std::env::temp_dir(), &settings(plan, &plan.tiers.haiku, &key))?;
    let mut child = std::process::Command::new(claude)
        .args(["-p", "Reply with the single word: ok", "--output-format", "json", "--no-session-persistence", "--model", &plan.tiers.haiku, "--settings"])
        .arg(&file.path)
        .current_dir(file.path.parent().unwrap_or(Path::new("/")))
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("starting {}", claude.display()))?;
    let deadline = Instant::now() + Duration::from_secs(120);
    while child.try_wait()?.is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Claude Code didn't hear back from {} in 2 minutes", plan.name);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let out = child.wait_with_output()?;
    let said: Value = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
    let text = said["result"].as_str().unwrap_or_default().trim().to_owned();
    if said["is_error"] == true || !out.status.success() {
        let why = if text.is_empty() { String::from_utf8_lossy(&out.stderr).trim().lines().last().unwrap_or("it ended with an error").to_owned() } else { text };
        bail!("{why}");
    }
    Ok(format!("{} answered through Claude Code", plan.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test's own folder, removed when the test ends.
    struct Folder(PathBuf);

    impl Folder {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_plans_are_whole() {
        for p in plans() {
            assert!(!p.models.is_empty() && p.base_url.starts_with("https://"), "{}", p.id);
            assert!(["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"].contains(&p.key_var.as_str()), "{}", p.id);
            for tier in [&p.tiers.opus, &p.tiers.sonnet, &p.tiers.haiku] {
                assert!(p.models.iter().any(|m| &m.id == tier), "{}: {tier} is one of its models", p.id);
            }
        }
        let (plan, model) = of_model("glm-coding/glm-5.3[1m]").unwrap();
        assert_eq!((plan.name.as_str(), model.name.as_str()), ("GLM Coding Plan", "GLM 5.3"));
        assert!(of_model("glm-coding/claude-opus-5-5").is_none());
    }

    #[test]
    fn settings_carry_the_plan() {
        let kimi = find("kimi-code").unwrap();
        let s = settings(kimi, "k3.1-pro", "sk-kimi");
        let env = &s["env"];
        assert_eq!((env["ANTHROPIC_BASE_URL"].as_str(), env["ANTHROPIC_API_KEY"].as_str(), env["ANTHROPIC_AUTH_TOKEN"].as_str()), (Some("https://api.kimi.ai/coding/"), Some("sk-kimi"), Some("")));
        assert_eq!((env["ANTHROPIC_MODEL"].as_str(), env["CLAUDE_CODE_SUBAGENT_MODEL"].as_str(), env["CLAUDE_CODE_MAX_CONTEXT_TOKENS"].as_str()), (Some("k3.1-pro"), Some("k3.1-pro"), Some("262144")));
        let glm = settings(find("glm-coding").unwrap(), "glm-5.3-flash[1m]", "k");
        assert_eq!((glm["env"]["ANTHROPIC_AUTH_TOKEN"].as_str(), glm["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"].as_str()), (Some("k"), Some("glm-5.3-flash[1m]")));
        assert!(glm["env"].get("CLAUDE_CODE_SUBAGENT_MODEL").is_none());
    }

    #[test]
    fn keys_are_private() {
        let dir = Folder::new("harlo-plan-keys");
        let file = dir.0.join("harlo/coding-plans.json");
        set_key_at(&file, "glm-coding", " k1 ").unwrap();
        set_key_at(&file, "kimi-code", "k2").unwrap();
        assert!(set_key_at(&file, "nowhere", "k").is_err() && set_key_at(&file, "kimi-code", "  ").is_err());
        assert_private(&file);
        remove_key_at(&file, "glm-coding").unwrap();
        assert_eq!(read_keys_at(&file), BTreeMap::from([("kimi-code".to_owned(), "k2".to_owned())]));
    }

    /// The file readable by the user alone, in a folder only they can open.
    #[cfg(unix)]
    fn assert_private(file: &Path) {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(std::fs::metadata(file).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(file.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
    }
    #[cfg(not(unix))]
    fn assert_private(_: &Path) {}

    #[test]
    fn a_settings_file_is_private_and_goes_away() {
        let dir = Folder::new("harlo-plans-test");
        // (One a harlo that has ended left: removed; one of a running one's
        // (this test's): kept.)
        let ended = dir.0.join("harlo-plan-999999999-1");
        let running = dir.0.join(format!("harlo-plan-{}-1", std::process::id()));
        std::fs::create_dir_all(&ended).unwrap();
        std::fs::create_dir_all(&running).unwrap();
        let path = {
            let f = write_settings(&dir.0, &json!({ "env": { "A": "b" } })).unwrap();
            assert_private(&f.path);
            assert_eq!(std::fs::read_to_string(&f.path).unwrap(), r#"{"env":{"A":"b"}}"#);
            f.path.clone()
        };
        assert!(!path.exists() && !path.parent().unwrap().exists(), "removed with its folder");
        assert!(!ended.exists() && running.exists(), "an ended harlo's folder goes, a running one's stays");
    }
}
