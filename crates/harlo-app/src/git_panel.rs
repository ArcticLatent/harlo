//! `GitPanel`: working-tree changes of one folder. The model rows are the
//! diff lines of the selected file; the file list and branch state are
//! properties.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel = crate::qt_base::qobject::QAbstractListModel;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(QString, directory)]
        /// The folder isn't a repository but holds several (a team's
        /// repositories): each one, JSON `[{root, name, branch, ahead,
        /// behind, changes, staged}]`, by name; "" for one repository. The
        /// rest of the panel shows the active one (`root`; `selectRepo`).
        #[qproperty(QString, repos_json, cxx_name = "reposJson")]
        /// A folder of repositories: each one's open pull request, by its
        /// root, JSON `{root: {url, number, word, checks}}` (`loadAllRequests`).
        #[qproperty(QString, requests_json, cxx_name = "requestsJson")]
        #[qproperty(QString, root)]
        #[qproperty(bool, is_repo, cxx_name = "isRepo")]
        #[qproperty(QString, branch)]
        #[qproperty(QString, upstream)]
        #[qproperty(i32, ahead)]
        #[qproperty(i32, behind)]
        /// `path\tindex\tworktree\tuntracked(0|1)\tconflicted(0|1)\torigPath` per file.
        #[qproperty(QStringList, files)]
        #[qproperty(i32, staged_count, cxx_name = "stagedCount")]
        #[qproperty(i32, unstaged_count, cxx_name = "unstagedCount")]
        #[qproperty(QString, selected_path, cxx_name = "selectedPath")]
        #[qproperty(bool, selected_staged, cxx_name = "selectedStaged")]
        /// "+12 −3" for the selected file.
        #[qproperty(QString, diff_stats, cxx_name = "diffStats")]
        #[qproperty(bool, busy)]
        /// Result of the last operation; `error` says whether it failed.
        #[qproperty(QString, message)]
        #[qproperty(bool, error)]
        /// A commit message is being written by the model.
        #[qproperty(i32, stash_count, cxx_name = "stashCount")]
        #[qproperty(bool, generating)]
        /// The last commit message the model wrote.
        #[qproperty(QString, generated_message, cxx_name = "generatedMessage")]
        /// Branches and stashes, JSON, as `load_branches` / `load_stashes`
        /// last found them (read off the UI thread: over SSH it can be slow).
        #[qproperty(QString, branches_list, cxx_name = "branchesList")]
        #[qproperty(QString, stash_list, cxx_name = "stashList")]
        /// Side by side (the diff tab): the selected file loads as each
        /// side's text, `splitLeft` / `splitRight`, and `splitJson`
        /// (`{rows, hunks, note}`, see `split_json`) instead of the rows.
        #[qproperty(bool, split_mode, cxx_name = "splitMode")]
        #[qproperty(QString, split_left, cxx_name = "splitLeft")]
        #[qproperty(QString, split_right, cxx_name = "splitRight")]
        #[qproperty(QString, split_json, cxx_name = "splitJson")]
        /// A pull request (merge request on GitLab) from the current branch:
        /// `{ state: "none" | "loading" | "ready" | "creating", service,
        /// word, host, repo, head, base, title, body, url, number, web,
        /// canCreate, error, checks }` (`url`: the branch's open one; `web`:
        /// the site's page for a new one; `checks`: its CI checks, see
        /// `Checks::to_json`).
        #[qproperty(QString, request, cxx_name = "requestJson")]
        /// What failed in the open pull request's checks, as a message for
        /// the agent (`reportFailures`); "" until it's ready.
        #[qproperty(QString, ci_report, cxx_name = "ciReport")]
        type GitPanel = super::GitPanelRust;
    }

    impl cxx_qt::Threading for GitPanel {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// Shows the repository containing `dir` (a session's folder), or
        /// the repositories inside it.
        #[qinvokable]
        fn set_folder(self: Pin<&mut GitPanel>, dir: &QString);
        /// Of the folder's repositories (`reposJson`), shows this one.
        #[qinvokable]
        fn select_repo(self: Pin<&mut GitPanel>, root: &QString);
        /// Re-reads status; reloads the selected diff if anything changed
        /// (or always, with `force`).
        #[qinvokable]
        fn refresh(self: Pin<&mut GitPanel>, force: bool);
        /// Shows a file's diff. A path may be absolute or repo-relative.
        #[qinvokable]
        fn select(self: Pin<&mut GitPanel>, path: &QString, staged: bool);
        /// Shows a file's diff (an absolute path) in the repository it's in.
        #[qinvokable]
        fn show_file(self: Pin<&mut GitPanel>, path: &QString, staged: bool);
        #[qinvokable]
        fn stage_file(self: Pin<&mut GitPanel>, path: &QString);
        #[qinvokable]
        fn unstage_file(self: Pin<&mut GitPanel>, path: &QString);
        /// Discards a file's unstaged changes (untracked files go to the trash).
        #[qinvokable]
        fn revert_file(self: Pin<&mut GitPanel>, path: &QString);
        #[qinvokable]
        fn stage_hunk(self: Pin<&mut GitPanel>, index: i32);
        #[qinvokable]
        fn unstage_hunk(self: Pin<&mut GitPanel>, index: i32);
        #[qinvokable]
        fn revert_hunk(self: Pin<&mut GitPanel>, index: i32);
        #[qinvokable]
        fn stage_all(self: Pin<&mut GitPanel>);
        #[qinvokable]
        fn commit(self: Pin<&mut GitPanel>, message: &QString);
        /// Has the harness's model write a commit message for the staged
        /// changes (or all changes when `staged_only` is false). `program`
        /// and `model` may be empty (found on PATH / the harness default).
        #[qinvokable]
        fn generate_message(self: Pin<&mut GitPanel>, harness: &QString, program: &QString, model: &QString, staged_only: bool);
        /// Stages every change, then commits.
        #[qinvokable]
        fn commit_all(self: Pin<&mut GitPanel>, message: &QString);
        #[qinvokable]
        fn push(self: Pin<&mut GitPanel>);
        #[qinvokable]
        fn pull(self: Pin<&mut GitPanel>);
        /// Local and remote-only branches, JSON
        /// `{current, local: [{name, upstream, ahead, date}], remote: [...]}`.
        #[qinvokable]
        fn branches_json(self: &GitPanel) -> QString;
        /// Switches branch (`remote`: an `origin/x` name; `create`: a new
        /// branch from HEAD), stashing uncommitted changes first if `stash`.
        #[qinvokable]
        fn switch_branch(self: Pin<&mut GitPanel>, name: &QString, create: bool, remote: bool, stash: bool);
        /// Stash entries, JSON `[{index, message, branch, date}]`.
        #[qinvokable]
        fn stashes_json(self: &GitPanel) -> QString;
        /// Reads whether the branch has a pull request (`requestJson`).
        #[qinvokable]
        fn load_request(self: Pin<&mut GitPanel>);
        /// Opens a pull request with gh / glab (pushing first if the branch
        /// is ahead); `requestJson` follows.
        #[qinvokable]
        fn create_request(self: Pin<&mut GitPanel>, title: &QString, body: &QString, base: &QString, draft: bool);
        /// A folder of repositories: reads every one's open pull request and
        /// its checks at once (`requestsJson`).
        #[qinvokable]
        fn load_all_requests(self: Pin<&mut GitPanel>);
        /// Gathers what failed in the pull request's checks (their logs
        /// too) into `ciReport`.
        #[qinvokable]
        fn report_failures(self: Pin<&mut GitPanel>);
        /// Fills `branchesList` (off the UI thread).
        #[qinvokable]
        fn load_branches(self: Pin<&mut GitPanel>);
        /// Fills `stashList` (off the UI thread).
        #[qinvokable]
        fn load_stashes(self: Pin<&mut GitPanel>);
        #[qinvokable]
        fn stash_push(self: Pin<&mut GitPanel>, message: &QString);
        /// Applies `stash@{index}`; `pop` also drops it.
        #[qinvokable]
        fn stash_apply(self: Pin<&mut GitPanel>, index: i32, pop: bool);
        #[qinvokable]
        fn stash_drop(self: Pin<&mut GitPanel>, index: i32);
    }

    extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut GitPanel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut GitPanel>);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &GitPanel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &GitPanel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &GitPanel, parent: &QModelIndex) -> i32;
    }
}

use std::path::{Path, PathBuf};
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QStringList, QVariant};
use harlo_core::git::{self, FileDiff, GitStatus, LineKind};
use harlo_harness::HarnessKind;

use qobject::GitPanel;

const ROLE_KIND: i32 = 0x100;
const ROLE_OLD: i32 = 0x101;
const ROLE_NEW: i32 = 0x102;
const ROLE_HTML: i32 = 0x103;
const ROLE_HUNK: i32 = 0x104;

/// Diff lines shown per file before the rest is cut off.
const MAX_DIFF_ROWS: usize = 5000;

#[derive(Clone)]
struct DiffRow {
    /// hunk | add | del | ctx | note
    kind: &'static str,
    old_no: i32,
    new_no: i32,
    html: String,
    hunk: i32,
}

#[derive(Default)]
pub struct GitPanelRust {
    branches_list: QString,
    stash_list: QString,
    directory: QString,
    root: QString,
    is_repo: bool,
    branch: QString,
    upstream: QString,
    ahead: i32,
    behind: i32,
    files: QStringList,
    staged_count: i32,
    unstaged_count: i32,
    selected_path: QString,
    selected_staged: bool,
    diff_stats: QString,
    busy: bool,
    message: QString,
    error: bool,
    stash_count: i32,
    generating: bool,
    generated_message: QString,

    split_mode: bool,
    split_left: QString,
    split_right: QString,
    split_json: QString,
    request: QString,
    ci_report: QString,

    status: Option<GitStatus>,
    diff: Option<FileDiff>,
    rows: Vec<DiffRow>,
    /// Bumped when the folder changes; stale results are dropped.
    generation: u64,
    repos_json: QString,
    requests_json: QString,
    /// `load_all_requests` is reading (one at a time).
    reading_requests: bool,
    /// Of the folder's repositories, the one shown (picked, or the first
    /// with changes).
    active_repo: Option<PathBuf>,
}

/// The repositories inside a folder that isn't one, looked for again after
/// a while (one may be cloned or removed).
fn nested_cached(dir: &Path) -> Vec<PathBuf> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};
    type Found = HashMap<PathBuf, (Instant, Vec<PathBuf>)>;
    static CACHE: OnceLock<Mutex<Found>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((at, list)) = cache.lock().unwrap().get(dir)
        && at.elapsed() < Duration::from_secs(30)
    {
        return list.clone();
    }
    let list = git::nested_repos(dir);
    cache.lock().unwrap().insert(dir.to_owned(), (Instant::now(), list.clone()));
    list
}

/// The folder's repositories, each with its status, for `reposJson`
/// (named by their path in the folder: "group/tools").
fn repos_summary(dir: &Path, statuses: &[GitStatus]) -> String {
    let list: Vec<serde_json::Value> = statuses
        .iter()
        .map(|st| {
            let name = st.root.strip_prefix(dir).ok().map(|p| p.to_string_lossy().into_owned()).filter(|n| !n.is_empty())
                .or_else(|| st.root.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default();
            serde_json::json!({
                "root": st.root.to_string_lossy(), "name": name, "branch": if st.branch.is_empty() { &st.head } else { &st.branch },
                "ahead": st.ahead, "behind": st.behind, "changes": st.files.len(), "staged": st.files.iter().filter(|f| f.staged()).count(),
            })
        })
        .collect();
    serde_json::Value::Array(list).to_string()
}

/// What a pull request from the branch would be: its site and repository,
/// the branches, a title and description from its commits, and an open one
/// (gh / glab say).
fn read_request(root: &Path) -> serde_json::Value {
    use harlo_core::git_hosting as hosting;
    use serde_json::json;
    let none = json!({ "state": "none" });
    let Some(rb) = git::request_base(root) else { return none };
    let Some((host, repo)) = hosting::parse_remote(&rb.remote_url) else { return none };
    let service = hosting::service_for(&host, &crate::setup::setting("gitlab_host"), &crate::setup::setting("forgejo_host"), &crate::setup::setting("git_host"));
    // One commit: its subject; more: the branch's name in words, the subjects listed.
    let title = match rb.subjects.as_slice() {
        [one] => one.clone(),
        _ => {
            let words = rb.head.rsplit('/').next().unwrap_or(&rb.head).replace(['-', '_'], " ");
            let mut c = words.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    };
    let body = if rb.subjects.len() > 1 { rb.subjects.iter().rev().map(|s| format!("- {s}")).collect::<Vec<_>>().join("\n") } else { String::new() };
    let full = format!("{host}/{repo}");
    // The branch's open one.
    let (mut url, mut number) = (String::new(), 0i64);
    if let Some(mut c) = crate::setup::tool(&service) {
        match service.as_str() {
            "github" => {
                c.args(["pr", "view", &rb.head, "--repo", &full, "--json", "url,number,state"]);
                if let Ok(out) = crate::setup::run(c, None)
                    && let Ok(v) = serde_json::from_str::<serde_json::Value>(out.trim())
                    && v["state"] == "OPEN"
                {
                    url = v["url"].as_str().unwrap_or_default().to_owned();
                    number = v["number"].as_i64().unwrap_or(0);
                }
            }
            "gitlab" => {
                c.args(["mr", "list", "--source-branch", &rb.head, "-R", &full, "-F", "json"]);
                if let Ok(out) = crate::setup::run(c, None)
                    && let Ok(v) = serde_json::from_str::<serde_json::Value>(out.trim())
                    && let Some(mr) = v.as_array().and_then(|a| a.first())
                {
                    url = mr["web_url"].as_str().unwrap_or_default().to_owned();
                    number = mr["iid"].as_i64().unwrap_or(0);
                }
            }
            _ => {}
        }
    }
    // Forgejo (Codeberg, or one's own server): its API, with the token git keeps.
    let forgejo = (service == "forgejo").then(|| crate::setup::forgejo_api(&host)).flatten();
    if let Some(api) = &forgejo
        && let Ok(Some((u, n))) = api.open_pull(&repo, &rb.head)
    {
        url = u;
        number = n;
    }
    let can_create = if service == "forgejo" { forgejo.is_some() } else { crate::setup::tool(&service).is_some() };
    // Its CI checks.
    let checks = if url.is_empty() { serde_json::Value::Null } else { crate::ci::read(&service, &full, &url, number).map_or(serde_json::Value::Null, |c| c.to_json()) };
    json!({ "checks": checks, "state": "ready", "service": service, "label": hosting::label(&service), "word": hosting::request_word(&service), "host": host, "repo": repo,
            "head": rb.head, "base": rb.base, "title": title, "body": body, "url": url, "number": number,
            "web": hosting::new_request_url(&service, &host, &repo, &rb.base, &rb.head), "canCreate": can_create, "error": "" })
}

/// What a background job hands back to the Qt thread.
enum Outcome {
    Diff(FileDiff, Vec<DiffRow>, Option<SplitView>),
    Done(Result<String, String>),
}

impl GitPanel {
    fn root_path(&self) -> Option<PathBuf> {
        self.status.as_ref().map(|s| s.root.clone())
    }

    /// Runs `job` off the UI thread, then applies its outcomes and refreshes.
    fn run(mut self: Pin<&mut Self>, refresh_after: bool, job: impl FnOnce() -> Vec<Outcome> + Send + 'static) {
        let thread = self.qt_thread();
        let generation = self.generation;
        self.as_mut().set_busy(true);
        crate::runtime().spawn_blocking(move || {
            let outcomes = job();
            let _ = thread.queue(move |mut this| {
                if this.generation != generation {
                    return;
                }
                this.as_mut().set_busy(false);
                for o in outcomes {
                    this.as_mut().apply(o);
                }
                if refresh_after {
                    this.refresh(true);
                }
            });
        });
    }

    fn apply(mut self: Pin<&mut Self>, outcome: Outcome) {
        match outcome {
            Outcome::Diff(diff, rows, split) => {
                let stats = if diff.binary { "binary".to_owned() } else { format!("+{} −{}", diff.added, diff.removed) };
                self.as_mut().set_diff_stats(QString::from(&stats));
                if let Some(v) = split {
                    // (Unchanged sides stay as they are: the view keeps its place.)
                    let (left, right, json) = (QString::from(&v.left), QString::from(&v.right), QString::from(&v.json));
                    if self.split_left != left {
                        self.as_mut().set_split_left(left);
                    }
                    if self.split_right != right {
                        self.as_mut().set_split_right(right);
                    }
                    if self.split_json != json {
                        self.as_mut().set_split_json(json);
                    }
                }
                self.as_mut().rust_mut().diff = Some(diff);
                unsafe {
                    self.as_mut().begin_reset_model();
                    self.as_mut().rust_mut().rows = rows;
                    self.as_mut().end_reset_model();
                }
            }
            Outcome::Done(result) => {
                let (text, failed) = match result {
                    Ok(t) => (t, false),
                    Err(e) => (e, true),
                };
                self.as_mut().set_error(failed);
                self.as_mut().set_message(QString::from(&text));
            }
        }
    }

    fn apply_status(mut self: Pin<&mut Self>, status: Option<GitStatus>) {
        let Some(st) = status else {
            self.as_mut().rust_mut().status = None;
            self.as_mut().set_is_repo(false);
            self.as_mut().set_root(QString::default());
            self.as_mut().set_files(QStringList::default());
            self.as_mut().set_staged_count(0);
            self.as_mut().set_unstaged_count(0);
            self.clear_selection();
            return;
        };
        let mut files = QStringList::default();
        for f in &st.files {
            files.append(QString::from(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                f.path,
                f.index,
                f.worktree,
                f.untracked as u8,
                f.conflicted as u8,
                f.orig_path.as_deref().unwrap_or("")
            )));
        }
        let staged = st.files.iter().filter(|f| f.staged()).count() as i32;
        let unstaged = st.files.iter().filter(|f| f.unstaged()).count() as i32;
        self.as_mut().set_is_repo(true);
        self.as_mut().set_root(QString::from(&st.root.to_string_lossy().into_owned()));
        self.as_mut().set_branch(QString::from(if st.branch.is_empty() { &st.head } else { &st.branch }));
        self.as_mut().set_upstream(QString::from(&st.upstream));
        self.as_mut().set_ahead(st.ahead as i32);
        self.as_mut().set_behind(st.behind as i32);
        self.as_mut().set_files(files);
        self.as_mut().set_staged_count(staged);
        self.as_mut().set_unstaged_count(unstaged);
        self.as_mut().set_stash_count(st.stashes as i32);
        // Keep the selection only while that side of the file still has changes.
        let selected = self.selected_path.to_string();
        let selected_staged = self.selected_staged;
        let still_there = st
            .files
            .iter()
            .any(|f| f.path == selected && if selected_staged { f.staged() } else { f.unstaged() });
        self.as_mut().rust_mut().status = Some(st);
        if !selected.is_empty() && !still_there {
            self.clear_selection();
        }
    }

    fn clear_selection(mut self: Pin<&mut Self>) {
        self.as_mut().set_selected_path(QString::default());
        self.as_mut().set_diff_stats(QString::default());
        if !self.split_json.is_empty() {
            self.as_mut().set_split_left(QString::default());
            self.as_mut().set_split_right(QString::default());
            self.as_mut().set_split_json(QString::default());
        }
        self.as_mut().rust_mut().diff = None;
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().rows.clear();
            self.as_mut().end_reset_model();
        }
    }

    pub fn set_folder(mut self: Pin<&mut Self>, dir: &QString) {
        if &self.directory == dir {
            return;
        }
        self.as_mut().rust_mut().generation += 1;
        self.as_mut().rust_mut().active_repo = None;
        self.as_mut().set_repos_json(QString::default());
        self.as_mut().set_requests_json(QString::default());
        self.as_mut().set_directory(dir.clone());
        self.as_mut().set_message(QString::default());
        self.as_mut().apply_status(None);
        self.refresh(true);
    }

    pub fn select_repo(mut self: Pin<&mut Self>, root: &QString) {
        let root = PathBuf::from(root.to_string());
        if self.root_path().as_ref() == Some(&root) {
            return;
        }
        // (The one shown stays until the other is read: no flicker.)
        self.as_mut().rust_mut().generation += 1;
        self.as_mut().rust_mut().active_repo = Some(root);
        self.as_mut().set_message(QString::default());
        self.as_mut().clear_selection();
        self.as_mut().set_request(QString::from(r#"{"state":"none"}"#));
        self.refresh(true);
    }

    pub fn refresh(self: Pin<&mut Self>, force: bool) {
        let dir = PathBuf::from(self.directory.to_string());
        if dir.as_os_str().is_empty() {
            return;
        }
        let previous = self.status.clone();
        let selection = self.selection();
        let split = self.split_mode;
        let thread = self.qt_thread();
        let generation = self.generation;
        let wanted = self.active_repo.clone();
        crate::runtime().spawn_blocking(move || {
            // A repository; else the repositories inside the folder, one of
            // them shown (the one picked, else the first with changes).
            let (status, repos) = match git::repo_root(&dir) {
                Some(root) => (git::status(&root).ok(), None),
                None => {
                    let all: Vec<GitStatus> = nested_cached(&dir).iter().filter_map(|r| git::status(r).ok()).collect();
                    let shown = wanted.as_ref().and_then(|w| all.iter().find(|s| &s.root == w))
                        .or_else(|| all.iter().find(|s| !s.files.is_empty()))
                        .or(all.first())
                        .cloned();
                    let summary = if all.is_empty() { String::new() } else { repos_summary(&dir, &all) };
                    (shown, Some(summary))
                }
            };
            let changed = status != previous;
            // (`show_file` selects by absolute path.)
            let requested = selection.as_ref().map(|(p, _)| p.clone());
            let selection = selection.map(|(path, staged)| (status.as_ref().map_or(path.clone(), |st| relative_to(&st.root, &path)), staged));
            let diff = match (&status, &selection) {
                (Some(st), Some((path, staged))) if changed || force => {
                    let untracked = st.files.iter().any(|f| &f.path == path && f.untracked);
                    load_diff(&st.root, path, *staged, untracked, split).ok()
                }
                _ => None,
            };
            let _ = thread.queue(move |mut this| {
                if this.generation != generation {
                    return;
                }
                if let (Some((path, _)), Some(requested)) = (&selection, &requested)
                    && this.selected_path.to_string() == *requested
                    && requested != path
                {
                    this.as_mut().set_selected_path(QString::from(path));
                }
                let summary = QString::from(&repos.unwrap_or_default());
                if this.repos_json != summary {
                    this.as_mut().set_repos_json(summary);
                }
                if changed {
                    // (The repository shown, if the folder holds several.)
                    let root = status.as_ref().map(|s| s.root.clone());
                    if !this.repos_json.is_empty() {
                        this.as_mut().rust_mut().active_repo = root;
                    }
                    this.as_mut().apply_status(status);
                }
                // The selection may have been cleared by the new status.
                if let Some((diff, rows, split)) = diff
                    && this.selection().is_some_and(|(p, _)| p == diff.path) {
                        this.as_mut().apply(Outcome::Diff(diff, rows, split));
                    }
            });
        });
    }

    fn selection(&self) -> Option<(String, bool)> {
        let path = self.selected_path.to_string();
        (!path.is_empty()).then_some((path, self.selected_staged))
    }

    pub fn select(mut self: Pin<&mut Self>, path: &QString, staged: bool) {
        let Some(root) = self.root_path() else { return };
        let path = relative_to(&root, &path.to_string());
        let untracked = self.status.as_ref().is_some_and(|s| s.files.iter().any(|f| f.path == path && f.untracked));
        self.as_mut().set_selected_path(QString::from(&path));
        self.as_mut().set_selected_staged(staged);
        let split = self.split_mode;
        self.run(false, move || match load_diff(&root, &path, staged, untracked, split) {
            Ok((diff, rows, split)) => vec![Outcome::Diff(diff, rows, split)],
            Err(e) => vec![Outcome::Done(Err(format!("{e:#}")))],
        });
    }

    pub fn show_file(mut self: Pin<&mut Self>, path: &QString, staged: bool) {
        let file = PathBuf::from(path.to_string());
        let Some(dir) = file.parent() else { return };
        let dir = QString::from(&dir.to_string_lossy().into_owned());
        if self.directory != dir {
            self.as_mut().rust_mut().generation += 1;
            self.as_mut().set_directory(dir);
            self.as_mut().set_message(QString::default());
            self.as_mut().apply_status(None);
        }
        // Made repo-relative once the repository is known (`refresh`).
        self.as_mut().set_selected_path(path.clone());
        self.as_mut().set_selected_staged(staged);
        self.refresh(true);
    }

    fn file_op(self: Pin<&mut Self>, path: &QString, op: fn(&Path, &str) -> anyhow::Result<()>, done: &'static str) {
        let Some(root) = self.root_path() else { return };
        let path = relative_to(&root, &path.to_string());
        self.run(true, move || vec![Outcome::Done(op(&root, &path).map(|_| format!("{done} {path}")).map_err(|e| format!("{e:#}")))]);
    }

    pub fn stage_file(self: Pin<&mut Self>, path: &QString) {
        self.file_op(path, git::stage_file, "Staged");
    }

    pub fn unstage_file(self: Pin<&mut Self>, path: &QString) {
        self.file_op(path, git::unstage_file, "Unstaged");
    }

    pub fn revert_file(self: Pin<&mut Self>, path: &QString) {
        let Some(root) = self.root_path() else { return };
        let rel = relative_to(&root, &path.to_string());
        let untracked = self.status.as_ref().is_some_and(|s| s.files.iter().any(|f| f.path == rel && f.untracked));
        if !untracked {
            return self.file_op(path, git::revert_file, "Reverted");
        }
        // An untracked file has no committed version: move it to the trash.
        self.run(true, move || {
            let result = harlo_core::remote::trash(&root.join(&rel));
            vec![Outcome::Done(result.map(|_| format!("Moved {rel} to the trash")).map_err(|e| format!("{e:#}")))]
        });
    }

    fn hunk_op(self: Pin<&mut Self>, index: i32, op: fn(&Path, &FileDiff, usize) -> anyhow::Result<()>, done: &'static str) {
        let (Some(root), Some(diff)) = (self.root_path(), self.diff.clone()) else { return };
        self.run(true, move || {
            vec![Outcome::Done(op(&root, &diff, index.max(0) as usize).map(|_| done.to_owned()).map_err(|e| format!("{e:#}")))]
        });
    }

    pub fn stage_hunk(self: Pin<&mut Self>, index: i32) {
        self.hunk_op(index, git::stage_hunk, "Staged hunk");
    }

    pub fn unstage_hunk(self: Pin<&mut Self>, index: i32) {
        self.hunk_op(index, git::unstage_hunk, "Unstaged hunk");
    }

    pub fn revert_hunk(self: Pin<&mut Self>, index: i32) {
        self.hunk_op(index, git::revert_hunk, "Reverted hunk");
    }

    pub fn stage_all(self: Pin<&mut Self>) {
        let Some(root) = self.root_path() else { return };
        self.run(true, move || vec![Outcome::Done(git::stage_all(&root).map(|_| "Staged all changes".to_owned()).map_err(|e| format!("{e:#}")))]);
    }

    pub fn commit(self: Pin<&mut Self>, message: &QString) {
        let Some(root) = self.root_path() else { return };
        let message = message.to_string();
        self.run(true, move || vec![Outcome::Done(git::commit(&root, &message).map_err(|e| format!("{e:#}")))]);
    }

    pub fn generate_message(mut self: Pin<&mut Self>, harness: &QString, program: &QString, model: &QString, staged_only: bool) {
        if self.generating {
            return;
        }
        let Some(root) = self.root_path() else { return };
        let kind = HarnessKind::parse(&harness.to_string()).filter(|k| *k != HarnessKind::Demo).unwrap_or(HarnessKind::Omp);
        let program = Some(program.to_string()).filter(|p| !p.trim().is_empty()).map(PathBuf::from);
        let model = model.to_string();
        let generation = self.generation;
        let thread = self.qt_thread();
        self.as_mut().set_generating(true);
        crate::runtime().spawn_blocking(move || {
            let result = harlo_core::commitmsg::generate(kind, program, &model, &root, staged_only);
            let _ = thread.queue(move |mut this| {
                this.as_mut().set_generating(false);
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(msg) => this.as_mut().set_generated_message(QString::from(&msg)),
                    Err(e) => {
                        this.as_mut().set_error(true);
                        this.as_mut().set_message(QString::from(&format!("Could not write a commit message: {e:#}")));
                    }
                }
            });
        });
    }

    pub fn commit_all(self: Pin<&mut Self>, message: &QString) {
        let Some(root) = self.root_path() else { return };
        let message = message.to_string();
        self.run(true, move || {
            let result = git::stage_all(&root).and_then(|_| git::commit(&root, &message));
            vec![Outcome::Done(result.map_err(|e| format!("{e:#}")))]
        });
    }

    pub fn push(self: Pin<&mut Self>) {
        let Some(root) = self.root_path() else { return };
        let set_upstream = self.upstream.is_empty();
        self.run(true, move || vec![Outcome::Done(git::push(&root, set_upstream).map_err(|e| format!("{e:#}")))]);
    }

    pub fn pull(self: Pin<&mut Self>) {
        let Some(root) = self.root_path() else { return };
        self.run(true, move || vec![Outcome::Done(git::pull(&root).map_err(|e| format!("{e:#}")))]);
    }

    pub fn branches_json(&self) -> QString {
        let Some(root) = self.root_path() else { return QString::default() };
        QString::from(&branches_text(&root))
    }

    pub fn switch_branch(self: Pin<&mut Self>, name: &QString, create: bool, remote: bool, stash: bool) {
        let Some(root) = self.root_path() else { return };
        let name = name.to_string();
        self.run(true, move || vec![Outcome::Done(git::switch_branch(&root, &name, create, remote, stash).map_err(|e| format!("{e:#}")))]);
    }

    pub fn stashes_json(&self) -> QString {
        let Some(root) = self.root_path() else { return QString::default() };
        QString::from(&stashes_text(&root))
    }

    pub fn load_request(mut self: Pin<&mut Self>) {
        let Some(root) = self.root_path() else {
            self.as_mut().set_request(QString::from(r#"{"state":"none"}"#));
            return;
        };
        let mut was: serde_json::Value = serde_json::from_str(&self.request.to_string()).unwrap_or_default();
        if was["state"] != "creating" {
            was["state"] = "loading".into();
            self.as_mut().set_request(QString::from(&was.to_string()));
        }
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let v = read_request(&root);
            let _ = thread.queue(move |mut this| {
                if this.root_path().as_ref() == Some(&root) {
                    this.as_mut().set_request(QString::from(&v.to_string()));
                }
            });
        });
    }

    pub fn create_request(mut self: Pin<&mut Self>, title: &QString, body: &QString, base: &QString, draft: bool) {
        let Some(root) = self.root_path() else { return };
        let (title, body, base) = (title.to_string(), body.to_string(), base.to_string());
        let mut req: serde_json::Value = serde_json::from_str(&self.request.to_string()).unwrap_or_default();
        if req["state"] != "ready" || title.trim().is_empty() {
            return;
        }
        let ahead = self.ahead > 0;
        req["state"] = "creating".into();
        req["error"] = "".into();
        self.as_mut().set_request(QString::from(&req.to_string()));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let service = req["service"].as_str().unwrap_or_default().to_owned();
            let full = format!("{}/{}", req["host"].as_str().unwrap_or_default(), req["repo"].as_str().unwrap_or_default());
            let head = req["head"].as_str().unwrap_or_default().to_owned();
            let word = req["word"].as_str().unwrap_or("pull request").to_owned();
            let result = (|| {
                // Its latest commits first.
                if ahead {
                    git::push(&root, false).map_err(|e| format!("Pushing first failed: {e:#}"))?;
                }
                if service == "forgejo" {
                    let api = crate::setup::forgejo_api(req["host"].as_str().unwrap_or_default()).ok_or("Sign in to Forgejo first (Settings → Git).")?;
                    let repo = req["repo"].as_str().unwrap_or_default();
                    let url = api.create_pull(repo, &head, &base, title.trim(), &body, draft).map_err(|e| format!("{e:#}"))?;
                    let number = url.rsplit('/').next().and_then(|n| n.parse::<i64>().ok()).unwrap_or(0);
                    let checks = crate::ci::read(&service, &full, &url, number).map_or(serde_json::Value::Null, |c| c.to_json());
                    return Ok((url, checks));
                }
                let mut c = crate::setup::tool(&service).ok_or_else(|| format!("Creating it here needs {}.", harlo_core::git_hosting::cli(&service).unwrap_or("its site")))?;
                match service.as_str() {
                    "github" => {
                        c.args(["pr", "create", "--repo", &full, "--head", &head, "--base", &base, "--title", title.trim(), "--body", &body]);
                        if draft {
                            c.arg("--draft");
                        }
                    }
                    "gitlab" => {
                        c.args(["mr", "create", "-R", &full, "--source-branch", &head, "--target-branch", &base, "--title", title.trim(), "--description", &body, "--yes"]);
                        if draft {
                            c.arg("--draft");
                        }
                    }
                    _ => return Err("Open it on the site.".to_owned()),
                }
                let out = crate::setup::run(c, None)?;
                let url = out.lines().rev().find_map(harlo_core::signin::url_in).unwrap_or_default();
                // Its checks, as they start.
                let number = url.rsplit('/').next().and_then(|n| n.parse::<i64>().ok()).unwrap_or(0);
                let checks = crate::ci::read(&service, &full, &url, number).map_or(serde_json::Value::Null, |c| c.to_json());
                Ok((url, checks))
            })();
            let _ = thread.queue(move |mut this| {
                if this.root_path().as_ref() != Some(&root) {
                    return;
                }
                let mut v: serde_json::Value = serde_json::from_str(&this.request.to_string()).unwrap_or_default();
                v["state"] = "ready".into();
                match result {
                    Ok((url, checks)) => {
                        let number = url.rsplit('/').next().and_then(|n| n.parse::<i64>().ok()).unwrap_or(0);
                        v["checks"] = checks;
                        v["url"] = url.into();
                        v["number"] = number.into();
                        this.as_mut().set_error(false);
                        this.as_mut().set_message(QString::from(&format!("Opened the {word}{}.", if number > 0 { format!(" #{number}") } else { String::new() })));
                    }
                    Err(e) => {
                        v["error"] = e.into();
                    }
                }
                this.as_mut().set_request(QString::from(&v.to_string()));
                this.refresh(true);
            });
        });
    }

    pub fn load_all_requests(mut self: Pin<&mut Self>) {
        if self.reading_requests || self.repos_json.is_empty() {
            return;
        }
        let roots: Vec<PathBuf> = serde_json::from_str::<serde_json::Value>(&self.repos_json.to_string())
            .ok()
            .and_then(|v| v.as_array().map(|a| a.iter().filter_map(|r| r["root"].as_str().map(PathBuf::from)).collect()))
            .unwrap_or_default();
        self.as_mut().rust_mut().reading_requests = true;
        // (The folder's: still good after picking another of its repositories.)
        let folder = self.directory.clone();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            // (In parallel: each asks gh / glab, which takes a moment.)
            let found: Vec<(String, serde_json::Value)> = std::thread::scope(|scope| {
                let handles: Vec<_> = roots.iter().map(|r| scope.spawn(move || (r.to_string_lossy().into_owned(), read_request(r)))).collect();
                handles.into_iter().filter_map(|h| h.join().ok()).collect()
            });
            let map: serde_json::Map<String, serde_json::Value> = found
                .into_iter()
                .filter(|(_, v)| !v["url"].as_str().unwrap_or_default().is_empty())
                .map(|(root, v)| {
                    let keep = serde_json::json!({ "url": v["url"], "number": v["number"], "word": v["word"], "checks": v["checks"],
                                                  "service": v["service"], "host": v["host"], "repo": v["repo"], "title": v["title"] });
                    (root, keep)
                })
                .collect();
            let text = serde_json::Value::Object(map).to_string();
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().reading_requests = false;
                if this.directory == folder && this.requests_json.to_string() != text {
                    this.as_mut().set_requests_json(QString::from(&text));
                }
            });
        });
    }

    pub fn report_failures(mut self: Pin<&mut Self>) {
        let req: serde_json::Value = serde_json::from_str(&self.request.to_string()).unwrap_or_default();
        let url = req["url"].as_str().unwrap_or_default().to_owned();
        if url.is_empty() {
            return;
        }
        self.as_mut().set_ci_report(QString::default());
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let s = |k: &str| req[k].as_str().unwrap_or_default().to_owned();
            let (service, full, number) = (s("service"), format!("{}/{}", s("host"), s("repo")), req["number"].as_i64().unwrap_or(0));
            let Some(checks) = crate::ci::read(&service, &full, &url, number).filter(|c| c.failed > 0) else { return };
            let report = crate::ci::failure_report(&service, &full, &s("word"), number, &url, &checks);
            let _ = thread.queue(move |mut this| this.as_mut().set_ci_report(QString::from(&report)));
        });
    }

    pub fn load_branches(self: Pin<&mut Self>) {
        let Some(root) = self.root_path() else { return };
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let json = branches_text(&root);
            let _ = thread.queue(move |mut this| {
                if this.root_path().as_ref() == Some(&root) {
                    this.as_mut().set_branches_list(QString::from(&json));
                }
            });
        });
    }

    pub fn load_stashes(self: Pin<&mut Self>) {
        let Some(root) = self.root_path() else { return };
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let json = stashes_text(&root);
            let _ = thread.queue(move |mut this| {
                if this.root_path().as_ref() == Some(&root) {
                    this.as_mut().set_stash_list(QString::from(&json));
                }
            });
        });
    }

    pub fn stash_push(self: Pin<&mut Self>, message: &QString) {
        let Some(root) = self.root_path() else { return };
        let message = message.to_string();
        self.run(true, move || vec![Outcome::Done(git::stash_push(&root, &message).map_err(|e| format!("{e:#}")))]);
    }

    pub fn stash_apply(self: Pin<&mut Self>, index: i32, pop: bool) {
        let Some(root) = self.root_path() else { return };
        let index = index.max(0) as usize;
        self.run(true, move || vec![Outcome::Done(git::stash_apply(&root, index, pop).map_err(|e| format!("{e:#}")))]);
    }

    pub fn stash_drop(self: Pin<&mut Self>, index: i32) {
        let Some(root) = self.root_path() else { return };
        let index = index.max(0) as usize;
        self.run(true, move || vec![Outcome::Done(git::stash_drop(&root, index).map_err(|e| format!("{e:#}")))]);
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(r) = self.rows.get(index.row() as usize) else {
            return QVariant::default();
        };
        match role {
            ROLE_KIND => QVariant::from(&QString::from(r.kind)),
            ROLE_OLD => QVariant::from(&r.old_no),
            ROLE_NEW => QVariant::from(&r.new_no),
            ROLE_HTML => QVariant::from(&QString::from(&r.html)),
            ROLE_HUNK => QVariant::from(&r.hunk),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [(ROLE_KIND, "kind"), (ROLE_OLD, "oldNo"), (ROLE_NEW, "newNo"), (ROLE_HTML, "html"), (ROLE_HUNK, "hunk")] {
            roles.insert(role, QByteArray::from(name));
        }
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rows.len() as i32
    }
}

/// The side-by-side view of a file's diff, for QML.
struct SplitView {
    left: String,
    right: String,
    json: String,
}

/// Rows past this many aren't laid out side by side.
const MAX_SPLIT_ROWS: usize = 60_000;

/// `{rows, hunks, note}`: each row
/// `[leftKind, leftNo, leftOffset, leftFrom, leftTo, rightKind, …]`
/// (kinds: 0 same, 1 changed, 2 filler; numbers 0 for filler; from / to
/// the changed part's columns, -1 when none), each hunk `[index, row]`.
fn split_view(root: &Path, path: &str, staged: bool, untracked: bool, diff: &FileDiff) -> SplitView {
    let note = |n: &str| SplitView { left: String::new(), right: String::new(), json: serde_json::json!({ "rows": [], "hunks": [], "note": n }).to_string() };
    if diff.binary {
        return note("A binary file: there's no text to compare.");
    }
    let full = match git::diff_file_full(root, path, staged, untracked) {
        Ok(f) => f,
        Err(e) => return note(&format!("{e:#}")),
    };
    let lines: usize = full.hunks.iter().map(|h| h.lines.len()).sum();
    if lines > MAX_SPLIT_ROWS {
        return note("This file is too big to compare side by side.");
    }
    let split = harlo_core::splitdiff::split(&full, diff);
    let cell = |c: &harlo_core::splitdiff::Cell| {
        let (from, to) = c.inline.map_or((-1, -1), |(a, b)| (a as i64, b as i64));
        [c.kind as i64, c.no as i64, c.offset as i64, from, to]
    };
    let rows: Vec<Vec<i64>> = split.rows.iter().map(|r| cell(&r.left).into_iter().chain(cell(&r.right)).collect()).collect();
    let note = if rows.is_empty() { "No changes." } else { "" };
    let json = serde_json::json!({ "rows": rows, "hunks": split.hunks, "note": note }).to_string();
    SplitView { left: split.left, right: split.right, json }
}

/// Loads a diff and renders its rows, or its side-by-side view (blocking).
fn load_diff(root: &Path, path: &str, staged: bool, untracked: bool, split: bool) -> anyhow::Result<(FileDiff, Vec<DiffRow>, Option<SplitView>)> {
    let diff = git::diff_file(root, path, staged, untracked)?;
    if split {
        let view = split_view(root, path, staged, untracked, &diff);
        return Ok((diff, Vec::new(), Some(view)));
    }
    let rows = render_rows(&diff);
    Ok((diff, rows, None))
}

fn render_rows(diff: &FileDiff) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    if diff.binary {
        rows.push(DiffRow { kind: "note", old_no: 0, new_no: 0, html: "Binary file".into(), hunk: -1 });
        return rows;
    }
    for (hi, hunk) in diff.hunks.iter().enumerate() {
        let mut header = String::new();
        harlo_core::markdown::escape_into(&mut header, &hunk.header);
        rows.push(DiffRow { kind: "hunk", old_no: 0, new_no: 0, html: header, hunk: hi as i32 });
        let texts: Vec<&str> = hunk.lines.iter().map(|l| l.text.as_str()).collect();
        let html = harlo_core::markdown::highlight_lines(&diff.path, &texts);
        for (line, html) in hunk.lines.iter().zip(html) {
            let kind = match line.kind {
                LineKind::Added => "add",
                LineKind::Removed => "del",
                LineKind::Context => "ctx",
                LineKind::NoNewline => "note",
            };
            rows.push(DiffRow {
                kind,
                old_no: line.old_no.map_or(0, |n| n as i32),
                new_no: line.new_no.map_or(0, |n| n as i32),
                html,
                hunk: hi as i32,
            });
            if rows.len() >= MAX_DIFF_ROWS {
                rows.push(DiffRow { kind: "note", old_no: 0, new_no: 0, html: "Diff truncated…".into(), hunk: -1 });
                return rows;
            }
        }
    }
    if rows.is_empty() {
        rows.push(DiffRow { kind: "note", old_no: 0, new_no: 0, html: "No changes".into(), hunk: -1 });
    }
    rows
}

/// Turns an absolute path (e.g. from a tool call) into one relative to the repo.
fn relative_to(root: &Path, path: &str) -> String {
    Path::new(path)
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.trim_start_matches("./").to_owned())
}

/// A repository's branches, JSON (`{current, local, remote}` or `{error}`).
fn branches_text(root: &Path) -> String {
    let entry = |b: &git::BranchInfo| serde_json::json!({ "name": b.name, "upstream": b.upstream, "ahead": b.ahead, "date": b.date });
    match git::branches(root) {
        Ok(b) => serde_json::json!({
            "current": b.current,
            "local": b.local.iter().map(entry).collect::<Vec<_>>(),
            "remote": b.remote.iter().map(entry).collect::<Vec<_>>(),
        }),
        Err(e) => serde_json::json!({ "error": format!("{e:#}") }),
    }
    .to_string()
}

/// A repository's stashes, JSON `[{index, message, branch, date}]`.
fn stashes_text(root: &Path) -> String {
    let list: Vec<serde_json::Value> = git::stashes(root)
        .unwrap_or_default()
        .iter()
        .map(|e| serde_json::json!({ "index": e.index, "message": e.message, "branch": e.branch, "date": e.date }))
        .collect();
    serde_json::Value::Array(list).to_string()
}
