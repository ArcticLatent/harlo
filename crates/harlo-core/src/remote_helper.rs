//! A program of harlo's put on another machine: harlo's own build of it for
//! that machine's kind (x86_64 or aarch64), stripped and gzipped once, sent
//! over SSH when that build isn't there yet, and kept in harlo's cache
//! there (`~/.cache/harlo/<name>-<build>`; older builds go). harlo-engines
//! (model servers there) and harlo-agent (Harlo's sessions there) go so.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Result, bail};

use crate::remote::{self, Host};

/// The machines harlo's helpers run on.
pub const ARCHES: [&str; 2] = ["x86_64", "aarch64"];

pub struct Helper {
    /// Its name, and its name on the machine (`<name>-<build>`).
    pub name: &'static str,
    /// Static builds only (`<name>-<arch>`, or a build's
    /// `target/<arch>-unknown-linux-musl/…`), never this computer's own: it
    /// may need a newer C library than the machine has.
    pub static_only: bool,
    /// What it's called in errors ("engines helper").
    pub what: &'static str,
    /// The arguments it's checked with once it's there.
    pub check: &'static str,
    packed: [Packed; 2],
}

/// A build as it goes over, and its name (made once).
type Packed = OnceLock<std::result::Result<(Vec<u8>, String), String>>;

impl Helper {
    pub const fn new(name: &'static str, static_only: bool, what: &'static str, check: &'static str) -> Self {
        Helper { name, static_only, what, check, packed: [OnceLock::new(), OnceLock::new()] }
    }

    /// Where it might be for machines of `arch`, beside harlo's program in
    /// `exe_dir`: there (a build), in `lib/harlo` next to its folder (an
    /// install), or for a static one, a build's musl folder.
    pub fn candidates(&self, exe_dir: &Path, arch: &str) -> Vec<PathBuf> {
        let name = if !self.static_only && arch == std::env::consts::ARCH { self.name.to_owned() } else { format!("{}-{arch}", self.name) };
        let mut out = vec![exe_dir.join(&name), exe_dir.join("../lib/harlo").join(&name)];
        if self.static_only {
            // (A test's program is in the build's deps folder.)
            let build = if exe_dir.ends_with("deps") { exe_dir.join("..") } else { exe_dir.to_owned() };
            let target = build.join(format!("../{arch}-unknown-linux-musl"));
            out.extend(["agent", "release", "debug"].map(|p| target.join(p).join(self.name)));
        }
        out
    }

    /// This harlo's build of it for `arch`, if it has one.
    pub fn local(&self, arch: &str) -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        self.candidates(exe.parent()?, arch).into_iter().find(|p| p.is_file())
    }

    /// The build for `arch` as it goes over (stripped of its debugging data
    /// when `strip` is here, gzipped), and a name for this build of it.
    fn packed(&self, arch: &str) -> Result<&(Vec<u8>, String)> {
        let i = ARCHES.iter().position(|a| *a == arch).ok_or_else(|| anyhow::anyhow!("harlo's {} runs on x86_64 and ARM (aarch64) machines, not {arch} ones.", self.what))?;
        self.packed[i]
            .get_or_init(|| {
                let helper = self.local(arch).ok_or_else(|| {
                    if !self.static_only && arch == std::env::consts::ARCH {
                        format!("{} isn't next to harlo's program: reinstall harlo", self.name)
                    } else {
                        format!("This harlo has no {} for {arch} machines (harlo's releases have one).", self.what)
                    }
                })?;
                let tmp = std::env::temp_dir().join(format!("{}-{arch}-{}", self.name, std::process::id()));
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

    /// Whether some build of it is on `host` (harlo has put it there before).
    pub fn there(&self, host: &Host) -> Result<bool> {
        let out = remote::run(host, &format!("ls \"$HOME\"/.cache/harlo/{}-* >/dev/null 2>&1 && echo yes || echo no", self.name), Duration::from_secs(30))?;
        Ok(String::from_utf8_lossy(&out.stdout).trim() == "yes")
    }

    /// It on `host`, put there first if this build of it isn't: its path
    /// there.
    pub fn on(&self, host: &Host) -> Result<String> {
        let arch = arch_of(host)?;
        let (gz, id) = self.packed(&arch).map_err(|e| anyhow::anyhow!("{} is an {arch} machine: {e:#}", host.name))?;
        let file = format!("{}-{id}", self.name);
        let out = remote::run(host, &format!("printf '%s\\n' \"$HOME\"; [ -x \"$HOME\"/.cache/harlo/{file} ] && echo ready || echo missing"), Duration::from_secs(30))?;
        let said = String::from_utf8_lossy(&out.stdout).into_owned();
        let mut lines = said.lines();
        let (home, state) = (lines.next().unwrap_or_default().trim().to_owned(), lines.next().unwrap_or_default().trim().to_owned());
        if home.is_empty() {
            bail!("{} didn't say where its home folder is.", host.name);
        }
        if state != "ready" {
            let out = remote::run_with_input(host, &put_script(self.name, &file, self.check), Some(gz), Duration::from_secs(300))?;
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                bail!("harlo's {} doesn't run on {}: {}", self.what, host.name, err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("it failed").trim());
            }
        }
        Ok(format!("{home}/.cache/harlo/{file}"))
    }
}

/// The script that puts a build there: its input (gzipped) into a file of
/// its own first (two sessions starting at once don't share one), then in
/// place, then the other builds go (not ones being put there now), then it's
/// run once.
fn put_script(name: &str, file: &str, check: &str) -> String {
    format!(
        r#"d="$HOME/.cache/harlo"; p="$d/{file}"; t="$p.tmp.$$"; mkdir -p "$d" && gzip -dc > "$t" && chmod 755 "$t" && mv -f "$t" "$p"; rm -f "$t"; for f in "$d"/{name}-*; do case "$f" in "$p"|*.tmp.*) ;; *) rm -f "$f";; esac; done; find "$d" -maxdepth 1 -name '{name}-*.tmp.*' -mmin +10 -exec rm -f {{}} + 2>/dev/null; "$p" {check}"#
    )
}

/// What kind of machine `host` is (`uname -m`, aarch64 for arm64), asked
/// once (or known from its check).
pub fn arch_of(host: &Host) -> Result<String> {
    static SEEN: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());
    if let Some((_, a)) = SEEN.lock().unwrap().iter().find(|(h, _)| *h == host.name) {
        return Ok(a.clone());
    }
    let arch = match remote::info_of(&host.name).map(|i| i.arch).filter(|a| !a.is_empty()) {
        Some(a) => a,
        None => normal_arch(String::from_utf8_lossy(&remote::run(host, "uname -m", Duration::from_secs(30))?.stdout).trim()),
    };
    SEEN.lock().unwrap().push((host.name.clone(), arch.clone()));
    Ok(arch)
}

/// `uname -m` as harlo names machines.
pub fn normal_arch(a: &str) -> String {
    match a {
        "arm64" => "aarch64".to_owned(),
        "amd64" => "x86_64".to_owned(),
        a => a.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_a_helper_is_looked_for() {
        let engines = Helper::new("harlo-engines", false, "engines helper", "version");
        let agent = Helper::new("harlo-agent", true, "agent", "--version");
        let dir = Path::new("/opt/harlo/bin");
        let here = std::env::consts::ARCH;
        let other = if here == "x86_64" { "aarch64" } else { "x86_64" };
        // This computer's kind: its own build; another: `-<arch>`.
        assert_eq!(engines.candidates(dir, here), [dir.join("harlo-engines"), dir.join("../lib/harlo/harlo-engines")]);
        assert_eq!(engines.candidates(dir, other)[1], dir.join(format!("../lib/harlo/harlo-engines-{other}")));
        // The agent: always a static one, also from a build's musl folder.
        let c = agent.candidates(Path::new("/w/target/debug"), here);
        assert_eq!(c[0], Path::new("/w/target/debug").join(format!("harlo-agent-{here}")));
        assert_eq!(c[1], Path::new("/w/target/debug/../lib/harlo").join(format!("harlo-agent-{here}")));
        assert_eq!(c[2], Path::new(&format!("/w/target/debug/../{here}-unknown-linux-musl/agent/harlo-agent")));
        assert_eq!(c.len(), 5);
        assert_eq!(normal_arch("arm64"), "aarch64");
    }

    #[test]
    fn putting_it_there_keeps_others_being_put() {
        let s = put_script("harlo-agent", "harlo-agent-00ff", "--version");
        assert!(s.contains(r#"t="$p.tmp.$$""#) && s.contains(r#"*.tmp.*) ;;"#) && s.ends_with(r#""$p" --version"#), "{s}");
        // A POSIX shell takes it.
        let ok = std::process::Command::new("sh").args(["-n", "-c", &s]).status().unwrap();
        assert!(ok.success());
    }
}
