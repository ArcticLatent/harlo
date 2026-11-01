//! pi packages (Settings → Providers → Pi, and a project's settings): what
//! pi has installed, from `pi list`, with what each one's package.json
//! says (its version, description, and what it adds to pi), and
//! installing, updating and removing one with pi's own commands (`pi
//! install`, `pi update`, `pi remove`; `-l` for a project's own).
//! Changes reach pi sessions started afterwards.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// An installed package.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Package {
    /// What it was installed from (`npm:pi-web-access`, `git:…`, a folder).
    pub source: String,
    pub path: PathBuf,
    /// A project's own (its `.pi/settings.json`), not yours.
    pub project: bool,
    pub version: String,
    pub description: String,
    /// What it adds to pi: "extensions", "skills", "prompts", "themes".
    pub adds: Vec<String>,
}

/// `pi list`'s packages: a heading per scope ("User packages:", "Project
/// packages:"), each source indented, its folder under it.
pub fn parse_list(text: &str) -> Vec<Package> {
    let mut out: Vec<Package> = Vec::new();
    let mut project = false;
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if indent == 0 {
            project = t.to_lowercase().starts_with("project");
            continue;
        }
        match out.last_mut() {
            // Its folder, under it.
            Some(last) if indent > 2 && last.path.as_os_str().is_empty() => last.path = PathBuf::from(t),
            _ if t.ends_with(':') => {}
            _ => out.push(Package { source: t.to_owned(), project, ..Default::default() }),
        }
    }
    out
}

/// Fills in what the package's own package.json says.
pub fn describe(p: &mut Package) {
    let v: Value = std::fs::read_to_string(p.path.join("package.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    p.version = v["version"].as_str().unwrap_or_default().to_owned();
    p.description = v["description"].as_str().unwrap_or_default().trim().to_owned();
    // Its manifest (`"pi": { "extensions": [...] }`), else the usual folders.
    p.adds = ["extensions", "skills", "prompts", "themes"]
        .into_iter()
        .filter(|k| match v["pi"].get(*k) {
            Some(list) => list.as_array().is_some_and(|a| !a.is_empty()),
            None => v["pi"].is_null() && p.path.join(k).is_dir(),
        })
        .map(str::to_owned)
        .collect();
}

/// Runs pi in `cwd` (blocking): its output, or why it failed (the end of
/// what it said).
pub fn run(pi: &Path, cwd: &Path, args: &[&str], limit: Duration) -> Result<String, String> {
    let mut child = Command::new(pi)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", pi.display()))?;
    // Read both as they come (npm says a lot).
    let read = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = r.read_to_string(&mut s);
            s
        })
    };
    let out = read(Box::new(child.stdout.take().expect("piped")));
    let err = read(Box::new(child.stderr.take().expect("piped")));
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("pi didn't finish in {} s.", limit.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let (out, err) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    if status.success() {
        return Ok(out);
    }
    let said = crate::signin::plain(&format!("{err}\n{out}"));
    let tail: Vec<&str> = said.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let said: String = tail[tail.len().saturating_sub(4)..].join("\n").chars().take(600).collect();
    Err(if said.is_empty() { "pi failed.".into() } else { said })
}

/// The packages installed for you, and (in a project) the project's own.
pub fn list(pi: &Path, cwd: &Path) -> Result<Vec<Package>, String> {
    let text = run(pi, cwd, &["list"], Duration::from_secs(30))?;
    let mut list = parse_list(&text);
    for p in &mut list {
        describe(p);
    }
    Ok(list)
}

/// `pi install|update|remove <source>` (`all`: every package), yours or
/// (`project`) the project's own in `cwd`.
pub fn change(pi: &Path, cwd: &Path, action: &str, source: &str, project: bool) -> Result<(), String> {
    let source = source.trim();
    let mut args: Vec<&str> = match action {
        "install" | "remove" if !source.is_empty() => vec![action, source],
        "update" if source == "all" => vec!["update", "--extensions"],
        "update" if !source.is_empty() => vec!["update", "--extension", source],
        _ => return Err("Nothing to do.".into()),
    };
    if project {
        // A project's own: in its settings, trusting its files for this.
        args.extend(["-l", "--approve"]);
    }
    run(pi, cwd, &args, Duration::from_secs(600)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pis_list() {
        let text = "User packages:\n  npm:pi-web-access\n    /home/u/.pi/agent/npm/node_modules/pi-web-access\n  git:github.com/a/b\n    /home/u/.pi/agent/git/github.com/a/b\nProject packages:\n  ./tools/pkg\n    /w/tools/pkg\n";
        let list = parse_list(text);
        let short: Vec<(&str, &str, bool)> = list.iter().map(|p| (p.source.as_str(), p.path.to_str().unwrap(), p.project)).collect();
        assert_eq!(short, [
            ("npm:pi-web-access", "/home/u/.pi/agent/npm/node_modules/pi-web-access", false),
            ("git:github.com/a/b", "/home/u/.pi/agent/git/github.com/a/b", false),
            ("./tools/pkg", "/w/tools/pkg", true),
        ]);
        assert!(parse_list("No packages installed.\n").is_empty());
    }

    #[test]
    fn says_what_a_package_adds() {
        let dir = std::env::temp_dir().join(format!("harlo-pi-packages-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/skills")).unwrap();
        std::fs::create_dir_all(dir.join("b/skills")).unwrap();
        std::fs::write(dir.join("a/package.json"), r#"{"version":"1.2.0","description":" Web search ","pi":{"extensions":["./dist"],"skills":[]}}"#).unwrap();
        std::fs::write(dir.join("b/package.json"), r#"{"version":"0.1.0"}"#).unwrap();
        let mut a = Package { path: dir.join("a"), ..Default::default() };
        describe(&mut a);
        assert_eq!((a.version.as_str(), a.description.as_str(), a.adds.clone()), ("1.2.0", "Web search", vec!["extensions".to_owned()]));
        // No manifest: its folders.
        let mut b = Package { path: dir.join("b"), ..Default::default() };
        describe(&mut b);
        assert_eq!(b.adds, ["skills"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runs_and_reports_failures() {
        let sh = Path::new("/bin/sh");
        assert_eq!(run(sh, Path::new("/"), &["-c", "echo hi"], Duration::from_secs(5)).unwrap(), "hi\n");
        assert_eq!(run(sh, Path::new("/"), &["-c", "echo working; echo 'npm error 404 Not Found' >&2; exit 1"], Duration::from_secs(5)).unwrap_err(), "npm error 404 Not Found\nworking");
        assert!(run(sh, Path::new("/"), &["-c", "sleep 5"], Duration::from_millis(300)).unwrap_err().contains("didn't finish"));
        assert!(change(sh, Path::new("/"), "install", " ", false).is_err());
    }
}
