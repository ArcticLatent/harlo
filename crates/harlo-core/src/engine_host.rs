//! Model servers on another machine: harlo's `harlo-engines` helper put
//! there (once per build of it), and its commands run there over SSH
//! (`harlo-engines` says what each does). Engines, models and servers go in
//! harlo's folders on that machine, as they would here.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::remote::{self, Host};

/// The helper for machines of `arch` (`uname -m`): this computer's kind is
/// `harlo-engines`, another `harlo-engines-<arch>`; beside harlo's own
/// program (a build puts it there), or in `lib/harlo` next to its folder (an
/// install).
fn local_helper(arch: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = if arch == std::env::consts::ARCH { "harlo-engines".to_owned() } else { format!("harlo-engines-{arch}") };
    [dir.join(&name), dir.join("../lib/harlo").join(&name)].into_iter().find(|p| p.is_file())
}

/// The machines the helper runs on.
const ARCHES: [&str; 2] = ["x86_64", "aarch64"];

/// The helper for `arch` as it goes over (stripped of its debugging data
/// when `strip` is here, gzipped), and a name for this build of it.
fn packed(arch: &str) -> Result<&'static (Vec<u8>, String)> {
    static PACKED: [OnceLock<std::result::Result<(Vec<u8>, String), String>>; 2] = [OnceLock::new(), OnceLock::new()];
    let i = ARCHES.iter().position(|a| *a == arch).ok_or_else(|| anyhow::anyhow!("harlo's engines helper runs on x86_64 and ARM (aarch64) machines, not {arch} ones."))?;
    PACKED[i]
        .get_or_init(|| {
            let helper = local_helper(arch).ok_or_else(|| {
                if arch == std::env::consts::ARCH {
                    "harlo-engines isn't next to harlo's program: reinstall harlo".to_owned()
                } else {
                    format!("This harlo has no engines helper for {arch} machines (harlo's releases have one).")
                }
            })?;
            let tmp = std::env::temp_dir().join(format!("harlo-engines-{arch}-{}", std::process::id()));
            let stripped = std::process::Command::new("strip").arg("-o").arg(&tmp).arg(&helper).stdin(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
            let bytes = std::fs::read(if stripped { &tmp } else { &helper }).map_err(|e| e.to_string());
            let _ = std::fs::remove_file(&tmp);
            let bytes = bytes?;
            let id = bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100_0000_01b3));
            let mut gz = std::process::Command::new("gzip").args(["-c", "-6"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().map_err(|e| format!("gzip: {e}"))?;
            let mut input = gz.stdin.take().expect("piped");
            let feed = std::thread::spawn(move || input.write_all(&bytes));
            let out = gz.wait_with_output().map_err(|e| e.to_string())?;
            let _ = feed.join();
            if !out.status.success() {
                return Err("gzip failed".into());
            }
            Ok((out.stdout, format!("{id:016x}")))
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// What kind of machine `host` is (`uname -m`, aarch64 for arm64).
fn arch_of(host: &Host) -> Result<String> {
    static SEEN: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());
    if let Some((_, a)) = SEEN.lock().unwrap().iter().find(|(h, _)| *h == host.name) {
        return Ok(a.clone());
    }
    let out = remote::run(host, "uname -m", Duration::from_secs(30))?;
    let arch = match String::from_utf8_lossy(&out.stdout).trim() {
        "arm64" => "aarch64".to_owned(),
        "amd64" => "x86_64".to_owned(),
        a => a.to_owned(),
    };
    SEEN.lock().unwrap().push((host.name.clone(), arch.clone()));
    Ok(arch)
}

/// Whether a helper is on `host` already (any build of it: harlo has set
/// something up there before).
fn helper_there(host: &Host) -> Result<bool> {
    let out = remote::run(host, "ls \"$HOME\"/.cache/harlo/harlo-engines-* >/dev/null 2>&1 && echo yes || echo no", Duration::from_secs(30))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim() == "yes")
}

/// The helper on `host` (put there first if this build of it isn't): a
/// shell word for it there.
fn helper_on(host: &Host) -> Result<String> {
    let arch = arch_of(host)?;
    let (gz, id) = packed(&arch).with_context(|| format!("{} is an {arch} machine", host.name))?;
    let path = format!("\"$HOME\"/.cache/harlo/harlo-engines-{id}");
    let out = remote::run(host, &format!("[ -x {path} ] && echo ready || echo missing"), Duration::from_secs(30))?;
    let said = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if said != "ready" {
        // (Older builds of it go.)
        let put = format!(
            "mkdir -p \"$HOME/.cache/harlo\" && rm -f \"$HOME\"/.cache/harlo/harlo-engines-* && gzip -dc > {path}.tmp && chmod 755 {path}.tmp && mv -f {path}.tmp {path} && {path} version"
        );
        let out = remote::run_with_input(host, &put, Some(gz), Duration::from_secs(300))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            // (Its C library older than the one harlo was built with, say.)
            bail!("harlo's engines helper doesn't run on {}: {}", host.name, err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("it failed").trim());
        }
    }
    Ok(path)
}

fn host(name: &str) -> Result<Host> {
    remote::host(name).with_context(|| format!("unknown machine {name}"))
}

/// A helper command's output on `host_name` (blocking).
pub fn run(host_name: &str, args: &[&str], limit: Duration) -> Result<String> {
    let h = host(host_name)?;
    let helper = helper_on(&h)?;
    let script = format!("{helper} {}", args.iter().map(|a| remote::quote(a)).collect::<Vec<_>>().join(" "));
    let out = remote::run(&h, &script, limit)?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).lines().rfind(|l| !l.trim().is_empty()).unwrap_or("harlo-engines failed").trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A helper command's output as JSON when harlo has set something up on
/// `host_name` before (`None`: it hasn't, and nothing's put there).
pub fn json_if_there(host_name: &str, args: &[&str], limit: Duration) -> Result<Option<Value>> {
    if !helper_there(&host(host_name)?)? {
        return Ok(None);
    }
    json(host_name, args, limit).map(Some)
}

/// The same, read as JSON.
pub fn json(host_name: &str, args: &[&str], limit: Duration) -> Result<Value> {
    let text = run(host_name, args, limit)?;
    serde_json::from_str(text.trim()).with_context(|| format!("harlo-engines {} said something unexpected", args.first().unwrap_or(&"")))
}

/// A setup (or update) there: `request` to it, each line it says to `line`
/// until it ends (blocking). An error when the machine can't be reached or
/// the connection drops.
pub fn setup(host_name: &str, job: &str, request: &Value, line: &mut dyn FnMut(Value)) -> Result<()> {
    let h = host(host_name)?;
    let helper = helper_on(&h)?;
    let mut cmd = remote::command(&h, &format!("{helper} setup {}", remote::quote(job)));
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().context("running ssh")?;
    {
        let mut input = child.stdin.take().expect("piped");
        input.write_all(request.to_string().as_bytes())?;
    }
    let stdout = child.stdout.take().expect("piped");
    let mut ended = false;
    for l in BufReader::new(stdout).lines() {
        let Ok(l) = l else { break };
        if let Ok(v) = serde_json::from_str::<Value>(&l) {
            ended |= v["job"]["state"].as_str().is_some_and(|s| s != "running");
            line(v);
        }
    }
    let out = child.wait_with_output()?;
    if !ended {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{}", err.lines().rfind(|l| !l.trim().is_empty()).map_or_else(|| format!("The connection to {host_name} ended."), |l| l.trim().to_owned()));
    }
    Ok(())
}

/// Stops a setup going on there.
pub fn cancel(host_name: &str, job: &str) -> Result<()> {
    run(host_name, &["cancel", job], Duration::from_secs(30)).map(|_| ())
}
