//! Agent jobs (installing or updating an agent, here or on a host): a
//! command run in the background, its output going to a log file that
//! "View output" shows in a terminal.

use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

pub struct Job {
    pub log: PathBuf,
    /// The local process (ssh for a host): following the log ends with it.
    pub pid: u32,
    child: Child,
}

/// Starts `cmd` with its output going to a new log (`name` names it).
pub fn start(mut cmd: Command, name: &str) -> anyhow::Result<Job> {
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    let log = std::env::temp_dir().join(format!("harlo-{safe}-{}.log", std::process::id()));
    let out = std::fs::File::create(&log)?;
    let err = out.try_clone()?;
    let child = cmd.stdin(std::process::Stdio::null()).stdout(out).stderr(err).spawn()?;
    Ok(Job { log, pid: child.id(), child })
}

/// Waits for the job (stopped after `timeout`): Ok when it worked, else why
/// not (the output's last line).
pub fn wait(mut job: Job, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let status = loop {
        match job.child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(200)),
            _ => {
                let _ = job.child.kill();
                let _ = job.child.wait();
                return Err(format!("it took over {} minutes and was stopped", timeout.as_secs() / 60));
            }
        }
    };
    if status.success() {
        return Ok(());
    }
    let last = last_line(&std::fs::read_to_string(&job.log).unwrap_or_default());
    Err(if last.is_empty() { format!("it stopped (exit {})", status.code().unwrap_or(-1)) } else { last })
}

/// A command showing a job's log in a terminal: followed while it runs.
pub fn output_command(log: &std::path::Path, pid: u32, running: bool) -> String {
    let follow = if running && pid > 0 { format!(" --pid={pid} -f") } else { String::new() };
    format!("tail -n +1{follow} {}", harlo_core::remote::quote(&log.to_string_lossy()))
}

/// The last line of a job's output worth showing (colours and
/// progress-bar redraws left out).
pub fn last_line(text: &str) -> String {
    let mut plain = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        plain.push(c);
    }
    plain
        .lines()
        .map(|l| l.rsplit('\r').next().unwrap_or(l).trim())
        .rfind(|l| !l.is_empty())
        .map(|l| l.chars().take(300).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn last_line_of_an_installer() {
        assert_eq!(super::last_line("downloading\r 50%\r100%\n\u{1b}[31merror: no space left\u{1b}[0m\n\n"), "error: no space left");
        assert_eq!(super::last_line(""), "");
    }
}
