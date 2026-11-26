//! The sandbox the `shell` tool's commands run in: the kernel's Landlock.
//!
//! A confined command reads and runs what is on the computer, except the
//! places that hold secrets (keys, sign-ins, browser profiles, the host's
//! own files). It changes the project, the temporary folders and tool
//! caches, and nothing else. The kernel holds the command and everything
//! it starts to that, however a path is spelled.
//!
//! Landlock only grants: no rule takes a place away again. So a hidden
//! place is left out by granting what lies beside it, at each level down
//! to it ([`grants`]). Two things follow. What is made later directly in
//! one of those levels (a new file right in the home folder) can't be
//! read by a command already running. And nothing is hidden inside a
//! folder commands may change, the temporary folders above all.
//!
//! The rules are put on by a helper: the host's own program, started with
//! [`HELPER`] as its first argument, restricts itself and then becomes the
//! command ([`enter`]). Nothing runs between `fork` and `exec` in the
//! host, and a command is confined from its first instruction. A program
//! that gains rights when started (`sudo`) doesn't gain them inside.
//!
//! Not confined: the network, and what a command reaches by talking to a
//! service of the user's (the session bus, and through it the keyring).
//! Sockets in hidden places can't be connected to on kernels that check
//! it (Landlock 9); on older ones only files are held back.

// Rust guideline compliant 2026-10-09

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// The first argument that makes the host's program confine itself and
/// run the command that follows.
pub const HELPER: &str = "confine";

/// Before a folder commands may change.
const WRITE: &str = "--write";
/// Before a place commands can't read.
const HIDE: &str = "--hide";
/// Before a place inside a hidden one that commands may read.
const SHOW: &str = "--show";
/// Before the command.
const END: &str = "--";

/// The exit code of a command that couldn't be started, as shells use it.
const NOT_STARTED: u8 = 126;

/// Under the home folder: what holds keys, credentials and sign-ins, and
/// browser profiles.
const HIDDEN_IN_HOME: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".azure",
    ".kube",
    ".docker",
    ".netrc",
    ".git-credentials",
    ".password-store",
    ".npmrc",
    ".pypirc",
    ".cargo/credentials",
    ".cargo/credentials.toml",
    ".mozilla",
    ".thunderbird",
    ".config/google-chrome",
    ".config/chromium",
    ".config/BraveSoftware",
    ".config/gcloud",
    ".config/gh",
    ".config/hub",
    ".config/op",
    ".local/share/keyrings",
    // Other coding agents' sign-ins and sessions.
    ".claude",
    ".claude.json",
    ".codex",
    ".local/share/opencode",
];

/// Under the user's runtime folder: sockets that let a command act as the
/// user (their keys, their containers).
const HIDDEN_IN_RUNTIME: &[&str] = &[
    "keyring",
    "gnupg",
    "gcr",
    "ssh-agent.socket",
    "docker.sock",
    "podman",
];

/// Elsewhere: the Docker daemon's socket, as good as root.
const HIDDEN_ELSEWHERE: &[&str] = &["/run/docker.sock"];

/// Where the device files are: `/dev/null` and the like are written to.
const DEVICES: &str = "/dev";

/// The folders anything may write to.
const TEMPORARY: &[&str] = &["/tmp", "/var/tmp", "/dev/shm"];

/// Under the home folder: the caches build tools write. Of Cargo's
/// folder only what it fills, since its sign-in is kept beside them.
const CACHES: &[&str] = &[
    ".cache",
    ".cargo/registry",
    ".cargo/git",
    ".cargo/bin",
    ".cargo/.package-cache",
    ".cargo/.package-cache-mutate",
    ".cargo/.global-cache",
    ".cargo/.crates.toml",
    ".cargo/.crates2.json",
    ".rustup",
    ".npm",
    ".yarn",
    ".local/share/pnpm",
    ".local/share/uv",
    "go",
    ".m2",
    ".gradle",
    ".nuget",
    ".bun",
    ".deno",
];

/// Caches made before a command that names their tool runs (any command,
/// for an empty name). Only a folder that exists can be opened to a
/// command, so a tool's first use would fail and send the agent looking
/// for another way.
const MADE: &[(&str, &str)] = &[
    (".cache", ""),
    (".npm", "npm"),
    (".yarn", "yarn"),
    (".local/share/pnpm", "pnpm"),
    (".local/share/uv", "uv"),
    (".m2", "mvn"),
    (".gradle", "gradle"),
    (".nuget", "dotnet"),
    (".bun", "bun"),
];

/// Variables that name caches kept elsewhere.
const CACHE_VARS: &[&str] = &[
    "XDG_CACHE_HOME",
    "CARGO_TARGET_DIR",
    "RUSTUP_HOME",
    "GOPATH",
    "GOMODCACHE",
    "GOCACHE",
    "npm_config_cache",
    "PNPM_HOME",
    "YARN_CACHE_FOLDER",
    "UV_CACHE_DIR",
    "UV_TOOL_DIR",
    "UV_PYTHON_INSTALL_DIR",
    "PIP_CACHE_DIR",
    "GRADLE_USER_HOME",
    "BUN_INSTALL",
    "DENO_DIR",
];

/// How a turn's commands are confined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sandbox {
    /// The program that confines itself and becomes the command when its
    /// first argument is [`HELPER`]: the host's own, which calls [`enter`].
    pub helper: PathBuf,
    /// More folders commands may change.
    pub writable: Vec<PathBuf>,
    /// More places commands can't read: the host's own files.
    pub hidden: Vec<PathBuf>,
    /// Places commands may read though they lie in hidden ones.
    pub shown: Vec<PathBuf>,
}

/// What a confined command reaches, as real paths of things that exist.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reach {
    /// The folders it may change.
    pub write: Vec<PathBuf>,
    /// The places it can't read.
    pub hide: Vec<PathBuf>,
    /// The places it may read though they lie in hidden ones.
    pub show: Vec<PathBuf>,
}

/// A command's confinement, ready to start it with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confined {
    helper: PathBuf,
    reach: Reach,
}

impl Confined {
    /// A confinement to `reach`, put on by the program `helper`.
    #[must_use]
    pub fn new(helper: PathBuf, reach: Reach) -> Self {
        Self { helper, reach }
    }

    /// The program to start, and the arguments that go before the
    /// command's own program and arguments.
    pub(crate) fn wrapper(&self) -> (&Path, Vec<OsString>) {
        let mut args = vec![OsString::from(HELPER)];
        for (flag, paths) in [
            (WRITE, &self.reach.write),
            (HIDE, &self.reach.hide),
            (SHOW, &self.reach.show),
        ] {
            for path in paths {
                args.push(flag.into());
                args.push(path.clone().into_os_string());
            }
        }
        args.push(END.into());
        (&self.helper, args)
    }
}

/// Where the folders a sandbox opens and hides are looked for.
struct Places<'a> {
    /// The home folder.
    home: Option<PathBuf>,
    /// The process's variables.
    var: &'a dyn Fn(&str) -> Option<OsString>,
}

impl Sandbox {
    /// The confinement of `command`, run in `cwd`.
    ///
    /// Makes the cache folder of a tool the command names, when it isn't
    /// there yet.
    #[must_use]
    pub fn confine(&self, cwd: &Path, command: &str) -> Confined {
        let places = Places {
            home: std::env::home_dir(),
            var: &|name| std::env::var_os(name),
        };
        Confined::new(self.helper.clone(), self.reach(&places, cwd, command))
    }

    /// What `command`, run in `cwd`, reaches.
    fn reach(&self, places: &Places<'_>, cwd: &Path, command: &str) -> Reach {
        let home = places.home.as_deref();
        let mut write = Vec::new();
        if project_changes(home, cwd) {
            write.extend(repository(cwd));
        }
        write.extend(TEMPORARY.iter().map(PathBuf::from));
        if let Some(home) = home {
            for (folder, tool) in MADE {
                if command.contains(tool) {
                    // (One that can't be made stays closed, as before.)
                    let _ = std::fs::create_dir_all(home.join(folder));
                }
            }
            write.extend(CACHES.iter().map(|folder| home.join(folder)));
        }
        let named = CACHE_VARS.iter().filter_map(|name| (places.var)(name));
        write.extend(named.map(PathBuf::from).filter(|path| path.is_absolute()));
        write.extend(self.writable.iter().cloned());

        let mut hide: Vec<PathBuf> = HIDDEN_ELSEWHERE.iter().map(PathBuf::from).collect();
        if let Some(home) = home {
            hide.extend(HIDDEN_IN_HOME.iter().map(|place| home.join(place)));
        }
        if let Some(runtime) = (places.var)("XDG_RUNTIME_DIR").map(PathBuf::from) {
            hide.extend(HIDDEN_IN_RUNTIME.iter().map(|place| runtime.join(place)));
        }
        hide.extend((places.var)("SSH_AUTH_SOCK").map(PathBuf::from));
        hide.extend(self.hidden.iter().cloned());
        Reach {
            write: real(write),
            hide: real(hide),
            show: real(self.shown.clone()),
        }
    }
}

/// What an agent is told about its commands' sandbox, for the system
/// prompt of a session in `cwd`.
#[must_use]
pub fn told(cwd: &Path) -> String {
    let home = std::env::home_dir();
    let changeable = if project_changes(home.as_deref(), cwd) {
        format!("{}, /tmp and tool caches", project(cwd).display())
    } else {
        "/tmp and tool caches (this session works in the home folder or above it, which \
         commands only read)"
            .to_owned()
    };
    format!(
        "Shell commands run in a sandbox: they can change only {changeable}, and can't read \
         places that hold secrets (SSH keys, credentials, sign-ins, browser profiles). If a \
         command fails because of that (say, `git push` needs SSH keys), run it again with \
         outside_sandbox: true; the user is asked first."
    )
}

/// Whether commands may change the project `cwd` is in: not when it is
/// the home folder or holds it, where everything of the user's lives.
fn project_changes(home: Option<&Path>, cwd: &Path) -> bool {
    let project = crate::tools::real_path(&project(cwd));
    !home.is_some_and(|home| crate::tools::real_path(home).starts_with(&project))
}

/// The project `cwd` is in: its repository's top folder, or `cwd`.
fn project(cwd: &Path) -> PathBuf {
    cwd.ancestors()
        .find(|folder| folder.join(".git").exists())
        .unwrap_or(cwd)
        .to_path_buf()
}

/// The project `cwd` is in and, for a linked worktree, where git keeps
/// what that worktree commits to: its own folder in the main repository,
/// and that repository's shared one.
fn repository(cwd: &Path) -> Vec<PathBuf> {
    let project = project(cwd);
    let mut folders = vec![project.clone()];
    // A worktree's `.git` is a file that names its folder.
    let named = std::fs::read_to_string(project.join(".git")).ok();
    let own = named
        .as_deref()
        .and_then(|text| text.trim().strip_prefix("gitdir:"))
        .map(|path| project.join(path.trim()));
    if let Some(own) = own {
        if let Ok(shared) = std::fs::read_to_string(own.join("commondir")) {
            folders.push(own.join(shared.trim()));
        }
        folders.push(own);
    }
    folders
}

/// The real paths of those of `paths` that exist, each once.
fn real(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for path in paths {
        if let Ok(path) = std::fs::canonicalize(&path)
            && !out.contains(&path)
        {
            out.push(path);
        }
    }
    out
}

/// What a command may do with a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum May {
    /// Nothing: it's hidden.
    Nothing,
    /// Read and run.
    Read,
    /// Read, run and write to what is there: the device files.
    Use,
    /// Everything.
    Change,
}

impl Reach {
    /// What a command may do with `path`.
    ///
    /// Inside a folder it may change, everything. Else as the nearest
    /// place above says: nothing in a hidden one, reading in one shown
    /// inside it, and reading anywhere else.
    fn may(&self, path: &Path) -> May {
        let within = |places: &[PathBuf]| {
            places
                .iter()
                .filter(|place| path.starts_with(place))
                .map(|place| place.components().count())
                .max()
        };
        if within(&self.write).is_some() {
            return May::Change;
        }
        match (within(&self.hide), within(&self.show)) {
            (Some(hidden), shown) if shown.is_none_or(|shown| shown < hidden) => May::Nothing,
            _ if path.starts_with(DEVICES) => May::Use,
            _ => May::Read,
        }
    }

    /// Whether something hidden lies somewhere inside `folder`.
    fn hides_in(&self, folder: &Path) -> bool {
        self.hide
            .iter()
            .any(|place| place != folder && place.starts_with(folder))
    }
}

/// The places to open to a command with `reach`, and what it may do in
/// each, there and in everything beneath.
///
/// A folder with nothing hidden inside is opened whole. One that holds a
/// hidden place isn't opened itself: each thing in it is, except the
/// hidden one, and so on down. Links aren't followed: what they lead to
/// is opened, or not, where it really is.
///
/// A folder the command may change is opened whole whatever lies in it:
/// the kernel has no way to let new things be made in a folder and keep
/// one of its things closed.
#[must_use]
fn grants(reach: &Reach) -> Vec<(PathBuf, May)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    open(reach, Path::new("/"), &mut out, &mut seen);
    // What is opened further than its surroundings: the device files,
    // folders that may be changed, and what is shown inside a hidden place.
    let devices = PathBuf::from(DEVICES);
    for place in [&devices]
        .into_iter()
        .chain(&reach.write)
        .chain(&reach.show)
    {
        open(reach, place, &mut out, &mut seen);
    }
    out
}

/// Opens `path` to a command: whole, or thing by thing around what is
/// hidden in it.
fn open(reach: &Reach, path: &Path, out: &mut Vec<(PathBuf, May)>, seen: &mut HashSet<PathBuf>) {
    let may = reach.may(path);
    if may == May::Nothing || !seen.insert(path.to_path_buf()) {
        return;
    }
    if may == May::Change || !reach.hides_in(path) {
        out.push((path.to_path_buf(), may));
        return;
    }
    // (A folder that can't be listed stays closed.)
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| !kind.is_symlink()) {
            open(reach, &entry.path(), out, seen);
        }
    }
}

/// Whether this system can confine commands.
#[must_use]
pub fn supported() -> bool {
    landlocked::supported()
}

/// Confines this process as `args` say and becomes the command they end
/// with; `args` are what followed [`HELPER`] on the command line.
///
/// Returns only when that failed, with the exit code to end with; why is
/// printed for whoever reads the command's output.
#[must_use]
pub fn enter(args: impl IntoIterator<Item = OsString>) -> u8 {
    use std::os::unix::process::CommandExt as _;
    let Some((reach, command)) = parse(args) else {
        eprintln!("Harlo's sandbox was started without a command.");
        return NOT_STARTED;
    };
    if let Err(why) = landlocked::restrict(&grants(&reach)) {
        eprintln!("Harlo's sandbox couldn't start, so the command didn't run: {why}");
        return NOT_STARTED;
    }
    let Some((program, rest)) = command.split_first() else {
        return NOT_STARTED;
    };
    let failed = std::process::Command::new(program).args(rest).exec();
    eprintln!("Couldn't start {}: {failed}", program.display());
    NOT_STARTED
}

/// Reads the helper's arguments: what the command reaches, and the
/// command; none when no command follows.
fn parse(args: impl IntoIterator<Item = OsString>) -> Option<(Reach, Vec<OsString>)> {
    let mut reach = Reach::default();
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        if flag == OsStr::new(END) {
            let command: Vec<OsString> = args.collect();
            return (!command.is_empty()).then_some((reach, command));
        }
        let places = match flag.to_str()? {
            WRITE => &mut reach.write,
            HIDE => &mut reach.hide,
            SHOW => &mut reach.show,
            _ => return None,
        };
        places.push(PathBuf::from(args.next()?));
    }
    None
}

#[cfg(target_os = "linux")]
mod landlocked {
    //! The rules, given to the kernel.

    use std::path::PathBuf;

    use landlock::{
        ABI, Access as _, AccessFs, BitFlags, CompatLevel, Compatible as _, PathBeneath, PathFd,
        Ruleset, RulesetAttr as _, RulesetCreatedAttr as _,
    };

    use super::May;

    /// The newest Landlock this was written for. On an older kernel the
    /// library leaves out what that kernel doesn't know, and the rest
    /// holds.
    const NEWEST: ABI = ABI::V10;

    /// What `may` comes to, in the kernel's terms.
    fn rights(may: May) -> BitFlags<AccessFs> {
        let read = AccessFs::Execute | AccessFs::ReadFile | AccessFs::ResolveUnix;
        match may {
            May::Nothing => BitFlags::empty(),
            May::Read => read,
            May::Use => read | AccessFs::WriteFile | AccessFs::Truncate | AccessFs::IoctlDev,
            May::Change => AccessFs::from_all(NEWEST),
        }
    }

    /// Whether the kernel has Landlock, and it's on.
    pub(super) fn supported() -> bool {
        Ruleset::default()
            .set_compatibility(CompatLevel::HardRequirement)
            .handle_access(AccessFs::Execute)
            .and_then(Ruleset::create)
            .is_ok()
    }

    /// Holds this process, and what it starts, to `grants`.
    ///
    /// On a system without Landlock nothing is held back and that is no
    /// error: the host asks [`supported`] before it promises a sandbox.
    pub(super) fn restrict(grants: &[(PathBuf, May)]) -> Result<(), String> {
        let mut rules = Ruleset::default()
            .handle_access(AccessFs::from_all(NEWEST))
            .and_then(Ruleset::create)
            .map_err(|e| e.to_string())?;
        // Every folder can be listed: names aren't the secret, and a
        // folder that only lets some of its things be read still lists.
        let everywhere = PathFd::new("/").map_err(|e| e.to_string())?;
        (&mut rules)
            .add_rule(PathBeneath::new(everywhere, AccessFs::ReadDir))
            .map_err(|e| e.to_string())?;
        for (path, may) in grants {
            // (What vanished meanwhile, or can't be opened, stays closed.)
            let Ok(place) = PathFd::new(path) else {
                continue;
            };
            let mut rights = rights(*may);
            if !path.is_dir() {
                rights &= AccessFs::from_file(NEWEST);
            }
            (&mut rules)
                .add_rule(PathBeneath::new(place, rights))
                .map_err(|e| e.to_string())?;
        }
        rules.restrict_self().map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
mod landlocked {
    //! No sandbox off Linux: commands run as they did.

    use std::path::PathBuf;

    use super::May;

    pub(super) fn supported() -> bool {
        false
    }

    pub(super) fn restrict(_grants: &[(PathBuf, May)]) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder with `things` in it: folders (ending in `/`) and
    /// files.
    fn scratch(things: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a scratch folder");
        for thing in things {
            let path = dir.path().join(thing);
            if thing.ends_with('/') {
                std::fs::create_dir_all(&path).expect("made");
            } else {
                std::fs::create_dir_all(path.parent().expect("a parent")).expect("made");
                std::fs::write(&path, "x").expect("written");
            }
        }
        dir
    }

    fn real_root(dir: &tempfile::TempDir) -> PathBuf {
        std::fs::canonicalize(dir.path()).expect("real")
    }

    #[test]
    fn a_hidden_place_is_left_out_by_opening_what_is_beside_it() {
        let dir = scratch(&[
            "home/.ssh/id",
            "home/.config/gh/hosts.yml",
            "home/.config/git/config",
            "home/notes.txt",
            "home/proj/src/",
        ]);
        let root = real_root(&dir);
        std::os::unix::fs::symlink(root.join("home/.ssh"), root.join("home/keys")).expect("link");
        let reach = Reach {
            write: vec![root.join("home/proj")],
            hide: vec![root.join("home/.ssh"), root.join("home/.config/gh")],
            show: Vec::new(),
        };
        let granted = grants(&reach);
        let may = |path: &str| {
            let path = root.join(path);
            granted
                .iter()
                .filter(|(place, _)| path.starts_with(place))
                .map(|(_, may)| *may)
                .max()
        };
        assert_eq!(may("home/.ssh/id"), None);
        assert_eq!(may("home/.config/gh/hosts.yml"), None);
        assert_eq!(may("home/.config/git/config"), Some(May::Read));
        assert_eq!(may("home/notes.txt"), Some(May::Read));
        assert_eq!(may("home/proj/src/new.rs"), Some(May::Change));
        // The link to the keys opens nothing: they are closed where they are.
        assert!(!granted.iter().any(|(place, _)| place.ends_with("keys")));
        // The folders on the way down aren't opened themselves.
        for folder in ["home", "home/.config"] {
            assert!(!granted.iter().any(|(place, _)| *place == root.join(folder)));
        }
        assert!(granted.contains(&(PathBuf::from("/usr"), May::Read)));
        assert!(granted.contains(&(PathBuf::from("/dev"), May::Use)));
    }

    #[test]
    fn the_deepest_place_decides_except_inside_what_may_be_changed() {
        let dir = scratch(&[
            "data/harlo/server.db",
            "data/harlo/worktrees/mine/a.rs",
            "data/harlo/skills/review/SKILL.md",
            "proj/.env",
        ]);
        let root = real_root(&dir);
        let reach = Reach {
            write: vec![root.join("data/harlo/worktrees/mine"), root.join("proj")],
            hide: vec![root.join("data/harlo"), root.join("proj/.env")],
            show: vec![root.join("data/harlo/skills")],
        };
        assert_eq!(reach.may(&root.join("data/harlo/server.db")), May::Nothing);
        assert_eq!(
            reach.may(&root.join("data/harlo/skills/review/SKILL.md")),
            May::Read
        );
        assert_eq!(
            reach.may(&root.join("data/harlo/worktrees/mine/a.rs")),
            May::Change
        );
        // Nothing is hidden inside a folder commands may change, and the
        // folder is opened itself: new things can be made in it.
        assert_eq!(reach.may(&root.join("proj/.env")), May::Change);
        let granted = grants(&reach);
        assert!(granted.contains(&(root.join("proj"), May::Change)));
        assert!(granted.contains(&(root.join("data/harlo/skills"), May::Read)));
        assert!(granted.contains(&(root.join("data/harlo/worktrees/mine"), May::Change)));
        assert!(
            !granted
                .iter()
                .any(|(place, _)| place.ends_with("server.db"))
        );
    }

    #[test]
    fn a_command_changes_its_project_the_temporary_folders_and_caches() {
        let dir = scratch(&[
            "home/.ssh/id",
            "home/.cargo/registry/",
            "home/.cargo/credentials.toml",
            "home/proj/.git/",
            "home/proj/src/",
            "home/own/service.json",
            "elsewhere/cache/",
        ]);
        let root = real_root(&dir);
        let home = root.join("home");
        let cache = root.join("elsewhere/cache").into_os_string();
        let places = Places {
            home: Some(home.clone()),
            var: &|name| (name == "UV_CACHE_DIR").then(|| cache.clone()),
        };
        let sandbox = Sandbox {
            helper: PathBuf::from("/proc/self/exe"),
            writable: Vec::new(),
            hidden: vec![home.join("own")],
            shown: Vec::new(),
        };
        let reach = sandbox.reach(&places, &home.join("proj/src"), "npm test");
        // The whole repository, from a folder inside it.
        assert!(reach.write.contains(&home.join("proj")));
        assert!(reach.write.contains(&PathBuf::from("/tmp")));
        assert!(reach.write.contains(&home.join(".cargo/registry")));
        assert!(reach.write.contains(&root.join("elsewhere/cache")));
        // Made for the tool the command names, and for no other.
        assert!(reach.write.contains(&home.join(".npm")));
        assert!(!home.join(".yarn").exists());
        assert!(reach.hide.contains(&home.join(".ssh")));
        assert!(reach.hide.contains(&home.join(".cargo/credentials.toml")));
        assert!(reach.hide.contains(&home.join("own")));
        // What isn't there is in neither list.
        assert!(!reach.hide.contains(&home.join(".aws")));

        // A session in the home folder only reads it.
        let at_home = sandbox.reach(&places, &home, "ls");
        assert!(!at_home.write.contains(&home));
        assert!(at_home.write.contains(&PathBuf::from("/tmp")));
    }

    #[test]
    fn a_worktree_commits_to_the_repository_it_belongs_to() {
        let dir = scratch(&[
            "main/.git/worktrees/side/commondir",
            "side/src/",
            "side/.git",
        ]);
        let root = real_root(&dir);
        let own = root.join("main/.git/worktrees/side");
        std::fs::write(own.join("commondir"), "../..\n").expect("written");
        std::fs::write(
            root.join("side/.git"),
            format!("gitdir: {}\n", own.display()),
        )
        .expect("written");
        let folders = real(repository(&root.join("side/src")));
        assert_eq!(
            folders,
            [root.join("side"), root.join("main/.git"), own.clone()]
        );
    }

    #[test]
    fn the_helpers_arguments_carry_what_a_command_reaches() {
        let reach = Reach {
            write: vec![PathBuf::from("/tmp"), PathBuf::from("/home/u/my proj")],
            hide: vec![PathBuf::from("/home/u/.ssh")],
            show: vec![PathBuf::from("/home/u/.claude/skills")],
        };
        let confined = Confined::new(PathBuf::from("/proc/self/exe"), reach.clone());
        let (program, mut args) = confined.wrapper();
        assert_eq!(program, Path::new("/proc/self/exe"));
        assert_eq!(args.remove(0), HELPER);
        args.extend(["bash", "-c", "echo -- hi"].map(OsString::from));
        let (read, command) = parse(args).expect("parsed");
        assert_eq!(read, reach);
        assert_eq!(command, ["bash", "-c", "echo -- hi"]);
        // Without a command there is nothing to run.
        assert_eq!(parse([OsString::from(END)]), None);
        assert_eq!(parse([OsString::from("--other")]), None);
    }
}
