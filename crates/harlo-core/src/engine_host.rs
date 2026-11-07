//! Model servers on another machine: harlo's `harlo-engines` helper put
//! there (once per build of it), and its commands run there over SSH
//! (`harlo-engines` says what each does). Engines, models and servers go in
//! harlo's folders on that machine, as they would here.

use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::remote::{self, Host};
use crate::remote_helper::Helper;

/// harlo-engines: this computer's own build for machines of its kind,
/// `harlo-engines-<arch>` for others.
static ENGINES: Helper = Helper::new("harlo-engines", false, "engines helper", "version");

/// Whether a helper is on `host` already (any build of it: harlo has set
/// something up there before).
fn helper_there(host: &Host) -> Result<bool> {
    ENGINES.there(host)
}

/// The helper on `host` (put there first if this build of it isn't): a
/// shell word for it there.
fn helper_on(host: &Host) -> Result<String> {
    Ok(remote::quote(&ENGINES.on(host)?))
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
