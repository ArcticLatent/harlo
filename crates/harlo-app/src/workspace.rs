//! `Workspace`: the QML singleton that owns projects, the on-disk session
//! list, live sessions and the sidebar model built from them.

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
        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, active_key, cxx_name = "activeKey")]
        #[qproperty(QString, active_title, cxx_name = "activeTitle")]
        #[qproperty(i32, project_count, cxx_name = "projectCount")]
        /// Plan limits (omp's accounts, Claude's): `{ accounts, updatedAt, error, loading }`.
        #[qproperty(QString, quota_json, cxx_name = "quotaJson")]
        /// Whose limits the sidebar meter shows: the account the last busy
        /// session used ("Claude", or a model provider such as
        /// "openai-codex"), unless one is pinned.
        #[qproperty(QString, meter_last, cxx_name = "meterLast")]
        #[qproperty(QString, meter_pin, cxx_name = "meterPin")]
        /// Bumped when a remote file's local copy (`localCopy`) is ready.
        #[qproperty(i32, copies_revision, cxx_name = "copiesRevision")]
        /// Bumped when sessions an agent started (orchestration) change.
        #[qproperty(i32, orchestration_revision, cxx_name = "orchestrationRevision")]
        /// Settings → Orchestration's switch.
        #[qproperty(bool, orchestration_on, cxx_name = "orchestrationOn")]
        /// Bumped whenever scheduled prompts change (views re-read them).
        #[qproperty(i32, schedules_revision, cxx_name = "schedulesRevision")]
        /// Bumped whenever a session's approval or plan mode changes.
        #[qproperty(i32, modes_revision, cxx_name = "modesRevision")]
        #[qproperty(i32, session_count, cxx_name = "sessionCount")]
        #[qproperty(QString, error_text, cxx_name = "errorText")]
        /// Result of a background job (worktree, setup, merge); `noticeError` marks failures.
        /// The last notification (`{ kind, title, body }`), for self-tests,
        /// which don't show them.
        #[qproperty(QString, last_notification, cxx_name = "lastNotification")]
        #[qproperty(QString, notice)]
        #[qproperty(bool, notice_error, cxx_name = "noticeError")]
        #[qproperty(bool, busy)]
        /// Open sessions as tabs: `key\ttitle\tstatus\tharness\tbranch` each.
        #[qproperty(QStringList, tabs)]
        /// The instance the window shows: "" this
        /// computer, else a remote host's name. Projects, sessions and tabs
        /// are that machine's.
        #[qproperty(QString, instance)]
        /// The instance harlo starts on ("" this computer).
        #[qproperty(QString, default_instance, cxx_name = "defaultInstance")]
        /// Appearance settings (saved; change them with the `choose_*` calls).
        #[qproperty(QString, theme_family, cxx_name = "themeFamily")]
        /// system | dark | light
        #[qproperty(QString, theme_mode, cxx_name = "themeMode")]
        /// Text size factor (1.0 = default).
        #[qproperty(f64, text_scale, cxx_name = "textScale")]
        /// Chosen interface / monospace font families; "" = the desktop's.
        #[qproperty(QString, ui_font, cxx_name = "uiFont")]
        #[qproperty(QString, mono_font, cxx_name = "monoFont")]
        /// Menus and dialogs show a blur of what's behind them (Settings →
        /// Appearance → Glass), and how solid they are (0.4–1.0).
        #[qproperty(bool, glass)]
        #[qproperty(f64, glass_opacity, cxx_name = "glassOpacity")]
        /// The rest of Settings → Appearance, as JSON: `{ contrast (50–200 %),
        /// diff ("red-green" | "blue-orange"), chatWidth ("comfortable" |
        /// "wide" | "full"), motion (panel animations, ms), monoScale (code
        /// text, 0.7–1.6), wrap (long code lines wrap) }` (missing: defaults).
        #[qproperty(QString, look)]
        /// A session as Markdown for the clipboard (`copyMarkdown`); the
        /// window copies it when it changes.
        #[qproperty(QString, clipboard_text, cxx_name = "clipboardText")]
        /// Session templates (harlo_core::templates), JSON.
        #[qproperty(QString, templates_json, cxx_name = "templatesJson")]
        /// Spending limits and what's held: `{ limits: { session, day },
        /// today, day ("" or why the day's limit holds every session),
        /// held: { key: why }, costs: { key: dollars } }`.
        #[qproperty(QString, spend_json, cxx_name = "spendJson")]
        /// Bumped when projects change (Settings → Projects follows it).
        #[qproperty(i32, projects_revision, cxx_name = "projectsRevision")]
        /// Bumped when a provider's check finishes (Settings → Providers).
        #[qproperty(i32, providers_revision, cxx_name = "providersRevision")]
        /// Running sessions that are working / waiting for you (tray icon).
        #[qproperty(i32, busy_count, cxx_name = "busyCount")]
        #[qproperty(i32, input_count, cxx_name = "inputCount")]
        type Workspace = super::WorkspaceRust;
    }

    impl cxx_qt::Threading for Workspace {}
    impl cxx_qt::Initialize for Workspace {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// Adds a project folder (a path or a `file://` URL).
        #[qinvokable]
        fn add_project(self: Pin<&mut Workspace>, path: &QString);
        #[qinvokable]
        fn remove_project(self: Pin<&mut Workspace>, id: i32);
        #[qinvokable]
        fn set_project_harness(self: Pin<&mut Workspace>, id: i32, harness: &QString);
        /// Collapses or expands a project (`project:<id>`) or `group:other`.
        #[qinvokable]
        fn toggle_collapsed(self: Pin<&mut Workspace>, key: &QString);
        #[qinvokable]
        fn show_more(self: Pin<&mut Workspace>, project_id: i32);
        #[qinvokable]
        fn set_filter(self: Pin<&mut Workspace>, text: &QString);
        /// Opens a sidebar session (live key or file path), makes it active
        /// and returns its live key.
        #[qinvokable]
        fn open(self: Pin<&mut Workspace>, key: &QString) -> QString;
        /// Starts a new session in a project (`-1`: home folder). An empty
        /// harness uses the project's default.
        #[qinvokable]
        fn new_session(self: Pin<&mut Workspace>, project_id: i32, harness: &QString) -> QString;
        #[qinvokable]
        fn set_archived(self: Pin<&mut Workspace>, key: &QString, archived: bool);
        #[qinvokable]
        fn set_pinned(self: Pin<&mut Workspace>, key: &QString, pinned: bool);
        /// Stops the session's harness process; the session stays on disk.
        #[qinvokable]
        fn close_session(self: Pin<&mut Workspace>, key: &QString);
        /// Moves the session file to the desktop trash.
        #[qinvokable]
        fn trash_session(self: Pin<&mut Workspace>, key: &QString);
        #[qinvokable]
        fn rename_session(self: Pin<&mut Workspace>, key: &QString, name: &QString);
        /// Row key at `index`, for scripted use (`--selftest`).
        #[qinvokable]
        fn key_at(self: &Workspace, index: i32) -> QString;
        /// The window reports focus so notifications only fire when useful.
        #[qinvokable]
        fn set_window_active(self: Pin<&mut Workspace>, active: bool);
        /// Settings → Notifications → Send test notification.
        #[qinvokable]
        fn send_test_notification(self: Pin<&mut Workspace>);
        /// The notification settings: `{ enabled, whileFocused, sound,
        /// events: { completion: { on, title, message }, … } }` ("" titles
        /// and messages use the defaults).
        #[qinvokable]
        fn notify_settings_json(self: &Workspace) -> QString;
        #[qinvokable]
        fn set_notify_settings(self: Pin<&mut Workspace>, json: &QString);
        /// `{ variables: [...], defaults: { completion: { title, message }, … } }`.
        #[qinvokable]
        fn notify_defaults_json(self: &Workspace) -> QString;
        /// Recounts uncommitted changes for the sidebar soon (after a commit).
        #[qinvokable]
        fn refresh_changes(self: &Workspace);
        /// The agent new sessions outside a project start with: the setting,
        /// or (Automatic) the first one on and installed.
        #[qinvokable]
        fn default_agent(self: &Workspace) -> QString;
        /// A saved setting (see `Settings` in workspace.rs); "" when unset.
        #[qinvokable]
        fn setting(self: &Workspace, key: &QString) -> QString;
        #[qinvokable]
        fn set_setting(self: &Workspace, key: &QString, value: &QString);
        /// Saves a session template (`{ id?, name, harness, model, approval,
        /// askFirst, goal, worktree, text }`; the same name replaces one):
        /// its id, or "" without a name.
        #[qinvokable]
        fn save_template(self: Pin<&mut Workspace>, json: &QString) -> QString;
        #[qinvokable]
        fn delete_template(self: Pin<&mut Workspace>, id: &QString);
        /// Watches a pull request's CI checks while they run (`{ service,
        /// host, repo, url, number, word, title }`) and notifies when they
        /// pass or fail.
        #[qinvokable]
        fn watch_request(self: Pin<&mut Workspace>, json: &QString);
        /// Saves a session (open or saved) as Markdown at `path`; the notice
        /// says how it went.
        #[qinvokable]
        fn export_markdown(self: Pin<&mut Workspace>, key: &QString, path: &QString);
        /// Copies a session as Markdown (`clipboardText`).
        #[qinvokable]
        fn copy_markdown(self: Pin<&mut Workspace>, key: &QString);
        /// Self-tests only: a phone's request (`{op, …}`) and harlo's answer.
        #[qinvokable]
        fn phone_test_call(self: Pin<&mut Workspace>, json: &QString) -> QString;
        /// Spending limits in dollars (0: none).
        #[qinvokable]
        fn set_spend_limits(self: Pin<&mut Workspace>, session: f64, day: f64);
        /// Lets a held session (or, with "", the day) spend another limit's
        /// worth.
        #[qinvokable]
        fn allow_more_spending(self: Pin<&mut Workspace>, key: &QString);
        /// Why a session takes no messages now ("" when it does).
        #[qinvokable]
        fn spend_hold(self: &Workspace, key: &QString) -> QString;
        /// Scheduled prompts, JSON: each schedule plus `describe` (when it
        /// runs, in words) and `projectName`.
        #[qinvokable]
        fn schedules_json(self: &Workspace) -> QString;
        /// Creates or updates a schedule from the editor's JSON (`repeat`:
        /// {kind: days, days, time "09:00"} | {kind: every, minutes} |
        /// {kind: once, at "2026-10-13 09:00"}). Returns an error, or "".
        #[qinvokable]
        fn save_schedule(self: Pin<&mut Workspace>, json: &QString) -> QString;
        #[qinvokable]
        fn delete_schedule(self: Pin<&mut Workspace>, id: i32);
        #[qinvokable]
        fn set_schedule_enabled(self: Pin<&mut Workspace>, id: i32, on: bool);
        #[qinvokable]
        fn run_schedule_now(self: Pin<&mut Workspace>, id: i32);
        /// Runs schedules that are due (called every 30 s). The first call
        /// also catches up runs missed while harlo was closed.
        #[qinvokable]
        fn tick_schedules(self: Pin<&mut Workspace>);
        /// Re-reads plan limits (`omp usage --json`) in the background.
        #[qinvokable]
        fn refresh_quota(self: Pin<&mut Workspace>);
        /// Pins the meter to an account's limits ("" follows the last used).
        #[qinvokable]
        fn pin_meter(self: Pin<&mut Workspace>, provider: &QString);
        /// A project's notes (-1: sessions outside any project).
        #[qinvokable]
        fn project_notes(self: &Workspace, project_id: i32) -> QString;
        #[qinvokable]
        fn set_project_notes(self: &Workspace, project_id: i32, text: &QString);
        /// Settings → General → start at login (an XDG autostart entry).
        #[qinvokable]
        fn autostart_enabled(self: &Workspace) -> bool;
        /// Turns it on or off; returns an error message, or "".
        #[qinvokable]
        fn set_autostart(self: &Workspace, on: bool) -> QString;
        /// The session's approval mode (`always-ask`, `write`, `yolo`):
        /// chosen in harlo, else the omp config's; "" for pi sessions.
        #[qinvokable]
        fn approval_mode(self: &Workspace, key: &QString) -> QString;
        /// The mode a new session of `harness` in the project (-1: none)
        /// starts in; "" when it has none.
        #[qinvokable]
        fn draft_approval(self: &Workspace, harness: &QString, project_id: i32) -> QString;
        /// Switches it; the running omp re-reads its overlay.
        #[qinvokable]
        fn set_approval_mode(self: Pin<&mut Workspace>, key: &QString, mode: &QString);
        /// Plan mode: the agent plans and doesn't change files.
        #[qinvokable]
        fn plan_mode(self: &Workspace, key: &QString) -> bool;
        #[qinvokable]
        fn set_plan_mode(self: Pin<&mut Workspace>, key: &QString, on: bool);
        /// A provider's logo as a data URL in `color` (`#rrggbb`), or "".
        #[qinvokable]
        fn provider_logo(self: &Workspace, provider: &QString, color: &QString) -> QString;
        /// A key press as shortcut text (`Ctrl+Shift+T`), for rebinding.
        #[qinvokable]
        fn key_sequence_text(self: &Workspace, key: i32, modifiers: i32) -> QString;
        /// Where harlo.log, crash reports and harness traffic logs go.
        #[qinvokable]
        fn logs_dir(self: &Workspace) -> QString;
        #[qinvokable]
        fn choose_theme(self: Pin<&mut Workspace>, family: &QString);
        #[qinvokable]
        fn choose_theme_mode(self: Pin<&mut Workspace>, mode: &QString);
        #[qinvokable]
        fn choose_text_scale(self: Pin<&mut Workspace>, scale: f64);
        #[qinvokable]
        fn choose_glass(self: Pin<&mut Workspace>, on: bool, opacity: f64);
        /// Sets one of `look`'s values (`value`: JSON); a null value goes
        /// back to the default.
        #[qinvokable]
        fn choose_look(self: Pin<&mut Workspace>, key: &QString, value: &QString);
        /// `kind` "ui" or "mono"; "" goes back to the desktop's font.
        #[qinvokable]
        fn choose_font(self: Pin<&mut Workspace>, kind: &QString, family: &QString);
        /// The desktop's interface (`fixed` false) or monospace font family.
        #[qinvokable]
        fn system_font(self: &Workspace, fixed: bool) -> QString;
        /// Installed font families; only fixed-pitch ones when `fixed`.
        #[qinvokable]
        fn font_families(self: &Workspace, fixed: bool) -> QStringList;
        /// Theme JSON (colours as `#AARRGGBB`) for a family and "dark"/"light".
        #[qinvokable]
        fn theme_json(self: &Workspace, family: &QString, variant: &QString) -> QString;
        /// `family\tdark preview\tlight preview` per theme family.
        #[qinvokable]
        fn theme_families(self: &Workspace) -> QStringList;
        /// Where `pi` / `omp` was found on this system, or "".
        #[qinvokable]
        fn detected_program(self: &Workspace, harness: &QString) -> QString;
        #[qinvokable]
        fn default_worktree_dir(self: &Workspace) -> QString;
        #[qinvokable]
        fn home_dir(self: &Workspace) -> QString;
        /// Models used in past sessions: JSON `[{ harness, model }]`, most
        /// recent first.
        #[qinvokable]
        fn known_models(self: &Workspace) -> QString;
        /// Starts a multirun: one prompt sent to several lanes. `form` is JSON
        /// `{ projectId, prompt, starter, attachments, askFirst, worktrees,
        /// base, lanes: [{ harness, model }] }`. Opens its overview; returns
        /// the run id.
        #[qinvokable]
        fn start_run(self: Pin<&mut Workspace>, form: &QString) -> QString;
        /// Runs, newest first: JSON `[{ id, title, created, kept, lanes }]`.
        #[qinvokable]
        fn runs_json(self: &Workspace) -> QString;
        /// One run with its lanes' live state (status, latest answer,
        /// changed files, key to open).
        #[qinvokable]
        fn run_json(self: &Workspace, id: &QString) -> QString;
        /// Keeps one lane: the others are snapshotted to
        /// `refs/harlo/runs/<run>/<lane>`, archived, and their worktrees and
        /// branches removed.
        #[qinvokable]
        fn keep_run_lane(self: Pin<&mut Workspace>, id: &QString, idx: i32);
        /// Forgets a run (its sessions stay).
        #[qinvokable]
        fn dismiss_run(self: Pin<&mut Workspace>, id: &QString);
        /// Sessions with a harness process: JSON `[{ key, title, status }]`,
        /// those waiting for you first.
        #[qinvokable]
        fn running_sessions(self: &Workspace) -> QString;
        /// The session file of a session key ("" before it is written).
        #[qinvokable]
        fn session_file(self: &Workspace, key: &QString) -> QString;
        /// Branches of a session file: JSON array of `{ leaf, current,
        /// messages, lastUser, divergesAt, timestamp }`, current first.
        #[qinvokable]
        fn session_branches(self: &Workspace, path: &QString) -> QString;
        /// `session_branches`, off the UI thread (a remote session's file is
        /// read over SSH): answered by `sessionBranchesReady`.
        #[qinvokable]
        fn request_session_branches(self: &Workspace, path: &QString);
        /// A temporary copy of one branch, to preview it read-only.
        #[qinvokable]
        fn branch_preview_file(self: &Workspace, path: &QString, leaf: &QString) -> QString;
        /// Copies a branch into a new session next to the original and opens
        /// it (read-only until you send). Returns its key.
        #[qinvokable]
        fn continue_branch(self: Pin<&mut Workspace>, path: &QString, leaf: &QString) -> QString;
        /// Full-text search over every session's messages: JSON array of
        /// `{ path, index, role, snippet, title, harness, cwd, modified }`.
        #[qinvokable]
        fn search_messages(self: &Workspace, query: &QString, limit: i32) -> QString;
        /// "<modified ns>:<size>" of a file, or "" when it does not exist;
        /// compared over time to notice changes.
        #[qinvokable]
        fn file_stamp(self: &Workspace, path: &QString) -> QString;
        /// A remote file copied here (for viewing an image); a local path
        /// as it is; "" if it can't be fetched.
        #[qinvokable]
        fn local_copy(self: &Workspace, path: &QString) -> QString;
        /// Repositories hidden from the sidebar change counts.
        #[qinvokable]
        fn ignored_repo_list(self: &Workspace) -> QStringList;
        /// Top level of the repository a sidebar session runs in, or "".
        #[qinvokable]
        fn session_repo(self: &Workspace, key: &QString) -> QString;
        #[qinvokable]
        fn is_repo_ignored(self: &Workspace, root: &QString) -> bool;
        /// Leaves a repository out of (or back in) the sidebar change counts.
        #[qinvokable]
        fn set_repo_ignored(self: Pin<&mut Workspace>, root: &QString, ignored: bool);

        /// Local branches of a project's repository, current branch first
        /// (empty when the project is not a git repository).
        #[qinvokable]
        fn branches(self: &Workspace, project_id: i32) -> QStringList;
        /// `branches`, off the UI thread (a remote project's go over SSH):
        /// answered by `branchesReady`.
        #[qinvokable]
        fn request_branches(self: &Workspace, project_id: i32);
        /// The sessions `key`'s agent started (orchestration), as its
        /// subagent dock lists them, JSON.
        #[qinvokable]
        fn orchestrated_json(self: &Workspace, key: &QString) -> QString;
        /// Stops what a session is doing (it stays open).
        #[qinvokable]
        fn stop_session(self: &Workspace, key: &QString);
        /// The composer's Delegate menu: what to send the agent, for
        /// `{ action, members: [{ agent, model }], note, worktree }`.
        #[qinvokable]
        fn delegate_prompt(self: &Workspace, request: &QString) -> QString;
        /// Checks the providers (their models) unless that's been done.
        #[qinvokable]
        fn ensure_providers(self: Pin<&mut Workspace>);
        /// Settings → Orchestration: the skills (installed or not) and the
        /// agents' bridges, JSON.
        #[qinvokable]
        fn orchestration_json(self: &Workspace) -> QString;
        /// The window has drawn once: the graphics choice works.
        #[qinvokable]
        fn graphics_drawn(self: &Workspace);
        /// The graphics choice this run started with ("" when the
        /// environment chose).
        #[qinvokable]
        fn graphics_started_with(self: &Workspace) -> QString;
        /// Starts harlo again after it quits (the caller quits it).
        #[qinvokable]
        fn restart_app(self: &Workspace);
        /// Turns orchestration on or off: off, sessions don't get harlo's
        /// tools, and the skills and bridges harlo installed are removed
        /// (and put back when it's on again); the problem, if any.
        #[qinvokable]
        fn set_orchestration_enabled(self: Pin<&mut Workspace>, on: bool) -> QString;
        /// Installs or removes a skill; the problem, if any.
        #[qinvokable]
        fn set_orchestration_skill(self: Pin<&mut Workspace>, name: &QString, on: bool) -> QString;
        /// Adds harlo's tools to pi's or omp's MCP config, or removes them;
        /// the problem, if any.
        #[qinvokable]
        fn set_orchestration_bridge(self: Pin<&mut Workspace>, agent: &QString, on: bool) -> QString;
        /// Looks again at what's set up for orchestration on the instance
        /// shown (a host: in the background, then `orchestrationRevision`).
        #[qinvokable]
        fn load_orchestration(self: Pin<&mut Workspace>);
        /// Conversation starters for the new-session page (JSON array of
        /// `{ id, label, icon, prompt }`).
        #[qinvokable]
        fn starters(self: &Workspace) -> QString;
        /// Starts a session from the new-session page and sends its first
        /// message. `form` is JSON: `{ projectId (-1 = home), harness ("" =
        /// the project's), worktreeBranch ("" = current checkout), base,
        /// starter (id or ""), text, attachments: [paths], askFirst }`.
        /// With a worktree, the session opens once it is ready and this
        /// returns ""; otherwise it returns the new session's key.
        #[qinvokable]
        fn start_draft(self: Pin<&mut Workspace>, form: &QString) -> QString;
        /// A fresh branch name such as `harlo/brave-otter`.
        #[qinvokable]
        fn suggest_branch(self: &Workspace) -> QString;
        /// Creates a worktree on `branch` (from `base` if new) and starts a
        /// session in it. Runs in the background; the session opens when ready.
        #[qinvokable]
        fn new_worktree_session(self: Pin<&mut Workspace>, project_id: i32, branch: &QString, base: &QString, harness: &QString);
        /// JSON describing the worktree a session runs in, or "" if none:
        /// {path, branch, base, mainBranch, exists, dirty, ahead, behind}.
        #[qinvokable]
        fn worktree_info(self: &Workspace, key: &QString) -> QString;
        /// Merges the session's worktree branch into the main checkout.
        #[qinvokable]
        fn merge_worktree(self: Pin<&mut Workspace>, key: &QString);
        /// Stops the worktree's sessions and removes its folder; optionally
        /// deletes the branch. `force` discards uncommitted changes.
        #[qinvokable]
        fn remove_worktree(self: Pin<&mut Workspace>, key: &QString, delete_branch: bool, force: bool);
        #[qinvokable]
        fn project_setup(self: &Workspace, project_id: i32) -> QString;
        #[qinvokable]
        fn set_project_setup(self: Pin<&mut Workspace>, project_id: i32, setup: &QString);
        #[qinvokable]
        fn clear_notice(self: Pin<&mut Workspace>);

        /// The instance harlo starts on ("" this computer).
        #[qinvokable]
        fn set_default_instance_to(self: Pin<&mut Workspace>, name: &QString);
        /// Shows a notice (the bar over the chat).
        #[qinvokable]
        fn show_notice(self: Pin<&mut Workspace>, text: &QString, error: bool);
        /// Shows another instance ("" this computer): its projects, sessions
        /// and tabs.
        #[qinvokable]
        fn switch_instance(self: Pin<&mut Workspace>, name: &QString);
        /// The instance a sidebar key, tab or path belongs to ("" this
        /// computer; "*" for views, which belong to every one).
        #[qinvokable]
        fn instance_of(self: &Workspace, key: &QString) -> QString;
        /// Shows a file (absolute path) in the single file tab (`file:<path>`),
        /// replacing whatever file it showed, and activates it.
        #[qinvokable]
        fn open_file_tab(self: Pin<&mut Workspace>, path: &QString);
        /// A file's changes side by side, in the diff tab (one: another
        /// file replaces the one in it). `staged`: the index against HEAD,
        /// else the working tree against the index.
        #[qinvokable]
        fn open_diff_tab(self: Pin<&mut Workspace>, path: &QString, staged: bool);
        /// Closes a tab. A running agent keeps running (it stays in the
        /// sidebar); a saved session without a process is released.
        #[qinvokable]
        fn close_tab(self: Pin<&mut Workspace>, key: &QString);
        /// Activates the tab `delta` places from the active one (wraps).
        #[qinvokable]
        fn next_tab(self: Pin<&mut Workspace>, delta: i32);
        #[qinvokable]
        fn activate_tab(self: Pin<&mut Workspace>, index: i32);
        #[qinvokable]
        fn move_tab(self: Pin<&mut Workspace>, from: i32, to: i32);
        /// Palette search over sessions and projects, best first:
        /// `kind\tkey\ttitle\tsubtitle\tharness\tmodified\tstatus` each,
        /// kind = session | project. An empty query lists recent sessions.
        #[qinvokable]
        fn search(self: &Workspace, query: &QString, limit: i32) -> QStringList;
        /// Folders of a project: its path plus its worktrees (for the
        /// Stats project filter).
        #[qinvokable]
        fn project_roots(self: &Workspace, project_id: i32) -> QStringList;
        /// Project of the active session (worktrees count), or -1.
        #[qinvokable]
        fn active_project_id(self: &Workspace) -> i32;
        /// `id\tname\tharness` per project, for the palette.
        #[qinvokable]
        fn project_list(self: &Workspace) -> QStringList;
        /// Settings → Projects: `[{ id, name, path, folder, harness, setup,
        /// icon, color, remote }]`.
        #[qinvokable]
        fn projects_json(self: &Workspace) -> QString;
        /// Settings → Providers: `[{ id, label, enabled, canChat, state
        /// ("checking", "ready", "missing", "error"), path, version,
        /// modelCount, models: [name], error }]`.
        #[qinvokable]
        fn providers_json(self: &Workspace) -> QString;
        /// Checks every provider again (in the background).
        #[qinvokable]
        fn refresh_providers(self: Pin<&mut Workspace>);
        #[qinvokable]
        fn set_provider_enabled(self: Pin<&mut Workspace>, id: &QString, on: bool);
        /// The providers harlo can start sessions with now (on, and
        /// supported): `[{ id, label }]`.
        #[qinvokable]
        fn chat_providers_json(self: &Workspace) -> QString;
        #[qinvokable]
        fn set_project_name(self: Pin<&mut Workspace>, id: i32, name: &QString);
        /// A preset icon ("" the folder) and colour ("" none).
        #[qinvokable]
        fn set_project_look(self: Pin<&mut Workspace>, id: i32, icon: &QString, color: &QString);
        /// A `qrc:` URL of the file-type icon for a file name or path.
        #[qinvokable]
        fn file_icon(self: &Workspace, path: &QString) -> QString;
    }

    unsafe extern "RustQt" {
        /// A later launch handed over a command: `show` or `toggle-dropdown`.
        #[qsignal]
        #[cxx_name = "remoteCommand"]
        fn remote_command(self: Pin<&mut Workspace>, command: &QString);
        /// A notification was clicked: the window should come forward.
        #[qsignal]
        #[cxx_name = "raiseRequested"]
        fn raise_requested(self: Pin<&mut Workspace>);
        /// A project's branches (`request_branches`), current first.
        #[qsignal]
        #[cxx_name = "branchesReady"]
        fn branches_ready(self: Pin<&mut Workspace>, project_id: i32, branches: &QStringList);
        /// A session file's branches (`request_session_branches`), JSON.
        #[qsignal]
        #[cxx_name = "sessionBranchesReady"]
        fn session_branches_ready(self: Pin<&mut Workspace>, path: &QString, branches: &QString);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut Workspace>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QVector_i32);
    }

    extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(self: Pin<&mut Workspace>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut Workspace>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(self: Pin<&mut Workspace>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut Workspace>);
    }

    unsafe extern "RustQt" {
        #[inherit]
        fn index(self: &Workspace, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &Workspace, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &Workspace) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &Workspace, parent: &QModelIndex) -> i32;
    }
}

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QStringList, QVariant, QVector};
use harlo_core::db::{Db, Project, SessionMeta, WorktreeRecord};
use harlo_core::git;
use harlo_core::listdiff::{ListChange, diff_by};
use harlo_core::scanner::{ScanDelta, Scanner};
use harlo_core::sidebar::{self, LiveInfo, SidebarInput, SidebarKind, SidebarRow};
use harlo_core::{SessionHandle, SessionSink, SessionStatus, SessionUpdate};
use harlo_harness::{HarnessConfig, HarnessKind, RemoteLaunch, SessionInfo, SessionTarget};
use serde_json::json;

use crate::registry::{self, LiveRef};
use qobject::Workspace;

/// A folder as people read it: `~/app` here, `nas:~/app` on a remote host.
fn where_on_host(path: &Path) -> String {
    match harlo_core::remote::split(path) {
        Some((host, p)) => {
            let home = harlo_core::remote::info_of(&host).map(|i| i.home).unwrap_or_default();
            let p = if !home.is_empty() && (p == home || p.starts_with(&format!("{home}/"))) { format!("~{}", &p[home.len()..]) } else { p };
            format!("{host}:{p}")
        }
        None => sidebar::tilde(path),
    }
}

/// The penguins (Theme.qml's `avatars`; icons/avatars/).
const AVATARS: [&str; 6] = ["classic", "beanie", "glasses", "headphones", "jacket", "scarf"];

const ROLE_KIND: i32 = 0x100;
const ROLE_KEY: i32 = 0x101;
const ROLE_PROJECT: i32 = 0x102;
const ROLE_TITLE: i32 = 0x103;
const ROLE_SUBTITLE: i32 = 0x104;
const ROLE_HARNESS: i32 = 0x105;
const ROLE_STATUS: i32 = 0x106;
const ROLE_PATH: i32 = 0x107;
const ROLE_MODIFIED: i32 = 0x108;
const ROLE_COLLAPSED: i32 = 0x109;
const ROLE_PINNED: i32 = 0x10a;
const ROLE_COUNT: i32 = 0x10b;
const ROLE_BRANCH: i32 = 0x10c;
const ROLE_CHANGES: i32 = 0x10d;
const ROLE_ICON: i32 = 0x10e;
const ROLE_COLOR: i32 = 0x10f;

const SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// Remote hosts' session folders are listed this often (over SSH).
const REMOTE_SCAN_INTERVAL: Duration = Duration::from_secs(10);
/// A host that didn't answer is tried again after this.
const REMOTE_RETRY: Duration = Duration::from_secs(60);
const WORKTREE_INTERVAL: Duration = Duration::from_secs(20);
/// How often the sidebar's uncommitted-change counts are refreshed (sooner
/// when a turn ends, the window comes back, or new folders show up).
const CHANGES_INTERVAL: Duration = Duration::from_secs(10);
/// The search index catches up with session files at least this often.
const SEARCH_INTERVAL: Duration = Duration::from_secs(60);
/// Database key: repositories left out of the change counts, one per line.
const IGNORED_REPOS_KEY: &str = "changes.ignored_repos";

struct LiveEntry {
    info: LiveInfo,
    sub: u64,
    /// Questions and approvals waiting, oldest first, with when each came
    /// (unix ms): for notifications, and phones' "Needs you".
    requests: Vec<(harlo_core::UiRequest, i64)>,
    /// The session's model (for notifications) and its `provider/id`.
    model: String,
    model_id: String,
    /// omp goal status ("" when there is no goal).
    goal_status: String,
}

/// Something a session did that may notify (Settings → Notifications).
enum Note {
    Event(harlo_core::Notable),
    /// It asks you something (an ask / select / approval): in words, and
    /// the request as phones get it.
    Question(String, serde_json::Value),
    /// Its omp goal ended (status, objective).
    GoalSettled(String, String),
}

/// How solid glass menus and dialogs are by default (80%).
pub const GLASS_OPACITY: f64 = 0.8;

#[derive(Default)]
pub struct WorkspaceRust {
    active_key: QString,
    theme_family: QString,
    theme_mode: QString,
    text_scale: f64,
    projects_revision: i32,
    providers_revision: i32,
    /// The last check of each provider (`None`: checking).
    provider_checks: HashMap<String, Option<harlo_core::providers::Status>>,
    /// Which round of checks is current (an earlier round's late results
    /// are dropped).
    provider_round: u64,
    glass: bool,
    glass_opacity: f64,
    look: QString,
    templates_json: QString,
    clipboard_text: QString,
    spend_json: QString,
    spend_limits: harlo_core::spending::Limits,
    spend_ledger: harlo_core::spending::Ledger,
    /// Live sessions' spending, by key.
    spend: HashMap<String, harlo_core::spending::SessionSpend>,
    /// Sessions stopped at a limit (told once until allowed more).
    spend_stopped: HashSet<String>,
    /// Pull requests whose checks run: the request, since when.
    ci_watch: Vec<(serde_json::Value, std::time::Instant)>,
    ci_polling: bool,
    ui_font: QString,
    mono_font: QString,
    busy_count: i32,
    input_count: i32,
    active_title: QString,
    project_count: i32,
    session_count: i32,
    error_text: QString,
    notice: QString,
    notice_error: bool,
    busy: bool,

    db: Option<Db>,
    worktrees: Vec<WorktreeRecord>,
    projects: Vec<Project>,
    schedules_revision: i32,
    modes_revision: i32,
    quota_json: QString,
    meter_last: QString,
    meter_pin: QString,
    copies_revision: i32,
    orchestration_revision: i32,
    orchestration_on: bool,
    /// Orchestration: each started session's `HARLO_SESSION` token → its
    /// key, and the sessions agents started (by key).
    orch_tokens: HashMap<String, String>,
    orch_children: HashMap<String, OrchChild>,
    /// Remote files being copied here (`localCopy`).
    copying: std::cell::RefCell<HashSet<PathBuf>>,
    quota_loading: bool,
    /// Scheduled prompts (Settings → Schedules).
    schedules: Vec<harlo_core::schedules::Schedule>,
    /// The first schedule check after starting handles missed runs.
    schedules_started: bool,
    sessions: HashMap<PathBuf, SessionInfo>,
    meta: HashMap<PathBuf, SessionMeta>,
    live: HashMap<String, LiveEntry>,
    next_live: u64,
    filter: String,
    other_collapsed: bool,
    expanded: Vec<i64>,
    rows: Vec<SidebarRow>,
    window_active: bool,
    tabs: QStringList,
    /// Open session keys in tab order.
    tab_keys: Vec<String>,
    instance: QString,
    default_instance: QString,
    /// The other instances' tabs (and active tab), kept while away.
    instance_tabs: HashMap<String, (Vec<String>, String)>,
    /// Orchestration on each host, as last looked at (Settings).
    orch_hosts: HashMap<String, serde_json::Value>,
    /// Tab session files to reopen once the first scan has found them, and
    /// which of them was active.
    restore_tabs: Option<(Vec<PathBuf>, Option<PathBuf>)>,
    /// Uncommitted changed files per session folder (inside a repository).
    change_counts: HashMap<PathBuf, usize>,
    /// Folders included in the last count, repositories or not.
    change_checked: HashSet<PathBuf>,
    /// Wakes the change counter early.
    change_kick: Arc<tokio::sync::Notify>,
    /// Repositories (top-level paths) whose changes the sidebar doesn't count.
    ignored_repos: HashSet<PathBuf>,
    /// Message search index, shared with its background updater.
    search: Option<Arc<std::sync::Mutex<harlo_core::search::SearchIndex>>>,
    /// Wakes the search indexer (new or changed session files).
    search_kick: Arc<tokio::sync::Notify>,
    /// Remote project folders per host, for the remote session scan.
    remote_roots: Arc<std::sync::Mutex<RemoteRoots>>,
    /// Wakes the remote session scan (a remote project was added).
    remote_kick: Arc<tokio::sync::Notify>,
    /// Restored tabs of remote sessions not found yet (their host is
    /// scanned after the local folders).
    pending_remote_tabs: Vec<PathBuf>,
    last_notification: QString,
    /// When each notification tag (event + session) last went out.
    notified_at: HashMap<String, std::time::Instant>,
    /// Running sessions that are run lanes: live key → (run id, lane index).
    lane_keys: HashMap<String, (String, i64)>,
    /// omp sessions' `--config` overlay files and the model each was last
    /// written for.
    overlays: HashMap<String, (PathBuf, String)>,
    /// Sessions' approval and plan modes, by live key.
    modes: HashMap<String, SessionModes>,
    /// A remote host's omp approval mode (its config, read over SSH off the
    /// UI thread; asked for once).
    remote_approval: Arc<std::sync::Mutex<HashMap<String, Option<String>>>>,
    /// Remote files' stamps (`fileStamp`): the last known, and whether a
    /// fresh one is being asked for.
    remote_stamps: Arc<std::sync::Mutex<HashMap<String, (String, bool)>>>,
    /// Remote session files as last read for their branches (the branch
    /// dialog previews and continues from these).
    branch_texts: Arc<std::sync::Mutex<HashMap<String, String>>>,
}

/// Remote project folders, grouped by host name.
type RemoteRoots = Vec<(String, Vec<PathBuf>)>;

/// A session's approval mode (None: the omp config's) and plan mode.
#[derive(Default, Clone)]
struct SessionModes {
    approval: Option<String>,
    plan: bool,
}

/// Forwards the parts of a live session's updates the sidebar needs.
struct WorkspaceSink {
    thread: CxxQtThread<Workspace>,
    key: String,
}

impl SessionSink for WorkspaceSink {
    fn deliver(&self, updates: Vec<SessionUpdate>) {
        let relevant: Vec<SessionUpdate> = updates
            .into_iter()
            .filter(|u| {
                matches!(
                    u,
                    SessionUpdate::Status(_)
                        | SessionUpdate::Meta(_)
                        | SessionUpdate::UiRequest(_)
                        | SessionUpdate::UiCleared(_)
                        | SessionUpdate::Notable(_)
                        | SessionUpdate::Goal(_)
                )
            })
            .collect();
        if relevant.is_empty() {
            return;
        }
        let key = self.key.clone();
        let _ = self.thread.queue(move |this| this.on_live_updates(&key, relevant));
    }
}

impl cxx_qt::Initialize for Workspace {
    fn initialize(mut self: Pin<&mut Self>) {
        // Overlays of earlier runs, and the shared one older versions wrote.
        let _ = std::fs::remove_dir_all(overlays_dir());
        if let Some(dir) = harlo_core::db::default_path().parent() {
            let _ = std::fs::remove_file(dir.join("omp-overlay.yml"));
        }
        match Db::open(&harlo_core::db::default_path()) {
            Ok(db) => {
                let projects = db.projects().unwrap_or_default();
                let meta = db.all_session_meta().unwrap_or_default().into_iter().collect();
                let other_collapsed = db.get("sidebar.other_collapsed").ok().flatten().as_deref() == Some("1");
                let saved_tabs: Vec<PathBuf> = db
                    .get("tabs")
                    .ok()
                    .flatten()
                    .map(|t| t.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect())
                    .unwrap_or_default();
                // Older versions only remembered the last session.
                let active_tab = db
                    .get("tabs.active")
                    .ok()
                    .flatten()
                    .or_else(|| db.get("last.session_file").ok().flatten())
                    .map(PathBuf::from);
                let worktrees = db.worktrees().unwrap_or_default();
                let schedules = db.schedules().unwrap_or_default();
                let ignored_repos = db
                    .get(IGNORED_REPOS_KEY)
                    .ok()
                    .flatten()
                    .map(|t| t.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect())
                    .unwrap_or_default();
                let get = |k: &str| db.get(&format!("settings.{k}")).ok().flatten().unwrap_or_default();
                let (family, mode, scale) = (get("theme_family"), get("theme_mode"), get("text_scale"));
                crate::diagnostics::set_protocol_logging(get("debug_protocol") == "1");
                let mut this = self.as_mut().rust_mut();
                this.ignored_repos = ignored_repos;
                // (Ember was called OpenChamber before 1.0.)
                let family = if family == "OpenChamber" { "Ember".to_owned() } else { family };
                this.theme_family = QString::from(if family.is_empty() { crate::themes::DEFAULT_FAMILY } else { &family });
                this.theme_mode = QString::from(if mode.is_empty() { "dark" } else { &mode });
                this.text_scale = scale.parse::<f64>().ok().filter(|s| (0.7..=1.6).contains(s)).unwrap_or(1.0);
                this.glass = get("glass") != "0";
                this.glass_opacity = get("glass_opacity").parse::<f64>().ok().filter(|o| (0.4..=1.0).contains(o)).unwrap_or(GLASS_OPACITY);
                this.ui_font = QString::from(&get("ui_font"));
                this.mono_font = QString::from(&get("mono_font"));
                this.spend_limits = harlo_core::spending::Limits::parse(&get("spend_limits"));
                this.spend_ledger = harlo_core::spending::Ledger::parse(&get("spend_today"));
                this.templates_json = QString::from(&harlo_core::templates::to_json(&harlo_core::templates::parse(&get("templates"))).to_string());
                this.look = QString::from(&Some(get("look")).filter(|l| serde_json::from_str::<serde_json::Value>(l).is_ok_and(|v| v.is_object())).unwrap_or_else(|| "{}".to_owned()));
                this.meter_last = QString::from(&get("meter_last"));
                this.meter_pin = QString::from(&get("meter_pin"));
                // The limits last seen, until fresh ones arrive.
                if let Some(cached) = db.get("cache.quota").ok().flatten() {
                    this.quota_json = QString::from(&cached);
                }
                let mut saved_tabs = saved_tabs;
                if let Some(a) = &active_tab
                    && !saved_tabs.contains(a) {
                        saved_tabs.push(a.clone());
                    }
                // Settings → General → Reopen tabs.
                this.restore_tabs = (get("restore_tabs") != "0").then_some((saved_tabs, active_tab));
                this.worktrees = worktrees;
                this.schedules = schedules;
                this.window_active = true;
                this.projects = projects;
                this.meta = meta;
                this.other_collapsed = other_collapsed;
                this.db = Some(db);
            }
            Err(e) => {
                self.as_mut().set_theme_family(QString::from(crate::themes::DEFAULT_FAMILY));
                self.as_mut().set_theme_mode(QString::from("dark"));
                self.as_mut().set_text_scale(1.0);
                self.as_mut().set_glass(true);
                self.as_mut().set_glass_opacity(GLASS_OPACITY);
                self.as_mut().set_error_text(QString::from(&format!("Could not open the database: {e:#}")))
            }
        }
        // The interface font for the whole app: the chosen one, else harlo's.
        crate::fonts::set_application_font_family(&QString::from(&crate::fonts::ui_family(&self.ui_font.to_string())));
        self.as_mut().rebuild();

        // Worktrees can also be made outside harlo; look for them now and then.
        let wt_thread = self.qt_thread();
        crate::runtime().spawn(async move {
            loop {
                if wt_thread.queue(|this| this.refresh_worktrees()).is_err() {
                    break;
                }
                tokio::time::sleep(WORKTREE_INTERVAL).await;
            }
        });

        // Commands from later launches (`harlo`, `harlo --toggle-dropdown`).
        let r_thread = self.qt_thread();
        crate::instance::set_handler(move |cmd| {
            let _ = r_thread.queue(move |this| this.remote_command(&QString::from(&cmd)));
        });

        // Uncommitted changes in the repositories of listed sessions.
        let ch_thread = self.qt_thread();
        let kick = self.change_kick.clone();
        crate::runtime().spawn(async move {
            let mut counter = git::ChangeCounter::default();
            loop {
                let (tx, rx) = tokio::sync::oneshot::channel();
                if ch_thread.queue(move |this| { let _ = tx.send(this.change_dirs()); }).is_err() {
                    break;
                }
                let Ok(dirs) = rx.await else { break };
                if let Some((dirs, ignored)) = dirs {
                    let (c, dirs, counts) = tokio::task::spawn_blocking(move || {
                        let counts = counter.count(&dirs, &ignored);
                        (counter, dirs, counts)
                    })
                    .await
                    .expect("change counter");
                    counter = c;
                    if ch_thread.queue(move |this| this.apply_change_counts(dirs, counts)).is_err() {
                        break;
                    }
                }
                tokio::select! {
                    _ = tokio::time::sleep(CHANGES_INTERVAL) => {}
                    _ = kick.notified() => {}
                }
                // Let bursts of triggers settle into one count.
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        });

        // Keep the message search index current.
        match harlo_core::search::SearchIndex::open(&harlo_core::search::default_path()) {
            Ok(index) => {
                let index = Arc::new(std::sync::Mutex::new(index));
                self.as_mut().rust_mut().search = Some(Arc::clone(&index));
                let s_thread = self.qt_thread();
                let kick = self.search_kick.clone();
                crate::runtime().spawn(async move {
                    loop {
                        // Wait for the first scan (or a later change) before reading files.
                        tokio::select! {
                            _ = kick.notified() => {}
                            _ = tokio::time::sleep(SEARCH_INTERVAL) => {}
                        }
                        // Every session the sidebar knows, here or on a remote
                        // host, with its stamp (modified, size) and agent.
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        if s_thread
                            .queue(move |this| {
                                let list: Vec<(PathBuf, i64, i64, HarnessKind)> = this
                                    .sessions
                                    .values()
                                    .map(|s| (s.path.clone(), s.modified.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64), s.size as i64, s.harness))
                                    .collect();
                                let _ = tx.send(list);
                            })
                            .is_err()
                        {
                            break;
                        }
                        let Ok(list) = rx.await else { break };
                        let index = Arc::clone(&index);
                        let _ = tokio::task::spawn_blocking(move || {
                            let kinds: HashMap<PathBuf, HarnessKind> = list.iter().map(|(p, _, _, k)| (p.clone(), *k)).collect();
                            let files: Vec<(PathBuf, i64, i64)> = list.into_iter().map(|(p, m, s, _)| (p, m, s)).collect();
                            if let Err(e) = index.lock().unwrap().update_with(&files, |p| session_messages(p, kinds.get(p).copied().unwrap_or(HarnessKind::Pi))) {
                                tracing::warn!("search index: {e:#}");
                            }
                        })
                        .await;
                    }
                });
            }
            Err(e) => tracing::warn!("search index unavailable: {e:#}"),
        }

        // Agents' harlo tools (orchestration).
        crate::orchestration::serve(self.qt_thread());
        // Start on the default instance.
        let start = self.saved_default_instance();
        self.as_mut().set_default_instance(QString::from(&start));
        self.as_mut().set_instance(QString::from(&start));
        // A graphics choice that didn't start last time.
        let dropped = crate::graphics::take_dropped();
        if !dropped.is_empty() {
            let name = match dropped.as_str() { "vulkan" => "Vulkan", "opengl" => "OpenGL", "software" => "Software", other => other };
            self.as_mut().set_notice_text(&format!("{name} graphics didn't start last time, so harlo went back to Automatic (Settings → General → Graphics)."), false);
        }
        let on = self.setting_str("orchestration") != "0";
        self.as_mut().set_orchestration_on(on);
        harlo_harness::set_orchestration(on);
        // Installed skills and bridges: brought up to this harlo.
        if on {
            harlo_core::orchestration::refresh_skills();
            if let Ok(exe) = std::env::current_exe() {
                harlo_core::orchestration::refresh_bridges(&exe);
            }
        }

        // Poll the session directories in the background.
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            let mut scanner = Scanner::default();
            loop {
                let (s, delta) = tokio::task::spawn_blocking(move || {
                    let delta = scanner.poll();
                    (scanner, delta)
                })
                .await
                .expect("scanner task");
                scanner = s;
                if !delta.is_empty() && thread.queue(move |this| this.apply_scan(delta)).is_err() {
                    break;
                }
                tokio::time::sleep(SCAN_INTERVAL).await;
            }
        });

        // The phone server's requests that need the workspace.
        let mut requests = crate::phone_bridge::serve();
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            while let Some((request, reply)) = requests.recv().await {
                let queued = thread.queue(move |this| {
                    let _ = reply.send(this.phone_call(&request));
                });
                if queued.is_err() {
                    break;
                }
            }
        });

        // Remote hosts with projects: their saved sessions, over SSH.
        let thread = self.qt_thread();
        let (roots, kick) = (self.remote_roots.clone(), self.remote_kick.clone());
        crate::runtime().spawn(async move {
            let mut scanner = harlo_core::remote_sessions::RemoteScanner::default();
            let mut retry_at: HashMap<String, std::time::Instant> = HashMap::new();
            let mut scanned: HashSet<String> = HashSet::new();
            loop {
                let hosts = roots.lock().unwrap().clone();
                for (name, host_roots) in hosts {
                    if retry_at.get(&name).is_some_and(|t| std::time::Instant::now() < *t) {
                        continue;
                    }
                    let host = harlo_core::remote::host(&name);
                    let (s, result) = tokio::task::spawn_blocking(move || {
                        let mut scanner = scanner;
                        let result = match &host {
                            Some(h) => scanner.poll(h, &host_roots),
                            // Removed from Settings: its sessions leave.
                            None => Ok(scanner.forget_host(&name)),
                        };
                        (scanner, (name, result))
                    })
                    .await
                    .expect("remote scan task");
                    scanner = s;
                    let (name, result) = result;
                    match result {
                        Ok(delta) => {
                            retry_at.remove(&name);
                            let first = scanned.insert(name);
                            if (first || !delta.is_empty()) && thread.queue(move |this| this.apply_scan(delta)).is_err() {
                                return;
                            }
                        }
                        Err(e) => {
                            tracing::info!("remote sessions on {name}: {e:#}");
                            retry_at.insert(name, std::time::Instant::now() + REMOTE_RETRY);
                        }
                    }
                }
                tokio::select! {
                    _ = kick.notified() => {}
                    _ = tokio::time::sleep(REMOTE_SCAN_INTERVAL) => {}
                }
            }
        });
    }
}

impl Workspace {
    fn db(&self) -> Option<&Db> {
        self.db.as_ref()
    }

    fn report<T>(mut self: Pin<&mut Self>, result: anyhow::Result<T>) -> Option<T> {
        match result {
            Ok(v) => Some(v),
            Err(e) => {
                self.as_mut().set_error_text(QString::from(&format!("{e:#}")));
                None
            }
        }
    }

    fn reload_projects(mut self: Pin<&mut Self>) {
        let projects = self.db().map(|db| db.projects()).transpose();
        if let Some(projects) = self.as_mut().report(projects) {
            self.as_mut().rust_mut().projects = projects.unwrap_or_default();
        }
        let n = self.projects_revision + 1;
        self.as_mut().set_projects_revision(n);
        // Remote projects, grouped by host, for the remote session scan.
        let mut roots: Vec<(String, Vec<PathBuf>)> = Vec::new();
        for p in &self.projects {
            if let Some(host) = harlo_core::remote::host_of(&p.path) {
                match roots.iter_mut().find(|(h, _)| *h == host) {
                    Some((_, list)) => list.push(p.path.clone()),
                    None => roots.push((host, vec![p.path.clone()])),
                }
            }
        }
        *self.remote_roots.lock().unwrap() = roots;
        self.remote_kick.notify_one();
        self.rebuild();
    }

    pub fn add_project(mut self: Pin<&mut Self>, path: &QString) {
        let path = PathBuf::from(url_to_path(&path.to_string()));
        // A remote folder was picked from its host's listing.
        let remote = harlo_core::remote::host_of(&path);
        if remote.is_none() && !path.is_dir() {
            self.set_error_text(QString::from(&format!("Not a folder: {}", path.display())));
            return;
        }
        let harness = match &remote {
            // What the host has (and Settings → Providers has on): omp, pi,
            // Claude, OpenCode, in that order; omp when nothing's known.
            Some(host) => harlo_core::remote::AGENTS
                .iter()
                .find(|a| harlo_core::remote::found_on(host, a) && HarnessKind::parse(a).is_some_and(|k| self.usable_harness(k)))
                .map_or("omp", |a| *a)
                .to_owned(),
            None => self.default_harness(),
        };
        // Another machine's project: show that instance.
        let inst = remote.clone().unwrap_or_default();
        if inst != self.instance.to_string() {
            self.as_mut().switch_instance(&QString::from(&inst));
        }
        let added = self.db().map(|db| db.add_project(&path, harness.as_str())).transpose();
        if self.as_mut().report(added).is_some() {
            self.as_mut().set_error_text(QString::default());
            self.as_mut().reload_projects();
            self.refresh_worktrees();
        }
    }

    pub fn remove_project(mut self: Pin<&mut Self>, id: i32) {
        let r = self.db().map(|db| db.remove_project(id as i64)).transpose();
        self.as_mut().report(r);
        self.reload_projects();
    }

    pub fn projects_json(&self) -> QString {
        let list: Vec<serde_json::Value> = self
            .projects
            .iter()
            .filter(|p| self.here(&p.path))
            .map(|p| {
                let remote = harlo_core::remote::is_remote(&p.path);
                serde_json::json!({
                    "id": p.id, "name": p.name, "harness": p.harness, "setup": p.setup,
                    "icon": harlo_core::project_icons::shown(&p.icon), "color": p.color,
                    "path": if remote { harlo_core::remote::display(&p.path) } else { p.path.to_string_lossy().into_owned() },
                    "folder": p.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                    "remote": remote,
                    "host": harlo_core::remote::host_of(&p.path).unwrap_or_default(),
                })
            })
            .collect();
        QString::from(&serde_json::Value::from(list).to_string())
    }

    fn provider_enabled(&self, kind: HarnessKind) -> bool {
        self.setting_str(&format!("provider_{}", kind.as_str())) != "0"
    }

    pub fn providers_json(&self) -> QString {
        let list: Vec<serde_json::Value> = HarnessKind::PROVIDERS
            .iter()
            .map(|&kind| {
                let id = kind.as_str();
                let check = self.provider_checks.get(id);
                let (state, status) = match check {
                    None | Some(None) => ("checking", None),
                    Some(Some(st)) if st.program.is_none() => ("missing", Some(st)),
                    Some(Some(st)) if st.error.is_some() => ("error", Some(st)),
                    Some(Some(st)) => ("ready", Some(st)),
                };
                serde_json::json!({
                    "id": id, "label": kind.label(), "enabled": self.provider_enabled(kind), "canChat": kind.can_chat(), "state": state,
                    "path": status.and_then(|s| s.program.as_ref()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
                    "version": status.and_then(|s| s.version.clone()).unwrap_or_default(),
                    "modelCount": status.map_or(0, |s| s.models.len()),
                    "models": status.map(|s| s.models.iter().map(|(_, name)| name.clone()).collect::<Vec<_>>()).unwrap_or_default(),
                    "modelIds": status.map(|s| s.models.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>()).unwrap_or_default(),
                    "error": status.and_then(|s| s.error.clone()).unwrap_or_default(),
                })
            })
            .collect();
        QString::from(&serde_json::Value::from(list).to_string())
    }

    pub fn refresh_providers(mut self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        self.as_mut().rust_mut().provider_round += 1;
        let round = self.provider_round;
        for kind in HarnessKind::PROVIDERS {
            let configured = self.setting_str(&format!("{}_path", kind.as_str()));
            self.as_mut().rust_mut().provider_checks.insert(kind.as_str().to_owned(), None);
            let thread = thread.clone();
            let handle = crate::runtime().handle().clone();
            crate::runtime().spawn_blocking(move || {
                let status = harlo_core::providers::check(kind, &configured, &handle);
                let _ = thread.queue(move |mut this| {
                    if this.provider_round != round {
                        return;
                    }
                    this.as_mut().rust_mut().provider_checks.insert(kind.as_str().to_owned(), Some(status));
                    let n = this.providers_revision + 1;
                    this.as_mut().set_providers_revision(n);
                });
            });
        }
        let n = self.providers_revision + 1;
        self.as_mut().set_providers_revision(n);
    }

    pub fn set_provider_enabled(mut self: Pin<&mut Self>, id: &QString, on: bool) {
        let Some(kind) = HarnessKind::parse(&id.to_string()) else { return };
        self.save_setting(&format!("provider_{}", kind.as_str()), if on { "1" } else { "0" });
        let n = self.providers_revision + 1;
        self.as_mut().set_providers_revision(n);
    }

    pub fn chat_providers_json(&self) -> QString {
        let list: Vec<serde_json::Value> = HarnessKind::PROVIDERS
            .iter()
            .filter(|&&k| k.can_chat() && self.provider_enabled(k))
            .map(|k| serde_json::json!({ "id": k.as_str(), "label": k.label() }))
            .collect();
        QString::from(&serde_json::Value::from(list).to_string())
    }

    /// Sessions can start with it: harlo supports it and it's on.
    /// The agents a new session in `dir` can use: on this computer, those
    /// turned on (Settings → Providers); on a remote host, those also found
    /// there (all of them until it's been checked).
    fn agents_at(&self, dir: &Path) -> Vec<&'static str> {
        let host = harlo_core::remote::host_of(dir);
        let info = host.as_deref().and_then(harlo_core::remote::info_of);
        let mut out: Vec<&'static str> = HarnessKind::PROVIDERS
            .iter()
            .filter(|&&k| self.usable_harness(k))
            .filter(|k| host.is_none() || info.as_ref().is_none_or(|i| i.has(k.as_str())))
            .map(|k| k.as_str())
            .collect();
        // (Tests start demo sessions.)
        if std::env::args().any(|a| a.starts_with("--selftest")) {
            out.push("demo");
        }
        out
    }

    fn usable_harness(&self, kind: HarnessKind) -> bool {
        kind == HarnessKind::Demo || (kind.can_chat() && self.provider_enabled(kind))
    }

    pub fn set_project_name(mut self: Pin<&mut Self>, id: i32, name: &QString) {
        let name = name.to_string().trim().to_owned();
        // Empty: back to the folder's name.
        let name = if name.is_empty() {
            let Some(p) = self.projects.iter().find(|p| p.id == id as i64) else { return };
            p.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.path.to_string_lossy().into_owned())
        } else {
            name
        };
        let r = self.db().map(|db| db.set_project_name(id as i64, &name)).transpose();
        self.as_mut().report(r);
        self.reload_projects();
    }

    pub fn set_project_look(mut self: Pin<&mut Self>, id: i32, icon: &QString, color: &QString) {
        let (icon, color) = (icon.to_string(), color.to_string());
        if !icon.is_empty() && !harlo_core::project_icons::PRESETS.contains(&icon.as_str()) {
            return;
        }
        let r = self.db().map(|db| db.set_project_look(id as i64, &icon, &color)).transpose();
        self.as_mut().report(r);
        self.reload_projects();
    }

    pub fn set_project_harness(mut self: Pin<&mut Self>, id: i32, harness: &QString) {
        let harness = harness.to_string();
        if !HarnessKind::parse(&harness).is_some_and(|k| k.can_chat()) {
            return;
        }
        let r = self.db().map(|db| db.set_project_harness(id as i64, &harness)).transpose();
        self.as_mut().report(r);
        self.reload_projects();
    }

    pub fn toggle_collapsed(mut self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        if key == "group:other" {
            let collapsed = !self.other_collapsed;
            self.as_mut().rust_mut().other_collapsed = collapsed;
            if let Some(db) = self.db() {
                let _ = db.set("sidebar.other_collapsed", if collapsed { "1" } else { "0" });
            }
            self.rebuild();
            return;
        }
        let Some(id) = key.strip_prefix("project:").and_then(|s| s.parse::<i64>().ok()) else { return };
        let Some(collapsed) = self.projects.iter().find(|p| p.id == id).map(|p| !p.collapsed) else { return };
        let r = self.db().map(|db| db.set_project_collapsed(id, collapsed)).transpose();
        self.as_mut().report(r);
        self.reload_projects();
    }

    pub fn show_more(mut self: Pin<&mut Self>, project_id: i32) {
        self.as_mut().rust_mut().expanded.push(project_id as i64);
        self.rebuild();
    }

    pub fn set_filter(mut self: Pin<&mut Self>, text: &QString) {
        self.as_mut().rust_mut().filter = text.to_string();
        self.rebuild();
    }

    pub fn open(mut self: Pin<&mut Self>, key: &QString) -> QString {
        let key = key.to_string();
        if let Some(path) = key.strip_prefix("file:") {
            self.as_mut().open_file_tab(&QString::from(path));
            return QString::from(&key);
        }
        if let Some((staged, path)) = diff_key(&key) {
            self.as_mut().open_diff_tab(&QString::from(path), staged);
            return QString::from(&key);
        }
        // App views (stats, settings) open as tabs of their own.
        if key.starts_with("view:") {
            self.as_mut().activate(&key);
            return QString::from(&key);
        }
        let Some(live_key) = self.as_mut().make_live(&key) else { return QString::default() };
        self.as_mut().activate(&live_key);
        QString::from(&live_key)
    }

    /// The live key for a session key (a saved session's file is opened,
    /// read-only until something is sent), without switching tabs. None if
    /// there's no such session.
    fn make_live(mut self: Pin<&mut Self>, key: &str) -> Option<String> {
        let live_key = if self.live.contains_key(key) {
            key.to_owned()
        } else if let Some(existing) = self.live_key_for_file(Path::new(key)) {
            existing
        } else {
            let info = self.sessions.get(Path::new(key)).cloned()?;
            let (program, mut remote) = self.launch_for(info.harness, &info.cwd);
            let overlay = self.as_mut().new_overlay(info.harness, "", remote.as_mut());
            let token = orch_token();
            let cfg = HarnessConfig {
                kind: info.harness,
                program,
                cwd: info.cwd.clone(),
                session: SessionTarget::Resume(info.path.clone()),
                extra_args: overlay.iter().map(|p| format!("--config={}", harlo_harness::on_host(p))).collect(),
                // (Orchestration: the agent's harlo tools know who's calling.)
                env: vec![("HARLO_SESSION".into(), token.clone())],
                remote,
            };
            let handle = {
                let _guard = crate::runtime().enter();
                SessionHandle::open_saved(cfg)
            };
            let key = self.as_mut().register_live(handle, info.harness.as_str(), info.cwd.clone(), Some(info.path.clone()), SessionStatus::Loading);
            self.as_mut().rust_mut().orch_tokens.insert(token, key.clone());
            if let Some(path) = overlay {
                self.as_mut().rust_mut().overlays.insert(key.clone(), (path, String::new()));
            }
            key
        };
        Some(live_key)
    }

    pub fn new_session(self: Pin<&mut Self>, project_id: i32, harness: &QString) -> QString {
        QString::from(&self.new_session_on(project_id, &harness.to_string(), ""))
    }

    /// A new session in a project (-1: the home folder) with an agent (""
    /// the project's) and a model (`provider/model`; "" the agent's own).
    fn new_session_on(self: Pin<&mut Self>, project_id: i32, harness: &str, model: &str) -> String {
        let project = self.projects.iter().find(|p| p.id == project_id as i64).cloned();
        let harness = if !harness.is_empty() {
            harness.to_owned()
        } else {
            project.as_ref().map(|p| p.harness.clone()).unwrap_or_else(|| self.default_harness())
        };
        let Some(kind) = HarnessKind::parse(&harness) else { return String::new() };
        let cwd = project
            .map(|p| p.path)
            .unwrap_or_else(|| self.instance_home());
        self.start_session_with(kind, cwd, model, true)
    }

    /// Spawns a session; `model` (`provider/model`) overrides the harness's
    /// default; `activate` makes it the active tab.
    fn start_session_with(mut self: Pin<&mut Self>, kind: HarnessKind, cwd: PathBuf, model: &str, activate: bool) -> String {
        let (program, mut remote) = self.launch_for(kind, &cwd);
        let overlay = self.as_mut().new_overlay(kind, model.trim(), remote.as_mut());
        let token = orch_token();
        let cfg = HarnessConfig {
            kind,
            program,
            cwd: cwd.clone(),
            // `--ephemeral` (testing): new sessions are not saved.
            session: if std::env::args().any(|a| a == "--ephemeral") { SessionTarget::Ephemeral } else { SessionTarget::New },
            extra_args: {
                let mut args: Vec<String> = overlay.iter().map(|p| format!("--config={}", harlo_harness::on_host(p))).collect();
                if !model.trim().is_empty() && kind != HarnessKind::Demo {
                    args.extend(["--model".into(), model.trim().to_owned()]);
                }
                args
            },
            // (Orchestration: the agent's harlo tools know who's calling.)
            env: vec![("HARLO_SESSION".into(), token.clone())],
            remote,
        };
        let handle = {
            let _guard = crate::runtime().enter();
            SessionHandle::spawn(cfg)
        };
        let key = self.as_mut().register_live(handle, kind.as_str(), cwd, None, SessionStatus::Starting);
        self.as_mut().rust_mut().orch_tokens.insert(token, key.clone());
        if let Some(path) = overlay {
            self.as_mut().rust_mut().overlays.insert(key.clone(), (path, model.trim().to_owned()));
        }
        if activate {
            self.as_mut().activate(&key);
        }
        key
    }

    fn register_live(
        mut self: Pin<&mut Self>,
        handle: SessionHandle,
        harness: &str,
        cwd: PathBuf,
        session_file: Option<PathBuf>,
        status: SessionStatus,
    ) -> String {
        self.as_mut().rust_mut().next_live += 1;
        let key = format!("live:{}", self.next_live);
        let handle = Arc::new(handle);
        let sub = handle.subscribe(WorkspaceSink { thread: self.qt_thread(), key: key.clone() });
        registry::insert(&key, LiveRef { handle, harness: harness.to_owned(), cwd: cwd.clone() });
        let info = LiveInfo {
            key: key.clone(),
            harness: harness.to_owned(),
            cwd,
            session_file,
            name: String::new(),
            first_message: String::new(),
            status,
            needs_input: false,
            unread: false,
            started_at: SystemTime::now(),
        };
        self.as_mut().rust_mut().live.insert(key.clone(), LiveEntry { info, sub, requests: Vec::new(), model: String::new(), model_id: String::new(), goal_status: String::new() });
        key
    }

    fn live_key_for_file(&self, path: &Path) -> Option<String> {
        self.live
            .values()
            .find(|e| e.info.session_file.as_deref() == Some(path))
            .map(|e| e.info.key.clone())
    }

    fn activate(mut self: Pin<&mut Self>, key: &str) {
        // Another machine's: show that instance first.
        if let Some(inst) = self.key_instance(key)
            && inst != self.instance.to_string()
        {
            self.as_mut().switch_instance(&QString::from(&inst));
        }
        let previous = self.active_key.to_string();
        if let Some(entry) = self.as_mut().rust_mut().live.get_mut(key) {
            entry.info.unread = false;
        }
        if let Some(file) = self.live.get(key).and_then(|e| e.info.session_file.clone()) {
            self.as_mut().mark_viewed(&file);
        }
        // New tabs open to the right of the active one.
        if !key.is_empty() && !self.tab_keys.iter().any(|k| k == key) {
            let at = self.tab_keys.iter().position(|k| *k == previous).map_or(self.tab_keys.len(), |i| i + 1);
            self.as_mut().rust_mut().tab_keys.insert(at, key.to_owned());
        }
        self.as_mut().set_active_key(QString::from(key));
        if previous != key {
            self.as_mut().release_if_idle(&previous);
        }
        self.as_mut().update_active_title();
        self.as_mut().save_tabs();
        self.rebuild();
    }

    /// Remembers the tabs' session files so they reopen next time.
    fn save_tabs(self: Pin<&mut Self>) {
        let Some(db) = self.db() else { return };
        // Session tabs are stored as their session file; file tabs keep their `file:` key.
        let saved = |k: &str| -> Option<String> {
            if k.starts_with("file:") || k.starts_with("view:") {
                return Some(k.to_owned());
            }
            self.live.get(k).and_then(|e| e.info.session_file.clone()).map(|p| p.to_string_lossy().into_owned())
        };
        // (Every instance's tabs: reopening sorts them out again.)
        let files: Vec<String> = self.tab_keys.iter().chain(self.instance_tabs.values().flat_map(|(k, _)| k)).filter_map(|k| saved(k)).collect();
        let _ = db.set("tabs", &files.join("\n"));
        let active = saved(&self.active_key.to_string()).unwrap_or_default();
        let _ = db.set("tabs.active", &active);
    }

    pub fn open_file_tab(mut self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        if path.is_empty() {
            return;
        }
        let key = format!("file:{path}");
        // One file tab: swap its file in place rather than adding a tab.
        if let Some(i) = self.tab_keys.iter().position(|k| k.starts_with("file:")) {
            self.as_mut().rust_mut().tab_keys[i] = key.clone();
        }
        self.activate(&key);
    }

    /// The instance of a key or path: Some("") this computer, Some(host),
    /// or None for views (every instance's).
    fn key_instance(&self, key: &str) -> Option<String> {
        let of = |p: &Path| harlo_core::remote::host_of(p).unwrap_or_default();
        if key.is_empty() || key.starts_with("view:") {
            return None;
        }
        if let Some(path) = key.strip_prefix("file:").or_else(|| diff_key(key).map(|d| d.1)) {
            return Some(of(Path::new(path)));
        }
        if let Some(id) = key.strip_prefix("project:").and_then(|i| i.parse::<i64>().ok()) {
            return self.projects.iter().find(|p| p.id == id).map(|p| of(&p.path));
        }
        if let Some(e) = self.live.get(key) {
            return Some(of(&e.info.cwd));
        }
        Some(of(Path::new(key)))
    }

    pub fn show_notice(self: Pin<&mut Self>, text: &QString, error: bool) {
        self.set_notice_text(&text.to_string(), error);
    }

    pub fn set_default_instance_to(mut self: Pin<&mut Self>, name: &QString) {
        self.save_setting("instance_default", &name.to_string());
        self.as_mut().set_default_instance(name.clone());
    }

    /// The saved default instance, if it still exists.
    fn saved_default_instance(&self) -> String {
        let name = self.setting_str("instance_default");
        if name.is_empty() || harlo_core::remote::host(&name).is_some() { name } else { String::new() }
    }

    pub fn instance_of(&self, key: &QString) -> QString {
        QString::from(&self.key_instance(&key.to_string()).unwrap_or_else(|| "*".into()))
    }

    pub fn switch_instance(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        let current = self.instance.to_string();
        if name == current {
            return;
        }
        // This one's tabs are kept for coming back; the other's come back.
        let keep = (std::mem::take(&mut self.as_mut().rust_mut().tab_keys), self.active_key.to_string());
        self.as_mut().rust_mut().instance_tabs.insert(current, keep);
        let (keys, active) = self.as_mut().rust_mut().instance_tabs.remove(&name).unwrap_or_default();
        self.as_mut().rust_mut().tab_keys = keys;
        self.as_mut().set_instance(QString::from(&name));
        let active = if self.tab_keys.contains(&active) { active } else { self.tab_keys.first().cloned().unwrap_or_default() };
        self.as_mut().set_active_key(QString::from(&active));
        self.as_mut().update_active_title();
        self.as_mut().save_tabs();
        let n = self.projects_revision + 1;
        self.as_mut().set_projects_revision(n);
        self.rebuild();
    }

    /// The home folder of the instance shown: this computer's, or the
    /// host's (as its check found it).
    fn instance_home(&self) -> PathBuf {
        let inst = self.instance.to_string();
        if !inst.is_empty()
            && let Some(home) = harlo_core::remote::info_of(&inst).map(|i| i.home).filter(|h| !h.is_empty())
        {
            return harlo_core::remote::join(&inst, &home);
        }
        PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/".into()))
    }

    /// A path is this instance's.
    fn here(&self, path: &Path) -> bool {
        harlo_core::remote::host_of(path).unwrap_or_default() == self.instance.to_string()
    }

    pub fn open_diff_tab(mut self: Pin<&mut Self>, path: &QString, staged: bool) {
        let path = path.to_string();
        if path.is_empty() {
            return;
        }
        let key = format!("diff:{}:{path}", staged as u8);
        if let Some(i) = self.tab_keys.iter().position(|k| k.starts_with("diff:")) {
            self.as_mut().rust_mut().tab_keys[i] = key.clone();
        }
        self.activate(&key);
    }

    pub fn close_tab(mut self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        let Some(pos) = self.tab_keys.iter().position(|k| *k == key) else { return };
        self.as_mut().rust_mut().tab_keys.remove(pos);
        if self.active_key.to_string() == key {
            // Prefer the tab that slid into this place, else the one before.
            let next = self.tab_keys.get(pos).or_else(|| self.tab_keys.get(pos.wrapping_sub(1))).cloned();
            match next {
                Some(n) => self.as_mut().activate(&n),
                None => {
                    self.as_mut().set_active_key(QString::default());
                    self.as_mut().set_active_title(QString::default());
                }
            }
        }
        self.as_mut().release_if_idle(&key);
        self.as_mut().save_tabs();
        self.rebuild();
    }

    pub fn next_tab(self: Pin<&mut Self>, delta: i32) {
        let n = self.tab_keys.len() as i32;
        if n == 0 {
            return;
        }
        let current = self.tab_keys.iter().position(|k| *k == self.active_key.to_string()).map_or(0, |i| i as i32);
        let index = (current + delta).rem_euclid(n);
        self.activate_tab(index);
    }

    pub fn activate_tab(mut self: Pin<&mut Self>, index: i32) {
        if let Some(key) = self.tab_keys.get(index.max(0) as usize).cloned() {
            self.as_mut().activate(&key);
        }
    }

    pub fn move_tab(mut self: Pin<&mut Self>, from: i32, to: i32) {
        let n = self.tab_keys.len();
        let (from, to) = (from.max(0) as usize, to.max(0) as usize);
        if from >= n || to >= n || from == to {
            return;
        }
        let key = self.as_mut().rust_mut().tab_keys.remove(from);
        self.as_mut().rust_mut().tab_keys.insert(to, key);
        self.as_mut().save_tabs();
        self.update_tabs();
    }

    fn update_tabs(mut self: Pin<&mut Self>) {
        let mut list = QStringList::default();
        for key in &self.tab_keys {
            if let Some(view) = key.strip_prefix("view:") {
                let run_title;
                let title = match view {
                    "stats" => "Stats",
                    "settings" => "Settings",
                    "new" => "New session",
                    v if v.starts_with("run:") => {
                        run_title = format!("Run: {}", self.find_run(&v[4..]).map(|r| r.title).unwrap_or_default());
                        run_title.as_str()
                    }
                    other => other,
                };
                list.append(QString::from(&format!("{key}\t{title}\t\tview\t")));
                continue;
            }
            if let Some(path) = key.strip_prefix("file:") {
                let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_owned());
                list.append(QString::from(&format!("{key}\t{}\t\tfile\t", name.replace(['\t', '\n'], " "))));
                continue;
            }
            if let Some((staged, path)) = diff_key(key) {
                let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_owned());
                let title = format!("{} ({})", name.replace(['\t', '\n'], " "), if staged { "Staged" } else { "Working Tree" });
                list.append(QString::from(&format!("{key}\t{title}\t\tdiff\t")));
                continue;
            }
            let Some(entry) = self.live.get(key) else { continue };
            let info = &entry.info;
            let saved = info.session_file.as_ref().and_then(|f| self.sessions.get(f));
            let title = if !info.name.is_empty() {
                info.name.clone()
            } else if let Some(s) = saved.filter(|s| !s.name.is_empty() || !s.first_message.is_empty()) {
                s.display_title().to_owned()
            } else if !info.first_message.is_empty() {
                info.first_message.clone()
            } else {
                "New session".to_owned()
            };
            let branch = self
                .worktrees
                .iter()
                .filter(|w| info.cwd.starts_with(&w.path))
                .max_by_key(|w| w.path.components().count())
                .map(|w| w.branch.clone())
                .unwrap_or_default();
            let clean = |s: &str| s.replace(['\t', '\n'], " ");
            list.append(QString::from(&format!(
                "{}\t{}\t{}\t{}\t{}",
                key,
                clean(&title),
                sidebar::live_status(info),
                info.harness,
                clean(&branch)
            )));
        }
        if list != self.tabs {
            self.as_mut().set_tabs(list);
        }
    }

    pub fn project_roots(&self, project_id: i32) -> QStringList {
        let mut list = QStringList::default();
        if let Some(p) = self.projects.iter().find(|p| p.id == project_id as i64) {
            list.append(QString::from(&p.path.to_string_lossy().into_owned()));
            for w in self.worktrees.iter().filter(|w| w.project_id == p.id) {
                list.append(QString::from(&w.path.to_string_lossy().into_owned()));
            }
        }
        list
    }

    pub fn active_project_id(&self) -> i32 {
        let key = self.active_key.to_string();
        let Some(cwd) = self.live.get(&key).map(|e| e.info.cwd.clone()) else { return -1 };
        if let Some(w) = self.worktrees.iter().filter(|w| cwd.starts_with(&w.path)).max_by_key(|w| w.path.components().count()) {
            return w.project_id as i32;
        }
        self.projects
            .iter()
            .filter(|p| cwd.starts_with(&p.path))
            .max_by_key(|p| p.path.components().count())
            .map_or(-1, |p| p.id as i32)
    }

    pub fn file_icon(&self, path: &QString) -> QString {
        QString::from(&crate::file_icons::url_for(&path.to_string()))
    }

    pub fn project_list(&self) -> QStringList {
        let mut list = QStringList::default();
        for p in self.projects.iter().filter(|p| self.here(&p.path)) {
            list.append(QString::from(&format!("{}\t{}\t{}", p.id, p.name.replace('\t', " "), p.harness)));
        }
        list
    }

    pub fn search(&self, query: &QString, limit: i32) -> QStringList {
        let tilde = sidebar::tilde;
        let project_of = |cwd: &Path| -> Option<String> {
            if let Some(w) = self.worktrees.iter().filter(|w| cwd.starts_with(&w.path)).max_by_key(|w| w.path.components().count()) {
                return self.projects.iter().find(|p| p.id == w.project_id).map(|p| format!("{} ⎇ {}", p.name, w.branch));
            }
            self.projects.iter().filter(|p| cwd.starts_with(&p.path)).max_by_key(|p| p.path.components().count()).map(|p| p.name.clone())
        };
        struct Item {
            line: String,
            haystack: String,
            modified: i64,
        }
        let unix = |t: SystemTime| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let mut items: Vec<Item> = Vec::new();
        let mut seen_files: HashSet<PathBuf> = HashSet::new();
        for e in self.live.values() {
            let info = &e.info;
            if let Some(f) = &info.session_file {
                seen_files.insert(f.clone());
            }
            let saved = info.session_file.as_ref().and_then(|f| self.sessions.get(f));
            let title = if !info.name.is_empty() {
                info.name.clone()
            } else if let Some(s) = saved {
                s.display_title().to_owned()
            } else if !info.first_message.is_empty() {
                info.first_message.clone()
            } else {
                "New session".to_owned()
            };
            let where_ = project_of(&info.cwd).unwrap_or_else(|| tilde(&info.cwd));
            let modified = saved.map_or(unix(info.started_at), |s| unix(s.modified));
            items.push(Item {
                haystack: format!("{title} {where_}"),
                line: format!("session\t{}\t{}\t{}\t{}\t{}\t{}", info.key, title, where_, info.harness, modified, sidebar::live_status(info)),
                modified,
            });
        }
        for s in self.sessions.values() {
            if seen_files.contains(&s.path) {
                continue;
            }
            let where_ = project_of(&s.cwd).unwrap_or_else(|| tilde(&s.cwd));
            let title = s.display_title().to_owned();
            items.push(Item {
                haystack: format!("{title} {where_}"),
                line: format!("session\t{}\t{}\t{}\t{}\t{}\t", s.path.display(), title, where_, s.harness.as_str(), unix(s.modified)),
                modified: unix(s.modified),
            });
        }
        items.sort_by_key(|i| std::cmp::Reverse(i.modified));
        for p in &self.projects {
            items.push(Item {
                haystack: p.name.clone(),
                line: format!("project\tproject:{}\t{}\t{}\t{}\t0\t", p.id, p.name, tilde(&p.path), p.harness),
                modified: 0,
            });
        }
        let clean: Vec<String> = items.iter().map(|i| i.haystack.replace(['\t', '\n'], " ")).collect();
        let order = harlo_core::palette::rank(&query.to_string(), &clean);
        let mut list = QStringList::default();
        for i in order.into_iter().take(limit.max(1) as usize) {
            list.append(QString::from(&items[i].line.replace('\n', " ")));
        }
        list
    }

    /// Drops a session that has no process, nothing unseen and no tab; it
    /// is cheap to reopen from disk.
    fn release_if_idle(mut self: Pin<&mut Self>, key: &str) {
        // (Another instance's tabs are still open, only out of sight.)
        if self.tab_keys.iter().any(|k| k == key) || self.instance_tabs.values().any(|(keys, _)| keys.iter().any(|k| k == key)) {
            return;
        }
        let Some(entry) = self.live.get(key) else { return };
        let idle = matches!(entry.info.status, SessionStatus::Offline | SessionStatus::Loading | SessionStatus::Exited);
        if idle && !entry.info.unread && entry.info.session_file.is_some() {
            self.as_mut().drop_live(key);
        }
    }

    fn drop_live(mut self: Pin<&mut Self>, key: &str) {
        self.as_mut().rust_mut().modes.remove(key);
        // (A remote overlay goes when its omp exits.)
        if let Some((path, _)) = self.as_mut().rust_mut().overlays.remove(key)
            && !harlo_core::remote::is_remote(&path) {
                let _ = std::fs::remove_file(path);
            }
        if let Some(entry) = self.as_mut().rust_mut().live.remove(key)
            && let Some(live) = registry::remove(key) {
                live.handle.unsubscribe(entry.sub);
            }
    }

    fn mark_viewed(mut self: Pin<&mut Self>, file: &Path) {
        let mut meta = self.meta.get(file).cloned().unwrap_or_default();
        meta.last_viewed_at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        if let Some(db) = self.db() {
            let _ = db.set_session_meta(file, &meta);
        }
        self.as_mut().rust_mut().meta.insert(file.to_owned(), meta);
    }

    fn update_active_title(mut self: Pin<&mut Self>) {
        let key = self.active_key.to_string();
        let title = match (key.strip_prefix("file:").or_else(|| diff_key(&key).map(|d| d.1)), key.strip_prefix("view:")) {
            (Some(path), _) => path.to_owned(),
            (_, Some("stats")) => "Stats".to_owned(),
            (_, Some("settings")) => "Settings".to_owned(),
            (_, Some("new")) => "New session".to_owned(),
            (_, Some(v)) if v.starts_with("run:") => format!("Run: {}", self.find_run(&v[4..]).map(|r| r.title).unwrap_or_default()),
            _ => self.rows.iter().find(|r| r.key == key).map(|r| r.title.clone()).unwrap_or_default(),
        };
        if self.active_title.to_string() != title {
            self.as_mut().set_active_title(QString::from(&title));
        }
    }

    /// Resolves a sidebar key to its session file.
    fn file_for_key(&self, key: &str) -> Option<PathBuf> {
        match self.live.get(key) {
            Some(e) => e.info.session_file.clone(),
            None => self.sessions.contains_key(Path::new(key)).then(|| PathBuf::from(key)),
        }
    }

    fn update_meta(mut self: Pin<&mut Self>, key: &str, f: impl FnOnce(&mut SessionMeta)) {
        let Some(file) = self.file_for_key(key) else { return };
        let mut meta = self.meta.get(&file).cloned().unwrap_or_default();
        f(&mut meta);
        let r = self.db().map(|db| db.set_session_meta(&file, &meta)).transpose();
        self.as_mut().report(r);
        self.as_mut().rust_mut().meta.insert(file, meta);
        self.rebuild();
    }

    pub fn set_archived(self: Pin<&mut Self>, key: &QString, archived: bool) {
        self.update_meta(&key.to_string(), |m| m.archived = archived);
    }

    pub fn set_pinned(self: Pin<&mut Self>, key: &QString, pinned: bool) {
        self.update_meta(&key.to_string(), |m| m.pinned = pinned);
    }

    pub fn close_session(mut self: Pin<&mut Self>, key: &QString) {
        // Close its tab first (activates a neighbour), then stop the process.
        self.as_mut().close_tab(key);
        let key = key.to_string();
        if self.active_key.to_string() == key {
            self.as_mut().set_active_key(QString::default());
            self.as_mut().set_active_title(QString::default());
        }
        // (An instance out of sight forgets its tab too.)
        for (keys, active) in self.as_mut().rust_mut().instance_tabs.values_mut() {
            keys.retain(|k| *k != key);
            if *active == key {
                active.clear();
            }
        }
        self.as_mut().drop_live(&key);
        self.rebuild();
    }

    pub fn trash_session(mut self: Pin<&mut Self>, key: &QString) {
        let key_s = key.to_string();
        let Some(file) = self.file_for_key(&key_s) else { return };
        self.as_mut().close_session(key);
        match harlo_core::remote::trash(&file) {
            Ok(()) => {
                self.as_mut().rust_mut().sessions.remove(&file);
                self.rebuild();
            }
            Err(e) => self.set_error_text(QString::from(&format!("{e:#}"))),
        }
    }

    pub fn rename_session(self: Pin<&mut Self>, key: &QString, name: &QString) {
        let key = self.open(key).to_string();
        if let Some(live) = registry::get(&key) {
            // Both harnesses persist the name; this starts the harness if needed.
            live.handle.send_raw(json!({ "type": "set_session_name", "name": name.to_string() }));
        }
    }

    pub fn key_at(&self, index: i32) -> QString {
        self.rows.get(index as usize).map(|r| QString::from(&r.key)).unwrap_or_default()
    }

    fn set_notice_text(mut self: Pin<&mut Self>, text: &str, error: bool) {
        self.as_mut().set_notice_error(error);
        self.as_mut().set_notice(QString::from(text));
    }

    pub fn clear_notice(self: Pin<&mut Self>) {
        self.set_notice_text("", false);
    }

    /// Runs `job` off the UI thread with the busy flag set, then `done` on
    /// the Qt thread with its result.
    fn background<T: Send + 'static>(
        mut self: Pin<&mut Self>,
        job: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(Pin<&mut Self>, T) + Send + 'static,
    ) {
        self.as_mut().set_busy(true);
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let result = job();
            let _ = thread.queue(move |mut this| {
                this.as_mut().set_busy(false);
                done(this, result);
            });
        });
    }

    fn reload_worktrees(mut self: Pin<&mut Self>) {
        if let Some(Ok(list)) = self.db().map(|db| db.worktrees()) {
            self.as_mut().rust_mut().worktrees = list;
        }
        self.rebuild();
    }

    /// Finds worktrees of every project repository and records new ones.
    fn refresh_worktrees(self: Pin<&mut Self>) {
        // (Remote projects have no worktrees in harlo yet.)
        let projects: Vec<(i64, PathBuf)> = self.projects.iter().filter(|p| !harlo_core::remote::is_remote(&p.path)).map(|p| (p.id, p.path.clone())).collect();
        if projects.is_empty() {
            return;
        }
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let mut found = Vec::new();
            for (id, path) in projects {
                let Some(root) = git::repo_root(&path) else { continue };
                for wt in git::list_worktrees(&root).unwrap_or_default() {
                    if !wt.is_main && !wt.prunable {
                        found.push(WorktreeRecord { path: wt.path, project_id: id, branch: wt.branch, base: String::new(), removed: false });
                    }
                }
            }
            let _ = thread.queue(move |mut this| {
                let mut changed = false;
                if let Some(db) = this.db() {
                    for rec in &found {
                        let known = this.worktrees.iter().find(|w| w.path == rec.path);
                        if known.is_none_or(|k| k.branch != rec.branch || k.removed) {
                            let _ = db.upsert_worktree(rec);
                            changed = true;
                        }
                    }
                    // Recorded worktrees whose folder disappeared.
                    for w in this.worktrees.iter().filter(|w| !w.removed && !w.path.exists()) {
                        let _ = db.mark_worktree_removed(&w.path);
                        changed = true;
                    }
                }
                if changed {
                    this.as_mut().reload_worktrees();
                }
            });
        });
    }

    fn project_root(&self, project_id: i32) -> Option<(Project, PathBuf)> {
        let project = self.projects.iter().find(|p| p.id == project_id as i64)?.clone();
        let root = git::repo_root(&project.path)?;
        Some((project, root))
    }

    pub fn request_branches(&self, project_id: i32) {
        let Some(path) = self.projects.iter().find(|p| p.id == project_id as i64).map(|p| p.path.clone()) else { return };
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let mut names = Vec::new();
            if let Some(root) = git::repo_root(&path)
                && let Ok((mut branches, current)) = git::local_branches(&root)
            {
                branches.retain(|b| *b != current);
                if !current.is_empty() {
                    names.push(current);
                }
                names.extend(branches);
            }
            let _ = thread.queue(move |this| {
                let mut list = QStringList::default();
                for b in &names {
                    list.append(QString::from(b));
                }
                this.branches_ready(project_id, &list);
            });
        });
    }

    pub fn branches(&self, project_id: i32) -> QStringList {
        let mut list = QStringList::default();
        if let Some((_, root)) = self.project_root(project_id)
            && let Ok((mut branches, current)) = git::local_branches(&root) {
                branches.retain(|b| *b != current);
                if !current.is_empty() {
                    branches.insert(0, current);
                }
                for b in branches {
                    list.append(QString::from(&b));
                }
            }
        list
    }

    pub fn suggest_branch(&self) -> QString {
        const ADJ: &[&str] = &["brave", "calm", "eager", "fuzzy", "gentle", "lucky", "nimble", "quiet", "rapid", "sunny", "tidy", "witty"];
        const NOUN: &[&str] = &["otter", "falcon", "maple", "comet", "badger", "harbor", "lynx", "meadow", "pebble", "quartz", "willow", "zephyr"];
        let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos() as usize ^ d.as_secs() as usize).unwrap_or(0);
        QString::from(&format!("harlo/{}-{}", ADJ[n % ADJ.len()], NOUN[(n / ADJ.len()) % NOUN.len()]))
    }

    pub fn new_worktree_session(self: Pin<&mut Self>, project_id: i32, branch: &QString, base: &QString, harness: &QString) {
        self.create_worktree_session(project_id, branch, base, harness, WorktreeStart::default());
    }

    pub fn starters(&self) -> QString {
        QString::from(&harlo_core::starters::json())
    }

    pub fn start_draft(mut self: Pin<&mut Self>, form: &QString) -> QString {
        let form: serde_json::Value = serde_json::from_str(&form.to_string()).unwrap_or_default();
        let field = |k: &str| form[k].as_str().unwrap_or_default().to_owned();
        let project_id = form["projectId"].as_i64().unwrap_or(-1) as i32;
        let (harness, worktree_branch, base, starter, text) = (field("harness"), field("worktreeBranch"), field("base"), field("starter"), field("text"));
        // The model picked ("" the agent's own).
        let model = field("model");
        let ask_first = form["askFirst"].as_bool().unwrap_or(false);
        // The shield's mode to start in ("" the agent's own).
        let approval = Some(field("approval")).filter(|m| harlo_core::omp_overlay::APPROVAL_MODES.contains(&m.as_str()));
        let goal = form["goal"].as_bool().unwrap_or(false).then(|| form["goalBudget"].as_u64().unwrap_or(0));
        let attachments: Vec<String> = form["attachments"].as_array().map(|a| a.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect()).unwrap_or_default();
        let mut message = text.trim().to_owned();
        let harness_name = if harness.is_empty() {
            self.projects.iter().find(|p| p.id == project_id as i64).map(|p| p.harness.clone()).unwrap_or_else(|| self.default_harness())
        } else {
            harness.to_string()
        };
        let goal = goal.filter(|_| matches!(harness_name.as_str(), "omp" | "demo"));
        // A goal runs on its own; asking first would stall it.
        if ask_first && goal.is_none() && matches!(harness_name.as_str(), "omp" | "demo") && !message.is_empty() {
            message.push_str(harlo_harness::file_blocks::ASK_FIRST_NOTE);
        }
        if let Some(s) = harlo_core::starters::find(&starter) {
            message = harlo_core::starters::wrap(s, &message);
        }
        let first = FirstPrompt { text: message, attachments: attachments.iter().map(|p| PathBuf::from(url_to_path(p))).collect(), goal };
        if first.text.trim().is_empty() && first.attachments.is_empty() {
            return QString::default();
        }
        if !worktree_branch.is_empty() {
            self.create_worktree_session(project_id, &QString::from(&worktree_branch), &QString::from(&base), &QString::from(&harness_name), WorktreeStart { first: Some(first), approval, model, ..Default::default() });
            return QString::default();
        }
        let key = self.as_mut().new_session_on(project_id, &harness_name, &model);
        if let Some(mode) = approval {
            self.as_mut().set_approval_mode(&QString::from(&key), &QString::from(&mode));
        }
        send_first_prompt(&key, first, Some(self.qt_thread()));
        QString::from(&key)
    }

    fn create_worktree_session(mut self: Pin<&mut Self>, project_id: i32, branch: &QString, base: &QString, harness: &QString, opts: WorktreeStart) {
        let WorktreeStart { first, model, lane, parent, approval } = opts;
        let Some((project, root)) = self.project_root(project_id) else {
            self.set_notice_text("This project is not a git repository.", true);
            return;
        };
        let branch = branch.to_string().trim().to_owned();
        let base = base.to_string();
        let harness = if harness.is_empty() { project.harness.clone() } else { harness.to_string() };
        let Some(kind) = HarnessKind::parse(&harness) else { return };
        // A remote project's go on its host, in its data folder (as here).
        let folder = format!("{}-{}", git::branch_slug(&project.name), project.id);
        let host = harlo_core::remote::host_of(&root).and_then(|h| harlo_core::remote::host(&h));
        let local = self.worktree_base().join(&folder).join(git::branch_slug(&branch));
        let setup = project.setup.clone();
        self.as_mut().set_notice_text(&format!("Creating worktree {branch}…"), false);
        let (b, bs, slug) = (branch.clone(), base.clone(), git::branch_slug(&branch));
        self.background(
            move || {
                let path = match &host {
                    // (Its data folder, as harlo's is here.)
                    Some(host) => {
                        let data = harlo_core::remote::run_ok(host, r#"printf '%s' "${XDG_DATA_HOME:-$HOME/.local/share}""#, Duration::from_secs(20))?;
                        harlo_core::remote::join(&host.name, &format!("{}/harlo/worktrees/{folder}/{slug}", data.trim()))
                    }
                    None => local,
                };
                git::create_worktree(&root, &path, &b, &bs)?;
                // git reports canonical paths; match them.
                Ok::<_, anyhow::Error>(if host.is_some() { path } else { path.canonicalize().unwrap_or(path) })
            },
            move |mut this, result| match result {
                Ok(path) => {
                    let rec = WorktreeRecord { path: path.clone(), project_id: project.id, branch: branch.clone(), base, removed: false };
                    if let Some(db) = this.db() {
                        let _ = db.upsert_worktree(&rec);
                    }
                    this.as_mut().reload_worktrees();
                    // Run lanes start in the background; the run's overview is the tab.
                    let key = this.as_mut().start_session_with(kind, path.clone(), &model, lane.is_none() && parent.is_none());
                    // Started by an agent: its subagent.
                    if let Some((parent, title, notify)) = parent {
                        this.as_mut().adopt(&key, &parent, &title, kind.as_str(), notify);
                    }
                    if let Some(lane) = lane {
                        this.as_mut().rust_mut().lane_keys.insert(key.clone(), lane);
                    }
                    if let Some(mode) = approval {
                        this.as_mut().set_approval_mode(&QString::from(&key), &QString::from(&mode));
                    }
                    if let Some(first) = first {
                        send_first_prompt(&key, first, Some(this.qt_thread()));
                    }
                    this.as_mut().set_notice_text(&format!("Worktree {branch} ready"), false);
                    this.run_setup(path, setup);
                }
                Err(e) => this.set_notice_text(&format!("Could not create the worktree: {e:#}"), true),
            },
        );
    }

    /// Runs a project's setup commands in a new worktree, one per line.
    fn run_setup(self: Pin<&mut Self>, dir: PathBuf, setup: String) {
        let commands: Vec<String> = setup.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_owned).collect();
        if commands.is_empty() {
            return;
        }
        self.background(
            move || {
                // A remote worktree's run on its host, in it.
                let host = harlo_core::remote::split(&dir).and_then(|(h, p)| Some((harlo_core::remote::host(&h)?, p)));
                for cmd in &commands {
                    let out = match &host {
                        Some((host, p)) => harlo_core::remote::run(host, &format!("cd {} && {cmd}", harlo_core::remote::quote(p)), Duration::from_secs(1800)).map_err(|e| std::io::Error::other(format!("{e:#}"))),
                        None => std::process::Command::new("sh").arg("-c").arg(cmd).current_dir(&dir).output(),
                    };
                    match out {
                        Ok(o) if o.status.success() => {}
                        Ok(o) => {
                            let err = String::from_utf8_lossy(&o.stderr);
                            return Err(format!("Setup command failed: {cmd}\n{}", err.trim().lines().last().unwrap_or("")));
                        }
                        Err(e) => return Err(format!("Setup command failed: {cmd}: {e}")),
                    }
                }
                Ok(commands.len())
            },
            |this, result| match result {
                Ok(n) => this.set_notice_text(&format!("Worktree setup finished ({n} command{})", if n == 1 { "" } else { "s" }), false),
                Err(e) => this.set_notice_text(&e, true),
            },
        );
    }

    /// The worktree record a sidebar key's session runs in.
    fn worktree_for_key(&self, key: &str) -> Option<WorktreeRecord> {
        let cwd = match self.live.get(key) {
            Some(e) => e.info.cwd.clone(),
            None => self.sessions.get(Path::new(key))?.cwd.clone(),
        };
        self.worktrees
            .iter()
            .filter(|w| cwd.starts_with(&w.path))
            .max_by_key(|w| w.path.components().count())
            .cloned()
    }

    pub fn worktree_info(&self, key: &QString) -> QString {
        let Some(w) = self.worktree_for_key(&key.to_string()) else { return QString::default() };
        let main = self.projects.iter().find(|p| p.id == w.project_id).and_then(|p| git::repo_root(&p.path));
        let main_branch = main.as_ref().and_then(|m| git::status(m).ok()).map(|s| s.branch).unwrap_or_default();
        let base = if w.base.is_empty() { main_branch.clone() } else { w.base.clone() };
        let exists = w.path.exists() && !w.removed;
        let summary = if exists { git::worktree_summary(&w.path, &base).ok() } else { None }.unwrap_or_default();
        QString::from(
            &json!({
                "path": w.path.to_string_lossy(),
                "branch": w.branch,
                "base": base,
                "mainBranch": main_branch,
                "exists": exists,
                "dirty": summary.dirty,
                "ahead": summary.ahead,
                "behind": summary.behind,
            })
            .to_string(),
        )
    }

    pub fn merge_worktree(mut self: Pin<&mut Self>, key: &QString) {
        let Some(w) = self.worktree_for_key(&key.to_string()) else { return };
        let Some(main) = self.projects.iter().find(|p| p.id == w.project_id).and_then(|p| git::repo_root(&p.path)) else { return };
        self.as_mut().set_notice_text(&format!("Merging {}…", w.branch), false);
        let branch = w.branch.clone();
        self.background(
            move || git::merge_branch(&main, &branch).map_err(|e| format!("{e:#}")),
            |this, result| match result {
                Ok(msg) => this.set_notice_text(&msg, false),
                Err(e) => this.set_notice_text(&e, true),
            },
        );
    }

    pub fn remove_worktree(mut self: Pin<&mut Self>, key: &QString, delete_branch: bool, force: bool) {
        let Some(w) = self.worktree_for_key(&key.to_string()) else { return };
        let Some(main) = self.projects.iter().find(|p| p.id == w.project_id).and_then(|p| git::repo_root(&p.path)) else { return };
        // Stop every session running in that folder first.
        let inside: Vec<String> = self.live.values().filter(|e| e.info.cwd.starts_with(&w.path)).map(|e| e.info.key.clone()).collect();
        for k in inside {
            self.as_mut().close_session(&QString::from(&k));
        }
        self.as_mut().set_notice_text(&format!("Removing worktree {}…", w.branch), false);
        let rec = w.clone();
        self.background(
            move || -> Result<String, String> {
                git::remove_worktree(&main, &rec.path, force).map_err(|e| format!("{e:#}"))?;
                if delete_branch && !rec.branch.is_empty() {
                    // Unmerged work is only discarded when the user forced removal.
                    if let Err(e) = git::delete_branch(&main, &rec.branch, force) {
                        return Ok(format!("Removed the worktree; kept branch {} ({e:#})", rec.branch));
                    }
                    return Ok(format!("Removed the worktree and branch {}", rec.branch));
                }
                Ok(format!("Removed the worktree (branch {} kept)", rec.branch))
            },
            move |mut this, result| {
                match result {
                    Ok(msg) => {
                        if let Some(db) = this.db() {
                            let _ = db.mark_worktree_removed(&w.path);
                        }
                        this.as_mut().set_notice_text(&msg, false);
                    }
                    Err(e) => this.as_mut().set_notice_text(&e, true),
                }
                this.reload_worktrees();
            },
        );
    }

    pub fn project_setup(&self, project_id: i32) -> QString {
        self.projects.iter().find(|p| p.id == project_id as i64).map(|p| QString::from(&p.setup)).unwrap_or_default()
    }

    pub fn set_project_setup(mut self: Pin<&mut Self>, project_id: i32, setup: &QString) {
        let r = self.db().map(|db| db.set_project_setup(project_id as i64, &setup.to_string())).transpose();
        self.as_mut().report(r);
        self.reload_projects();
    }

    fn apply_scan(mut self: Pin<&mut Self>, delta: ScanDelta) {
        self.search_kick.notify_one();
        let mut this = self.as_mut().rust_mut();
        for path in delta.removed {
            this.sessions.remove(&path);
        }
        for info in delta.upserts {
            this.sessions.insert(info.path.clone(), info);
        }
        self.as_mut().rebuild();
        // Tabs of remote sessions, once their host's sessions arrive.
        if !self.pending_remote_tabs.is_empty() {
            let (found, rest): (Vec<PathBuf>, Vec<PathBuf>) = std::mem::take(&mut self.as_mut().rust_mut().pending_remote_tabs).into_iter().partition(|f| self.sessions.contains_key(f));
            self.as_mut().rust_mut().pending_remote_tabs = rest;
            let active = self.active_key.clone();
            for f in found {
                self.as_mut().open(&QString::from(&f.to_string_lossy().into_owned()));
            }
            // Opening a tab activates it; keep the one you were on.
            if !active.is_empty() {
                self.as_mut().activate(&active.to_string());
            }
        }
        // Reopen the tabs (read-only, no processes) after the first scan.
        if let Some((files, active)) = self.as_mut().rust_mut().restore_tabs.take() {
            // Self-tests skip the restore unless they ask for it.
            let quiet = std::env::args().any(|a| a.starts_with("--selftest")) && !std::env::args().any(|a| a == "--restore-tabs");
            if !quiet && self.active_key.is_empty() {
                let (remote_tabs, files): (Vec<PathBuf>, Vec<PathBuf>) = files.into_iter().partition(|f| harlo_core::remote::is_remote(f) && !self.sessions.contains_key(f));
                self.as_mut().rust_mut().pending_remote_tabs = remote_tabs;
                let present: Vec<PathBuf> = files
                    .into_iter()
                    .filter(|f| {
                        let s = f.to_string_lossy();
                        // Settings moved to a dialog; older saves may list its tab.
                        if s.starts_with("view:") {
                            return s != "view:settings";
                        }
                        match s.strip_prefix("file:") {
                            Some(p) => Path::new(p).is_file(),
                            None => self.sessions.contains_key(f),
                        }
                    })
                    .collect();
                for f in present {
                    self.as_mut().open(&QString::from(&f.to_string_lossy().into_owned()));
                }
                if let Some(a) = active.filter(|a| {
                    let s = a.to_string_lossy();
                    (s.starts_with("view:") && s != "view:settings") || s.strip_prefix("file:").map_or_else(|| self.sessions.contains_key(a), |p| Path::new(p).is_file())
                }) {
                    self.as_mut().open(&QString::from(&a.to_string_lossy().into_owned()));
                }
                // (Reopening moved between instances: back to the default.)
                let start = self.saved_default_instance();
                self.as_mut().switch_instance(&QString::from(&start));
            }
        }
    }

    pub fn set_window_active(mut self: Pin<&mut Self>, active: bool) {
        self.as_mut().rust_mut().window_active = active;
        if active {
            self.change_kick.notify_one();
        }
    }

    pub fn refresh_changes(&self) {
        self.change_kick.notify_one();
    }

    /// Distinct folders of the listed sessions, or `None` while the window
    /// is in the background (counts then wait until it comes back), with the
    /// ignored repositories.
    fn change_dirs(&self) -> Option<(Vec<PathBuf>, HashSet<PathBuf>)> {
        if !self.window_active {
            return None;
        }
        let mut seen = HashSet::new();
        let dirs = self
            .rows
            .iter()
            .filter(|r| r.kind == SidebarKind::Session && !r.cwd.as_os_str().is_empty())
            .filter(|r| seen.insert(r.cwd.clone()))
            .map(|r| r.cwd.clone())
            .collect();
        Some((dirs, self.ignored_repos.clone()))
    }

    /// A saved setting (`settings.<key>` in the database), or "".
    fn setting_str(&self, key: &str) -> String {
        self.db.as_ref().and_then(|db| db.get(&format!("settings.{key}")).ok().flatten()).unwrap_or_default()
    }

    fn save_setting(&self, key: &str, value: &str) {
        if let Some(db) = &self.db {
            let _ = db.set(&format!("settings.{key}"), value);
        }
    }

    pub fn default_agent(&self) -> QString {
        QString::from(&self.default_harness())
    }

    pub fn setting(&self, key: &QString) -> QString {
        QString::from(&self.setting_str(&key.to_string()))
    }

    pub fn set_setting(&self, key: &QString, value: &QString) {
        let (key, value) = (key.to_string(), value.to_string());
        self.save_setting(&key, value.trim());
        if key == "debug_protocol" {
            crate::diagnostics::set_protocol_logging(value.trim() == "1");
        }
    }

    pub fn save_template(mut self: Pin<&mut Self>, json: &QString) -> QString {
        use harlo_core::templates;
        let mut list = templates::parse(&self.templates_json.to_string());
        let edit: serde_json::Value = serde_json::from_str(&json.to_string()).unwrap_or_default();
        let new_id = format!("t{}", chrono::Local::now().timestamp_millis());
        let Some(id) = templates::save(&mut list, &edit, &new_id) else { return QString::default() };
        self.as_mut().store_templates(&list);
        QString::from(&id)
    }

    pub fn delete_template(mut self: Pin<&mut Self>, id: &QString) {
        use harlo_core::templates;
        let mut list = templates::parse(&self.templates_json.to_string());
        templates::delete(&mut list, &id.to_string());
        self.as_mut().store_templates(&list);
    }

    fn store_templates(mut self: Pin<&mut Self>, list: &[harlo_core::templates::Template]) {
        let text = harlo_core::templates::to_json(list).to_string();
        self.save_setting("templates", &text);
        self.as_mut().set_templates_json(QString::from(&text));
    }

    pub fn phone_test_call(self: Pin<&mut Self>, json: &QString) -> QString {
        if !std::env::args().any(|a| a.starts_with("--selftest")) {
            return QString::default();
        }
        let msg: serde_json::Value = serde_json::from_str(&json.to_string()).unwrap_or_default();
        QString::from(&self.phone_call(&msg).to_string())
    }

    pub fn export_markdown(mut self: Pin<&mut Self>, key: &QString, path: &QString) {
        let path = PathBuf::from(path.to_string().trim_start_matches("file://"));
        let Some(live) = self.as_mut().make_live(&key.to_string()).and_then(|k| registry::get(&k)) else {
            return self.set_notice_text("That session isn't available.", true);
        };
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            let result = match live.handle.export_markdown().await {
                Some(md) => std::fs::write(&path, md).map_err(|e| e.to_string()),
                None => Err("The session closed.".to_owned()),
            };
            let _ = thread.queue(move |this| match result {
                Ok(()) => this.set_notice_text(&format!("Saved as {}", sidebar::tilde(&path)), false),
                Err(e) => this.set_notice_text(&format!("Couldn't save it: {e}"), true),
            });
        });
    }

    pub fn copy_markdown(mut self: Pin<&mut Self>, key: &QString) {
        let Some(live) = self.as_mut().make_live(&key.to_string()).and_then(|k| registry::get(&k)) else { return };
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            let Some(md) = live.handle.export_markdown().await else { return };
            let _ = thread.queue(move |mut this| {
                // (Set anew each time, so the same text copies again.)
                this.as_mut().set_clipboard_text(QString::default());
                this.as_mut().set_clipboard_text(QString::from(&md));
                this.set_notice_text("Copied the session as Markdown.", false);
            });
        });
    }

    pub fn watch_request(mut self: Pin<&mut Self>, json: &QString) {
        let req: serde_json::Value = serde_json::from_str(&json.to_string()).unwrap_or_default();
        let url = req["url"].as_str().unwrap_or_default();
        if url.is_empty() || self.ci_watch.iter().any(|(r, _)| r["url"] == url) {
            return;
        }
        self.as_mut().rust_mut().ci_watch.push((req, std::time::Instant::now()));
    }

    /// Reads the watched pull requests' checks (off the UI thread); those
    /// that finished notify and stop being watched (as do any after 6 hours).
    fn poll_requests(mut self: Pin<&mut Self>) {
        if self.ci_watch.is_empty() || self.ci_polling {
            return;
        }
        self.as_mut().rust_mut().ci_watch.retain(|(_, since)| since.elapsed() < Duration::from_secs(6 * 3600));
        self.as_mut().rust_mut().ci_polling = true;
        let watched: Vec<serde_json::Value> = self.ci_watch.iter().map(|(r, _)| r.clone()).collect();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let read: Vec<(serde_json::Value, Option<harlo_core::git_hosting::Checks>)> = watched
                .into_iter()
                .map(|r| {
                    let s = |k: &str| r[k].as_str().unwrap_or_default().to_owned();
                    let checks = crate::ci::read(&s("service"), &format!("{}/{}", s("host"), s("repo")), &s("url"), r["number"].as_i64().unwrap_or(0));
                    (r, checks)
                })
                .collect();
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().ci_polling = false;
                for (req, checks) in read {
                    let Some(checks) = checks.filter(|c| c.settled()) else { continue };
                    this.as_mut().rust_mut().ci_watch.retain(|(r, _)| r["url"] != req["url"]);
                    this.as_mut().ci_settled(&req, &checks);
                }
            });
        });
    }

    /// A watched pull request's checks finished: say so here and on phones.
    fn ci_settled(mut self: Pin<&mut Self>, req: &serde_json::Value, checks: &harlo_core::git_hosting::Checks) {
        let passed = checks.state() == "passing";
        let number = req["number"].as_i64().unwrap_or(0);
        let name = format!("{} #{number}", if req["word"] == "merge request" { "Merge request" } else { "Pull request" });
        let title = format!("{}: {name}", if passed { "Checks passed" } else { "Checks failed" });
        let body = format!("{}. {}", checks.summary(), req["title"].as_str().unwrap_or_default());
        let settings = harlo_core::notifications::Settings::load(&self.setting_str("notify"), &self.setting_str("notifications"));
        let quiet = settings.quiet.hushes_now(false);
        crate::phone_push::send(crate::phone_push::Alert {
            what: if passed { crate::phone_push::What::Done } else { crate::phone_push::What::Failed },
            key: String::new(), title: title.clone(), body: body.clone(), badge: self.waiting_count(), quiet,
        });
        let event = if passed { "completion" } else { "error" };
        if !settings.enabled || !settings.event(event).is_some_and(|e| e.0) || quiet || (self.window_active && !settings.while_focused) {
            return;
        }
        self.as_mut().deliver(&format!("ci-{number}"), "ci", "", &title, &body, settings.sound);
    }

    pub fn set_spend_limits(mut self: Pin<&mut Self>, session: f64, day: f64) {
        let limits = harlo_core::spending::Limits::parse(&json!({ "session": session, "day": day }).to_string());
        self.save_setting("spend_limits", &limits.to_json().to_string());
        self.as_mut().rust_mut().spend_limits = limits;
        self.as_mut().refresh_spending();
    }

    pub fn allow_more_spending(mut self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        let limits = self.spend_limits;
        {
            let mut this = self.as_mut().rust_mut();
            if key.is_empty() {
                this.spend_ledger.extra += limits.day;
            } else if let Some(s) = this.spend.get_mut(&key) {
                // Enough past what it has cost for another limit's worth.
                s.extra = (s.cost - limits.session).max(0.0) + limits.session;
            }
            this.spend_stopped.clear();
        }
        self.save_setting("spend_today", &self.spend_ledger.to_json().to_string());
        self.as_mut().refresh_spending();
    }

    pub fn spend_hold(&self, key: &QString) -> QString {
        QString::from(&registry::get(&key.to_string()).and_then(|l| l.handle.held()).unwrap_or_default())
    }

    /// Re-checks the spending limits: holds sessions over theirs (and all of
    /// them over the day's), stops a run past one and says so.
    fn refresh_spending(mut self: Pin<&mut Self>) {
        use harlo_core::spending::{day_over, session_over};
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        if self.as_mut().rust_mut().spend_ledger.roll(&today) {
            self.save_setting("spend_today", &self.spend_ledger.to_json().to_string());
        }
        let live: HashSet<String> = self.live.keys().cloned().collect();
        self.as_mut().rust_mut().spend.retain(|k, _| live.contains(k));
        let limits = self.spend_limits;
        let day = day_over(&limits, &self.spend_ledger).map(|o| o.message());
        harlo_core::session::set_day_hold(day.clone());
        let mut held = serde_json::Map::new();
        let mut costs = serde_json::Map::new();
        let mut stops: Vec<(String, String)> = Vec::new();
        for (key, s) in &self.spend {
            costs.insert(key.clone(), json!(s.cost));
            let own = session_over(&limits, s).map(|o| o.message());
            if let Some(live) = registry::get(key) {
                live.handle.set_hold(own.clone());
            }
            let Some(why) = own.or_else(|| day.clone()) else { continue };
            held.insert(key.clone(), json!(why));
            let busy = self.live.get(key).is_some_and(|e| e.info.status == SessionStatus::Busy);
            if busy && !self.spend_stopped.contains(key) {
                stops.push((key.clone(), why));
            }
        }
        for (key, why) in stops {
            if let Some(live) = registry::get(&key) {
                live.handle.abort();
            }
            self.as_mut().rust_mut().spend_stopped.insert(key.clone());
            self.as_mut().notify(&key, Note::Event(harlo_core::Notable::Error { message: format!("Stopped: {why}") }));
        }
        let text = json!({ "limits": limits.to_json(), "today": self.spend_ledger.total, "day": day.unwrap_or_default(), "held": held, "costs": costs }).to_string();
        if text != self.spend_json.to_string() {
            self.as_mut().set_spend_json(QString::from(&text));
        }
    }

    pub fn schedules_json(&self) -> QString {
        let list: Vec<serde_json::Value> = self
            .schedules
            .iter()
            .map(|s| {
                let mut v = s.to_json();
                v["describe"] = serde_json::json!(s.repeat.describe());
                v["projectName"] = serde_json::json!(self.projects.iter().find(|p| p.id == s.project_id).map_or("Home folder".to_owned(), |p| p.name.clone()));
                v
            })
            .collect();
        QString::from(&serde_json::Value::Array(list).to_string())
    }

    pub fn save_schedule(mut self: Pin<&mut Self>, json: &QString) -> QString {
        use harlo_core::schedules::{Repeat, Schedule, parse_local, parse_time};
        let v: serde_json::Value = serde_json::from_str(&json.to_string()).unwrap_or_default();
        let prompt = v["prompt"].as_str().unwrap_or_default().trim().to_owned();
        if prompt.is_empty() {
            return QString::from("Write the prompt to send.");
        }
        let r = &v["repeat"];
        let repeat = match r["kind"].as_str().unwrap_or_default() {
            "days" => {
                let Some((hour, minute)) = parse_time(r["time"].as_str().unwrap_or_default()) else { return QString::from("Use a time like 09:00.") };
                let days = r["days"].as_u64().unwrap_or(0) as u8 & 0x7f;
                if days == 0 {
                    return QString::from("Pick at least one day.");
                }
                Repeat::Days { days, hour, minute }
            }
            "every" => {
                let minutes = r["minutes"].as_u64().unwrap_or(0);
                if minutes < 1 {
                    return QString::from("Use an interval of at least a minute.");
                }
                Repeat::Every { minutes: minutes as u32 }
            }
            "once" => {
                let Some(at) = parse_local(r["at"].as_str().unwrap_or_default()) else { return QString::from("Use a date and time like 2026-10-13 09:00.") };
                if at <= chrono::Local::now().timestamp() {
                    return QString::from("That time has passed.");
                }
                Repeat::Once { at }
            }
            _ => return QString::from("Choose when it runs."),
        };
        let id = v["id"].as_i64().unwrap_or(0);
        let previous = self.schedules.iter().find(|s| s.id == id).cloned();
        let title = v["title"].as_str().unwrap_or_default().trim().to_owned();
        let mut schedule = Schedule {
            id,
            title: if title.is_empty() { prompt.lines().next().unwrap_or_default().chars().take(48).collect() } else { title },
            prompt,
            project_id: v["projectId"].as_i64().unwrap_or(-1),
            harness: v["harness"].as_str().unwrap_or_default().to_owned(),
            model: v["model"].as_str().unwrap_or_default().trim().to_owned(),
            repeat,
            enabled: v["enabled"].as_bool().unwrap_or(true),
            catch_up: v["catchUp"].as_bool().unwrap_or(true),
            last_run: previous.and_then(|p| p.last_run),
            next_run: None,
        };
        schedule.reschedule(chrono::Local::now());
        let Some(db) = self.db() else { return QString::from("harlo's database is not available.") };
        if let Err(e) = db.save_schedule(&mut schedule) {
            return QString::from(&format!("Could not save: {e:#}"));
        }
        let list = &mut self.as_mut().rust_mut().schedules;
        match list.iter_mut().find(|s| s.id == schedule.id) {
            Some(existing) => *existing = schedule,
            None => list.push(schedule),
        }
        self.bump_schedules();
        QString::default()
    }

    pub fn delete_schedule(mut self: Pin<&mut Self>, id: i32) {
        if let Some(db) = self.db() {
            let _ = db.delete_schedule(id as i64);
        }
        self.as_mut().rust_mut().schedules.retain(|s| s.id != id as i64);
        self.bump_schedules();
    }

    pub fn set_schedule_enabled(mut self: Pin<&mut Self>, id: i32, on: bool) {
        let Some(mut s) = self.schedules.iter().find(|s| s.id == id as i64).cloned() else { return };
        s.enabled = on;
        s.reschedule(chrono::Local::now());
        self.as_mut().store_schedule(s);
    }

    pub fn run_schedule_now(mut self: Pin<&mut Self>, id: i32) {
        let Some(mut s) = self.schedules.iter().find(|s| s.id == id as i64).cloned() else { return };
        self.as_mut().start_scheduled(&s);
        let now = chrono::Local::now();
        s.last_run = Some(now.timestamp());
        // An interval starts over; days and one-offs keep their next time.
        if matches!(s.repeat, harlo_core::schedules::Repeat::Every { .. }) {
            s.reschedule(now);
        }
        self.as_mut().store_schedule(s);
    }

    pub fn tick_schedules(mut self: Pin<&mut Self>) {
        // (A new day lifts the daily spending limit.)
        self.as_mut().refresh_spending();
        self.as_mut().poll_requests();
        // `--clock-ahead=<seconds>` (testing): check as if it were later.
        let ahead = std::env::args().find_map(|a| a.strip_prefix("--clock-ahead=").and_then(|n| n.parse::<i64>().ok())).unwrap_or(0);
        let now = chrono::Local::now() + chrono::Duration::seconds(ahead);
        let startup = !self.schedules_started;
        self.as_mut().rust_mut().schedules_started = true;
        let list = self.schedules.clone();
        for mut s in list {
            let run = if startup {
                let before = s.clone();
                let catch_up = s.missed_at_startup(now);
                if !catch_up && s != before {
                    self.as_mut().store_schedule(s.clone());
                }
                catch_up
            } else {
                s.is_due(now)
            };
            if run {
                self.as_mut().start_scheduled(&s);
                s.ran(now);
                self.as_mut().store_schedule(s);
            }
        }
    }

    /// Starts a schedule's session in the background (no tab switch).
    fn start_scheduled(mut self: Pin<&mut Self>, s: &harlo_core::schedules::Schedule) {
        let project = self.projects.iter().find(|p| p.id == s.project_id).cloned();
        let harness = if !s.harness.is_empty() {
            s.harness.clone()
        } else {
            project.as_ref().map(|p| p.harness.clone()).unwrap_or_else(|| self.default_harness())
        };
        let Some(kind) = HarnessKind::parse(&harness) else { return };
        let cwd = project.map(|p| p.path).unwrap_or_else(|| self.instance_home());
        tracing::info!("scheduled prompt \"{}\" starts", s.title);
        let key = self.as_mut().start_session_with(kind, cwd, &s.model, false);
        let first = FirstPrompt { text: s.prompt.clone(), ..Default::default() };
        send_first_prompt(&key, first, Some(self.qt_thread()));
        self.set_notice_text(&format!("Scheduled prompt started: {}", s.title), false);
    }

    fn store_schedule(mut self: Pin<&mut Self>, mut s: harlo_core::schedules::Schedule) {
        if let Some(db) = self.db() {
            let _ = db.save_schedule(&mut s);
        }
        if let Some(existing) = self.as_mut().rust_mut().schedules.iter_mut().find(|x| x.id == s.id) {
            *existing = s;
        }
        self.bump_schedules();
    }

    fn bump_schedules(mut self: Pin<&mut Self>) {
        let next = self.schedules_revision + 1;
        self.as_mut().set_schedules_revision(next);
    }

    /// Plan limits: omp's accounts (`omp usage`) and Claude Code's (its own
    /// `/usage`).
    /// Each source keeps its last numbers when it can't be asked.
    pub fn refresh_quota(mut self: Pin<&mut Self>) {
        if self.quota_loading {
            return;
        }
        // Each when it's on (Settings → Providers) and there.
        let omp = self
            .provider_enabled(HarnessKind::Omp)
            .then(|| self.program_for(HarnessKind::Omp).or_else(|| harlo_harness::resolve_program(HarnessKind::Omp)))
            .flatten()
            .filter(|p| p.is_file());
        let claude = self
            .provider_enabled(HarnessKind::Claude)
            .then(|| self.program_for(HarnessKind::Claude).or_else(|| harlo_harness::resolve_program(HarnessKind::Claude)))
            .flatten()
            .filter(|p| p.is_file())
            .zip(harlo_harness::claude::config_dir());
        if omp.is_none() && claude.is_none() {
            self.as_mut().set_quota_json(QString::from(&serde_json::json!({ "accounts": [], "error": "" }).to_string()));
            return;
        }
        self.as_mut().rust_mut().quota_loading = true;
        // Keep showing the last numbers while new ones load.
        let mut current: serde_json::Value = serde_json::from_str(&self.quota_json.to_string()).unwrap_or_else(|_| serde_json::json!({ "accounts": [] }));
        current["loading"] = serde_json::json!(true);
        self.as_mut().set_quota_json(QString::from(&current.to_string()));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let from_omp = omp.map(|p| harlo_core::quota::fetch(&p));
            let from_claude = claude.map(|(p, d)| harlo_core::quota::claude(&p, &d));
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().quota_loading = false;
                let previous: serde_json::Value = serde_json::from_str(&this.quota_json.to_string()).unwrap_or_default();
                let kept = |claude: bool| -> Vec<serde_json::Value> {
                    previous["accounts"].as_array().into_iter().flatten().filter(|a| (a["provider"] == "Claude") == claude).cloned().collect()
                };
                let (mut accounts, mut errors) = (Vec::new(), Vec::new());
                match from_claude {
                    Some(Ok(Some(a))) => accounts.push(a),
                    Some(Ok(None)) | None => {}
                    Some(Err(e)) => {
                        accounts.extend(kept(true));
                        errors.push(format!("Claude: {e:#}"));
                    }
                }
                match from_omp {
                    Some(Ok(list)) => accounts.extend(list),
                    None => {}
                    Some(Err(e)) => {
                        accounts.extend(kept(false));
                        errors.push(format!("{e:#}"));
                    }
                }
                let updated = if errors.is_empty() { serde_json::json!(chrono::Local::now().timestamp_millis()) } else { previous["updatedAt"].clone() };
                let json = serde_json::json!({ "accounts": accounts, "updatedAt": updated, "error": errors.join("\n") });
                // Shown at once next time.
                if let Some(db) = this.db.as_ref() {
                    let _ = db.set("cache.quota", &serde_json::json!({ "accounts": json["accounts"], "updatedAt": json["updatedAt"], "error": "" }).to_string());
                }
                this.as_mut().set_quota_json(QString::from(&json.to_string()));
            });
        });
    }

    pub fn pin_meter(mut self: Pin<&mut Self>, provider: &QString) {
        self.save_setting("meter_pin", &provider.to_string());
        self.as_mut().set_meter_pin(provider.clone());
    }

    fn notes_key(project_id: i32) -> String {
        if project_id < 0 { "notes.project.none".to_owned() } else { format!("notes.project.{project_id}") }
    }

    pub fn project_notes(&self, project_id: i32) -> QString {
        let text = self.db.as_ref().and_then(|db| db.get(&Self::notes_key(project_id)).ok().flatten()).unwrap_or_default();
        QString::from(&text)
    }

    pub fn set_project_notes(&self, project_id: i32, text: &QString) {
        if let Some(db) = &self.db {
            let _ = db.set(&Self::notes_key(project_id), &text.to_string());
        }
    }

    pub fn autostart_enabled(&self) -> bool {
        harlo_core::autostart::is_enabled()
    }

    pub fn set_autostart(&self, on: bool) -> QString {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("harlo"));
        match harlo_core::autostart::set(on, &exe) {
            Ok(()) => QString::default(),
            Err(e) => QString::from(&format!("Could not change the autostart entry: {e}")),
        }
    }

    pub fn approval_mode(&self, key: &QString) -> QString {
        let key = key.to_string();
        let Some(entry) = self.live.get(&key) else { return QString::default() };
        if !matches!(entry.info.harness.as_str(), "omp" | "demo" | "claude" | "opencode") {
            return QString::default();
        }
        if let Some(mode) = self.modes.get(&key).and_then(|m| m.approval.clone()) {
            return QString::from(&mode);
        }
        self.starting_approval(&entry.info.harness, &entry.info.cwd)
    }

    pub fn draft_approval(&self, harness: &QString, project_id: i32) -> QString {
        let harness = harness.to_string();
        if !matches!(harness.as_str(), "omp" | "demo" | "claude" | "opencode") {
            return QString::default();
        }
        let cwd = self.projects.iter().find(|p| p.id == project_id as i64).map(|p| p.path.clone()).unwrap_or_else(|| self.instance_home());
        self.starting_approval(&harness, &cwd)
    }

    /// The shield's mode a session starts in, before it's switched.
    fn starting_approval(&self, harness: &str, cwd: &Path) -> QString {
        // Claude asks first unless told otherwise (its own settings may
        // differ); for OpenCode the shield answers what OpenCode asks.
        if matches!(harness, "claude" | "opencode") {
            return QString::from("always-ask");
        }
        // On a remote host: that machine's omp config (read with its overlay).
        let remote = harlo_core::remote::host_of(cwd);
        match remote {
            // Read over SSH in the background; until then, ask first.
            Some(host) => {
                let mut cache = self.remote_approval.lock().unwrap();
                match cache.get(&host) {
                    Some(Some(mode)) => QString::from(mode),
                    Some(None) => QString::from("always-ask"),
                    None => {
                        cache.insert(host.clone(), None);
                        let (cache, thread) = (std::sync::Arc::clone(&self.remote_approval), self.qt_thread());
                        crate::runtime().spawn_blocking(move || {
                            let mode = harlo_core::omp_overlay::approval_mode(&harlo_core::remote::home_file_peek(&host, OMP_CONFIG).unwrap_or_default());
                            cache.lock().unwrap().insert(host.clone(), Some(mode));
                            let _ = thread.queue(|mut this| {
                                let next = this.modes_revision + 1;
                                this.as_mut().set_modes_revision(next);
                            });
                        });
                        QString::from("always-ask")
                    }
                }
            }
            None => QString::from(&user_approval_mode()),
        }
    }

    pub fn set_approval_mode(mut self: Pin<&mut Self>, key: &QString, mode: &QString) {
        let (key, mode) = (key.to_string(), mode.to_string());
        if !harlo_core::omp_overlay::APPROVAL_MODES.contains(&mode.as_str()) {
            return;
        }
        self.as_mut().rust_mut().modes.entry(key.clone()).or_default().approval = Some(mode);
        self.as_mut().modes_changed(&key);
    }

    pub fn plan_mode(&self, key: &QString) -> bool {
        self.modes.get(&key.to_string()).is_some_and(|m| m.plan)
    }

    pub fn set_plan_mode(mut self: Pin<&mut Self>, key: &QString, on: bool) {
        let key = key.to_string();
        self.as_mut().rust_mut().modes.entry(key.clone()).or_default().plan = on;
        self.as_mut().modes_changed(&key);
    }

    /// Rewrites the session's overlay (omp re-reads it) and tells views.
    fn modes_changed(mut self: Pin<&mut Self>, key: &str) {
        if let Some((path, model)) = self.overlays.get(key).cloned() {
            let modes = self.modes.get(key).cloned().unwrap_or_default();
            self.as_mut().save_overlay(path, model, modes);
        }
        // Claude switches its permission mode, OpenCode its agent, on the spot.
        if self.live.get(key).is_some_and(|e| matches!(e.info.harness.as_str(), "claude" | "opencode"))
            && let Some(live) = registry::get(key)
        {
            let modes = self.modes.get(key).cloned().unwrap_or_default();
            live.handle.send_raw(serde_json::json!({ "type": "harlo_set_modes", "approval": modes.approval.unwrap_or_default(), "plan": modes.plan }));
        }
        let next = self.modes_revision + 1;
        self.as_mut().set_modes_revision(next);
    }

    pub fn provider_logo(&self, provider: &QString, color: &QString) -> QString {
        QString::from(&crate::provider_logos::data_url(&provider.to_string(), &color.to_string()))
    }

    pub fn key_sequence_text(&self, key: i32, modifiers: i32) -> QString {
        crate::platform::key_sequence_text(key, modifiers)
    }

    pub fn logs_dir(&self) -> QString {
        QString::from(&crate::diagnostics::logs_dir().to_string_lossy().into_owned())
    }

    pub fn choose_theme(mut self: Pin<&mut Self>, family: &QString) {
        self.save_setting("theme_family", &family.to_string());
        self.as_mut().set_theme_family(family.clone());
    }

    pub fn choose_theme_mode(mut self: Pin<&mut Self>, mode: &QString) {
        let mode = mode.to_string();
        if !["system", "dark", "light"].contains(&mode.as_str()) {
            return;
        }
        self.save_setting("theme_mode", &mode);
        self.as_mut().set_theme_mode(QString::from(&mode));
    }

    pub fn choose_text_scale(mut self: Pin<&mut Self>, scale: f64) {
        let scale = (scale.clamp(0.7, 1.6) * 100.0).round() / 100.0;
        self.save_setting("text_scale", &scale.to_string());
        self.as_mut().set_text_scale(scale);
    }

    pub fn choose_glass(mut self: Pin<&mut Self>, on: bool, opacity: f64) {
        let opacity = (opacity.clamp(0.4, 1.0) * 100.0).round() / 100.0;
        self.save_setting("glass", if on { "1" } else { "0" });
        self.save_setting("glass_opacity", &opacity.to_string());
        self.as_mut().set_glass(on);
        self.as_mut().set_glass_opacity(opacity);
    }

    pub fn choose_look(mut self: Pin<&mut Self>, key: &QString, value: &QString) {
        let mut look: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&self.look.to_string()).unwrap_or_default();
        match serde_json::from_str::<serde_json::Value>(&value.to_string()) {
            Ok(serde_json::Value::Null) | Err(_) => {
                look.remove(&key.to_string());
            }
            Ok(v) => {
                look.insert(key.to_string(), v);
            }
        }
        let text = serde_json::Value::Object(look).to_string();
        self.save_setting("look", &text);
        self.as_mut().set_look(QString::from(&text));
    }

    pub fn choose_font(mut self: Pin<&mut Self>, kind: &QString, family: &QString) {
        let family = QString::from(family.to_string().trim());
        match kind.to_string().as_str() {
            "ui" => {
                self.save_setting("ui_font", &family.to_string());
                crate::fonts::set_application_font_family(&QString::from(&crate::fonts::ui_family(&family.to_string())));
                self.as_mut().set_ui_font(family);
            }
            "mono" => {
                self.save_setting("mono_font", &family.to_string());
                self.as_mut().set_mono_font(family);
            }
            _ => {}
        }
    }

    pub fn system_font(&self, fixed: bool) -> QString {
        crate::fonts::system_font_family(fixed)
    }

    pub fn font_families(&self, fixed: bool) -> QStringList {
        crate::fonts::font_families(fixed)
    }

    pub fn theme_json(&self, family: &QString, variant: &QString) -> QString {
        QString::from(&crate::themes::find(&family.to_string(), &variant.to_string()).json)
    }

    pub fn theme_families(&self) -> QStringList {
        let mut list = QStringList::default();
        for f in crate::themes::families() {
            list.append(QString::from(&f));
        }
        list
    }

    pub fn detected_program(&self, harness: &QString) -> QString {
        let found = HarnessKind::parse(&harness.to_string()).and_then(harlo_harness::resolve_program);
        QString::from(&found.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())
    }

    pub fn local_copy(&self, path: &QString) -> QString {
        let p = PathBuf::from(path.to_string());
        if !harlo_core::remote::is_remote(&p) {
            return path.clone();
        }
        let dir = std::env::temp_dir().join(format!("harlo-remote-files-{}", std::process::id()));
        let name = format!("{:016x}-{}", {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            p.hash(&mut h);
            h.finish()
        }, p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        let local = dir.join(name);
        if local.is_file() {
            return QString::from(&local.to_string_lossy().into_owned());
        }
        // Copied over SSH off the UI thread; `copiesRevision` says when it's here.
        if self.copying.borrow_mut().insert(p.clone()) {
            let thread = self.qt_thread();
            crate::runtime().spawn_blocking(move || {
                let fetched = harlo_core::remote::read(&p).and_then(|bytes| {
                    std::fs::create_dir_all(&dir)?;
                    std::fs::write(&local, bytes)?;
                    Ok(())
                });
                if let Err(e) = &fetched {
                    tracing::warn!("copying {}: {e:#}", p.display());
                }
                let _ = thread.queue(move |mut this| {
                    this.copying.borrow_mut().remove(&p);
                    let next = this.copies_revision + 1;
                    this.as_mut().set_copies_revision(next);
                });
            });
        }
        QString::default()
    }

    pub fn file_stamp(&self, path: &QString) -> QString {
        // On a remote host: its stat, over SSH off the UI thread (this is
        // polled): the last one known now, a fresh one for the next poll.
        if let Some((host, f)) = harlo_core::remote::split(Path::new(&path.to_string())).and_then(|(h, f)| Some((harlo_core::remote::host(&h)?, f))) {
            let key = path.to_string();
            let mut stamps = self.remote_stamps.lock().unwrap();
            let known = stamps.get(&key).cloned();
            if known.as_ref().is_none_or(|(_, asking)| !asking) {
                stamps.insert(key.clone(), (known.as_ref().map(|k| k.0.clone()).unwrap_or_default(), true));
                let stamps = Arc::clone(&self.remote_stamps);
                crate::runtime().spawn_blocking(move || {
                    let q = harlo_core::remote::quote(&f);
                    let out = harlo_core::remote::run(&host, &format!("stat -c '%Y:%s' -- {q} 2>/dev/null || stat -f '%m:%z' -- {q}"), Duration::from_secs(10));
                    let stamp = out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default();
                    stamps.lock().unwrap().insert(key, (stamp, false));
                });
            }
            return QString::from(known.map(|k| k.0).unwrap_or_default().as_str());
        }
        // An OpenCode session (a subagent's): when it last changed.
        if let Some(t) = harlo_harness::opencode::stamp(Path::new(&path.to_string())) {
            return QString::from(&t);
        }
        let stamp = std::fs::metadata(path.to_string()).ok().map(|m| {
            let modified = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
            format!("{modified}:{}", m.len())
        });
        QString::from(&stamp.unwrap_or_default())
    }

    pub fn known_models(&self) -> QString {
        let files = harlo_core::stats::all_sessions();
        let list: Vec<serde_json::Value> = harlo_core::stats::known_models(&files)
            .into_iter()
            .take(40)
            .map(|(h, m, _)| serde_json::json!({ "harness": h, "model": m }))
            .collect();
        QString::from(&serde_json::Value::Array(list).to_string())
    }

    pub fn start_run(mut self: Pin<&mut Self>, form: &QString) -> QString {
        use harlo_core::db::{RunLane, RunRecord};
        let form: serde_json::Value = serde_json::from_str(&form.to_string()).unwrap_or_default();
        let project_id = form["projectId"].as_i64().unwrap_or(-1);
        let prompt = form["prompt"].as_str().unwrap_or_default().trim().to_owned();
        let starter = form["starter"].as_str().unwrap_or_default().to_owned();
        let lanes: Vec<(String, String)> = form["lanes"]
            .as_array()
            .map(|a| a.iter().map(|l| (l["harness"].as_str().unwrap_or("omp").to_owned(), l["model"].as_str().unwrap_or_default().to_owned())).collect())
            .unwrap_or_default();
        if lanes.len() < 2 || (prompt.is_empty() && starter.is_empty()) {
            self.set_notice_text("A run needs a prompt and at least two lanes.", true);
            return QString::default();
        }
        // The message every lane gets (starter instructions, Ask first).
        let mut message = prompt.clone();
        if form["askFirst"].as_bool() == Some(true) && !message.is_empty() {
            message.push_str(harlo_harness::file_blocks::ASK_FIRST_NOTE);
        }
        let starter_info = harlo_core::starters::find(&starter);
        if let Some(st) = starter_info {
            message = harlo_core::starters::wrap(st, &message);
        }
        let attachments: Vec<PathBuf> = form["attachments"].as_array().map(|a| a.iter().filter_map(|p| p.as_str().map(|p| PathBuf::from(url_to_path(p)))).collect()).unwrap_or_default();
        let first = FirstPrompt { text: message, attachments, goal: None };
        let title: String = {
            let t = harlo_harness::file_blocks::title_text(&prompt);
            let t = if t.is_empty() { starter_info.map(|s| s.label.to_owned()).unwrap_or_default() } else { t };
            t.chars().take(60).collect()
        };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let id = format!("{:x}", now.as_millis());
        let project = self.projects.iter().find(|p| p.id == project_id).cloned();
        let cwd = project.as_ref().map(|p| p.path.clone()).unwrap_or_else(|| self.instance_home());
        let use_worktrees = form["worktrees"].as_bool().unwrap_or(false) && project.is_some() && self.project_root(project_id as i32).is_some();
        let base = form["base"].as_str().unwrap_or_default().to_owned();
        let slug: String = git::branch_slug(&title.to_lowercase()).chars().take(24).collect::<String>().trim_matches('-').to_owned();
        let mut record = RunRecord { id: id.clone(), title: title.clone(), prompt: prompt.clone(), project_id, ..Default::default() };
        for (idx, (harness, model)) in lanes.iter().enumerate() {
            let letter = (b'a' + idx as u8) as char;
            let Some(kind) = HarnessKind::parse(harness) else { continue };
            let lane_ref = (id.clone(), idx as i64);
            if use_worktrees {
                let branch = format!("harlo/run-{}-{}-{letter}", if slug.is_empty() { "run" } else { &slug }, &id[id.len().saturating_sub(4)..]);
                let project = project.as_ref().unwrap();
                let path = self.worktree_base().join(format!("{}-{}", git::branch_slug(&project.name), project.id)).join(git::branch_slug(&branch));
                record.lanes.push(RunLane { idx: idx as i64, harness: harness.clone(), model: model.clone(), cwd: path, branch: branch.clone(), session_file: PathBuf::new() });
                self.as_mut().create_worktree_session(
                    project_id as i32,
                    &QString::from(&branch),
                    &QString::from(&base),
                    &QString::from(harness),
                    WorktreeStart { first: Some(first.clone()), model: model.clone(), lane: Some(lane_ref), parent: None, approval: None },
                );
            } else {
                record.lanes.push(RunLane { idx: idx as i64, harness: harness.clone(), model: model.clone(), cwd: cwd.clone(), branch: String::new(), session_file: PathBuf::new() });
                let key = self.as_mut().start_session_with(kind, cwd.clone(), model, false);
                self.as_mut().rust_mut().lane_keys.insert(key.clone(), lane_ref);
                send_first_prompt(&key, first.clone(), None);
            }
        }
        if let Some(db) = self.db() {
            let _ = db.add_run(&record);
        }
        let view = format!("view:run:{id}");
        self.as_mut().activate(&view);
        QString::from(&id)
    }

    fn find_run(&self, id: &str) -> Option<harlo_core::db::RunRecord> {
        self.db.as_ref()?.runs().ok()?.into_iter().find(|r| r.id == id)
    }

    pub fn runs_json(&self) -> QString {
        let runs = self.db.as_ref().and_then(|db| db.runs().ok()).unwrap_or_default();
        let list: Vec<serde_json::Value> = runs
            .iter()
            .map(|r| serde_json::json!({ "id": r.id, "title": r.title, "created": r.created_at, "kept": r.kept, "lanes": r.lanes.len() }))
            .collect();
        QString::from(&serde_json::Value::Array(list).to_string())
    }

    pub fn run_json(&self, id: &QString) -> QString {
        let Some(run) = self.find_run(&id.to_string()) else { return QString::default() };
        let lanes: Vec<serde_json::Value> = run
            .lanes
            .iter()
            .map(|l| {
                let live_key = self.lane_keys.iter().find(|(_, v)| v.0 == run.id && v.1 == l.idx).map(|(k, _)| k.clone())
                    .or_else(|| (!l.session_file.as_os_str().is_empty()).then(|| self.live_key_for_file(&l.session_file)).flatten());
                let status = match live_key.as_ref().and_then(|k| self.live.get(k)) {
                    Some(e) => sidebar::live_status(&e.info),
                    None if !l.session_file.as_os_str().is_empty() => "saved".to_owned(),
                    None => "starting".to_owned(),
                };
                let key = live_key.unwrap_or_else(|| l.session_file.to_string_lossy().into_owned());
                let answer = if l.session_file.as_os_str().is_empty() {
                    String::new()
                } else {
                    harlo_harness::store::read_saved(&l.session_file)
                        .ok()
                        .and_then(|s| {
                            s.messages.iter().rev().find(|m| m["role"] == "assistant").map(|m| match &m["content"] {
                                serde_json::Value::Array(b) => b.iter().filter(|x| x["type"] == "text").filter_map(|x| x["text"].as_str()).collect::<Vec<_>>().join("\n"),
                                serde_json::Value::String(t) => t.clone(),
                                _ => String::new(),
                            })
                        })
                        .unwrap_or_default()
                };
                let answer: String = answer.chars().take(900).collect();
                let changes = if !l.branch.is_empty() && l.cwd.is_dir() { git::status(&l.cwd).map(|s| s.files.len() as i64).unwrap_or(-1) } else { -1 };
                serde_json::json!({
                    "idx": l.idx,
                    "letter": ((b'A' + l.idx as u8) as char).to_string(),
                    "harness": l.harness,
                    "model": l.model,
                    "branch": l.branch,
                    "worktreeGone": !l.branch.is_empty() && !l.cwd.is_dir(),
                    "key": key,
                    "status": status,
                    "answer": answer,
                    "changes": changes,
                })
            })
            .collect();
        QString::from(&serde_json::json!({ "id": run.id, "title": run.title, "prompt": run.prompt, "kept": run.kept, "lanes": lanes }).to_string())
    }

    pub fn keep_run_lane(mut self: Pin<&mut Self>, id: &QString, idx: i32) {
        let Some(run) = self.find_run(&id.to_string()) else { return };
        let root = self.project_root(run.project_id as i32).map(|(_, r)| r);
        let mut cleanup = Vec::new();
        for lane in run.lanes.iter().filter(|l| l.idx != idx as i64) {
            // Stop and archive the other lanes' sessions.
            let key = self.lane_keys.iter().find(|(_, v)| v.0 == run.id && v.1 == lane.idx).map(|(k, _)| k.clone());
            if let Some(k) = key {
                self.as_mut().close_session(&QString::from(&k));
                self.as_mut().rust_mut().lane_keys.remove(&k);
            } else if let Some(k) = (!lane.session_file.as_os_str().is_empty()).then(|| self.live_key_for_file(&lane.session_file)).flatten() {
                self.as_mut().close_session(&QString::from(&k));
            }
            if !lane.session_file.as_os_str().is_empty() {
                self.as_mut().set_archived(&QString::from(&lane.session_file.to_string_lossy().into_owned()), true);
            }
            if !lane.branch.is_empty() {
                cleanup.push(lane.clone());
            }
        }
        if let Some(db) = self.db() {
            let _ = db.set_run_kept(&run.id, idx as i64);
        }
        let run_id = run.id.clone();
        self.as_mut().set_notice_text("Keeping the chosen lane; cleaning up the others…", false);
        self.background(
            move || {
                let mut failed = Vec::new();
                for lane in cleanup {
                    let letter = (b'a' + lane.idx as u8) as char;
                    let result = (|| -> anyhow::Result<()> {
                        if lane.cwd.is_dir() {
                            // Lanes rarely commit: keep their work reachable before removing it.
                            git::snapshot_worktree(&lane.cwd, &format!("refs/harlo/runs/{run_id}/{letter}"), &format!("harlo run {run_id}: lane {letter}"))?;
                            if let Some(root) = &root {
                                git::remove_worktree(root, &lane.cwd, true)?;
                            }
                        }
                        if let Some(root) = &root {
                            let _ = git::delete_branch(root, &lane.branch, true);
                        }
                        Ok(())
                    })();
                    if let Err(e) = result {
                        failed.push(format!("lane {}: {e:#}", letter.to_ascii_uppercase()));
                    }
                }
                failed
            },
            |mut this, failed: Vec<String>| {
                this.as_mut().reload_worktrees();
                if failed.is_empty() {
                    this.set_notice_text("Kept the lane; the others are archived (their work is saved under refs/harlo/runs/).", false);
                } else {
                    this.set_notice_text(&format!("Kept the lane, but cleanup failed for {}", failed.join("; ")), true);
                }
            },
        );
    }

    pub fn dismiss_run(mut self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        if let Some(db) = self.db() {
            let _ = db.delete_run(&id);
        }
        self.as_mut().rust_mut().lane_keys.retain(|_, v| v.0 != id);
        self.close_tab(&QString::from(&format!("view:run:{id}")));
    }

    pub fn running_sessions(&self) -> QString {
        let mut list: Vec<(u8, serde_json::Value)> = self
            .live
            .iter()
            .filter(|(_, e)| e.info.status != SessionStatus::Exited)
            .map(|(key, e)| {
                let title = self.rows.iter().find(|r| &r.key == key).map(|r| r.title.clone()).unwrap_or_else(|| "New session".into());
                let status = sidebar::live_status(&e.info);
                let rank = if e.info.needs_input { 0 } else if status == "busy" { 1 } else { 2 };
                (rank, serde_json::json!({ "key": key, "title": title, "status": status }))
            })
            .collect();
        list.sort_by_key(|(rank, _)| *rank);
        QString::from(&serde_json::Value::Array(list.into_iter().map(|(_, v)| v).collect()).to_string())
    }

    pub fn session_file(&self, key: &QString) -> QString {
        let key = key.to_string();
        let file = match self.live.get(&key) {
            Some(entry) => entry.info.session_file.clone(),
            None => Some(PathBuf::from(&key)).filter(|p| self.sessions.contains_key(p)),
        };
        QString::from(&file.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())
    }

    pub fn session_branches(&self, path: &QString) -> QString {
        // (A remote one: as last read by `request_session_branches`.)
        let list = if harlo_core::remote::is_remote(Path::new(&path.to_string())) {
            self.branch_texts.lock().unwrap().get(&path.to_string()).map(|t| harlo_harness::branches::branches_text(t)).unwrap_or_default()
        } else {
            harlo_harness::branches::branches(Path::new(&path.to_string())).unwrap_or_default()
        };
        QString::from(&branches_json(list))
    }

    pub fn request_session_branches(&self, path: &QString) {
        let (path, thread, texts) = (path.to_string(), self.qt_thread(), Arc::clone(&self.branch_texts));
        crate::runtime().spawn_blocking(move || {
            let p = Path::new(&path);
            let list = if harlo_core::remote::is_remote(p) {
                let text = harlo_core::remote::read(p).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                let list = harlo_harness::branches::branches_text(&text);
                texts.lock().unwrap().insert(path.clone(), text);
                list
            } else {
                harlo_harness::branches::branches(p).unwrap_or_default()
            };
            let json = branches_json(list);
            let _ = thread.queue(move |this| this.session_branches_ready(&QString::from(&path), &QString::from(&json)));
        });
    }

    pub fn branch_preview_file(&self, path: &QString, leaf: &QString) -> QString {
        let dir = std::env::temp_dir().join(format!("harlo-branch-preview-{}", std::process::id()));
        // (A remote session: from its text as last read.)
        if harlo_core::remote::is_remote(Path::new(&path.to_string())) {
            let Some(text) = self.branch_texts.lock().unwrap().get(&path.to_string()).cloned() else { return QString::default() };
            let Ok((name, body)) = harlo_harness::branches::branch_file(&text, &leaf.to_string(), &path.to_string()) else { return QString::default() };
            let dest = dir.join(name);
            return if std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&dest, body)).is_ok() { QString::from(&dest.to_string_lossy().into_owned()) } else { QString::default() };
        }
        match harlo_harness::branches::write_branch_in(Path::new(&path.to_string()), &leaf.to_string(), &dir) {
            Ok(p) => QString::from(&p.to_string_lossy().into_owned()),
            Err(_) => QString::default(),
        }
    }

    pub fn continue_branch(mut self: Pin<&mut Self>, path: &QString, leaf: &QString) -> QString {
        let original = PathBuf::from(path.to_string());
        let Some(kind) = self.sessions.get(&original).map(|s| s.harness) else { return QString::default() };
        // A remote session: the new file is written on its host (over SSH,
        // in the background), then opened.
        if harlo_core::remote::is_remote(&original) {
            let Some(text) = self.branch_texts.lock().unwrap().get(&path.to_string()).cloned() else { return QString::default() };
            let leaf = leaf.to_string();
            self.as_mut().background(
                move || -> Result<(PathBuf, String), String> {
                    let (name, body) = harlo_harness::branches::branch_file(&text, &leaf, &harlo_harness::on_host(&original)).map_err(|e| e.to_string())?;
                    let dest = original.with_file_name(name);
                    harlo_core::remote::write(&dest, body.as_bytes()).map_err(|e| format!("{e:#}"))?;
                    Ok((dest, body))
                },
                move |mut this, result| match result {
                    Ok((dest, body)) => {
                        // (Its folder, as the host names it: on that host.)
                        if let Some(mut info) = harlo_harness::store::session_from_text(kind, &dest, body.len() as u64, std::time::SystemTime::now(), &body, None) {
                            if let Some(host) = harlo_core::remote::host_of(&dest) {
                                info.cwd = harlo_core::remote::join(&host, &info.cwd.to_string_lossy());
                            }
                            this.as_mut().rust_mut().sessions.insert(dest.clone(), info);
                            this.as_mut().rebuild();
                        }
                        this.open(&QString::from(&dest.to_string_lossy().into_owned()));
                    }
                    Err(e) => this.set_notice_text(&format!("Could not copy the branch: {e}"), true),
                },
            );
            return QString::default();
        }
        let new_path = match harlo_harness::branches::write_branch(&original, &leaf.to_string()) {
            Ok(p) => p,
            Err(e) => {
                self.set_notice_text(&format!("Could not copy the branch: {e}"), true);
                return QString::default();
            }
        };
        // Make it known right away (the scanner would find it shortly).
        if let Some(info) = harlo_harness::store::read_session(kind, &new_path) {
            self.as_mut().rust_mut().sessions.insert(new_path.clone(), info);
            self.as_mut().rebuild();
        }
        self.open(&QString::from(&new_path.to_string_lossy().into_owned()))
    }

    pub fn search_messages(&self, query: &QString, limit: i32) -> QString {
        let Some(index) = &self.search else { return QString::from("[]") };
        let hits = index.lock().unwrap().search(&query.to_string(), limit.clamp(1, 500) as usize).unwrap_or_default();
        let list: Vec<serde_json::Value> = hits
            .into_iter()
            .map(|h| {
                let info = self.sessions.get(&h.path);
                serde_json::json!({
                    "path": h.path.to_string_lossy(),
                    "index": h.index,
                    "role": h.role,
                    "snippet": h.snippet,
                    "title": info.map(|i| i.display_title().to_owned()).unwrap_or_default(),
                    "harness": info.map(|i| i.harness.as_str()).unwrap_or_default(),
                    "cwd": info.map(|i| harlo_core::sidebar::tilde(&i.cwd)).unwrap_or_default(),
                    "modified": info.map(|i| i.modified.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)).unwrap_or(0),
                })
            })
            .collect();
        QString::from(&serde_json::Value::Array(list).to_string())
    }

    pub fn home_dir(&self) -> QString {
        QString::from(&std::env::var("HOME").unwrap_or_else(|_| "/".into()))
    }

    pub fn default_worktree_dir(&self) -> QString {
        QString::from(&worktrees_dir().to_string_lossy().into_owned())
    }

    pub fn ignored_repo_list(&self) -> QStringList {
        let mut repos: Vec<String> = self.ignored_repos.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        repos.sort();
        let mut list = QStringList::default();
        for r in repos {
            list.append(QString::from(&r));
        }
        list
    }

    /// A new omp session's `--config` overlay (see `harlo_core::omp_overlay`):
    /// goals continue between turns, and subagents fall back to the
    /// session's model (`model`, `provider/id`, when already known). None
    /// when Settings → Providers turns both off.
    /// How a session in `cwd` runs: here with Settings' program, or on its
    /// remote host over SSH (that machine's PATH finds pi / omp).
    fn launch_for(&self, kind: HarnessKind, cwd: &Path) -> (Option<PathBuf>, Option<RemoteLaunch>) {
        match harlo_core::remote::host_of(cwd).and_then(|h| harlo_core::remote::host(&h)) {
            Some(host) => (None, Some(harlo_core::remote::launch(&host))),
            None => (self.program_for(kind), None),
        }
    }

    /// A new omp session's `--config` overlay. On a remote host the launch
    /// command writes it (a private file in /tmp, removed when omp exits);
    /// the complete version, with that machine's omp config merged in,
    /// follows over SSH.
    fn new_overlay(self: Pin<&mut Self>, kind: HarnessKind, model: &str, remote: Option<&mut RemoteLaunch>) -> Option<PathBuf> {
        if kind != HarnessKind::Omp {
            return None;
        }
        let goal = self.setting_str("goal_continue") != "0";
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(remote) = remote {
            let file = format!("/tmp/harlo-overlay-{:016x}-{n}.yml", run_token());
            let first = harlo_core::omp_overlay::Overlay { goal_continue: goal, subagent_fallback: None, approval_mode: None, plan: None }.render();
            let quoted = harlo_core::remote::quote(&file);
            remote.prelude.push_str(&format!(" ( umask 077; cat > {quoted} <<'HARLO_OVERLAY'\n{first}HARLO_OVERLAY\n);"));
            remote.cleanup = format!("rm -f {quoted}");
            let path = harlo_core::remote::join(&remote.host, &file);
            self.save_overlay(path.clone(), model.to_owned(), SessionModes::default());
            return Some(path);
        }
        let dir = overlays_dir();
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join(format!("{}-{n}.yml", std::process::id()));
        self.save_overlay(path.clone(), model.to_owned(), SessionModes::default());
        path.exists().then_some(path)
    }

    /// Writes an overlay (omp watches it): here at once, on a remote host
    /// in the background (then views re-read the approval mode).
    fn save_overlay(self: Pin<&mut Self>, path: PathBuf, model: String, modes: SessionModes) {
        let (goal, subagents) = (self.setting_str("goal_continue") != "0", self.setting_str("subagent_fallback") != "0");
        let Some(host) = harlo_core::remote::host_of(&path) else {
            let text = overlay_text(&user_omp_config(), goal, subagents, &model, &modes);
            if let Err(e) = std::fs::write(&path, text) {
                tracing::warn!("could not write {}: {e}", path.display());
            }
            return;
        };
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let config = harlo_core::remote::home_file_cached(&host, OMP_CONFIG, Duration::from_secs(60));
            if let Err(e) = harlo_core::remote::write(&path, overlay_text(&config, goal, subagents, &model, &modes).as_bytes()) {
                tracing::warn!("could not write {}: {e:#}", path.display());
            }
            let _ = thread.queue(|mut this| {
                let next = this.modes_revision + 1;
                this.as_mut().set_modes_revision(next);
            });
        });
    }

    /// The session's model changed: its overlay follows (omp re-reads it).
    fn update_overlay(mut self: Pin<&mut Self>, key: &str, model: &str) {
        let Some((path, last)) = self.overlays.get(key).cloned() else { return };
        if model.is_empty() || model == last {
            return;
        }
        let modes = self.modes.get(key).cloned().unwrap_or_default();
        self.as_mut().rust_mut().overlays.insert(key.to_owned(), (path.clone(), model.to_owned()));
        self.save_overlay(path, model.to_owned(), modes);
    }

    /// The binary set in Settings for a harness, if any (else found on PATH).
    fn program_for(&self, kind: HarnessKind) -> Option<PathBuf> {
        let path = self.setting_str(&format!("{}_path", kind.as_str()));
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    /// Harness for new sessions outside a project and for new projects:
    /// the setting, else omp if installed, else pi.
    fn default_harness(&self) -> String {
        let set = self.setting_str("default_harness");
        if let Some(kind) = HarnessKind::parse(&set)
            && set != "demo"
            && self.usable_harness(kind)
        {
            return set;
        }
        // The first one on and installed, in Settings → Providers' order.
        let usable: Vec<HarnessKind> = HarnessKind::PROVIDERS.iter().copied().filter(|&k| self.usable_harness(k)).collect();
        usable
            .iter()
            .find(|&&k| self.program_for(k).is_some() || harlo_harness::resolve_program(k).is_some())
            .or(usable.first())
            .map_or("omp", |k| k.as_str())
            .into()
    }

    /// Where new worktrees go: the setting, else harlo's data folder.
    fn worktree_base(&self) -> PathBuf {
        let set = self.setting_str("worktree_dir");
        if set.is_empty() { worktrees_dir() } else { PathBuf::from(url_to_path(&set)) }
    }

    pub fn session_repo(&self, key: &QString) -> QString {
        let key = key.to_string();
        let root = self
            .rows
            .iter()
            .find(|r| r.key == key)
            .filter(|r| !r.cwd.as_os_str().is_empty())
            .and_then(|r| git::repo_root(&r.cwd));
        QString::from(&root.map(|r| r.to_string_lossy().into_owned()).unwrap_or_default())
    }

    pub fn is_repo_ignored(&self, root: &QString) -> bool {
        self.ignored_repos.contains(&PathBuf::from(root.to_string()))
    }

    pub fn set_repo_ignored(mut self: Pin<&mut Self>, root: &QString, ignored: bool) {
        let root = PathBuf::from(root.to_string());
        if root.as_os_str().is_empty() {
            return;
        }
        let mut this = self.as_mut().rust_mut();
        let changed = if ignored { this.ignored_repos.insert(root) } else { this.ignored_repos.remove(&root) };
        if !changed {
            return;
        }
        let mut list: Vec<String> = this.ignored_repos.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        list.sort();
        if let Some(db) = &this.db {
            let _ = db.set(IGNORED_REPOS_KEY, &list.join("\n"));
        }
        this.change_kick.notify_one();
    }

    fn apply_change_counts(mut self: Pin<&mut Self>, dirs: Vec<PathBuf>, counts: HashMap<PathBuf, usize>) {
        let mut this = self.as_mut().rust_mut();
        this.change_checked = dirs.into_iter().collect();
        if this.change_counts == counts {
            return;
        }
        this.change_counts = counts;
        self.rebuild();
    }

    /// Notifies when a background session finishes or needs input.
    /// A desktop notification for a session, as Settings → Notifications
    /// says: on for that event, not for the session you're looking at, only
    /// in the background unless "while focused", at most once per 5 s per
    /// event and session. Clicking it opens the session.
    /// Questions and approvals waiting, in all sessions.
    fn waiting_count(&self) -> usize {
        self.live.values().map(|e| e.requests.len()).sum()
    }

    /// The iOS app's notification for `note`, while it's closed (see
    /// `phone_push`): questions and approvals, finished and failed runs.
    fn push_to_phones(&self, key: &str, note: &Note, goal_active: bool, quiet: &harlo_core::notifications::Quiet) {
        use crate::phone_push::What;
        use harlo_core::Notable;
        let (what, body) = match note {
            // Between a goal's turns omp goes on by itself: its end notifies.
            Note::Event(Notable::Completed { .. }) if goal_active => return,
            Note::Event(Notable::Completed { last_message }) => (What::Done, last_message.clone()),
            Note::Event(Notable::Error { message }) => (What::Failed, message.clone()),
            Note::Event(Notable::SubagentDone { .. }) => return,
            Note::Question(text, request) => (What::Ask(request.clone()), text.clone()),
            Note::GoalSettled(status, objective) => (What::Done, format!("Goal {}: {objective}", if status.starts_with("complete") { "complete" } else { status.as_str() })),
        };
        let title = self.rows.iter().find(|r| r.key == key).map(|r| r.title.clone()).unwrap_or_default();
        let body = harlo_core::notifications::summary(&body, harlo_core::notifications::LAST_MESSAGE_CHARS);
        // Quiet hours: it comes silently.
        let quiet = quiet.hushes_now(matches!(what, What::Ask(_)));
        crate::phone_push::send(crate::phone_push::Alert { what, key: key.to_owned(), title, body, badge: self.waiting_count(), quiet });
    }

    fn notify(mut self: Pin<&mut Self>, key: &str, note: Note) {
        use harlo_core::Notable;
        let settings = harlo_core::notifications::Settings::load(&self.setting_str("notify"), &self.setting_str("notifications"));
        let entry = self.live.get(key);
        let goal_active = entry.is_some_and(|e| e.goal_status == "active");
        self.push_to_phones(key, &note, goal_active, &settings.quiet);
        let model = entry.map(|e| e.model.clone()).unwrap_or_default();
        let plan = self.modes.get(key).is_some_and(|m| m.plan);
        let title_case = |s: &str| s.split(['-', '_', ' ']).filter(|w| !w.is_empty()).map(|w| { let mut c = w.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }).collect::<Vec<_>>().join(" ");
        let mut agent = if plan { "Plan".to_owned() } else { "Build".to_owned() };
        // event, last message, fallback title and body
        let (event, last, fallback) = match note {
            // Between a goal's turns omp goes on by itself: its end notifies.
            Note::Event(Notable::Completed { .. }) if goal_active => return,
            Note::Event(Notable::Completed { last_message }) => ("completion", last_message, (format!("{agent} agent is ready"), format!("{} completed the task", if model.is_empty() { "The agent".to_owned() } else { model.clone() }))),
            Note::Event(Notable::Error { message }) => ("error", message, ("Tool error".to_owned(), "An error occurred".to_owned())),
            Note::Event(Notable::SubagentDone { agent: sub, status, task, .. }) => {
                agent = title_case(&sub);
                let failed = status != "completed";
                let last = if failed { format!("{agent} {status}: {task}") } else { task };
                (if failed { "error" } else { "subagent" }, last, (format!("{agent} is ready"), format!("{agent} finished")))
            }
            Note::Question(text, _) => ("question", text, ("Input needed".to_owned(), "The agent is waiting for your answer".to_owned())),
            Note::GoalSettled(status, objective) => {
                let title = match status.as_str() {
                    "complete" | "completed" => "Goal complete",
                    "budget-limited" => "Goal reached its token budget",
                    _ => "Goal blocked",
                };
                ("completion", objective, (title.to_owned(), String::new()))
            }
        };
        let goal_note = fallback.1.is_empty();
        if !settings.enabled || !settings.event(event).is_some_and(|e| e.0) {
            return;
        }
        // Quiet hours: none on the desktop.
        if settings.quiet.hushes_now(event == "question") {
            return;
        }
        let viewing = self.active_key.to_string() == key;
        if self.window_active && (!settings.while_focused || viewing) {
            return;
        }
        let tag = format!("{event}-{key}");
        let now = std::time::Instant::now();
        if self.notified_at.get(&tag).is_some_and(|t| now.duration_since(*t) < Duration::from_secs(5)) {
            return;
        }
        self.as_mut().rust_mut().notified_at.insert(tag.clone(), now);

        let row = self.rows.iter().find(|r| r.key == key).cloned();
        let cwd = row.as_ref().map(|r| r.cwd.clone()).unwrap_or_default();
        let project = row.as_ref().and_then(|r| self.projects.iter().find(|p| p.id == r.project_id)).map(|p| p.name.clone())
            .unwrap_or_else(|| cwd.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        let session = row.as_ref().map(|r| r.title.clone()).unwrap_or_default();
        let branch = row.as_ref().map(|r| r.branch.clone()).unwrap_or_default();
        let last = harlo_core::notifications::summary(&last, harlo_core::notifications::LAST_MESSAGE_CHARS);
        let thread = self.qt_thread();
        let key = key.to_owned();
        crate::runtime().spawn_blocking(move || {
            // The branch: the row's (worktrees), else git's (here or on the host).
            let branch = if branch.is_empty() && !cwd.as_os_str().is_empty() { git::output_in(&cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).trim().to_owned() } else { branch };
            let vars = [
                ("project_name", project),
                ("worktree", sidebar::tilde(&cwd)),
                ("branch", branch),
                ("session_name", session),
                ("agent_name", agent),
                ("model_name", model),
                ("last_message", last),
            ];
            let (title, body) = if goal_note {
                (fallback.0, vars[6].1.clone())
            } else {
                settings.render_event(event, &vars, (&fallback.0, &fallback.1))
            };
            let _ = thread.queue(move |mut this| this.as_mut().deliver(&tag, event, &key, &title, &body, settings.sound));
        });
    }

    /// Shows a notification (self-tests only record it).
    fn deliver(mut self: Pin<&mut Self>, tag: &str, kind: &str, key: &str, title: &str, body: &str, sound: bool) {
        self.as_mut().set_last_notification(QString::from(&json!({ "kind": kind, "key": key, "title": title, "body": body }).to_string()));
        if std::env::args().any(|a| a.starts_with("--selftest")) {
            return;
        }
        let thread = self.qt_thread();
        let key = key.to_owned();
        crate::notify::send(tag, title, body, sound, move || {
            let _ = thread.queue(move |mut this| {
                if !key.is_empty() {
                    this.as_mut().open(&QString::from(&key));
                }
                this.as_mut().raise_requested();
            });
        });
    }

    pub fn notify_settings_json(&self) -> QString {
        let s = harlo_core::notifications::Settings::load(&self.setting_str("notify"), &self.setting_str("notifications"));
        QString::from(&s.to_json().to_string())
    }

    pub fn set_notify_settings(self: Pin<&mut Self>, json: &QString) {
        // Parsed first: only known, valid parts are kept.
        let s = harlo_core::notifications::Settings::parse(&json.to_string());
        self.save_setting("notify", &s.to_json().to_string());
    }

    pub fn notify_defaults_json(&self) -> QString {
        use harlo_core::notifications::{EVENTS, VARIABLES, default_template};
        let defaults: serde_json::Map<String, serde_json::Value> = EVENTS
            .iter()
            .map(|e| {
                let (title, message) = default_template(e);
                ((*e).to_owned(), json!({ "title": title, "message": message }))
            })
            .collect();
        QString::from(&json!({ "variables": VARIABLES, "defaults": defaults }).to_string())
    }

    pub fn send_test_notification(self: Pin<&mut Self>) {
        let sound = harlo_core::notifications::Settings::load(&self.setting_str("notify"), &self.setting_str("notifications")).sound;
        self.deliver("test", "test", "", "Test notification", "This is a test notification from harlo.", sound);
    }

    fn on_live_updates(mut self: Pin<&mut Self>, key: &str, updates: Vec<SessionUpdate>) {
        let active = self.active_key.to_string() == key;
        let mut this = self.as_mut().rust_mut();
        let Some(entry) = this.live.get_mut(key) else { return };
        let mut became_idle = false;
        let mut became_busy = false;
        let mut lane_file: Option<PathBuf> = None;
        let mut model_id: Option<String> = None;
        let mut cost: Option<f64> = None;
        let mut notes: Vec<Note> = Vec::new();
        let mut answered = false;
        let needed_input = entry.info.needs_input;
        for u in updates {
            match u {
                SessionUpdate::Status(s) => {
                    became_idle |= entry.info.status == SessionStatus::Busy && s == SessionStatus::Idle;
                    became_busy |= entry.info.status != SessionStatus::Busy && s == SessionStatus::Busy;
                    entry.info.status = s;
                }
                SessionUpdate::Meta(m) => {
                    cost = Some(m.cost);
                    entry.info.session_file = m.session_file.or(entry.info.session_file.take());
                    lane_file = entry.info.session_file.clone();
                    entry.info.name = m.name;
                    entry.info.first_message = m.first_message;
                    entry.model = m.model;
                    entry.model_id = m.model_id.clone();
                    model_id = Some(m.model_id);
                }
                SessionUpdate::UiRequest(r) => {
                    if !entry.requests.iter().any(|(q, _)| q.id == r.id) {
                        notes.push(Note::Question(question_text(&r), crate::phone_server::request_json(&r)));
                        let asked = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
                        entry.requests.push((r, asked));
                    }
                }
                SessionUpdate::Notable(n) => notes.push(Note::Event(n)),
                SessionUpdate::Goal(goal) => {
                    let status = goal["status"].as_str().unwrap_or_default().to_owned();
                    if entry.goal_status == "active" && matches!(status.as_str(), "complete" | "completed" | "budget-limited" | "blocked") {
                        notes.push(Note::GoalSettled(status.clone(), goal["objective"].as_str().unwrap_or_default().to_owned()));
                    }
                    entry.goal_status = status;
                }
                SessionUpdate::UiCleared(id) => {
                    let before = entry.requests.len();
                    entry.requests.retain(|(q, _)| q.id != id);
                    answered |= entry.requests.len() != before;
                }
                _ => {}
            }
        }
        entry.info.needs_input = !entry.requests.is_empty();
        let _ = needed_input;
        // Whose limits it's using: Claude's sign-in, or (omp, pi) the
        // model's provider, as omp names its accounts.
        let used = became_busy
            .then(|| match entry.info.harness.as_str() {
                "claude" => Some("Claude".to_owned()),
                "omp" | "pi" => entry.model_id.split_once('/').map(|(p, _)| p.to_owned()).filter(|p| !p.is_empty()),
                _ => None,
            })
            .flatten();
        if became_idle && !active {
            entry.info.unread = true;
        }
        if became_idle {
            // The agent may have edited files.
            this.change_kick.notify_one();
        }
        // A run lane's session file: remember it for the run.
        if let Some(file) = lane_file
            && let (Some((run, idx)), Some(db)) = (self.lane_keys.get(key), &self.db) {
                let _ = db.set_lane_file(run, *idx, &file);
            }
        if let Some(model) = model_id {
            self.as_mut().update_overlay(key, &model);
        }
        // What it cost since the last report: the day's too.
        if let Some(cost) = cost {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let mut this = self.as_mut().rust_mut();
            let added = this.spend.entry(key.to_owned()).or_default().observe(cost);
            if added > 0.0 {
                this.spend_ledger.add(&today, added);
                self.save_setting("spend_today", &self.spend_ledger.to_json().to_string());
            }
            self.as_mut().refresh_spending();
        }
        if let Some(used) = used.filter(|u| *u != self.meter_last.to_string()) {
            self.save_setting("meter_last", &used);
            self.as_mut().set_meter_last(QString::from(&used));
        }
        self.as_mut().rebuild();
        // A session an agent started: its state, and its last reply back.
        if became_busy || became_idle {
            self.as_mut().child_progress(key, became_idle, &notes);
        }
        for note in notes {
            self.as_mut().notify(key, note);
        }
        // The phone's badge: one fewer waiting.
        if answered {
            crate::phone_push::send(crate::phone_push::Alert {
                what: crate::phone_push::What::Badge, key: key.to_owned(), title: String::new(), body: String::new(), badge: self.waiting_count(), quiet: false,
            });
        }
        if active {
            self.update_active_title();
        }
    }

    /// A phone's request (see `phone_server`): `{ op, … }` → its reply.
    fn phone_call(mut self: Pin<&mut Self>, msg: &serde_json::Value) -> serde_json::Value {
        // (The agents' models, for a phone's new session.)
        if msg["op"] == "projects" {
            self.as_mut().ensure_providers();
        }
        let text = |k: &str| msg[k].as_str().unwrap_or_default().to_owned();
        let key = text("key");
        let fail = |e: &str| json!({ "error": e });
        match msg["op"].as_str().unwrap_or_default() {
            "publish" => {
                self.publish_phone_sessions();
                json!({})
            }
            "open" => {
                let live = self.as_mut().make_live(&key);
                self.as_mut().rebuild();
                match live {
                    Some(k) => json!({ "key": k }),
                    None => fail("That session isn't available."),
                }
            }
            "projects" => {
                let mut agents: Vec<serde_json::Value> = HarnessKind::PROVIDERS
                    .iter()
                    .filter(|&&k| self.usable_harness(k))
                    .map(|k| json!({ "id": k.as_str(), "label": k.label() }))
                    .collect();
                // (Tests start demo sessions.)
                if std::env::args().any(|a| a.starts_with("--selftest")) {
                    agents.push(json!({ "id": "demo", "label": "Demo" }));
                }
                // The shield's mode each agent starts in, there ("" none).
                let approvals = |project: i64| -> serde_json::Value {
                    agents.iter().map(|a| {
                        let id = a["id"].as_str().unwrap_or_default();
                        (id.to_owned(), json!(self.draft_approval(&QString::from(id), project as i32).to_string()))
                    }).collect::<serde_json::Map<_, _>>().into()
                };
                // Every machine's projects (whichever one the window shows),
                // each with its machine and the agents there.
                let projects: Vec<serde_json::Value> = self
                    .projects
                    .iter()
                    .map(|p| json!({ "id": p.id, "name": p.name, "where": where_on_host(&p.path), "agent": p.harness, "icon": p.icon, "color": p.color,
                                     "approvals": approvals(p.id), "host": harlo_core::remote::host_of(&p.path).unwrap_or_default(),
                                     "agents": self.agents_at(&p.path) }))
                    .collect();
                let models: serde_json::Value = serde_json::from_str(&self.known_models().to_string()).unwrap_or_default();
                // Every model each agent offers (Settings → Providers reads them).
                let all: serde_json::Map<String, serde_json::Value> = self
                    .provider_checks
                    .iter()
                    .filter_map(|(id, st)| {
                        let list: Vec<serde_json::Value> = st.as_ref()?.models.iter().map(|(m, name)| json!({ "id": m, "name": name })).collect();
                        Some((id.clone(), serde_json::Value::from(list)))
                    })
                    .collect();
                let templates: serde_json::Value = serde_json::from_str(&self.templates_json.to_string()).unwrap_or_default();
                json!({ "projects": projects, "agents": agents, "defaultAgent": self.default_harness(), "models": models, "agentModels": all, "approvals": approvals(-1),
                        "templates": templates })
            }
            "new" => {
                let project = msg["project"].as_i64().unwrap_or(-1);
                let project = self.projects.iter().find(|p| p.id == project).cloned();
                let agent = match text("agent") {
                    a if a.is_empty() => project.as_ref().map(|p| p.harness.clone()).unwrap_or_else(|| self.default_harness()),
                    a => a,
                };
                // (No project: this computer's home folder, whichever machine the window shows.)
                let cwd = project.as_ref().map(|p| p.path.clone()).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/".into())));
                // (On the project's own machine: its agents.)
                let Some(kind) = HarnessKind::parse(&agent).filter(|k| self.agents_at(&cwd).contains(&k.as_str())) else {
                    return fail(&format!("harlo can't start {agent} sessions there."));
                };
                let (first, images) = (text("text"), phone_images(&msg["images"]));
                // The model picked, else the agent's default from Settings (the
                // setup guide; not on another machine: its agent's own).
                let remote = harlo_core::remote::host_of(&cwd).is_some();
                let model = match text("model") {
                    m if m.is_empty() && !remote => self.setting_str(&format!("default_model_{}", kind.as_str())),
                    m => m,
                };
                let key = self.as_mut().start_session_with(kind, cwd, &model, false);
                // The shield's mode it starts in (before its first message).
                let approval = text("approval");
                if !approval.is_empty() {
                    self.as_mut().set_approval_mode(&QString::from(&key), &QString::from(&approval));
                }
                self.as_mut().rebuild();
                if (!first.trim().is_empty() || !images.is_empty())
                    && let Some(live) = registry::get(&key)
                    && !live.handle.prompt(&format!("{}{first}", image_blocks(images.len())), images, harlo_harness::SendMode::Normal)
                {
                    return json!({ "key": key, "harness": kind.as_str(), "error": live.handle.held().unwrap_or_default() });
                }
                json!({ "key": key, "harness": kind.as_str() })
            }
            "send" => {
                let Some(live) = registry::get(&key) else { return fail("That session isn't running.") };
                let mode = match msg["mode"].as_str() {
                    Some("steer") => harlo_harness::SendMode::Steer,
                    Some("followUp") => harlo_harness::SendMode::FollowUp,
                    _ => harlo_harness::SendMode::Normal,
                };
                let images = phone_images(&msg["images"]);
                let mut text = format!("{}{}", image_blocks(images.len()), text("text"));
                // Plan mode: a plan first, no changes (as the desktop sends).
                if self.modes.get(&key).is_some_and(|m| m.plan) && mode == harlo_harness::SendMode::Normal {
                    text.push_str(harlo_harness::file_blocks::PLAN_NOTE);
                }
                if !live.handle.prompt(&text, images, mode) {
                    return fail(&live.handle.held().unwrap_or_else(|| "harlo holds this session's messages.".into()));
                }
                json!({ "ok": true })
            }
            "modes" => self.phone_modes(&key),
            "setModes" => {
                if let Some(mode) = msg["approval"].as_str() {
                    self.as_mut().set_approval_mode(&QString::from(&key), &QString::from(mode));
                }
                if let Some(on) = msg["plan"].as_bool() {
                    self.as_mut().set_plan_mode(&QString::from(&key), on);
                }
                self.phone_modes(&key)
            }
            "rename" => {
                let name = text("name");
                let Some(live_key) = self.as_mut().make_live(&key) else { return fail("That session isn't available.") };
                if let Some(live) = registry::get(&live_key) {
                    live.handle.send_raw(json!({ "type": "set_session_name", "name": name.trim() }));
                }
                json!({ "ok": true })
            }
            "archive" => {
                if self.file_for_key(&key).is_none() {
                    return fail("This session isn't saved yet.");
                }
                let on = msg["archived"].as_bool().unwrap_or(true);
                self.as_mut().update_meta(&key, |m| m.archived = on);
                json!({ "ok": true })
            }
            "pin" => {
                if self.file_for_key(&key).is_none() {
                    return fail("This session isn't saved yet.");
                }
                let on = msg["pinned"].as_bool().unwrap_or(true);
                self.as_mut().update_meta(&key, |m| m.pinned = on);
                json!({ "ok": true })
            }
            "theme" => self.phone_theme(),
            // The phone picked your penguin: the left rail's too.
            "setAvatar" => {
                let name = text("avatar");
                if !AVATARS.contains(&name.as_str()) {
                    return fail("harlo has no such avatar.");
                }
                self.as_mut().choose_look(&QString::from("avatar"), &QString::from(&json!(name).to_string()));
                self.phone_theme()
            }
            other => fail(&format!("unknown op {other}")),
        }
    }

    /// The theme's colours, its mode and your avatar, for phones.
    fn phone_theme(&self) -> serde_json::Value {
        let family = self.theme_family.to_string();
        let colors = |variant: &str| -> serde_json::Value {
            serde_json::from_str::<serde_json::Value>(&crate::themes::find(&family, variant).json).map(|v| v["colors"].clone()).unwrap_or_default()
        };
        // Your avatar (the left rail's penguin), for the phone's chats.
        let look: serde_json::Value = serde_json::from_str(&self.look.to_string()).unwrap_or_default();
        let avatar = look["avatar"].as_str().filter(|a| AVATARS.contains(a)).unwrap_or("classic").to_owned();
        json!({ "family": family, "mode": self.theme_mode.to_string(), "dark": colors("dark"), "light": colors("light"), "avatar": avatar })
    }

    /// A session's approval mode and Plan, for phones.
    fn phone_modes(&self, key: &str) -> serde_json::Value {
        let approval = self.approval_mode(&QString::from(key)).to_string();
        let approvals: Vec<&str> = if approval.is_empty() { Vec::new() } else { harlo_core::omp_overlay::APPROVAL_MODES.to_vec() };
        json!({ "approval": approval, "approvals": approvals, "plan": self.plan_mode(&QString::from(key)) })
    }

    /// Every session (projects expanded, no "show more"), for phones.
    fn publish_phone_sessions(&self) {
        let sessions: Vec<SessionInfo> = self.sessions.values().cloned().collect();
        let live: Vec<LiveInfo> = self.live.values().map(|e| e.info.clone()).collect();
        let projects: Vec<Project> = self.projects.iter().cloned().map(|mut p| { p.collapsed = false; p }).collect();
        let mut expanded: Vec<i64> = projects.iter().map(|p| p.id).collect();
        expanded.push(-1);
        let rows = sidebar::build(&SidebarInput {
            projects: &projects,
            worktrees: &self.worktrees,
            sessions: &sessions,
            live: &live,
            meta: &self.meta,
            filter: "",
            show_archived: false,
            other_collapsed: false,
            expanded: &expanded,
        });
        let mut group = String::new();
        let mut list = Vec::new();
        for r in rows {
            match r.kind {
                SidebarKind::Project => group = r.title.clone(),
                SidebarKind::OtherGroup => group = String::new(),
                SidebarKind::Session => {
                    // What it waits on you for (a phone answers from its list).
                    let requests: Vec<serde_json::Value> = self.live.get(&r.key).into_iter()
                        .flat_map(|e| &e.requests)
                        .map(|(q, asked)| { let mut v = crate::phone_server::request_json(q); v["asked"] = json!(asked); v })
                        .collect();
                    list.push(json!({
                        "key": r.key, "title": r.title, "project": group, "harness": r.harness,
                        "status": r.status, "modified": r.modified, "branch": r.branch, "pinned": r.pinned,
                        "where": if r.cwd.as_os_str().is_empty() { String::new() } else { sidebar::tilde(&r.cwd) },
                        "requests": requests,
                    }))
                }
                SidebarKind::More => {}
            }
        }
        crate::phone_bridge::publish_sessions(serde_json::Value::Array(list));
    }

    fn rebuild(mut self: Pin<&mut Self>) {
        if crate::phone_bridge::active() {
            self.publish_phone_sessions();
        }
        // This instance's: its projects, sessions (by their file) and runs.
        let sessions: Vec<SessionInfo> = self.sessions.values().filter(|s| self.here(&s.path)).cloned().collect();
        let live: Vec<LiveInfo> = self.live.values().filter(|e| self.here(&e.info.cwd)).map(|e| e.info.clone()).collect();
        let projects: Vec<Project> = self.projects.iter().filter(|p| self.here(&p.path)).cloned().collect();
        let new_rows = sidebar::build(&SidebarInput {
            projects: &projects,
            worktrees: &self.worktrees,
            sessions: &sessions,
            live: &live,
            meta: &self.meta,
            filter: &self.filter,
            show_archived: false,
            other_collapsed: self.other_collapsed,
            expanded: &self.expanded,
        });
        let mut new_rows = new_rows;
        let mut unchecked = false;
        for r in new_rows.iter_mut().filter(|r| r.kind == SidebarKind::Session) {
            r.changes = self.change_counts.get(&r.cwd).copied().unwrap_or(0);
            unchecked |= !r.cwd.as_os_str().is_empty() && !self.change_checked.contains(&r.cwd);
        }
        if unchecked {
            self.change_kick.notify_one();
        }
        let changes = diff_by(&self.rows, &new_rows, |a, b| a.key == b.key, |a, b| a == b);
        self.as_mut().apply_changes(changes);
        self.as_mut().update_tabs();
        let busy = self.live.values().filter(|e| matches!(e.info.status, SessionStatus::Busy | SessionStatus::Starting)).count() as i32;
        let input = self.live.values().filter(|e| e.info.needs_input).count() as i32;
        if self.busy_count != busy {
            self.as_mut().set_busy_count(busy);
        }
        if self.input_count != input {
            self.as_mut().set_input_count(input);
        }
        let (projects, count) = (self.projects.len() as i32, sessions.len() as i32);
        if self.project_count != projects {
            self.as_mut().set_project_count(projects);
        }
        if self.session_count != count {
            self.as_mut().set_session_count(count);
        }
    }

    fn apply_changes(mut self: Pin<&mut Self>, changes: Vec<ListChange<SidebarRow>>) {
        let root = QModelIndex::default();
        for change in changes {
            match change {
                ListChange::Update { at, row } => {
                    self.as_mut().rust_mut().rows[at] = row;
                    let idx = self.index(at as i32, 0, &root);
                    self.as_mut().data_changed(&idx, &idx, &QVector::<i32>::default());
                }
                ListChange::Remove { at, count } => unsafe {
                    self.as_mut().begin_remove_rows(&root, at as i32, (at + count - 1) as i32);
                    self.as_mut().rust_mut().rows.drain(at..at + count);
                    self.as_mut().end_remove_rows();
                },
                ListChange::Insert { at, rows } => unsafe {
                    self.as_mut().begin_insert_rows(&root, at as i32, (at + rows.len() - 1) as i32);
                    self.as_mut().rust_mut().rows.splice(at..at, rows);
                    self.as_mut().end_insert_rows();
                },
            }
        }
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(r) = self.rows.get(index.row() as usize) else {
            return QVariant::default();
        };
        let s = |v: &str| QVariant::from(&QString::from(v));
        match role {
            ROLE_KIND => s(r.kind.as_str()),
            ROLE_KEY => s(&r.key),
            ROLE_PROJECT => QVariant::from(&(r.project_id as i32)),
            ROLE_TITLE => s(&r.title),
            ROLE_SUBTITLE => s(&r.subtitle),
            ROLE_HARNESS => s(&r.harness),
            ROLE_STATUS => s(&r.status),
            ROLE_PATH => s(&r.path),
            ROLE_MODIFIED => QVariant::from(&(r.modified as f64)),
            ROLE_COLLAPSED => QVariant::from(&r.collapsed),
            ROLE_PINNED => QVariant::from(&r.pinned),
            ROLE_COUNT => QVariant::from(&(r.count as i32)),
            ROLE_BRANCH => s(&r.branch),
            ROLE_CHANGES => QVariant::from(&(r.changes as i32)),
            ROLE_ICON => s(harlo_core::project_icons::shown(&r.icon)),
            ROLE_COLOR => s(&r.color),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (ROLE_KIND, "kind"),
            (ROLE_KEY, "rowKey"),
            (ROLE_PROJECT, "projectId"),
            (ROLE_TITLE, "title"),
            (ROLE_SUBTITLE, "subtitle"),
            (ROLE_HARNESS, "harness"),
            (ROLE_STATUS, "status"),
            (ROLE_PATH, "path"),
            (ROLE_MODIFIED, "modified"),
            (ROLE_COLLAPSED, "collapsed"),
            (ROLE_PINNED, "pinned"),
            (ROLE_COUNT, "count"),
            (ROLE_BRANCH, "branch"),
            (ROLE_CHANGES, "changes"),
            (ROLE_ICON, "projectIcon"),
            (ROLE_COLOR, "projectColor"),
        ] {
            roles.insert(role, QByteArray::from(name));
        }
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rows.len() as i32
    }
}

/// Where harlo creates worktrees: `$XDG_DATA_HOME/harlo/worktrees`.
/// How a worktree session starts: its first message, its model, and the
/// run lane it belongs to.
#[derive(Default)]
struct WorktreeStart {
    first: Option<FirstPrompt>,
    model: String,
    lane: Option<(String, i64)>,
    /// Started by an agent (orchestration): its session, the title, notify.
    parent: Option<(String, String, bool)>,
    /// The shield's mode to start in.
    approval: Option<String>,
}

/// A session an agent started (orchestration).
#[derive(Clone)]
struct OrchChild {
    parent: String,
    title: String,
    agent: String,
    /// Its last reply goes back to the parent when it finishes.
    notify: bool,
    /// running | completed | failed | aborted
    status: String,
}

/// The first message of a session started from the new-session page.
#[derive(Clone, Default)]
struct FirstPrompt {
    text: String,
    attachments: Vec<PathBuf>,
    /// omp: start a goal with the text as its objective instead of sending
    /// it (`Some(token budget)`, 0 for none).
    goal: Option<u64>,
}

/// Sends a new session's first message (attachments are read off the UI
/// thread). The harness holds it until it has started. A goal that omp
/// refuses is reported through `notify` (the session stays open).
fn send_first_prompt(key: &str, first: FirstPrompt, notify: Option<CxxQtThread<Workspace>>) {
    let Some(live) = registry::get(key) else { return };
    crate::runtime().spawn_blocking(move || {
        let prepared = harlo_core::attach::prepare(&first.attachments);
        let text = format!("{}{}", prepared.prefix, first.text);
        let Some(budget) = first.goal else {
            live.handle.prompt(&text, prepared.images, harlo_harness::SendMode::Normal);
            return;
        };
        let mut command = serde_json::json!({ "type": "goal", "op": "create", "objective": text.trim() });
        if budget > 0 {
            command["token_budget"] = serde_json::json!(budget);
        }
        crate::runtime().spawn(async move {
            if let Err(e) = live.handle.request(command).await
                && let Some(thread) = notify
            {
                let _ = thread.queue(move |this| this.set_notice_text(&format!("omp did not start the goal: {e}"), true));
            }
        });
    });
}

/// Photos a phone sent (`[{ data, mimeType }]`, base64) as image contents;
/// other types are dropped.
fn phone_images(list: &serde_json::Value) -> Vec<serde_json::Value> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter(|i| matches!(i["mimeType"].as_str(), Some("image/jpeg" | "image/png" | "image/gif" | "image/webp")) && i["data"].as_str().is_some_and(|d| !d.is_empty()))
        .map(|i| json!({ "type": "image", "data": i["data"], "mimeType": i["mimeType"] }))
        .collect()
}

/// The `<file>` blocks naming a phone's photos (as attachments are named).
fn image_blocks(n: usize) -> String {
    (1..=n).map(|i| format!("<file name=\"Photo {i}\"></file>\n")).collect()
}

/// Where the omp sessions' `--config` overlays live (cleared at startup).
fn overlays_dir() -> PathBuf {
    harlo_core::db::default_path().parent().map(|p| p.join("omp-overlays")).unwrap_or_else(|| PathBuf::from("omp-overlays"))
}

/// What a question asks, for its notification: an ask's first question,
/// else the dialog's message or title.
fn question_text(r: &harlo_core::UiRequest) -> String {
    let asked = r.raw["questions"][0]["question"].as_str().unwrap_or_default();
    [asked, r.message.as_str(), r.title.as_str()].into_iter().map(str::trim).find(|t| !t.is_empty()).unwrap_or_default().to_owned()
}

/// omp's config file, relative to the home folder.
const OMP_CONFIG: &str = ".omp/agent/config.yml";

/// An overlay's text, with the user's omp config (`config`) merged in.
fn overlay_text(config: &str, goal: bool, subagents: bool, model: &str, modes: &SessionModes) -> String {
    let roles = harlo_core::omp_overlay::model_roles(config);
    let policies = harlo_core::omp_overlay::tool_policies(config);
    harlo_core::omp_overlay::Overlay {
        goal_continue: goal,
        subagent_fallback: subagents.then_some((&roles, model)),
        approval_mode: modes.approval.as_deref(),
        plan: modes.plan.then_some(&policies),
    }
    .render()
}

/// Random per run, so remote overlay names can't be guessed.
fn run_token() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    static TOKEN: OnceLock<u64> = OnceLock::new();
    *TOKEN.get_or_init(|| {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        h.write_u32(std::process::id());
        h.finish()
    })
}

/// The text of omp's global config ("" if none).
fn user_omp_config() -> String {
    harlo_core::omp_overlay::user_config_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default()
}

/// The approval mode omp's config sets (its default: `yolo`).
fn user_approval_mode() -> String {
    harlo_core::omp_overlay::approval_mode(&user_omp_config())
}

fn worktrees_dir() -> PathBuf {
    harlo_core::db::default_path().parent().map(|p| p.join("worktrees")).unwrap_or_else(|| PathBuf::from("worktrees"))
}

/// Accepts a plain path or a `file://` URL (from a QML dialog or drop).
pub fn url_to_path(s: &str) -> String {
    let Some(rest) = s.strip_prefix("file://") else { return s.to_owned() };
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Branches as the branch dialog reads them.
fn branches_json(list: Vec<harlo_harness::branches::BranchInfo>) -> String {
    let json: Vec<serde_json::Value> = list
        .into_iter()
        .map(|b| serde_json::json!({ "leaf": b.leaf, "current": b.current, "messages": b.messages, "lastUser": b.last_user, "divergesAt": b.diverges_at, "timestamp": b.timestamp }))
        .collect();
    serde_json::Value::Array(json).to_string()
}

/// A saved session's messages for the search index: read as its agent
/// writes them, here or (over SSH, or through OpenCode's server) on its host.
fn session_messages(path: &Path, kind: HarnessKind) -> Vec<serde_json::Value> {
    let remote = harlo_core::remote::host_of(path).and_then(|h| harlo_core::remote::host(&h));
    if kind == HarnessKind::OpenCode {
        return match &remote {
            Some(host) => harlo_harness::opencode_remote_saved(&harlo_core::remote::launch(host), path).messages,
            None => harlo_harness::opencode::saved(path).messages,
        };
    }
    let text = match remote {
        Some(_) => harlo_core::remote::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default(),
        None => std::fs::read_to_string(path).unwrap_or_default(),
    };
    harlo_harness::store::parse_saved_for(kind, &text).messages
}

impl Workspace {
    /// An agent's tool call (orchestration): `token` is its session's
    /// `HARLO_SESSION`; `opencode`: it came through OpenCode's server (whose
    /// running session is the caller).
    pub(crate) fn orchestrate(mut self: Pin<&mut Self>, token: &str, opencode: bool, tool: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
        use serde_json::json;
        let caller = self.orch_tokens.get(token).cloned().or_else(|| {
            opencode
                .then(|| {
                    self.live
                        .values()
                        .filter(|e| e.info.harness == "opencode" && e.info.status == SessionStatus::Busy)
                        .max_by_key(|e| e.info.started_at)
                        .map(|e| e.info.key.clone())
                })
                .flatten()
        });
        if !self.orchestration_on {
            return Err("Orchestration is turned off in harlo (Settings → Orchestration).".into());
        }
        if tool == harlo_core::orchestration::ENABLED {
            return Ok(json!(true));
        }
        // (A host's bridge asks for the tools themselves.)
        if tool == "_tools" {
            return Ok(harlo_core::orchestration::tools());
        }
        let text = |k: &str| args[k].as_str().map(str::trim).unwrap_or_default().to_owned();
        let session = |this: &Self| -> Result<String, String> {
            let key = text("session");
            this.live.contains_key(&key).then_some(key.clone()).ok_or_else(|| format!("There's no open session {key}."))
        };
        match tool {
            "list_agents" => {
                let list: Vec<serde_json::Value> = HarnessKind::PROVIDERS
                    .iter()
                    .filter(|k| self.usable_harness(**k))
                    .map(|k| {
                        let installed = harlo_harness::resolve_program(*k).is_some() || !self.setting_str(&format!("{}_path", k.as_str())).is_empty();
                        json!({ "id": k.as_str(), "name": k.label(), "installed": installed })
                    })
                    .collect();
                Ok(json!({ "agents": list }))
            }
            "list_sessions" => {
                let list: Vec<serde_json::Value> = self
                    .live
                    .values()
                    .map(|e| {
                        let i = &e.info;
                        let title = if !i.name.is_empty() { i.name.clone() } else { i.first_message.chars().take(80).collect() };
                        json!({
                            "session": i.key, "title": title, "agent": i.harness, "status": if i.needs_input { "waiting for input" } else { i.status.as_str() },
                            "folder": i.cwd.to_string_lossy(), "startedBy": self.orch_children.get(&i.key).map(|c| c.parent.clone()),
                            "you": caller.as_deref() == Some(i.key.as_str()),
                        })
                    })
                    .collect();
                Ok(json!({ "sessions": list }))
            }
            "start_session" => {
                let agent = text("agent").to_lowercase();
                let Some(kind) = HarnessKind::parse(&agent).filter(|k| *k != HarnessKind::Demo || agent == "demo") else {
                    return Err(format!("Unknown agent \"{agent}\": use claude, opencode, pi or omp (list_agents)."));
                };
                if !self.usable_harness(kind) {
                    return Err(format!("{} is turned off in harlo (Settings → Providers).", kind.label()));
                }
                let prompt = text("prompt");
                if prompt.is_empty() {
                    return Err("Give it a prompt.".into());
                }
                let folder = text("folder");
                let cwd = if !folder.is_empty() {
                    PathBuf::from(&folder)
                } else {
                    caller.as_ref().and_then(|c| self.live.get(c)).map(|e| e.info.cwd.clone()).unwrap_or_else(|| self.instance_home())
                };
                if !harlo_core::remote::is_remote(&cwd) && !cwd.is_dir() {
                    return Err(format!("{} isn't a folder.", cwd.display()));
                }
                let title = { let t = text("title"); if t.is_empty() { prompt.lines().next().unwrap_or_default().chars().take(60).collect() } else { t } };
                let notify = args["notify"].as_bool().unwrap_or(true) && caller.is_some();
                let model = text("model");
                let parent = caller.clone().unwrap_or_default();
                // In a new worktree: made in the background; it shows when ready.
                let branch = text("worktree");
                if !branch.is_empty() {
                    let Some(project) = self.projects.iter().find(|p| cwd.starts_with(&p.path)).cloned() else {
                        return Err("Worktrees are for a project's folder (that one isn't a harlo project).".into());
                    };
                    let first = FirstPrompt { text: prompt, ..Default::default() };
                    self.as_mut().create_worktree_session(
                        project.id as i32,
                        &QString::from(&branch),
                        &QString::default(),
                        &QString::from(kind.as_str()),
                        WorktreeStart { first: Some(first), model, lane: None, parent: caller.as_ref().map(|_| (parent, title.clone(), notify)), approval: None },
                    );
                    return Ok(json!({ "status": "creating the worktree; the session starts in it shortly (see list_sessions)", "branch": branch, "title": title }));
                }
                let key = self.as_mut().start_session_with(kind, cwd.clone(), &model, false);
                // (Named in the background: renaming would open it.)
                if let Some(live) = registry::get(&key) {
                    live.handle.send_raw(json!({ "type": "set_session_name", "name": title }));
                }
                if caller.is_some() {
                    self.as_mut().adopt(&key, &parent, &title, kind.as_str(), notify);
                }
                send_first_prompt(&key, FirstPrompt { text: prompt, ..Default::default() }, Some(self.qt_thread()));
                Ok(json!({ "session": key, "title": title, "agent": kind.as_str(), "folder": cwd.to_string_lossy(), "notify": notify }))
            }
            "send_message" => {
                let key = session(&self)?;
                let message = text("message");
                if message.is_empty() {
                    return Err("Give it a message.".into());
                }
                let busy = self.live.get(&key).is_some_and(|e| e.info.status == SessionStatus::Busy);
                if let Some(live) = registry::get(&key) {
                    live.handle.prompt(&message, Vec::new(), if busy { harlo_harness::SendMode::FollowUp } else { harlo_harness::SendMode::Normal });
                }
                if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(&key) {
                    c.status = "running".into();
                }
                self.as_mut().bump_orchestration();
                Ok(json!({ "session": key, "queued": busy }))
            }
            "read_session" => {
                let key = session(&self)?;
                let e = &self.live[&key];
                let title = if !e.info.name.is_empty() { e.info.name.clone() } else { e.info.first_message.chars().take(80).collect() };
                Ok(json!({ "session": key, "title": title, "agent": e.info.harness, "status": if e.info.needs_input { "waiting for input" } else { e.info.status.as_str() } }))
            }
            "stop_session" => {
                let key = session(&self)?;
                self.stop_session(&QString::from(&key));
                Ok(json!({ "session": key, "stopped": true }))
            }
            _ => Err(format!("harlo has no tool {tool}.")),
        }
    }

    /// `key` was started by `parent`'s agent: it's that session's subagent.
    fn adopt(mut self: Pin<&mut Self>, key: &str, parent: &str, title: &str, agent: &str, notify: bool) {
        self.as_mut().rust_mut().orch_children.insert(key.to_owned(), OrchChild { parent: parent.to_owned(), title: title.to_owned(), agent: agent.to_owned(), notify, status: "running".into() });
        self.bump_orchestration();
    }

    fn bump_orchestration(mut self: Pin<&mut Self>) {
        let next = self.orchestration_revision + 1;
        self.as_mut().set_orchestration_revision(next);
    }

    /// A started session got busy or finished: its state, and (finished)
    /// its last reply to the session that started it.
    fn child_progress(mut self: Pin<&mut Self>, key: &str, finished: bool, notes: &[Note]) {
        let Some(child) = self.orch_children.get(key).cloned() else { return };
        let reply = notes.iter().find_map(|n| match n {
            Note::Event(harlo_core::Notable::Completed { last_message }) => Some(last_message.clone()),
            _ => None,
        });
        let failed = notes.iter().any(|n| matches!(n, Note::Event(harlo_core::Notable::Error { .. })));
        let status = if !finished { "running" } else if failed && reply.is_none() { "failed" } else { "completed" };
        if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(key) {
            c.status = status.into();
        }
        self.as_mut().bump_orchestration();
        if !finished || !child.notify {
            return;
        }
        let Some(parent) = registry::get(&child.parent) else { return };
        let body = reply.unwrap_or_else(|| "(It finished without a reply.)".into());
        let message = format!("[harlo] The {} session \"{}\" ({key}) finished. Its last reply:\n\n{body}", HarnessKind::parse(&child.agent).map_or(child.agent.as_str(), |k| k.label()), child.title);
        let busy = self.live.get(&child.parent).is_some_and(|e| e.info.status == SessionStatus::Busy);
        parent.handle.prompt(&message, Vec::new(), if busy { harlo_harness::SendMode::FollowUp } else { harlo_harness::SendMode::Normal });
    }

    pub fn delegate_prompt(&self, request: &QString) -> QString {
        let r: serde_json::Value = serde_json::from_str(&request.to_string()).unwrap_or_default();
        let members: Vec<harlo_core::orchestration::Member> = r["members"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                let kind = HarnessKind::parse(m["agent"].as_str()?)?;
                Some(harlo_core::orchestration::Member { label: kind.label().to_owned(), agent: kind.as_str().to_owned(), model: m["model"].as_str().unwrap_or_default().to_owned() })
            })
            .collect();
        let text = harlo_core::orchestration::delegate_prompt(r["action"].as_str().unwrap_or_default(), &members, r["note"].as_str().unwrap_or_default(), r["worktree"] == true);
        QString::from(&text.unwrap_or_default())
    }

    pub fn ensure_providers(self: Pin<&mut Self>) {
        if self.provider_checks.is_empty() {
            self.refresh_providers();
        }
    }

    pub fn orchestration_json(&self) -> QString {
        use harlo_core::orchestration as orch;
        let inst = self.instance.to_string();
        let host = (!inst.is_empty()).then_some(inst.as_str());
        // Here: read now. A host: what the last look found (`load_orchestration`).
        let found = match host {
            None => orch::overview(None).unwrap_or_default(),
            Some(h) => self.orch_hosts.get(h).cloned().unwrap_or_else(|| serde_json::json!({ "loading": true })),
        };
        let agents: Vec<serde_json::Value> = HarnessKind::PROVIDERS
            .iter()
            .map(|k| {
                let bridge = if orch::BRIDGE_AGENTS.contains(k) { found["bridges"][k.as_str()].clone() } else { serde_json::json!("built-in") };
                let config = match host {
                    None => harlo_core::sidebar::tilde(&harlo_core::mcp::config_path(*k, &harlo_core::mcp::Scope::Global)),
                    // (Its path, without asking the host for its home now.)
                    Some(h) => format!("{h}:~/{}", if *k == HarnessKind::Pi { ".pi/agent/mcp.json" } else { ".omp/agent/mcp.json" }),
                };
                serde_json::json!({ "id": k.as_str(), "name": k.label(), "bridge": bridge, "config": orch::BRIDGE_AGENTS.contains(k).then_some(config) })
            })
            .collect();
        let skills = found.get("skills").cloned().unwrap_or_else(|| {
            serde_json::json!(orch::SKILLS.iter().map(|s| serde_json::json!({ "name": s.name, "summary": s.summary, "installed": false })).collect::<Vec<_>>())
        });
        QString::from(
            &serde_json::json!({
                "host": inst, "skills": skills, "agents": agents, "skillDirs": found.get("skillDirs").cloned().unwrap_or_else(|| serde_json::json!([])),
                "loading": found["loading"] == true, "problem": found["problem"].as_str().unwrap_or_default(), "busy": found["busy"] == true,
            })
            .to_string(),
        )
    }

    pub fn load_orchestration(self: Pin<&mut Self>) {
        let inst = self.instance.to_string();
        if inst.is_empty() {
            return self.bump_orchestration();
        }
        // A host: looked at in the background (over SSH).
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let found = harlo_core::orchestration::overview(Some(&inst)).unwrap_or_else(|e| serde_json::json!({ "problem": format!("{e:#}") }));
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().orch_hosts.insert(inst, found);
                this.bump_orchestration();
            });
        });
    }

    /// A change on a host: made in the background, then looked at again.
    fn orchestrate_on_host(mut self: Pin<&mut Self>, host: String, job: impl FnOnce(&str) -> anyhow::Result<()> + Send + 'static) {
        if let Some(e) = self.as_mut().rust_mut().orch_hosts.get_mut(&host) {
            e["busy"] = serde_json::json!(true);
        }
        self.as_mut().bump_orchestration();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let problem = job(&host).err().map(|e| format!("{e:#}")).unwrap_or_default();
            let mut found = harlo_core::orchestration::overview(Some(&host)).unwrap_or_else(|e| serde_json::json!({ "problem": format!("{e:#}") }));
            if !problem.is_empty() {
                found["problem"] = serde_json::json!(problem);
            }
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().orch_hosts.insert(host, found);
                this.bump_orchestration();
            });
        });
    }

    pub fn graphics_drawn(&self) {
        crate::graphics::drawn();
    }

    pub fn graphics_started_with(&self) -> QString {
        QString::from(&crate::graphics::started_with())
    }

    pub fn restart_app(&self) {
        crate::graphics::restart();
    }

    pub fn set_orchestration_enabled(mut self: Pin<&mut Self>, on: bool) -> QString {
        use harlo_core::orchestration as orch;
        if on == self.orchestration_on {
            return QString::default();
        }
        let exe = std::env::current_exe().unwrap_or_default();
        let mut problems: Vec<String> = Vec::new();
        if on {
            // What was installed when it was turned off.
            let kept: serde_json::Value = serde_json::from_str(&self.setting_str("orchestration_kept")).unwrap_or_default();
            for name in kept["skills"].as_array().into_iter().flatten().filter_map(|v| v.as_str()) {
                if let Err(e) = orch::set_skill(name, true) {
                    problems.push(format!("{e:#}"));
                }
            }
            for kind in kept["bridges"].as_array().into_iter().flatten().filter_map(|v| HarnessKind::parse(v.as_str()?)) {
                if let Err(e) = orch::set_bridge(kind, true, &exe) {
                    problems.push(format!("{e:#}"));
                }
            }
            self.save_setting("orchestration_kept", "");
        } else {
            let skills = orch::installed_skills();
            let bridges: Vec<&str> = orch::BRIDGE_AGENTS.iter().filter(|k| orch::bridge_installed(**k)).map(|k| k.as_str()).collect();
            self.save_setting("orchestration_kept", &serde_json::json!({ "skills": skills, "bridges": bridges }).to_string());
            for name in &skills {
                if let Err(e) = orch::set_skill(name, false) {
                    problems.push(format!("{e:#}"));
                }
            }
            for kind in orch::BRIDGE_AGENTS.into_iter().filter(|k| bridges.contains(&k.as_str())) {
                if let Err(e) = orch::set_bridge(kind, false, &exe) {
                    problems.push(format!("{e:#}"));
                }
            }
        }
        self.save_setting("orchestration", if on { "1" } else { "0" });
        harlo_harness::set_orchestration(on);
        self.as_mut().set_orchestration_on(on);
        self.as_mut().bump_orchestration();
        QString::from(&problems.join("\n"))
    }

    pub fn set_orchestration_skill(self: Pin<&mut Self>, name: &QString, on: bool) -> QString {
        let inst = self.instance.to_string();
        if !inst.is_empty() {
            let name = name.to_string();
            self.orchestrate_on_host(inst, move |h| harlo_core::orchestration::set_skill_on(Some(h), &name, on));
            return QString::default();
        }
        match harlo_core::orchestration::set_skill(&name.to_string(), on) {
            Ok(()) => QString::default(),
            Err(e) => QString::from(&format!("{e:#}")),
        }
    }

    pub fn set_orchestration_bridge(self: Pin<&mut Self>, agent: &QString, on: bool) -> QString {
        let Some(kind) = HarnessKind::parse(&agent.to_string()).filter(|k| harlo_core::orchestration::BRIDGE_AGENTS.contains(k)) else {
            return QString::from("Only pi and omp take harlo's tools from their config.");
        };
        let inst = self.instance.to_string();
        if !inst.is_empty() {
            self.orchestrate_on_host(inst, move |h| harlo_core::orchestration::set_bridge_on(kind, Some(h), on, std::path::Path::new("")));
            return QString::default();
        }
        let result = std::env::current_exe().map_err(anyhow::Error::from).and_then(|exe| harlo_core::orchestration::set_bridge(kind, on, &exe));
        match result {
            Ok(()) => QString::default(),
            Err(e) => QString::from(&format!("{e:#}")),
        }
    }

    pub fn orchestrated_json(&self, key: &QString) -> QString {
        let key = key.to_string();
        let list: Vec<serde_json::Value> = self
            .orch_children
            .iter()
            .filter(|(_, c)| c.parent == key)
            .map(|(k, c)| {
                let live = self.live.get(k);
                let status = if live.is_none() { "ended".to_owned() } else { c.status.clone() };
                serde_json::json!({
                    "id": k, "name": c.title, "agent": c.agent, "status": status, "openKey": k,
                    "canCancel": live.is_some(), "canSteer": false, "startedAt": live.map(|e| e.info.started_at.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())),
                })
            })
            .collect();
        let mut list = list;
        list.sort_by_key(|e| e["startedAt"].as_u64().unwrap_or(0));
        QString::from(&serde_json::Value::Array(list).to_string())
    }

    pub fn stop_session(&self, key: &QString) {
        if let Some(live) = registry::get(&key.to_string()) {
            live.handle.abort();
        }
    }
}

/// A session's `HARLO_SESSION` token (its agent's harlo tools say it).
fn orch_token() -> String {
    let mut b = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut b);
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::url_to_path;

    #[test]
    fn decodes_file_urls() {
        assert_eq!(url_to_path("file:///home/u/My%20Project"), "/home/u/My Project");
        assert_eq!(url_to_path("/plain/path"), "/plain/path");
        assert_eq!(url_to_path("file:///a%2"), "/a%2");
    }
}

/// A diff tab's key, `diff:<0|1>:<path>`: (staged, path).
fn diff_key(key: &str) -> Option<(bool, &str)> {
    let rest = key.strip_prefix("diff:")?;
    let (staged, path) = rest.split_once(':')?;
    Some((staged == "1", path))
}
