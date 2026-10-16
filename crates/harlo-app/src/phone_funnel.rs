//! Tailscale Funnel: a public HTTPS address for harlo's phone port, so a
//! phone reaches harlo from anywhere with nothing hosted. harlo runs
//! `tailscale funnel` in the foreground while it's wanted; when that stops
//! (harlo quits, the switch goes off) Tailscale takes the address down.
//!
//! It uses HTTPS port 8443 (`https://<computer>.<tailnet>.ts.net:8443`), so
//! it leaves alone anything the user shares on 443 themselves.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{mpsc, oneshot};

/// The public port Funnel serves harlo on.
pub const HTTPS_PORT: u16 = 8443;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Starting,
    /// Funnel isn't allowed on the tailnet yet: the user approves it here.
    NeedsApproval(String),
    /// Live at this address.
    Online(String),
    /// Stopped (harlo tries again).
    Failed(String),
}

/// `tailscale` on PATH.
pub fn program() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join("tailscale")).find(|p| p.is_file())
}

/// A running funnel; dropping it stops it.
pub struct Funnel {
    stop: Option<oneshot::Sender<()>>,
}

impl Drop for Funnel {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

/// What a line of `tailscale funnel`'s output says, if anything.
fn read_line(line: &str) -> Option<Status> {
    let url = line.split_whitespace().find(|w| w.starts_with("https://")).map(|u| u.trim_end_matches(['/', '.', ',']).to_owned());
    let lower = line.to_lowercase();
    if lower.contains("access denied") || lower.contains("--operator") {
        return Some(Status::Failed(
            "Tailscale didn't let harlo share its port. Allow it once in a terminal: sudo tailscale set --operator=$USER".to_owned(),
        ));
    }
    match url {
        Some(u) if u.contains("login.tailscale.com") || lower.contains("enable") => Some(Status::NeedsApproval(u)),
        Some(u) if u.contains(".ts.net") => Some(Status::Online(u)),
        _ => None,
    }
}

/// Runs `program funnel` for `port` until dropped, restarting it (with
/// backoff) when it stops. `status` hears every change.
pub fn start(program: PathBuf, port: u16, status: impl Fn(Status) + Send + Sync + 'static) -> Funnel {
    let (stop, mut stopped) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let mut backoff = Duration::from_secs(3);
        loop {
            status(Status::Starting);
            let run = run_once(&program, port, &status);
            let (why, was_online) = tokio::select! {
                r = run => r,
                _ = &mut stopped => return,
            };
            if was_online {
                backoff = Duration::from_secs(3);
            }
            status(Status::Failed(why));
            tokio::select! {
                _ = tokio::time::sleep(backoff) => {}
                _ = &mut stopped => return,
            }
            backoff = (backoff * 2).min(Duration::from_secs(60));
        }
    });
    Funnel { stop: Some(stop) }
}

/// One run of `tailscale funnel`: why it ended, and whether it was online.
async fn run_once(program: &Path, port: u16, status: &(impl Fn(Status) + Sync)) -> (String, bool) {
    let mut command = tokio::process::Command::new(program);
    command
        .args(["funnel", &format!("--https={HTTPS_PORT}"), &format!("http://127.0.0.1:{port}")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // If harlo goes without stopping it (a crash, a kill), the address
    // still comes down: the kernel ends it with harlo.
    // SAFETY: prctl is async-signal-safe, the only thing run after fork.
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    let child = command.spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return (format!("Couldn't run tailscale: {e}"), false),
    };
    let (lines_tx, mut lines) = mpsc::unbounded_channel::<String>();
    for pipe in [child.stdout.take().map(|p| Box::new(p) as Box<dyn tokio::io::AsyncRead + Send + Unpin>), child.stderr.take().map(|p| Box::new(p) as _)]
        .into_iter()
        .flatten()
    {
        let tx = lines_tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(pipe).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    drop(lines_tx);
    let mut online = false;
    let mut said = Vec::<String>::new();
    let mut failure = None;
    while let Some(line) = lines.recv().await {
        let line = line.trim().to_owned();
        if line.is_empty() {
            continue;
        }
        match read_line(&line) {
            Some(Status::Online(url)) if !online => {
                online = true;
                status(Status::Online(url));
            }
            Some(Status::Failed(why)) => failure = Some(why),
            Some(s @ Status::NeedsApproval(_)) => status(s),
            _ => {}
        }
        said.push(line);
        if said.len() > 6 {
            said.remove(0);
        }
    }
    let code = child.wait().await.ok().and_then(|s| s.code());
    let why = failure.unwrap_or_else(|| {
        let last = said.iter().rev().find(|l| !l.starts_with("https://")).cloned().unwrap_or_default();
        match code {
            Some(c) if !last.is_empty() => format!("Tailscale Funnel stopped ({c}): {last}"),
            Some(c) => format!("Tailscale Funnel stopped ({c})."),
            None => "Tailscale Funnel stopped.".to_owned(),
        }
    });
    (why, online)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tailscales_output() {
        assert_eq!(read_line("https://desk.tail1234.ts.net:8443/"), Some(Status::Online("https://desk.tail1234.ts.net:8443".into())));
        assert_eq!(read_line("Available on the internet:"), None);
        assert_eq!(read_line("|-- proxy http://127.0.0.1:7676"), None);
        assert_eq!(
            read_line("Funnel is not enabled on your tailnet. To enable, visit: https://login.tailscale.com/f/funnel?node=abc"),
            Some(Status::NeedsApproval("https://login.tailscale.com/f/funnel?node=abc".into()))
        );
        assert!(matches!(read_line("Access denied: serve config denied"), Some(Status::Failed(_))));
    }

    #[tokio::test]
    async fn runs_and_stops_tailscale() {
        let dir = std::env::temp_dir().join(format!("harlo-funnel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("tailscale");
        let args = dir.join("args");
        let script = format!(
            "#!/bin/sh\necho \"$@\" > '{}'\necho 'Available on the internet:'\necho\necho 'https://desk.tail1234.ts.net:8443/'\necho '|-- proxy http://127.0.0.1:7676'\nexec sleep 30\n",
            args.display()
        );
        std::fs::write(&fake, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let funnel = start(fake, 7676, move |s| {
            let _ = tx.send(s);
        });
        let mut next = async || tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.unwrap().unwrap();
        assert_eq!(next().await, Status::Starting);
        assert_eq!(next().await, Status::Online("https://desk.tail1234.ts.net:8443".into()));
        assert_eq!(std::fs::read_to_string(&args).unwrap().trim(), "funnel --https=8443 http://127.0.0.1:7676");
        // Dropped: the process goes too.
        drop(funnel);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let running = std::process::Command::new("pgrep").args(["-f", &format!("{}/tailscale", dir.display())]).output().unwrap();
        assert!(running.stdout.is_empty(), "still running");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
