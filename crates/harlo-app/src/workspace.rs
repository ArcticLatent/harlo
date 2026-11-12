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
        /// Plan limits (Harlo's plan's, Claude's): `{ accounts, updatedAt, error, loading }`.
        #[qproperty(QString, quota_json, cxx_name = "quotaJson")]
        /// Other machines' plan limits, as their Claude sessions say them:
        /// `{ "<host>": { accounts, updatedAt } }` (this one's: `quotaJson`).
        #[qproperty(QString, host_limits_json, cxx_name = "hostLimitsJson")]
        /// Whose limits the sidebar meter shows: the account the last busy
        /// session on the machine shown used ("Claude", or a model provider
        /// such as "openai-codex"), unless one is pinned.
        #[qproperty(QString, meter_last, cxx_name = "meterLast")]
        #[qproperty(QString, meter_pin, cxx_name = "meterPin")]
        /// Bumped when a remote file's local copy (`localCopy`) is ready.
        #[qproperty(i32, copies_revision, cxx_name = "copiesRevision")]
        /// Bumped when sessions an agent started (orchestration) change.
        #[qproperty(i32, orchestration_revision, cxx_name = "orchestrationRevision")]
        /// Settings → Orchestration's switch.
        #[qproperty(bool, orchestration_on, cxx_name = "orchestrationOn")]
        /// Bumped when a session's shared plan changes (`planJson`).
        #[qproperty(i32, plans_revision, cxx_name = "plansRevision")]
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
        /// Sessions to continue when a limit resets: `{ session: unix ms }`.
        #[qproperty(QString, limit_resumes, cxx_name = "limitResumes")]
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
        /// What a project's sessions are, for asking before removing it:
        /// `{ count, trash, archived, running, worktree }`.
        #[qinvokable]
        fn project_sessions(self: &Workspace, id: i32) -> QString;
        /// Removes a project and deletes its sessions, each as Delete session
        /// does (in the background; one that can't be deleted stays, in
        /// Other sessions, and says why).
        #[qinvokable]
        fn remove_project_and_sessions(self: Pin<&mut Workspace>, id: i32);
        /// Moves a project to just before `before` (-1: last). Projects
        /// stay where they're put, whatever runs in them.
        #[qinvokable]
        fn move_project(self: Pin<&mut Workspace>, id: i32, before: i32);
        /// Holds the sidebar's rows as they are (while a project is
        /// dragged: a row made again would drop the drag), then lets them
        /// catch up.
        #[qinvokable]
        fn hold_rows(self: Pin<&mut Workspace>, hold: bool);
        /// Moves a project up (-1) or down (1) a place.
        #[qinvokable]
        fn move_project_step(self: Pin<&mut Workspace>, id: i32, step: i32);
        /// Where a project is among them: "first", "last", "only" or "".
        #[qinvokable]
        fn project_place(self: &Workspace, id: i32) -> QString;
        #[qinvokable]
        fn set_project_harness(self: Pin<&mut Workspace>, id: i32, harness: &QString);
        /// Collapses or expands a project (`project:<id>`), `group:other`,
        /// or a project's Settled shelf (`shelf:<id>`).
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
        /// Takes a session off its project's Settled shelf (it stays off
        /// until there's newer work in it and a reason again).
        #[qinvokable]
        fn put_back(self: Pin<&mut Workspace>, key: &QString);
        /// Archives everything on a project's Settled shelf.
        #[qinvokable]
        fn archive_shelf(self: Pin<&mut Workspace>, project_id: i32);
        /// A session's shared plan: `{ enabled, commits, steps: [{ id,
        /// title, detail, status, summary, commit, commitNote,
        /// editedByUser }] }` (`enabled`: the plan is on for its project).
        #[qinvokable]
        fn plan_json(self: &Workspace, key: &QString) -> QString;
        /// A session's title, as the sidebar shows it ("" when unknown).
        #[qinvokable]
        fn session_title(self: &Workspace, key: &QString) -> QString;
        /// Your edit of a session's plan: `{ op: "add", title, detail,
        /// after }`, `{ op: "edit", id, title, detail }`, `{ op: "remove",
        /// id }`, `{ op: "shift", id, by }`, `{ op: "skip", id, skipped }`,
        /// `{ op: "helper", id, agent, model }` (who does it in a run;
        /// agent "" the run's), `{ op: "clear" }`.
        #[qinvokable]
        fn plan_edit(self: Pin<&mut Workspace>, key: &QString, op: &QString);
        /// Runs a session's plan with helpers (`agent` "": its team's, else as
        /// the session; `model` "": the agent's default), or goes on with a
        /// paused run; the problem, if any.
        #[qinvokable]
        fn run_plan(self: Pin<&mut Workspace>, key: &QString, agent: &QString, model: &QString) -> QString;
        /// Who a session's helpers are (its project's team, else the same as
        /// it): `{ agent, agentName, model, thinking, team }`, or {}.
        #[qinvokable]
        fn helpers_json(self: &Workspace, key: &QString) -> QString;
        /// A project's team helpers, or {} (the same as the session).
        #[qinvokable]
        fn project_helpers_json(self: &Workspace, project_id: i32) -> QString;
        /// Stops a plan's run (helpers at work go on).
        #[qinvokable]
        fn stop_plan_run(self: Pin<&mut Workspace>, key: &QString);
        /// Has another agent review a finished step's commit; its review
        /// comes back to the session. The problem, if any.
        #[qinvokable]
        fn review_step(self: Pin<&mut Workspace>, key: &QString, id: i32, agent: &QString) -> QString;
        /// Settings → Worktrees: the Settled shelf on (sessions go on it
        /// now) or off (all come back).
        #[qinvokable]
        fn set_settling(self: Pin<&mut Workspace>, on: bool);
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
        /// Plays one of the sound theme's sounds (Settings: a reset's sound).
        #[qinvokable]
        fn play_sound(self: &Workspace, name: &QString);
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
        /// worktree, text }`; the same name replaces one):
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
        /// Re-reads plan limits in the background.
        #[qinvokable]
        fn refresh_quota(self: Pin<&mut Workspace>);
        /// Another machine's limits, asked for now (Settings → Plan limits).
        #[qinvokable]
        fn refresh_host_quota(self: Pin<&mut Workspace>, host: &QString);
        /// Every half minute: a limit nearly or fully used that has reset
        /// is told of (a sound of its own), and its numbers asked for again.
        #[qinvokable]
        fn check_limits(self: Pin<&mut Workspace>);
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
        /// chosen in harlo, else the one it starts in; "" without a shield.
        #[qinvokable]
        fn approval_mode(self: &Workspace, key: &QString) -> QString;
        /// The mode a new session of `harness` in the project (-1: none)
        /// starts in; "" when it has none.
        #[qinvokable]
        fn draft_approval(self: &Workspace, harness: &QString, project_id: i32) -> QString;
        /// Switches it; the running agent takes it at once.
        #[qinvokable]
        fn set_approval_mode(self: Pin<&mut Workspace>, key: &QString, mode: &QString);
        /// The thinking level set on a session, kept for next time.
        #[qinvokable]
        fn remember_thinking(self: Pin<&mut Workspace>, key: &QString, level: &QString);
        /// What's on the clipboard to attach: its local files, or its image
        /// saved to a file. Empty when it's text.
        #[qinvokable]
        fn paste_files(self: &Workspace) -> QStringList;
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
        /// Where the agent's program was found on this system, or "".
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
        /// `{ projectId, prompt, starter, attachments, worktrees, base,
        /// lanes: [{ harness, model }] }`. Opens its overview; returns
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
        /// The sessions used last, newest first, for the search's empty
        /// state: JSON array of `{ path, title, harness, cwd, modified }`.
        #[qinvokable]
        fn recent_sessions(self: &Workspace, limit: i32) -> QString;
        /// Sessions to offer for `@` in a composer: `cwd`'s, newest first,
        /// matching `query`, not `key`'s own. JSON array of `{ mention,
        /// title, harness, path, modified }`.
        #[qinvokable]
        fn session_mentions(self: &Workspace, cwd: &QString, key: &QString, query: &QString) -> QString;
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
        /// The folder a new session in a project (-1: the home folder of the
        /// instance shown) starts in; "" while a host's home isn't known yet.
        #[qinvokable]
        fn draft_folder(self: &Workspace, project_id: i32) -> QString;
        /// What an agent offers on the host of a new session's folder,
        /// answered by `hostModelsReady`.
        #[qinvokable]
        fn request_host_models(self: Pin<&mut Workspace>, project_id: i32, harness: &QString);
        /// The same for a session's folder (`folder`, an `ssh://` path),
        /// answered by `modelsAtReady`.
        #[qinvokable]
        fn request_models_at(self: Pin<&mut Workspace>, folder: &QString, harness: &QString);
        /// The sessions `key`'s agent started (orchestration), as its
        /// subagent dock lists them, JSON.
        #[qinvokable]
        fn orchestrated_json(self: &Workspace, key: &QString) -> QString;
        /// Answers a helper's request (`value`: Approve, Deny, …) from
        /// its parent's Subagents box.
        #[qinvokable]
        fn answer_helper(self: Pin<&mut Workspace>, key: &QString, request_id: &QString, value: &QString);
        /// Stops what a session is doing (it stays open).
        #[qinvokable]
        fn stop_session(self: &Workspace, key: &QString);
        /// The Stop button: the helpers a session started stop with it.
        #[qinvokable]
        fn stop_helpers_of(self: Pin<&mut Workspace>, key: &QString);
        /// Whether helpers a session started are still at work.
        #[qinvokable]
        fn helpers_working(self: &Workspace, key: &QString) -> bool;
        /// The composer's Delegate menu: what to send the agent, for
        /// `{ action, members: [{ agent, model }], note, worktree }`.
        #[qinvokable]
        fn delegate_prompt(self: &Workspace, request: &QString) -> QString;
        /// A project's shared plan is on (as set; by default in a git
        /// repository).
        #[qinvokable]
        fn plan_on(self: &Workspace, project_id: i32) -> bool;
        /// harlo commits a project's finished plan steps (as set; by
        /// default when its plan was turned on, not only on by default).
        #[qinvokable]
        fn plan_commits_on(self: &Workspace, project_id: i32) -> bool;
        /// Checks the providers (their models) unless that's been done.
        #[qinvokable]
        fn ensure_providers(self: Pin<&mut Workspace>);
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
        /// tools.
        #[qinvokable]
        fn set_orchestration_enabled(self: Pin<&mut Workspace>, on: bool);
        /// Conversation starters for the new-session page (JSON array of
        /// `{ id, label, icon, prompt }`).
        #[qinvokable]
        fn starters(self: &Workspace) -> QString;
        /// Starts a session from the new-session page and sends its first
        /// message. `form` is JSON: `{ projectId (-1 = home), harness ("" =
        /// the project's), worktreeBranch ("" = current checkout), base,
        /// starter (id or ""), text, attachments: [paths], approval ("" =
        /// the agent's), model, plan (starts in Plan) }`.
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
        /// Whether a session runs in a worktree harlo made (asks nothing).
        #[qinvokable]
        fn in_worktree(self: &Workspace, key: &QString) -> bool;
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
        /// Continues a session (its message goes then, even after a restart)
        /// once a limit has reset, at `at` (unix ms).
        #[qinvokable]
        fn resume_at_reset(self: Pin<&mut Workspace>, key: &QString, at: f64);
        #[qinvokable]
        fn cancel_resume(self: Pin<&mut Workspace>, key: &QString);
        /// When a session continues (unix ms), 0 if it isn't going to.
        #[qinvokable]
        fn resume_at_for(self: &Workspace, key: &QString) -> f64;
        /// Sessions on a host that are working or waiting on you (closing the
        /// connection stops them).
        #[qinvokable]
        fn working_on_host(self: &Workspace, name: &QString) -> i32;
        /// Keeps one of harlo's windows (by its title) above the others, or
        /// lets it go. (Wayland ignores Qt's hint: KDE's window manager does it.)
        #[qinvokable]
        fn keep_window_above(self: &Workspace, title: &QString, on: bool);
        /// Looks at the hosts' sessions now (one has just connected).
        #[qinvokable]
        fn check_remote_sessions(self: Pin<&mut Workspace>);
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
        /// A phone connected a machine: the desktop shows it connected too.
        #[qsignal]
        #[cxx_name = "hostConnected"]
        fn host_connected(self: Pin<&mut Workspace>, host: &QString);
        #[qsignal]
        #[cxx_name = "raiseRequested"]
        fn raise_requested(self: Pin<&mut Workspace>);
        /// Another machine's worktree, asked there (`worktree_info` gave its
        /// record first).
        #[qsignal]
        #[cxx_name = "worktreeInfoReady"]
        fn worktree_info_ready(self: Pin<&mut Workspace>, key: &QString, json: &QString);
        /// A project's branches (`request_branches`), current first.
        #[qsignal]
        #[cxx_name = "branchesReady"]
        fn branches_ready(self: Pin<&mut Workspace>, project_id: i32, branches: &QStringList);
        /// `request_host_models`: `{ models: [{ id, name, local }] }` or
        /// `{ error }`, as JSON.
        #[qsignal]
        #[cxx_name = "hostModelsReady"]
        fn host_models_ready(self: Pin<&mut Workspace>, project_id: i32, harness: &QString, json: &QString);
        /// What `request_models_at` asked for.
        #[qsignal]
        #[cxx_name = "modelsAtReady"]
        fn models_at_ready(self: Pin<&mut Workspace>, folder: &QString, harness: &QString, json: &QString);
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
use std::sync::{Arc, Mutex, OnceLock};
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
/// The folder a harlo writes branch previews in, by its process id.
const BRANCH_PREVIEW: &str = "harlo-branch-preview-";

/// Removes the branch previews of harlos no longer running (each writes
/// them in its own folder and nothing else removes it). Elsewhere than
/// Linux, ones a day old.
pub fn sweep_branch_previews() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for e in entries.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.strip_prefix(BRANCH_PREVIEW)).and_then(|p| p.parse::<u32>().ok()) else { continue };
        if pid == std::process::id() {
            continue;
        }
        let gone = if cfg!(target_os = "linux") {
            !Path::new(&format!("/proc/{pid}")).exists()
        } else {
            e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > std::time::Duration::from_secs(86_400))
        };
        if gone {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

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
const ROLE_SETTLED: i32 = 0x110;
const ROLE_HELPER_OF: i32 = 0x111;
const ROLE_NEST: i32 = 0x112;

const SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// Remote hosts' session folders are listed this often (over SSH).
const REMOTE_SCAN_INTERVAL: Duration = Duration::from_secs(10);
/// A host that didn't answer is tried again after this, doubling up to
/// `REMOTE_RETRY_MAX` while it keeps not answering.
const REMOTE_RETRY: Duration = Duration::from_secs(60);
const REMOTE_RETRY_MAX: Duration = Duration::from_secs(30 * 60);
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
    /// Its thinking level ("" its default, or not known).
    thinking: String,
}

/// Something a session did that may notify (Settings → Notifications).
enum Note {
    Event(harlo_core::Notable),
    /// It asks you something (an ask / select / approval): in words, and
    /// the request as phones get it.
    Question(String, serde_json::Value),
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
    host_limits_json: QString,
    meter_last: QString,
    meter_pin: QString,
    copies_revision: i32,
    orchestration_revision: i32,
    orchestration_on: bool,
    /// Orchestration: each started session's `HARLO_SESSION` token → its
    /// key, and the sessions agents started (by key).
    orch_tokens: HashMap<String, String>,
    orch_children: HashMap<String, OrchChild>,
    /// Plans running with helpers, by the session whose plan it is.
    plan_runs: HashMap<String, PlanRun>,
    /// Helpers that did their task while you were looking at them (or
    /// before their file was known): settled once you're not.
    settle_later: HashSet<String>,
    /// Helpers that failed (or ended without saying): settled once you've
    /// opened them and gone on, so you see what went wrong first.
    settle_once_seen: HashSet<String>,
    /// Agents' offers to split into helpers (`propose_helpers`), by the id
    /// of the card asking the user: who offered, and the helpers'
    /// start_session arguments.
    proposals: HashMap<String, (String, Vec<serde_json::Value>)>,
    /// Sessions you stopped, with their helpers: a helper whose worktree
    /// is still being made doesn't start (until the session works again).
    halted: HashSet<String>,
    /// Sessions harlo asked to send a diagram again, fixed: their next
    /// reply isn't checked (once, never a loop).
    diagram_asked: HashSet<String>,
    /// Remote files being copied here (`localCopy`).
    copying: std::cell::RefCell<HashSet<PathBuf>>,
    quota_loading: bool,
    /// When limits were last asked for (a turn on a provider with limits
    /// asks again, a minute apart at most).
    limits_asked: Option<std::time::Instant>,
    /// When each other machine's limits were last asked for.
    host_limits_asked: HashMap<String, std::time::Instant>,
    /// Resets told of ("provider|label|when"), and when harlo started (unix
    /// ms; resets before then aren't).
    limits_told: std::collections::HashSet<String>,
    limits_since: i64,
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
    /// The sidebar's rows held still (a project being dragged), and
    /// whether they changed meanwhile.
    rows_held: bool,
    rows_pending: bool,
    expanded: Vec<i64>,
    /// Projects whose Settled shelf is open.
    shelves_open: Vec<i64>,
    plans_revision: i32,
    /// A settling pass is looking up pull requests.
    settling: bool,
    rows: Vec<SidebarRow>,
    window_active: bool,
    tabs: QStringList,
    /// Open session keys in tab order.
    tab_keys: Vec<String>,
    instance: QString,
    default_instance: QString,
    limit_resumes: QString,
    /// The other instances' tabs (and active tab), kept while away.
    instance_tabs: HashMap<String, (Vec<String>, String)>,
    /// Tab session files to reopen once the first scan has found them, and
    /// which of them was active.
    restore_tabs: Option<(Vec<PathBuf>, Option<PathBuf>)>,
    /// Uncommitted changed files per session folder (inside a repository).
    change_counts: HashMap<PathBuf, usize>,
    /// Another machine's folders: their repository, as the change count
    /// found it (asked there without waiting).
    remote_repo_roots: HashMap<PathBuf, PathBuf>,
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
    /// Sessions' approval and plan modes, by live key.
    modes: HashMap<String, SessionModes>,
    /// Remote files' stamps (`fileStamp`): the last known, and whether a
    /// fresh one is being asked for.
    remote_stamps: Arc<std::sync::Mutex<HashMap<String, (String, bool)>>>,
    branch_reads: Arc<std::sync::Mutex<BranchReads>>,
}

/// Local session files' branches: reading a long one takes a while, and asking
/// again while it's unchanged or still being read shouldn't start another.
#[derive(Default)]
struct BranchReads {
    /// Being read, and whether it was asked for again meanwhile.
    running: HashMap<String, bool>,
    /// The answer for a file as it was (modified, size).
    known: HashMap<String, ((std::time::SystemTime, u64), String)>,
}

/// Remote project folders, grouped by host name.
type RemoteRoots = Vec<(String, Vec<PathBuf>)>;

/// Remote projects, grouped by host, for the remote session scan.
/// A worktree's state for the finish dialog: its main checkout's branch,
/// and its commits and changes against its base (blocking: git there).
fn worktree_details(w: &WorktreeRecord, project: Option<&Path>, exists: bool) -> serde_json::Value {
    let main = project.and_then(git::repo_root);
    let main_branch = main.as_ref().and_then(|m| git::status(m).ok()).map(|s| s.branch).unwrap_or_default();
    let base = if w.base.is_empty() { main_branch.clone() } else { w.base.clone() };
    let summary = if exists { git::worktree_summary(&w.path, &base).ok() } else { None }.unwrap_or_default();
    json!({
        "path": w.path.to_string_lossy(),
        "branch": w.branch,
        "base": base,
        "mainBranch": main_branch,
        "exists": exists,
        "dirty": summary.dirty,
        "ahead": summary.ahead,
        "behind": summary.behind,
    })
}

fn remote_roots_of<'a>(paths: impl Iterator<Item = &'a PathBuf>) -> RemoteRoots {
    let mut roots = RemoteRoots::new();
    for path in paths {
        if let Some(host) = harlo_core::remote::host_of(path) {
            match roots.iter_mut().find(|(h, _)| *h == host) {
                Some((_, list)) => list.push(path.clone()),
                None => roots.push((host, vec![path.clone()])),
            }
        }
    }
    roots
}

fn parse_resumes(text: &str) -> serde_json::Map<String, serde_json::Value> {
    serde_json::from_str(text).unwrap_or_default()
}

/// Which sessions' limits have reset by `now` (a minute's grace: the limit
/// must have lifted), and whether it's been so long there's no point any more.
fn due_resumes(resumes: &serde_json::Map<String, serde_json::Value>, now: i64) -> Vec<(String, bool)> {
    resumes
        .iter()
        .filter_map(|(id, at)| {
            let at = at.as_i64()?;
            (now >= at + 60_000).then(|| (id.clone(), now > at + 24 * 3600 * 1000))
        })
        .collect()
}

/// harlo-agent: harlo's tools on its pipe (when orchestration is off,
/// harlo says so and it has none).
/// Harlo's: harlo's tools, and its sandbox when it isn't the default
/// (`sandbox`, `network`: Settings → Providers → Harlo).
fn harlo_args(kind: HarnessKind, sandbox: bool, network: bool) -> Vec<String> {
    if kind != HarnessKind::Harlo {
        return Vec::new();
    }
    let mut args = vec!["--harlo-tools".to_owned()];
    match (sandbox, network) {
        (false, _) => args.extend(["--sandbox".into(), "off".into()]),
        (true, false) => args.extend(["--sandbox".into(), "no-network".into()]),
        (true, true) => {}
    }
    args
}

/// Harlo's subagents' model and the one it goes on with at the plan's
/// limit, as its launch arguments (empty settings: none).
fn model_args(task_model: &str, fallback: &str) -> Vec<String> {
    let mut args = Vec::new();
    for (flag, value) in [("--task-model", task_model.trim()), ("--fallback", fallback.trim())] {
        if !value.is_empty() && !value.contains(char::is_whitespace) {
            args.extend([flag.to_owned(), value.to_owned()]);
        }
    }
    args
}

/// Whether the agent's sessions have a shield (harlo's approval modes).
fn has_shield(harness: &str, _cwd: &Path) -> bool {
    matches!(harness, "demo" | "claude" | "harlo")
}

/// A session's approval mode (None: the one it starts in) and plan mode.
#[derive(Default, Clone)]
struct SessionModes {
    approval: Option<String>,
    plan: bool,
    /// Set by hand (Claude: its session file doesn't record it).
    thinking: Option<String>,
}

/// Forwards the parts of a live session's updates the sidebar needs.
struct WorkspaceSink {
    thread: CxxQtThread<Workspace>,
    key: String,
}

/// The session updates the workspace acts on (the rest are the chat's).
fn workspace_wants(u: &SessionUpdate) -> bool {
    matches!(
        u,
        SessionUpdate::Status(_)
            | SessionUpdate::Meta(_)
            | SessionUpdate::UiRequest(_)
            | SessionUpdate::UiCleared(_)
            | SessionUpdate::Notable(_)
            | SessionUpdate::Limits(_)
            // (The user's answer to an approval request: to the agent.)
            | SessionUpdate::GateAnswered { .. }
            // (Its helpers stop with it.)
            | SessionUpdate::Stopped
            // (harlo-agent calling harlo's tools.)
            | SessionUpdate::HarloCall { .. }
    )
}

impl SessionSink for WorkspaceSink {
    fn deliver(&self, updates: Vec<SessionUpdate>) {
        let relevant: Vec<SessionUpdate> = updates.into_iter().filter(workspace_wants).collect();
        if relevant.is_empty() {
            return;
        }
        let key = self.key.clone();
        let _ = self.thread.queue(move |this| this.on_live_updates(&key, relevant));
    }
}

impl cxx_qt::Initialize for Workspace {
    fn initialize(mut self: Pin<&mut Self>) {
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
                harlo_core::openwiki::set_telemetry(get("openwiki_telemetry") == "1");
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
                this.host_limits_json = QString::from(&db.get("cache.host_quota").ok().flatten().filter(|c| serde_json::from_str::<serde_json::Value>(c).is_ok_and(|v| v.is_object())).unwrap_or_else(|| "{}".to_owned()));
                this.meter_pin = QString::from(&get("meter_pin"));
                // The limits last seen, until fresh ones arrive.
                if let Some(cached) = db.get("cache.quota").ok().flatten() {
                    this.quota_json = QString::from(&cached);
                }
                publish_phone_limits(&this.quota_json, &this.host_limits_json);
                this.limits_since = chrono::Local::now().timestamp_millis();
                let mut saved_tabs = saved_tabs;
                if let Some(a) = &active_tab
                    && !saved_tabs.contains(a) {
                        saved_tabs.push(a.clone());
                    }
                // Settings → General → Reopen tabs.
                this.restore_tabs = (get("restore_tabs") != "0").then_some((saved_tabs, active_tab));
                this.limit_resumes = QString::from(&get("limit_resumes"));
                this.worktrees = worktrees;
                this.schedules = schedules;
                this.window_active = true;
                // (From the start: without them no host's sessions are looked for.)
                *this.remote_roots.lock().unwrap() = remote_roots_of(projects.iter().map(|p| &p.path));
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

        // The Settled shelf: a minute after starting, then every half hour.
        let settle_thread = self.qt_thread();
        crate::runtime().spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            loop {
                if settle_thread.queue(|this| this.settle_sessions()).is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(30 * 60)).await;
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
                    // (Other machines' repositories, for asking without SSH.)
                    let roots: HashMap<PathBuf, PathBuf> = counter.roots().iter().filter(|(d, _)| harlo_core::remote::is_remote(d)).map(|(d, r)| (d.clone(), r.clone())).collect();
                    if ch_thread.queue(move |mut this| {
                        this.as_mut().rust_mut().remote_repo_roots = roots;
                        this.apply_change_counts(dirs, counts);
                    }).is_err() {
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
                            if let Err(e) = index.lock().unwrap().update_parts(&files, |p, resume| session_part(p, kinds.get(p).copied().unwrap_or(HarnessKind::Harlo), resume)) {
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
        self.as_mut().show_meter_of(&start);
        // A graphics choice that didn't start last time.
        let dropped = crate::graphics::take_dropped();
        if !dropped.is_empty() {
            let name = match dropped.as_str() { "vulkan" => "Vulkan", "opengl" => "OpenGL", "software" => "Software", other => other };
            self.as_mut().set_notice_text(&format!("{name} graphics didn't start last time, so harlo went back to Automatic (Settings → General → Graphics)."), false);
        }
        let on = self.setting_str("orchestration") != "0";
        self.as_mut().set_orchestration_on(on);
        harlo_harness::set_orchestration(on);
        // The skills earlier versions installed.
        harlo_core::orchestration::remove_old_skills();

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
            // A host that doesn't answer: tried again later each time (one
            // minute, doubling to half an hour), and logged once.
            let mut retry_at: HashMap<String, (std::time::Instant, u32)> = HashMap::new();
            let mut scanned: HashSet<String> = HashSet::new();
            let mut looked_at: HashSet<String> = HashSet::new();
            loop {
                // Hosts with projects, and those connected without any
                // (their sessions go in Other sessions).
                let mut hosts = roots.lock().unwrap().clone();
                for h in harlo_core::remote::hosts() {
                    if harlo_core::remote::info_of(&h.name).is_some() && !hosts.iter().any(|(n, _)| *n == h.name) {
                        hosts.push((h.name, Vec::new()));
                    }
                }
                // One no longer looked at (removed, say): its sessions leave.
                let now: HashSet<String> = hosts.iter().map(|(n, _)| n.clone()).collect();
                for name in looked_at.difference(&now).cloned().collect::<Vec<_>>() {
                    let delta = scanner.forget_host(&name);
                    scanned.remove(&name);
                    if !delta.is_empty() && thread.queue(move |this| this.apply_scan(delta)).is_err() {
                        return;
                    }
                }
                looked_at = now;
                for (name, _) in hosts {
                    if harlo_core::remote::is_disconnected(&name) || retry_at.get(&name).is_some_and(|(t, _)| std::time::Instant::now() < *t) {
                        continue;
                    }
                    let host = harlo_core::remote::host(&name);
                    let (s, result) = tokio::task::spawn_blocking(move || {
                        let mut scanner = scanner;
                        let result = match &host {
                            Some(h) => scanner.poll(h),
                            // Removed from Settings: its sessions leave.
                            None => Ok(scanner.forget_host(&name)),
                        };
                        (scanner, (name, result))
                    })
                    .await
                    .expect("remote scan task");
                    scanner = s;
                    let (name, result) = result;
                    // Disconnected meanwhile: what this look opened goes too.
                    if harlo_core::remote::is_disconnected(&name)
                        && let Some(host) = harlo_core::remote::host(&name)
                    {
                        tokio::task::spawn_blocking(move || harlo_core::remote::disconnect(&host));
                    }
                    match result {
                        Ok(delta) => {
                            if retry_at.remove(&name).is_some() {
                                tracing::debug!("remote sessions on {name}: reachable again");
                            }
                            let first = scanned.insert(name);
                            if (first || !delta.is_empty()) && thread.queue(move |this| this.apply_scan(delta)).is_err() {
                                return;
                            }
                        }
                        Err(e) => {
                            let failures = retry_at.get(&name).map_or(0, |(_, n)| *n) + 1;
                            // (Background work nobody asked for: the debug log only.)
                            if failures == 1 {
                                tracing::debug!("remote sessions on {name}: {e:#} (trying again later, less and less often)");
                            }
                            let wait = REMOTE_RETRY.saturating_mul(1 << (failures - 1).min(5)).min(REMOTE_RETRY_MAX);
                            retry_at.insert(name, (std::time::Instant::now() + wait, failures));
                        }
                    }
                }
                tokio::select! {
                    // Asked for (a host connected, its instance shown): now.
                    _ = kick.notified() => {
                        for (t, _) in retry_at.values_mut() {
                            *t = std::time::Instant::now();
                        }
                    }
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
        let roots = remote_roots_of(self.projects.iter().map(|p| &p.path));
        // (Only when they changed: collapsing a project changes nothing there.)
        let changed = {
            let mut current = self.remote_roots.lock().unwrap();
            let changed = *current != roots;
            *current = roots;
            changed
        };
        if changed {
            self.remote_kick.notify_one();
        }
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
            // Harlo when it can run there (and Settings → Providers has it
            // on), else what the host has; Harlo when nothing's known.
            Some(host) => {
                let harlo_there = harlo_core::remote::info_of(host).is_none_or(|i| harlo_core::agent_host::not_there(&i).is_none());
                if harlo_there && self.usable_harness(HarnessKind::Harlo) {
                    "harlo".to_owned()
                } else {
                    harlo_core::remote::AGENTS
                        .iter()
                        .find(|a| harlo_core::remote::found_on(host, a) && HarnessKind::parse(a).is_some_and(|k| self.usable_harness(k)))
                        .map_or("harlo", |a| *a)
                        .to_owned()
                }
            }
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

    /// This instance's sessions of a project.
    pub fn move_project(mut self: Pin<&mut Self>, id: i32, before: i32) {
        let (id, before) = (i64::from(id), i64::from(before));
        if id == before {
            return;
        }
        let mut ids: Vec<i64> = self.projects.iter().map(|p| p.id).filter(|p| *p != id).collect();
        let at = ids.iter().position(|p| *p == before).unwrap_or(ids.len());
        ids.insert(at, id);
        let r = self.db().map(|db| db.set_project_order(&ids)).transpose();
        self.as_mut().report(r);
        self.reload_projects();
    }

    pub fn hold_rows(mut self: Pin<&mut Self>, hold: bool) {
        self.as_mut().rust_mut().rows_held = hold;
        if !hold && std::mem::take(&mut self.as_mut().rust_mut().rows_pending) {
            self.rebuild();
        }
    }

    pub fn move_project_step(self: Pin<&mut Self>, id: i32, step: i32) {
        let ids: Vec<i64> = self.projects.iter().filter(|p| self.here(&p.path)).map(|p| p.id).collect();
        let Some(at) = ids.iter().position(|p| *p == i64::from(id)) else { return };
        // (Down a place: before the one after the next, or last.)
        let before = match step {
            s if s < 0 && at > 0 => ids[at - 1],
            s if s > 0 && at + 1 < ids.len() => ids.get(at + 2).copied().unwrap_or(-1),
            _ => return,
        };
        self.move_project(id, before as i32);
    }

    pub fn project_place(&self, id: i32) -> QString {
        let ids: Vec<i64> = self.projects.iter().filter(|p| self.here(&p.path)).map(|p| p.id).collect();
        let at = ids.iter().position(|p| *p == i64::from(id));
        QString::from(match at {
            Some(_) if ids.len() == 1 => "only",
            Some(0) => "first",
            Some(i) if i + 1 == ids.len() => "last",
            _ => "",
        })
    }

    /// Projects used to go in the order they were last worked in: once,
    /// they're placed in that order, to stay there.
    fn place_projects_once(mut self: Pin<&mut Self>) {
        const PLACED: &str = "projects_placed";
        if self.sessions.is_empty() || self.db().is_none_or(|db| db.get(PLACED).ok().flatten().is_some()) {
            return;
        }
        let sessions: Vec<SessionInfo> = self.sessions.values().cloned().collect();
        let ids = sidebar::by_latest(&SidebarInput {
            projects: &self.projects,
            worktrees: &self.worktrees,
            sessions: &sessions,
            live: &[],
            meta: &self.meta,
            filter: "",
            show_archived: true,
            other_collapsed: false,
            expanded: &[],
            shelves_open: &[],
        });
        let r = self.db().map(|db| db.set_project_order(&ids).and_then(|_| db.set(PLACED, "1"))).transpose();
        self.as_mut().report(r);
        self.reload_projects();
    }

    fn sessions_of_project(&self, id: i64) -> Vec<sidebar::ProjectSession> {
        let sessions: Vec<SessionInfo> = self.sessions.values().filter(|s| self.here(&s.path)).cloned().collect();
        let live: Vec<LiveInfo> = self.live.values().filter(|e| self.here(&e.info.cwd)).map(|e| e.info.clone()).collect();
        let projects: Vec<Project> = self.projects.iter().filter(|p| self.here(&p.path)).cloned().collect();
        sidebar::project_sessions(
            id,
            &SidebarInput { projects: &projects, worktrees: &self.worktrees, sessions: &sessions, live: &live, meta: &self.meta, filter: "", show_archived: true, other_collapsed: false, expanded: &[], shelves_open: &[] },
        )
    }

    pub fn project_sessions(&self, id: i32) -> QString {
        let list = self.sessions_of_project(id as i64);
        let count = |f: &dyn Fn(&sidebar::ProjectSession) -> bool| list.iter().filter(|s| f(s)).count();
        // (A session with nothing sent yet has nothing to delete, and goes when it stops.)
        let saved = |s: &sidebar::ProjectSession| self.file_for_key(&s.key).is_some();
        let v = serde_json::json!({
            "count": list.len(),
            "trash": count(&|s| saved(s)),
            "archived": count(&|s| s.archived),
            "running": count(&|s| s.running),
            "worktree": count(&|s| s.worktree),
        });
        QString::from(&v.to_string())
    }

    pub fn remove_project_and_sessions(mut self: Pin<&mut Self>, id: i32) {
        let list = self.sessions_of_project(id as i64);
        let mut files = Vec::new();
        for s in &list {
            let file = self.file_for_key(&s.key);
            self.as_mut().close_session(&QString::from(&s.key));
            files.extend(file);
        }
        self.as_mut().rebuild();
        // The files, off the UI thread: the project goes when they have, so
        // they don't pass through Other sessions on the way.
        self.background(
            move || {
                files
                    .into_iter()
                    .map(|file| {
                        let done = harlo_core::remote::trash(&file).map_err(|e| format!("{e:#}"));
                        (file, done)
                    })
                    .collect::<Vec<_>>()
            },
            move |mut this, results| {
                let mut failed = Vec::new();
                for (file, done) in results {
                    match done {
                        Ok(()) => {
                            this.as_mut().rust_mut().sessions.remove(&file);
                        }
                        Err(e) => failed.push(e),
                    }
                }
                this.as_mut().remove_project(id);
                if let Some(first) = failed.first() {
                    this.set_error_text(QString::from(&format!("Could not delete {} of the project's sessions: {first}", failed.len())));
                }
            },
        );
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
                    "localIds": status.map(|s| s.local.clone()).unwrap_or_default(),
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
            // (Harlo: where harlo can put its agent.)
            .filter(|&&k| host.is_none() || info.as_ref().is_none_or(|i| i.has(k.as_str())))
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
        if let Some(id) = key.strip_prefix("shelf:").and_then(|s| s.parse::<i64>().ok()) {
            let open = &mut self.as_mut().rust_mut().shelves_open;
            match open.iter().position(|p| *p == id) {
                Some(i) => {
                    open.remove(i);
                }
                None => open.push(id),
            }
            self.rebuild();
            return;
        }
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
            let (program, remote) = self.launch_for(info.harness, &info.cwd);
            let token = orch_token();
            let cfg = HarnessConfig {
                kind: info.harness,
                program,
                cwd: info.cwd.clone(),
                session: SessionTarget::Resume(info.path.clone()),
                extra_args: self.harlo_args(info.harness),
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
            self.as_mut().restore_modes(&key, &info.path);
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
    fn start_session_with(self: Pin<&mut Self>, kind: HarnessKind, cwd: PathBuf, model: &str, activate: bool) -> String {
        self.start_session_thinking(kind, cwd, model, "", activate)
    }

    /// The same, at a thinking level ("" the agent's default): Claude's
    /// from its start (it restarts to change it), others' set as it opens.
    fn start_session_thinking(self: Pin<&mut Self>, kind: HarnessKind, cwd: PathBuf, model: &str, thinking: &str, activate: bool) -> String {
        self.start_session_budget(kind, cwd, model, thinking, activate, None)
    }

    /// The same, with a token budget (Harlo's: `--budget`).
    fn start_session_budget(mut self: Pin<&mut Self>, kind: HarnessKind, cwd: PathBuf, model: &str, thinking: &str, activate: bool, budget: Option<u64>) -> String {
        let (program, remote) = self.launch_for(kind, &cwd);
        let token = orch_token();
        let cfg = HarnessConfig {
            kind,
            program,
            cwd: cwd.clone(),
            // `--ephemeral` (testing): new sessions are not saved.
            session: if std::env::args().any(|a| a == "--ephemeral") { SessionTarget::Ephemeral } else { SessionTarget::New },
            extra_args: {
                let mut args: Vec<String> = self.harlo_args(kind);
                if let Some(b) = budget.filter(|_| kind == HarnessKind::Harlo) {
                    args.extend(["--budget".into(), b.to_string()]);
                }
                if !model.trim().is_empty() && kind != HarnessKind::Demo {
                    args.extend(["--model".into(), model.trim().to_owned()]);
                }
                if kind == HarnessKind::Claude && !thinking.trim().is_empty() {
                    args.push(format!("{}{}", harlo_harness::claude::THINKING_ARG, thinking.trim()));
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
        self.as_mut().rust_mut().live.insert(key.clone(), LiveEntry { info, sub, requests: Vec::new(), model: String::new(), model_id: String::new(), thinking: String::new() });
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
        // A failed helper you open: settled as you go on from it.
        if self.as_mut().rust_mut().settle_once_seen.remove(key) {
            self.as_mut().rust_mut().settle_later.insert(key.to_owned());
        }
        // A helper that finished while you looked at it: settled as you go.
        let leaving = previous != key && self.settle_later.contains(&previous);
        if let Some(file) = self.live.get(key).and_then(|e| e.info.session_file.clone()) {
            self.as_mut().mark_viewed(&file);
        }
        // New tabs open to the right of the active one.
        if !key.is_empty() && !self.tab_keys.iter().any(|k| k == key) {
            let at = self.tab_keys.iter().position(|k| *k == previous).map_or(self.tab_keys.len(), |i| i + 1);
            self.as_mut().rust_mut().tab_keys.insert(at, key.to_owned());
        }
        self.as_mut().set_active_key(QString::from(key));
        if leaving {
            self.as_mut().settle_helper(&previous);
        }
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

    /// A session's name that lasts: its file, once it has one.
    fn resume_id(&self, key: &str) -> String {
        self.live.get(key).and_then(|e| e.info.session_file.as_ref()).map_or_else(|| key.to_owned(), |f| f.to_string_lossy().into_owned())
    }

    fn store_resumes(mut self: Pin<&mut Self>, resumes: &serde_json::Map<String, serde_json::Value>) {
        let text = serde_json::Value::Object(resumes.clone()).to_string();
        self.save_setting("limit_resumes", &text);
        self.as_mut().set_limit_resumes(QString::from(&text));
    }

    pub fn resume_at_reset(self: Pin<&mut Self>, key: &QString, at: f64) {
        let id = self.resume_id(&key.to_string());
        let mut resumes = parse_resumes(&self.limit_resumes.to_string());
        resumes.insert(id, json!(at as i64));
        self.store_resumes(&resumes);
    }

    pub fn cancel_resume(self: Pin<&mut Self>, key: &QString) {
        let (key, id) = (key.to_string(), self.resume_id(&key.to_string()));
        let mut resumes = parse_resumes(&self.limit_resumes.to_string());
        // (Asked before it had a file: under its key.)
        if resumes.remove(&id).is_some() | resumes.remove(&key).is_some() {
            self.store_resumes(&resumes);
        }
    }

    pub fn resume_at_for(&self, key: &QString) -> f64 {
        let (key, resumes) = (key.to_string(), parse_resumes(&self.limit_resumes.to_string()));
        resumes.get(&self.resume_id(&key)).or_else(|| resumes.get(&key)).and_then(|v| v.as_i64()).unwrap_or(0) as f64
    }

    /// The sessions whose limit has reset get their message.
    fn continue_after_limits(mut self: Pin<&mut Self>) {
        let mut resumes = parse_resumes(&self.limit_resumes.to_string());
        let due = due_resumes(&resumes, chrono::Local::now().timestamp_millis());
        if due.is_empty() {
            return;
        }
        for (id, give_up) in due {
            if give_up {
                resumes.remove(&id);
                continue;
            }
            let path = Path::new(&id);
            // (Disconnected by hand: not until it's connected again.)
            if harlo_core::remote::host_of(path).is_some_and(|h| harlo_core::remote::is_disconnected(&h)) {
                continue;
            }
            let key = if self.live.contains_key(&id) { Some(id.clone()) } else { self.live_key_for_file(path) };
            // Not open: opened beside the tab you're on (its host's sessions
            // may not have arrived yet: tried again next time).
            let key = key.or_else(|| {
                self.sessions.contains_key(path).then(|| {
                    let active = self.active_key.to_string();
                    let key = self.as_mut().open(&QString::from(&id)).to_string();
                    if !active.is_empty() {
                        self.as_mut().activate(&active);
                    }
                    key
                })
            });
            let Some(key) = key else { continue };
            resumes.remove(&id);
            // (Already going again: left alone.)
            if self.live.get(&key).is_some_and(|e| e.info.status == SessionStatus::Busy) {
                continue;
            }
            if let Some(live) = registry::get(&key) {
                live.handle.prompt("Your usage limit has reset. Please continue where you left off.", Vec::new(), harlo_harness::SendMode::Normal);
                self.as_mut().notify_limit(&format!("resume-{id}"), "A session is working again", "Its limit has reset, so it continues where it left off.", true);
            }
        }
        self.store_resumes(&resumes);
    }

    pub fn keep_window_above(&self, title: &QString, on: bool) {
        let title = title.to_string();
        std::thread::spawn(move || {
            if let Err(e) = crate::keep_above::set(&title, on) {
                tracing::debug!("keeping {title:?} on top: {e}");
            }
        });
    }

    pub fn check_remote_sessions(mut self: Pin<&mut Self>) {
        self.remote_kick.notify_one();
        // (Just connected, say: the limits of the machine shown too.)
        let shown = self.instance.to_string();
        self.as_mut().refresh_host_limits(&shown, true, false);
    }

    pub fn working_on_host(&self, name: &QString) -> i32 {
        let name = name.to_string();
        self.live
            .values()
            .filter(|e| harlo_core::remote::host_of(&e.info.cwd).as_deref() == Some(name.as_str()))
            .filter(|e| e.info.needs_input || matches!(e.info.status, SessionStatus::Busy | SessionStatus::Starting))
            .count() as i32
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
        self.as_mut().show_meter_of(&name);
        // Its sessions now, even if it didn't answer a while ago.
        if !name.is_empty() {
            self.remote_kick.notify_one();
        }
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
        self.home_on(&self.instance.to_string())
    }

    /// A machine's home folder ("" this one's).
    fn home_on(&self, inst: &str) -> PathBuf {
        let inst = inst.to_owned();
        if !inst.is_empty()
            && let Some(home) = harlo_core::remote::info_of(&inst).map(|i| i.home).filter(|h| !h.is_empty())
        {
            return harlo_core::remote::join(&inst, &home);
        }
        // (Its home not known yet: still that machine, never this one.)
        if !inst.is_empty() {
            return harlo_core::remote::join(&inst, "/");
        }
        PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/".into()))
    }

    pub fn draft_folder(&self, project_id: i32) -> QString {
        let inst = self.instance.to_string();
        let path = match self.projects.iter().find(|p| p.id == project_id as i64) {
            Some(p) => p.path.clone(),
            None if !inst.is_empty() && harlo_core::remote::info_of(&inst).is_none_or(|i| i.home.is_empty()) => return QString::default(),
            None => self.instance_home(),
        };
        QString::from(&path.to_string_lossy().into_owned())
    }

    pub fn request_host_models(self: Pin<&mut Self>, project_id: i32, harness: &QString) {
        let cwd = PathBuf::from(self.draft_folder(project_id).to_string());
        self.models_there(harness, cwd, move |this, name, json| this.host_models_ready(project_id, name, json));
    }

    pub fn request_models_at(self: Pin<&mut Self>, folder: &QString, harness: &QString) {
        let folder = folder.to_string();
        self.models_there(harness, PathBuf::from(&folder), move |this, name, json| this.models_at_ready(&QString::from(&folder), name, json));
    }

    /// What an agent offers on `cwd`'s machine, asked there, given to
    /// `reply` (`{ models: [{ id, name, local }] }` or `{ error }`).
    fn models_there(self: Pin<&mut Self>, harness: &QString, cwd: PathBuf, reply: impl FnOnce(Pin<&mut Self>, &QString, &QString) + Send + 'static) {
        let name = harness.to_string();
        let Some(kind) = HarnessKind::parse(&name) else { return };
        let (_, Some(remote)) = self.launch_for(kind, &cwd) else {
            // (Its home not known yet, say: the menu mustn't wait for ever.)
            reply(self, harness, &QString::from(&serde_json::json!({ "error": "harlo isn't connected to it yet." }).to_string()));
            return;
        };
        let thread = self.qt_thread();
        let claude_on = harlo_core::remote::host_of(&cwd).and_then(|h| harlo_core::remote::host(&h)).filter(|_| kind == HarnessKind::Claude);
        crate::runtime().spawn(async move {
            // (Signed out, it would list its models all the same, and refuse the first message.)
            let signed_out = match claude_on {
                Some(host) => tokio::task::spawn_blocking(move || harlo_core::providers::claude_signed_in_on(&host)).await.ok().flatten() == Some(false),
                None => false,
            };
            let answer = match if signed_out { Ok(Vec::new()) } else { harlo_core::providers::host_models(kind, remote, cwd).await } {
                Ok(list) => serde_json::json!({ "models": list.into_iter().map(|(id, name, local)| serde_json::json!({ "id": id, "name": name, "local": local })).collect::<Vec<_>>() }),
                Err(e) if e == "harness exited before responding" => serde_json::json!({ "error": "it quit before answering, which usually means it has no login there yet." }),
                Err(e) => serde_json::json!({ "error": e }),
            };
            let _ = thread.queue(move |this| reply(this, &QString::from(&name), &QString::from(&answer.to_string())));
        });
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
                    "stats" => "Usage",
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
                let title = format!("{} ({})", name.replace(['\t', '\n'], " "), if staged { "staged" } else { "changes" });
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
        QString::from(&crate::file_icons::icon_text(&path.to_string()))
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
            (_, Some("stats")) => "Usage".to_owned(),
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

    pub fn put_back(self: Pin<&mut Self>, key: &QString) {
        self.update_meta(&key.to_string(), |m| m.settled_why.clear());
    }

    pub fn archive_shelf(mut self: Pin<&mut Self>, project_id: i32) {
        let id = i64::from(project_id);
        let Some(project) = self.projects.iter().find(|p| p.id == id).cloned().map(|mut p| { p.collapsed = false; p }) else { return };
        let sessions: Vec<SessionInfo> = self.sessions.values().filter(|s| self.here(&s.path)).cloned().collect();
        let live: Vec<LiveInfo> = self.live.values().map(|e| e.info.clone()).collect();
        let rows = sidebar::build(&SidebarInput {
            projects: std::slice::from_ref(&project),
            worktrees: &self.worktrees,
            sessions: &sessions,
            live: &live,
            meta: &self.meta,
            filter: "",
            show_archived: false,
            other_collapsed: true,
            expanded: &[id],
            shelves_open: &[id],
        });
        // The rows after its shelf's.
        let files: Vec<PathBuf> = rows
            .iter()
            .skip_while(|r| r.kind != SidebarKind::Shelf)
            .skip(1)
            .take_while(|r| r.kind == SidebarKind::Session)
            .filter(|r| !r.settled.is_empty())
            .filter_map(|r| self.file_for_key(&r.key))
            .collect();
        for file in files {
            let mut meta = self.meta.get(&file).cloned().unwrap_or_default();
            meta.archived = true;
            let r = self.db().map(|db| db.set_session_meta(&file, &meta)).transpose();
            self.as_mut().report(r);
            self.as_mut().rust_mut().meta.insert(file, meta);
        }
        self.rebuild();
    }

    /// What a session's project has on: `(cwd, wiki, plan, commits)`
    /// (sessions in a harlo project only).
    fn features_of(&self, key: &str) -> Option<(PathBuf, bool, bool, bool)> {
        let (cwd, project) = self.project_of(key)?;
        let on = |k: &str| self.setting_str(&format!("{k}_{project}")) == "1";
        Some((cwd, on("wiki"), self.plan_on(project as i32), self.plan_commits_on(project as i32)))
    }

    /// harlo commits a project's finished plan steps: as set, else (by
    /// default) when you turned its plan on yourself (not when it's only on
    /// by default: nothing's committed you didn't ask for).
    pub fn plan_commits_on(&self, project_id: i32) -> bool {
        match self.setting_str(&format!("plan_commits_{project_id}")).as_str() {
            "1" => true,
            "0" => false,
            _ => self.setting_str(&format!("plan_{project_id}")) == "1",
        }
    }

    /// A session's folder, and the project it's in (its worktree's).
    fn project_of(&self, key: &str) -> Option<(PathBuf, i64)> {
        let cwd = self.live.get(key).map(|e| e.info.cwd.clone()).or_else(|| self.sessions.get(Path::new(key)).map(|s| s.cwd.clone()))?;
        let project = self
            .worktrees
            .iter()
            .filter(|w| cwd.starts_with(&w.path))
            .max_by_key(|w| w.path.components().count())
            .map(|w| w.project_id)
            .or_else(|| self.projects.iter().filter(|p| cwd.starts_with(&p.path)).max_by_key(|p| p.path.components().count()).map(|p| p.id))?;
        Some((cwd, project))
    }

    /// Who a session's helpers are: its project's team (Settings →
    /// Projects), else the same as the session (its agent, model and
    /// thinking).
    fn helpers_for(&self, key: &str) -> Option<HelperSetup> {
        let team = self.project_of(key).and_then(|(_, p)| self.team_of(p));
        team.or_else(|| {
            let e = self.live.get(key)?;
            let agent = HarnessKind::parse(&e.info.harness)?;
            let thinking = self.modes.get(key).and_then(|m| m.thinking.clone()).unwrap_or_else(|| e.thinking.clone());
            Some(HelperSetup { agent, model: e.model_id.clone(), thinking, team: false, reviewer: None })
        })
    }

    /// A project's team helpers, if it has them.
    fn team_of(&self, project: i64) -> Option<HelperSetup> {
        let v: serde_json::Value = serde_json::from_str(&self.setting_str(&format!("team_helpers_{project}"))).ok()?;
        let agent = HarnessKind::parse(v["agent"].as_str()?).filter(|k| self.usable_harness(*k))?;
        let s = |k: &str| v[k].as_str().unwrap_or_default().trim().to_owned();
        let reviewer = HarnessKind::parse(v["reviewer"]["agent"].as_str().unwrap_or_default())
            .filter(|k| self.usable_harness(*k))
            .map(|k| (k, v["reviewer"]["model"].as_str().unwrap_or_default().trim().to_owned()));
        Some(HelperSetup { agent, model: s("model"), thinking: s("thinking"), team: true, reviewer })
    }

    /// A session's helpers (`helpers_for`), for the plan's run menu and the
    /// Delegate menu: `{ agent, agentName, model, thinking, team }`, or {}.
    pub fn helpers_json(&self, key: &QString) -> QString {
        let v = self.helpers_for(&key.to_string()).map_or(json!({}), |h| {
            json!({ "agent": h.agent.as_str(), "agentName": h.agent.label(), "model": h.model, "thinking": h.thinking, "team": h.team,
                    "reviewer": h.reviewer.as_ref().map(|(k, m)| json!({ "agent": k.as_str(), "agentName": k.label(), "model": m })) })
        });
        QString::from(&v.to_string())
    }

    /// A project's team helpers (Settings → Projects → Team), as
    /// `helpers_json` gives them, or {} when it has none (the same as the
    /// session).
    pub fn project_helpers_json(&self, project_id: i32) -> QString {
        let v = self.team_of(project_id as i64).map_or(json!({}), |h| {
            json!({ "agent": h.agent.as_str(), "agentName": h.agent.label(), "model": h.model, "thinking": h.thinking, "team": true })
        });
        QString::from(&v.to_string())
    }

    /// Sets a new session's thinking level ("": leaves it). (Claude's
    /// started at it: `start_session_thinking`.)
    fn apply_thinking(mut self: Pin<&mut Self>, key: &str, level: &str) {
        let level = level.trim();
        if level.is_empty() {
            return;
        }
        let claude = self.live.get(key).is_some_and(|e| e.info.harness == "claude");
        if !claude && let Some(live) = registry::get(key) {
            live.handle.send_raw(json!({ "type": "set_thinking_level", "level": level }));
        }
        // (Claude's session file doesn't say: as if set by hand.)
        if claude {
            self.as_mut().rust_mut().modes.entry(key.to_owned()).or_default().thinking = Some(level.to_owned());
            self.as_mut().modes_changed(key);
        }
    }

    /// A project's shared plan is on: as set, else (by default) in a git
    /// repository.
    pub fn plan_on(&self, project_id: i32) -> bool {
        match self.setting_str(&format!("plan_{project_id}")).as_str() {
            "1" => true,
            "0" => false,
            _ => self.projects.iter().find(|p| p.id == project_id as i64).is_some_and(|p| harlo_core::git::looks_like_repo(&p.path)),
        }
    }

    pub fn session_title(&self, key: &QString) -> QString {
        QString::from(self.title_of(&key.to_string()).as_str())
    }

    pub fn plan_json(&self, key: &QString) -> QString {
        let key = key.to_string();
        let (enabled, commits) = self.features_of(&key).map_or((false, false), |(_, _, plan, commits)| (plan, commits));
        let mut v = crate::plans::get(self.file_for_key(&key).as_deref(), &key).json();
        v["enabled"] = json!(enabled);
        v["commits"] = json!(commits);
        // Running with helpers: the agent, and why it paused.
        if let Some(run) = self.plan_runs.get(&key) {
            // (Steps under review: the session reviewing each.)
            let reviewing: serde_json::Map<String, serde_json::Value> = run.reviews.iter().map(|(id, r)| (id.to_string(), json!(r.reviewer))).collect();
            v["run"] = json!({ "agent": run.agent.as_str(), "agentName": run.agent.label(), "model": run.model, "paused": run.paused, "review": run.review, "reviewing": reviewing });
        }
        QString::from(&v.to_string())
    }

    pub fn plan_edit(mut self: Pin<&mut Self>, key: &QString, op: &QString) {
        let key = key.to_string();
        let op: serde_json::Value = serde_json::from_str(&op.to_string()).unwrap_or_default();
        let id = op["id"].as_u64().unwrap_or(0) as u32;
        let text = |k: &str| op[k].as_str().unwrap_or_default().to_owned();
        crate::plans::update(self.file_for_key(&key).as_deref(), &key, |p| match op["op"].as_str().unwrap_or_default() {
            "add" if !text("title").trim().is_empty() => {
                p.add(&text("title"), &text("detail"), op["after"].as_u64().map(|a| a as u32));
            }
            "edit" if !text("title").trim().is_empty() => {
                let _ = p.edit(id, &text("title"), &text("detail"));
            }
            "remove" => p.remove(id),
            "shift" => p.shift(id, op["by"].as_i64().unwrap_or(0) as i32),
            "skip" => {
                let _ = p.set_skipped(id, op["skipped"].as_bool().unwrap_or(true));
            }
            "alongside" => {
                let _ = p.set_alongside(id, op["alongside"].as_bool().unwrap_or(true));
            }
            "helper" => {
                let _ = p.set_helper(id, &text("agent"), &text("model"));
            }
            "clear" => p.steps.clear(),
            _ => {}
        });
        self.as_mut().plans_changed();
    }

    pub(crate) fn plans_changed(mut self: Pin<&mut Self>) {
        let next = self.plans_revision + 1;
        self.as_mut().set_plans_revision(next);
        crate::phone_bridge::plans_changed();
    }

    pub fn review_step(mut self: Pin<&mut Self>, key: &QString, id: i32, agent: &QString) -> QString {
        let key = key.to_string();
        let plan = crate::plans::get(self.file_for_key(&key).as_deref(), &key);
        let Some(step) = plan.step(id as u32).filter(|s| !s.commit.is_empty()) else { return QString::from("That step has no commit to review.") };
        let Some(kind) = HarnessKind::parse(&agent.to_string()).filter(|k| self.usable_harness(*k)) else { return QString::from("That agent isn't available.") };
        let Some((cwd, ..)) = self.features_of(&key) else { return QString::from("That session isn't in a project here.") };
        if !self.live.contains_key(&key) {
            return QString::from("Open the session first: the review comes back to it.");
        }
        let prompt = format!(
            "Review one step of another agent's work: commit {sha} in {folder}, \"{title}\"{summary}. See it with `git show {sha}`. \
             Check it does what the step says, and look for bugs, missed cases, broken behavior elsewhere, and docs it should have updated. \
             Don't change any files. Reply with what you found, most serious first, each with the file and line; or say it looks right.",
            sha = step.commit,
            folder = cwd.display(),
            title = step.title,
            summary = if step.summary.is_empty() { String::new() } else { format!(" ({})", step.summary) },
        );
        let title = format!("Review: {}", step.title);
        let child = self.as_mut().start_session_with(kind, cwd, "", false);
        if let Some(live) = registry::get(&child) {
            live.handle.send_raw(json!({ "type": "set_session_name", "name": title }));
        }
        self.as_mut().adopt(&child, &key, &title, kind.as_str(), true, None);
        // (Its progress shows in the session's Subagents box.)
        send_first_prompt(&child, FirstPrompt { text: prompt, ..Default::default() }, Some(self.qt_thread()));
        QString::default()
    }

    pub fn set_settling(mut self: Pin<&mut Self>, on: bool) {
        self.save_setting("settle_sessions", if on { "1" } else { "0" });
        if on {
            return self.settle_sessions();
        }
        let shelved: Vec<PathBuf> = self.meta.iter().filter(|(_, m)| !m.settled_why.is_empty()).map(|(p, _)| p.clone()).collect();
        for file in shelved {
            let mut meta = self.meta.get(&file).cloned().unwrap_or_default();
            meta.settled_why.clear();
            let r = self.db().map(|db| db.set_session_meta(&file, &meta)).transpose();
            self.as_mut().report(r);
            self.as_mut().rust_mut().meta.insert(file, meta);
        }
        self.rebuild();
    }

    /// The Settled shelf (`harlo_core::shelf`): puts done and quiet
    /// worktree sessions on their project's shelf, and takes off those
    /// worked in again. Pull requests are looked up in the background.
    fn settle_sessions(mut self: Pin<&mut Self>) {
        if self.settling || self.setting_str("settle_sessions") == "0" {
            return;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let unix = |t: SystemTime| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let live: HashSet<PathBuf> = self.live.values().filter_map(|e| e.info.session_file.clone()).collect();
        // Sessions in a worktree on this computer that could go on the
        // shelf, by worktree; and those worked in since they went.
        struct Tree {
            path: PathBuf,
            branch: String,
            /// Session files, with their last activity.
            sessions: Vec<(PathBuf, i64)>,
        }
        let mut by_tree: Vec<Tree> = Vec::new();
        let mut back: Vec<PathBuf> = Vec::new();
        // Settled for 30 days, untouched: archived (unless that's off).
        let archive_old = self.setting_str("archive_settled") != "0";
        let mut old: Vec<PathBuf> = Vec::new();
        for s in self.sessions.values() {
            let meta = self.meta.get(&s.path).cloned().unwrap_or_default();
            let modified = unix(s.modified);
            if !meta.settled_why.is_empty() {
                if modified > meta.settled_at {
                    back.push(s.path.clone());
                } else if archive_old && !meta.archived && !meta.pinned && !live.contains(&s.path) && harlo_core::shelf::archive_now(modified, meta.settled_at, now) {
                    old.push(s.path.clone());
                }
                continue;
            }
            if meta.archived || meta.pinned || meta.settled_at == modified || live.contains(&s.path) {
                continue;
            }
            let Some(w) = self.worktrees.iter().filter(|w| !w.removed && s.cwd.starts_with(&w.path)).max_by_key(|w| w.path.components().count()) else { continue };
            if harlo_core::remote::is_remote(&w.path) || !w.path.is_dir() {
                continue;
            }
            match by_tree.iter_mut().find(|t| t.path == w.path) {
                Some(t) => t.sessions.push((s.path.clone(), modified)),
                None => by_tree.push(Tree { path: w.path.clone(), branch: w.branch.clone(), sessions: vec![(s.path.clone(), modified)] }),
            }
        }
        for file in old {
            let mut meta = self.meta.get(&file).cloned().unwrap_or_default();
            meta.archived = true;
            let r = self.db().map(|db| db.set_session_meta(&file, &meta)).transpose();
            self.as_mut().report(r);
            self.as_mut().rust_mut().meta.insert(file, meta);
        }
        for file in back {
            let mut meta = self.meta.get(&file).cloned().unwrap_or_default();
            meta.settled_why.clear();
            let r = self.db().map(|db| db.set_session_meta(&file, &meta)).transpose();
            self.as_mut().report(r);
            self.as_mut().rust_mut().meta.insert(file, meta);
        }
        if by_tree.is_empty() {
            self.rebuild();
            return;
        }
        self.as_mut().rust_mut().settling = true;
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let mut settled: Vec<(PathBuf, i64, &'static str)> = Vec::new();
            for tree in by_tree {
                let review = crate::ci::review(&tree.path, &tree.branch);
                settled.extend(tree.sessions.into_iter().filter_map(|(file, modified)| harlo_core::shelf::why(modified, now, review).map(|why| (file, modified, why))));
            }
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().settling = false;
                for (file, modified, why) in settled {
                    // Unless it changed meanwhile (opened, archived, pinned).
                    let mut meta = this.meta.get(&file).cloned().unwrap_or_default();
                    let now_modified = this.sessions.get(&file).map(|s| unix(s.modified));
                    if meta.archived || meta.pinned || now_modified != Some(modified) || this.live.values().any(|e| e.info.session_file.as_ref() == Some(&file)) {
                        continue;
                    }
                    meta.settled_why = why.to_owned();
                    meta.settled_at = modified;
                    let r = this.db().map(|db| db.set_session_meta(&file, &meta)).transpose();
                    this.as_mut().report(r);
                    this.as_mut().rust_mut().meta.insert(file, meta);
                }
                this.rebuild();
            });
        });
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
        // (Another machine's: while it's connected; never waits to connect.)
        let projects: Vec<(i64, PathBuf)> = self
            .projects
            .iter()
            .filter(|p| match harlo_core::remote::host_of(&p.path) {
                None => true,
                Some(h) => harlo_core::remote::host(&h).is_some_and(|h| harlo_core::remote::connected(&h)),
            })
            .map(|p| (p.id, p.path.clone()))
            .collect();
        if projects.is_empty() {
            return;
        }
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let mut found = Vec::new();
            // Projects on other machines whose worktrees were listed.
            let mut listed: Vec<i64> = Vec::new();
            for (id, path) in projects {
                let Some(root) = git::repo_root(&path) else { continue };
                let Ok(list) = git::list_worktrees(&root) else { continue };
                if harlo_core::remote::is_remote(&root) {
                    listed.push(id);
                }
                for wt in list {
                    if !wt.is_main && !wt.prunable {
                        found.push(WorktreeRecord { path: wt.path, project_id: id, branch: wt.branch, base: String::new(), base_commit: String::new(), removed: false });
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
                    // Recorded worktrees whose folder disappeared (another
                    // machine's: not in its list, when it was listed).
                    let gone = |w: &WorktreeRecord| {
                        if harlo_core::remote::is_remote(&w.path) {
                            listed.contains(&w.project_id) && !found.iter().any(|f| f.path == w.path)
                        } else {
                            !w.path.exists()
                        }
                    };
                    for w in this.worktrees.iter().filter(|w| !w.removed && gone(w)) {
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

    /// `branch`, or `branch-2`, `-3`… when a branch or worktree folder by
    /// that name is there already (a helper's from an earlier run). A
    /// remote project's is left as asked (asking its host would wait here).
    fn free_branch(&self, project: &Project, branch: &str) -> String {
        let Some(root) = git::repo_root(&project.path).filter(|r| harlo_core::remote::host_of(r).is_none()) else { return branch.to_owned() };
        let branches = git::local_branches(&root).map(|(b, _)| b).unwrap_or_default();
        let folder = self.worktree_base().join(format!("{}-{}", git::branch_slug(&project.name), project.id));
        (1..100)
            .map(|n| if n == 1 { branch.to_owned() } else { format!("{branch}-{n}") })
            .find(|b| !branches.contains(b) && !folder.join(git::branch_slug(b)).exists())
            .unwrap_or_else(|| branch.to_owned())
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
        let mut project_id = form["projectId"].as_i64().unwrap_or(-1) as i32;
        // (The page says what it shows: never a project of another machine.)
        if let Some(p) = self.projects.iter().find(|p| p.id == project_id as i64)
            && !self.here(&p.path)
        {
            tracing::warn!("new session: project {} isn't on the machine shown ({:?})", p.path.display(), self.instance.to_string());
            project_id = -1;
        }
        let (harness, worktree_branch, base, starter, text) = (field("harness"), field("worktreeBranch"), field("base"), field("starter"), field("text"));
        // The model picked ("" the agent's own).
        let model = field("model");
        // The shield's mode to start in ("" the agent's own).
        let approval = Some(field("approval")).filter(|m| harlo_core::session::APPROVAL_MODES.contains(&m.as_str()));
        // Started in Plan: a plan first, no files changed.
        let plan = form["plan"] == true;
        let attachments: Vec<String> = form["attachments"].as_array().map(|a| a.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect()).unwrap_or_default();
        let mut message = text.trim().to_owned();
        let harness_name = if harness.is_empty() {
            self.projects.iter().find(|p| p.id == project_id as i64).map(|p| p.harness.clone()).unwrap_or_else(|| self.default_harness())
        } else {
            harness.to_string()
        };
        if let Some(s) = harlo_core::starters::find(&starter) {
            message = harlo_core::starters::wrap(s, &message);
        }
        let server = Some((harness_name.clone(), model.clone()));
        let mut first = FirstPrompt { text: message, attachments: attachments.iter().map(|p| PathBuf::from(url_to_path(p))).collect(), server };
        if first.text.trim().is_empty() && first.attachments.is_empty() {
            return QString::default();
        }
        // (As the chat sends a message in Plan.)
        if plan {
            first.text.push_str(harlo_harness::file_blocks::PLAN_NOTE);
        }
        if !worktree_branch.is_empty() {
            self.create_worktree_session(project_id, &QString::from(&worktree_branch), &QString::from(&base), &QString::from(&harness_name), WorktreeStart { first: Some(first), approval, model, plan, ..Default::default() });
            return QString::default();
        }
        let key = self.as_mut().new_session_on(project_id, &harness_name, &model);
        if let Some(mode) = approval {
            self.as_mut().set_approval_mode(&QString::from(&key), &QString::from(&mode));
        }
        if plan {
            self.as_mut().set_plan_mode(&QString::from(&key), true);
        }
        send_first_prompt(&key, first, Some(self.qt_thread()));
        QString::from(&key)
    }

    fn create_worktree_session(mut self: Pin<&mut Self>, project_id: i32, branch: &QString, base: &QString, harness: &QString, opts: WorktreeStart) {
        let WorktreeStart { first, model, thinking, lane, parent, approval, plan, plan_step, budget } = opts;
        let step_of = parent.as_ref().map(|(p, ..)| p.clone()).zip(plan_step);
        let Some((project, root)) = self.project_root(project_id) else {
            self.as_mut().set_notice_text("This project is not a git repository.", true);
            // An agent's helper, or a plan's step: told, not left waiting.
            let why = "the project isn't a git repository";
            match (step_of, parent) {
                (Some((owner, id)), _) => self.plan_step_ended(&owner, id, Err(format!("its worktree couldn't be made: {why}"))),
                (None, Some((parent, title, ..))) => self.tell_worktree_failed(&parent, &title, &branch.to_string(), why),
                _ => {}
            }
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
        let (b, bs, slug) = (branch.clone(), base, git::branch_slug(&branch));
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
                // No base named (a helper's, a plan step's from a session
                // that isn't in a worktree): the branch checked out now.
                let bs = if bs.trim().is_empty() { git::head_branch(&root) } else { bs };
                git::create_worktree(&root, &path, &b, &bs)?;
                // (Where it starts: what bring_over takes its changes from.)
                let start = git::output_in(&path, &["rev-parse", "--verify", "-q", "HEAD"]).trim().to_owned();
                // git reports canonical paths; match them.
                Ok::<_, anyhow::Error>((if host.is_some() { path } else { path.canonicalize().unwrap_or(path) }, bs, start))
            },
            move |mut this, result| match result {
                Ok((path, base, base_commit)) => {
                    let rec = WorktreeRecord { path: path.clone(), project_id: project.id, branch: branch.clone(), base, base_commit, removed: false };
                    if let Some(db) = this.db() {
                        let _ = db.upsert_worktree(&rec);
                    }
                    this.as_mut().reload_worktrees();
                    // Its parent was stopped meanwhile: not started (the
                    // worktree stays, in the list).
                    if parent.as_ref().is_some_and(|(p, ..)| this.halted.contains(p)) {
                        return this.as_mut().set_notice_text(&format!("The helper on {branch} wasn't started: you stopped the session that asked for it."), false);
                    }
                    // Run lanes start in the background; the run's overview is the tab.
                    let key = this.as_mut().start_session_budget(kind, path.clone(), &model, &thinking, lane.is_none() && parent.is_none(), budget);
                    // Started by an agent: its subagent.
                    if let Some((parent, title, notify)) = parent {
                        this.as_mut().adopt(&key, &parent, &title, kind.as_str(), notify, plan_step);
                    }
                    if let Some(lane) = lane {
                        if let Some(mut l) = this.find_run(&lane.0).and_then(|r| r.lanes.into_iter().find(|l| l.idx == lane.1)) {
                            l.cwd = path.clone();
                            if let Some(db) = this.db() {
                                let _ = db.upsert_lane(&lane.0, &l);
                            }
                        }
                        this.as_mut().rust_mut().lane_keys.insert(key.clone(), lane);
                    }
                    if let Some(mode) = approval {
                        this.as_mut().set_approval_mode(&QString::from(&key), &QString::from(&mode));
                    }
                    if plan {
                        this.as_mut().set_plan_mode(&QString::from(&key), true);
                    }
                    this.as_mut().apply_thinking(&key, &thinking);
                    if let Some(first) = first {
                        send_first_prompt(&key, first, Some(this.qt_thread()));
                    }
                    this.as_mut().set_notice_text(&format!("Worktree {branch} ready"), false);
                    this.run_setup(path, setup);
                }
                Err(e) => {
                    this.as_mut().set_notice_text(&format!("Could not create the worktree: {e:#}"), true);
                    // A plan's step: back to waiting, and the run paused.
                    if let Some((owner, id)) = step_of {
                        this.as_mut().plan_step_ended(&owner, id, Err(format!("its worktree couldn't be made: {e:#}")));
                    } else if let Some((parent, title, ..)) = parent {
                        this.tell_worktree_failed(&parent, &title, &branch, &format!("{e:#}"));
                    }
                }
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

    pub fn in_worktree(&self, key: &QString) -> bool {
        self.worktree_for_key(&key.to_string()).is_some()
    }

    pub fn worktree_info(&self, key: &QString) -> QString {
        let key = key.to_string();
        let Some(w) = self.worktree_for_key(&key) else { return QString::default() };
        let project = self.projects.iter().find(|p| p.id == w.project_id).map(|p| p.path.clone());
        // Another machine's: its record now ("loading"), the rest asked
        // there in the background (`worktreeInfoReady`).
        if harlo_core::remote::is_remote(&w.path) {
            let thread = self.qt_thread();
            let (w2, k) = (w.clone(), key.clone());
            crate::runtime().spawn_blocking(move || {
                let info = worktree_details(&w2, project.as_deref(), !w2.removed);
                let _ = thread.queue(move |mut this| this.as_mut().worktree_info_ready(&QString::from(&k), &QString::from(&info.to_string())));
            });
            return QString::from(
                &json!({ "path": w.path.to_string_lossy(), "branch": w.branch, "base": w.base, "mainBranch": "", "exists": !w.removed,
                         "dirty": 0, "ahead": 0, "behind": 0, "loading": true })
                .to_string(),
            );
        }
        QString::from(&worktree_details(&w, project.as_deref(), w.path.exists() && !w.removed).to_string())
    }

    pub fn merge_worktree(mut self: Pin<&mut Self>, key: &QString) {
        let Some(w) = self.worktree_for_key(&key.to_string()) else { return };
        // (Its repository found in the background: maybe another machine's.)
        let Some(project) = self.projects.iter().find(|p| p.id == w.project_id).map(|p| p.path.clone()) else { return };
        self.as_mut().set_notice_text(&format!("Merging {}…", w.branch), false);
        let branch = w.branch.clone();
        self.background(
            move || {
                let main = git::repo_root(&project).ok_or_else(|| "The project isn't a git repository.".to_owned())?;
                git::merge_branch(&main, &branch).map_err(|e| format!("{e:#}"))
            },
            |this, result| match result {
                Ok(msg) => this.set_notice_text(&msg, false),
                Err(e) => this.set_notice_text(&e, true),
            },
        );
    }

    pub fn remove_worktree(mut self: Pin<&mut Self>, key: &QString, delete_branch: bool, force: bool) {
        let Some(w) = self.worktree_for_key(&key.to_string()) else { return };
        let Some(project) = self.projects.iter().find(|p| p.id == w.project_id).map(|p| p.path.clone()) else { return };
        // Stop every session running in that folder first.
        let inside: Vec<String> = self.live.values().filter(|e| e.info.cwd.starts_with(&w.path)).map(|e| e.info.key.clone()).collect();
        for k in inside {
            self.as_mut().close_session(&QString::from(&k));
        }
        self.as_mut().set_notice_text(&format!("Removing worktree {}…", w.branch), false);
        let rec = w.clone();
        self.background(
            move || -> Result<String, String> {
                let main = git::repo_root(&project).ok_or_else(|| "The project isn't a git repository.".to_owned())?;
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
        self.as_mut().place_projects_once();
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
    /// Harlo's arguments, with its sandbox as set.
    fn harlo_args(&self, kind: HarnessKind) -> Vec<String> {
        let mut args = harlo_args(kind, self.setting_str("harlo_sandbox") != "0", self.setting_str("harlo_sandbox_net") != "0");
        if kind == HarnessKind::Harlo {
            args.extend(model_args(&self.setting_str("harlo_task_model"), &self.setting_str("harlo_fallback")));
        }
        args
    }

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
        if key == "openwiki_telemetry" {
            harlo_core::openwiki::set_telemetry(value.trim() == "1");
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
                let home = if s.host.is_empty() { "Home folder".to_owned() } else { format!("Home folder on {}", s.host) };
                v["projectName"] = serde_json::json!(self.projects.iter().find(|p| p.id == s.project_id).map_or(home, |p| p.name.clone()));
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
            // (Without a project: the machine it was made on, kept through edits.)
            host: previous.as_ref().map_or_else(|| self.instance.to_string(), |p| p.host.clone()),
            harness: v["harness"].as_str().unwrap_or_default().to_owned(),
            model: v["model"].as_str().unwrap_or_default().trim().to_owned(),
            repeat,
            enabled: v["enabled"].as_bool().unwrap_or(true),
            catch_up: v["catchUp"].as_bool().unwrap_or(true),
            last_run: previous.as_ref().and_then(|p| p.last_run),
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
        self.as_mut().continue_after_limits();
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
        // (Without a project: the home folder of the machine it was made on.)
        let cwd = project.map(|p| p.path).unwrap_or_else(|| self.home_on(&s.host));
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

    /// Plan limits: Harlo's ChatGPT plan's (`harlo-agent usage`) and Claude
    /// Code's (its own `/usage`).
    /// Each source keeps its last numbers when it can't be asked.
    pub fn refresh_quota(mut self: Pin<&mut Self>) {
        self.as_mut().refresh_limits(true);
        // The other machines' too: those connected, or shown before.
        let shown: Vec<String> = serde_json::from_str::<serde_json::Value>(&self.host_limits_json.to_string())
            .ok()
            .and_then(|v| v.as_object().map(|o| o.keys().cloned().collect()))
            .unwrap_or_default();
        for h in harlo_core::remote::hosts() {
            if harlo_core::remote::info_of(&h.name).is_some() || shown.contains(&h.name) {
                self.as_mut().refresh_host_limits(&h.name, true, false);
            }
        }
    }

    /// [`Self::refresh_quota`]; `claude` false keeps Claude's numbers as they
    /// are (a running session keeps them current).
    fn refresh_limits(mut self: Pin<&mut Self>, ask_claude: bool) {
        if self.quota_loading {
            return;
        }
        self.as_mut().rust_mut().limits_asked = Some(std::time::Instant::now());
        // Each when it's on (Settings → Providers) and there.
        let claude = self
            .provider_enabled(HarnessKind::Claude)
            .then(|| self.program_for(HarnessKind::Claude).or_else(|| harlo_harness::resolve_program(HarnessKind::Claude)))
            .flatten()
            .filter(|p| p.is_file())
            .zip(harlo_harness::claude::config_dir());
        let keep_claude = !ask_claude && claude.is_some();
        let claude = claude.filter(|_| ask_claude);
        // Harlo: its Codex sign-in's usage (none without that sign-in).
        let harlo = self
            .provider_enabled(HarnessKind::Harlo)
            .then(|| self.program_for(HarnessKind::Harlo).or_else(|| harlo_harness::resolve_program(HarnessKind::Harlo)))
            .flatten()
            .filter(|p| p.is_file());
        if claude.is_none() && harlo.is_none() && !keep_claude {
            self.as_mut().set_quota_json(QString::from(&serde_json::json!({ "accounts": [], "error": "" }).to_string()));
            self.publish_phone_limits();
            return;
        }
        self.as_mut().rust_mut().quota_loading = true;
        // Keep showing the last numbers while new ones load.
        let mut current: serde_json::Value = serde_json::from_str(&self.quota_json.to_string()).unwrap_or_else(|_| serde_json::json!({ "accounts": [] }));
        current["loading"] = serde_json::json!(true);
        self.as_mut().set_quota_json(QString::from(&current.to_string()));
        self.publish_phone_limits();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let from_claude = claude.map(|(p, d)| harlo_core::quota::claude(&p, &d));
            let from_harlo = harlo.map(|p| harlo_core::quota::fetch_harlo(&p));
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().quota_loading = false;
                let previous: serde_json::Value = serde_json::from_str(&this.quota_json.to_string()).unwrap_or_default();
                let kept = |claude: bool| -> Vec<serde_json::Value> {
                    previous["accounts"].as_array().into_iter().flatten().filter(|a| (a["provider"] == "Claude") == claude).cloned().collect()
                };
                let (mut accounts, mut errors) = (Vec::new(), Vec::new());
                match from_claude {
                    Some(Ok(Some(a))) => accounts.push(a),
                    None if keep_claude => accounts.extend(kept(true)),
                    Some(Ok(None)) | None => {}
                    Some(Err(e)) => {
                        accounts.extend(kept(true));
                        errors.push(format!("Claude: {e:#}"));
                    }
                }
                match from_harlo {
                    Some(Ok(Some(usage))) => {
                        harlo_core::quota::apply_live(&mut accounts, &usage);
                    }
                    Some(Ok(None)) | None => {}
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
                this.publish_phone_limits();
            });
        });
    }

    /// The limits shown, with `change` made to their accounts (`fresh`: as
    /// they are now); kept for next time as well.
    fn change_limits(mut self: Pin<&mut Self>, fresh: bool, change: impl FnOnce(&mut Vec<serde_json::Value>)) {
        let mut json: serde_json::Value = serde_json::from_str(&self.quota_json.to_string()).unwrap_or_else(|_| json!({ "accounts": [] }));
        let mut accounts: Vec<serde_json::Value> = json["accounts"].as_array().cloned().unwrap_or_default();
        change(&mut accounts);
        json["accounts"] = json!(accounts);
        if fresh {
            json["updatedAt"] = json!(chrono::Local::now().timestamp_millis());
        }
        if let Some(db) = self.db.as_ref() {
            let _ = db.set("cache.quota", &json!({ "accounts": json["accounts"], "updatedAt": json["updatedAt"], "error": "" }).to_string());
        }
        self.as_mut().set_quota_json(QString::from(&json.to_string()));
        self.publish_phone_limits();
    }

    /// What a Claude session on `host` said of its limits: that machine's
    /// (its own sign-in, maybe not this one's account).
    fn apply_host_limits(mut self: Pin<&mut Self>, host: &str, info: &serde_json::Value) {
        let mut all: serde_json::Value = serde_json::from_str(&self.host_limits_json.to_string()).ok().filter(|v: &serde_json::Value| v.is_object()).unwrap_or_else(|| json!({}));
        let mut accounts: Vec<serde_json::Value> = all[host]["accounts"].as_array().cloned().unwrap_or_default();
        let ran_out = harlo_core::quota::apply_claude_live(&mut accounts, info);
        all[host] = json!({ "accounts": accounts, "updatedAt": chrono::Local::now().timestamp_millis() });
        let text = all.to_string();
        if let Some(db) = self.db.as_ref() {
            let _ = db.set("cache.host_quota", &text);
        }
        self.as_mut().set_host_limits_json(QString::from(&text));
        self.publish_phone_limits();
        for label in ran_out {
            let when = info["unifiedWindows"].as_object().into_iter().flat_map(|w| w.values()).filter(|w| w["status"] == "rejected").filter_map(|w| w["resetsAt"].as_i64()).max();
            let body = when.map_or_else(|| "Claude stops there until it resets.".to_owned(), |s| format!("It resets at {}.", harlo_core::quota::reset_time(s * 1000)));
            self.as_mut().notify_limit(&format!("limit-{host}-{label}"), &format!("{} on {host} is used up", harlo_core::quota::limit_name("Claude", &label)), &body, false);
        }
    }

    /// The meter follows the machine shown: what was used last there (and
    /// that machine's limits, asked for again).
    fn show_meter_of(mut self: Pin<&mut Self>, host: &str) {
        let last = self.setting_str(&meter_setting(host));
        if last != self.meter_last.to_string() {
            self.as_mut().set_meter_last(QString::from(&last));
        }
        self.refresh_host_limits(host, true, false);
    }

    /// Another machine's limits, `ask_claude`: its Claude Code's (its
    /// `/usage`, as here). At most once a minute unless `now` (Refresh);
    /// nothing when it isn't connected. When it can't say, the last numbers
    /// stay. (A Harlo session there says its own as it runs.)
    fn refresh_host_limits(mut self: Pin<&mut Self>, host: &str, ask_claude: bool, now: bool) {
        if host.is_empty() || harlo_core::remote::is_disconnected(host) {
            return;
        }
        let Some(h) = harlo_core::remote::host(host) else { return };
        let info = harlo_core::remote::info_of(host);
        // (Not checked yet: tried; else only when it's there and signed in.)
        let claude_key = format!("{host}#claude");
        let due = now || self.host_limits_asked.get(&claude_key).is_none_or(|t| t.elapsed() >= Duration::from_secs(60));
        if !(ask_claude && due && info.as_ref().is_none_or(|i| i.has("claude") && i.claude_auth != "no")) {
            return;
        }
        self.as_mut().rust_mut().host_limits_asked.insert(claude_key, std::time::Instant::now());
        let host = host.to_owned();
        self.as_mut().change_host_limits(&host, |entry| entry["loading"] = json!(true));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let from_claude = harlo_core::quota::claude_on(&h);
            let _ = thread.queue(move |mut this| {
                let mut errors = Vec::new();
                // None: signed in without a plan (an API key): none.
                let claude_account = match from_claude {
                    Ok(a) => Some(a),
                    Err(e) => {
                        tracing::debug!("{host}'s Claude limits: {e:#}");
                        errors.push(format!("{e:#}"));
                        None
                    }
                };
                this.as_mut().change_host_limits(&host, |entry| {
                    let before: Vec<serde_json::Value> = entry["accounts"].as_array().cloned().unwrap_or_default();
                    let mut merged: Vec<serde_json::Value> = match claude_account {
                        Some(a) => a.into_iter().collect(),
                        None => before.iter().filter(|a| a["provider"] == "Claude").cloned().collect(),
                    };
                    merged.extend(before.iter().filter(|a| a["provider"] != "Claude").cloned());
                    *entry = json!({ "accounts": merged, "updatedAt": chrono::Local::now().timestamp_millis(), "error": errors.join("\n") });
                });
            });
        });
    }

    /// Changes one machine's entry in `host_limits_json` (and its cache).
    fn change_host_limits(mut self: Pin<&mut Self>, host: &str, change: impl FnOnce(&mut serde_json::Value)) {
        let mut all: serde_json::Value = serde_json::from_str(&self.host_limits_json.to_string()).ok().filter(|v: &serde_json::Value| v.is_object()).unwrap_or_else(|| json!({}));
        if !all[host].is_object() {
            all[host] = json!({ "accounts": [] });
        }
        change(&mut all[host]);
        let text = all.to_string();
        if let Some(db) = self.db.as_ref() {
            // (Kept without "checking…": a quit mid-way doesn't leave it.)
            let mut kept = all.clone();
            for entry in kept.as_object_mut().into_iter().flat_map(|o| o.values_mut()) {
                if let Some(e) = entry.as_object_mut() {
                    e.remove("loading");
                }
            }
            let _ = db.set("cache.host_quota", &kept.to_string());
        }
        self.as_mut().set_host_limits_json(QString::from(&text));
        self.publish_phone_limits();
    }

    pub fn refresh_host_quota(self: Pin<&mut Self>, host: &QString) {
        self.refresh_host_limits(&host.to_string(), true, true);
    }

    /// What a running Claude session said of its limits, shown at once.
    fn apply_live_limits(mut self: Pin<&mut Self>, info: &serde_json::Value) {
        let mut ran_out = Vec::new();
        // Harlo's (its plan's, from the response): the provider it names.
        if let Some(provider) = info["provider"].as_str() {
            self.as_mut().change_limits(true, |accounts| ran_out = harlo_core::quota::apply_live(accounts, info));
            let windows = info["windows"].as_array().cloned().unwrap_or_default();
            for label in ran_out {
                let when = windows
                    .iter()
                    .filter(|w| w["windowMinutes"].as_i64().map(harlo_core::quota::window_label).as_deref() == Some(label.as_str()))
                    .filter_map(|w| w["resetsAt"].as_i64())
                    .max();
                let name = harlo_core::quota::limit_name(provider, &label);
                let body = when.map_or_else(|| "Its sessions stop until it resets.".to_owned(), |s| format!("It resets at {}.", harlo_core::quota::reset_time(s * 1000)));
                self.as_mut().notify_limit(&format!("limit-{provider}-{label}"), &format!("{name} is used up"), &body, false);
            }
            return;
        }
        self.as_mut().change_limits(true, |accounts| ran_out = harlo_core::quota::apply_claude_live(accounts, info));
        for label in ran_out {
            let when = info["unifiedWindows"].as_object().into_iter().flat_map(|w| w.values()).filter(|w| w["status"] == "rejected").filter_map(|w| w["resetsAt"].as_i64()).max();
            let body = when.map_or_else(|| "Claude stops until it resets.".to_owned(), |s| format!("It resets at {}.", harlo_core::quota::reset_time(s * 1000)));
            self.as_mut().notify_limit(&format!("limit-{label}"), &format!("{} is used up", harlo_core::quota::limit_name("Claude", &label)), &body, false);
        }
    }

    pub fn check_limits(mut self: Pin<&mut Self>) {
        let now = chrono::Local::now().timestamp_millis();
        self.as_mut().check_host_limits(now);
        let json: serde_json::Value = serde_json::from_str(&self.quota_json.to_string()).unwrap_or_default();
        let accounts: Vec<serde_json::Value> = json["accounts"].as_array().cloned().unwrap_or_default();
        let due: Vec<_> = harlo_core::quota::resets_due(&accounts, self.limits_since, now)
            .into_iter()
            .filter(|(p, l, at)| self.as_mut().rust_mut().limits_told.insert(format!("{p}|{l}|{at}")))
            .collect();
        if due.is_empty() {
            return;
        }
        self.as_mut().change_limits(false, |accounts| {
            for (provider, label, _) in &due {
                harlo_core::quota::mark_reset(accounts, provider, label);
            }
        });
        for (provider, label, at) in &due {
            let name = harlo_core::quota::limit_name(provider, label);
            self.as_mut().notify_limit(&format!("reset-{provider}-{label}-{at}"), &format!("{name} has reset"), "You can use it again.", true);
        }
        self.refresh_quota();
    }

    /// The same for the other machines' limits: each reset told once, "on
    /// <machine>", and that machine's numbers asked for again.
    fn check_host_limits(mut self: Pin<&mut Self>, now: i64) {
        let all: serde_json::Value = serde_json::from_str(&self.host_limits_json.to_string()).unwrap_or_default();
        let Some(hosts) = all.as_object() else { return };
        let mut due: Vec<(String, String, String, i64)> = Vec::new();
        for (host, entry) in hosts {
            let accounts: Vec<serde_json::Value> = entry["accounts"].as_array().cloned().unwrap_or_default();
            for (p, l, at) in harlo_core::quota::resets_due(&accounts, self.limits_since, now) {
                if self.as_mut().rust_mut().limits_told.insert(format!("{host}|{p}|{l}|{at}")) {
                    due.push((host.clone(), p, l, at));
                }
            }
        }
        let mut asked = Vec::new();
        for (host, provider, label, at) in &due {
            self.as_mut().change_host_limits(host, |entry| {
                if let Some(accounts) = entry["accounts"].as_array_mut() {
                    harlo_core::quota::mark_reset(accounts, provider, label);
                }
            });
            let name = harlo_core::quota::limit_name(provider, label);
            self.as_mut().notify_limit(&format!("reset-{host}-{provider}-{label}-{at}"), &format!("{name} on {host} has reset"), "You can use it again.", true);
            if !asked.contains(host) {
                asked.push(host.clone());
            }
        }
        for host in asked {
            self.as_mut().refresh_host_limits(&host, true, true);
        }
    }

    /// A plan limit's notification (Settings → Notifications → Plan limits);
    /// a reset has a sound of its own.
    fn notify_limit(self: Pin<&mut Self>, tag: &str, title: &str, body: &str, reset: bool) {
        let settings = harlo_core::notifications::Settings::load(&self.setting_str("notify"), &self.setting_str("notifications"));
        // Phones that want limits (their own switch; quiet hours: silently).
        crate::phone_push::send(crate::phone_push::Alert {
            what: crate::phone_push::What::Limit { reset }, key: tag.to_owned(), title: title.to_owned(), body: body.to_owned(),
            badge: self.waiting_count(), quiet: settings.quiet.hushes_now(false),
        });
        if !settings.enabled || !settings.event("limits").is_some_and(|e| e.0) || settings.quiet.hushes_now(false) {
            return;
        }
        if self.window_active && !settings.while_focused {
            return;
        }
        // (A reset's sound is the one picked, whether or not the others play.)
        let sound = if reset { Some(settings.reset_sound.as_str()) } else { settings.sound.then_some("dialog-warning") };
        self.deliver_with(tag, "limits", "", title, body, sound);
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
        if !has_shield(&entry.info.harness, &entry.info.cwd) {
            return QString::default();
        }
        if let Some(mode) = self.modes.get(&key).and_then(|m| m.approval.clone()) {
            return QString::from(&mode);
        }
        self.starting_approval(&entry.info.harness, &entry.info.cwd)
    }

    pub fn draft_approval(&self, harness: &QString, project_id: i32) -> QString {
        let harness = harness.to_string();
        let cwd = self.projects.iter().find(|p| p.id == project_id as i64).map(|p| p.path.clone()).unwrap_or_else(|| self.instance_home());
        if !has_shield(&harness, &cwd) {
            return QString::default();
        }
        self.starting_approval(&harness, &cwd)
    }

    /// The shield's mode a session starts in, before it's switched: ask
    /// first (Claude's own settings may differ).
    fn starting_approval(&self, _harness: &str, _cwd: &Path) -> QString {
        QString::from("always-ask")
    }

    pub fn set_approval_mode(mut self: Pin<&mut Self>, key: &QString, mode: &QString) {
        let (key, mode) = (key.to_string(), mode.to_string());
        if !harlo_core::session::APPROVAL_MODES.contains(&mode.as_str()) {
            return;
        }
        self.as_mut().rust_mut().modes.entry(key.clone()).or_default().approval = Some(mode.clone());
        self.as_mut().modes_changed(&key);
        self.as_mut().remember_modes(&key);
        // Its helpers at work (theirs too) follow it, whatever their agent
        // (one without a permission shield has none to follow).
        for helper in self.working_helpers(&key) {
            let shielded = self.live.get(&helper).is_some_and(|e| has_shield(&e.info.harness, &e.info.cwd));
            if shielded && self.modes.get(&helper).and_then(|m| m.approval.as_deref()) != Some(mode.as_str()) {
                self.as_mut().rust_mut().modes.entry(helper.clone()).or_default().approval = Some(mode.clone());
                self.as_mut().modes_changed(&helper);
                self.as_mut().remember_modes(&helper);
            }
        }
    }

    pub fn remember_thinking(mut self: Pin<&mut Self>, key: &QString, level: &QString) {
        let key = key.to_string();
        if self.live.get(&key).is_some_and(|e| e.info.harness == "claude") {
            self.as_mut().rust_mut().modes.entry(key.clone()).or_default().thinking = Some(level.to_string());
            self.remember_modes(&key);
        }
    }

    /// Keeps a session's permission mode and thinking level with its file (a
    /// new session's once it has one).
    fn remember_modes(mut self: Pin<&mut Self>, key: &str) {
        let Some(file) = self.live.get(key).and_then(|e| e.info.session_file.clone()) else { return };
        let Some(modes) = self.modes.get(key).cloned() else { return };
        let mut meta = self.meta.get(&file).cloned().unwrap_or_default();
        let before = (meta.approval.clone(), meta.thinking.clone());
        if let Some(approval) = modes.approval {
            meta.approval = approval;
        }
        if let Some(thinking) = modes.thinking {
            meta.thinking = thinking;
        }
        if (meta.approval.clone(), meta.thinking.clone()) == before {
            return;
        }
        if let Some(db) = self.db() {
            let _ = db.set_session_meta(&file, &meta);
        }
        self.as_mut().rust_mut().meta.insert(file, meta);
    }

    /// What a session was left on, when it's opened again. Nothing starts for
    /// this: its agent gets them when it does.
    fn restore_modes(mut self: Pin<&mut Self>, key: &str, file: &Path) {
        let Some(saved) = self.meta.get(file).cloned() else { return };
        if harlo_core::session::APPROVAL_MODES.contains(&saved.approval.as_str()) {
            self.as_mut().rust_mut().modes.entry(key.to_owned()).or_default().approval = Some(saved.approval);
            self.as_mut().modes_changed(key);
        }
        if !saved.thinking.is_empty() && self.live.get(key).is_some_and(|e| e.info.harness == "claude") {
            self.as_mut().rust_mut().modes.entry(key.to_owned()).or_default().thinking = Some(saved.thinking.clone());
            if let Some(live) = registry::get(key) {
                live.handle.send_raw(serde_json::json!({ "type": "set_thinking_level", "level": saved.thinking }));
            }
        }
    }

    pub fn paste_files(&self) -> QStringList {
        let mut list = crate::platform::clipboard_files();
        if list.is_empty() {
            let dir = harlo_core::db::default_path().with_file_name("pasted");
            let _ = std::fs::create_dir_all(&dir);
            prune_pasted(&dir);
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
            let saved = crate::platform::clipboard_image_to(&QString::from(&dir.join(format!("paste-{stamp}")).to_string_lossy().into_owned()));
            if !saved.is_empty() {
                list.append(saved);
            }
        }
        list
    }

    pub fn plan_mode(&self, key: &QString) -> bool {
        self.modes.get(&key.to_string()).is_some_and(|m| m.plan)
    }

    pub fn set_plan_mode(mut self: Pin<&mut Self>, key: &QString, on: bool) {
        let key = key.to_string();
        self.as_mut().rust_mut().modes.entry(key.clone()).or_default().plan = on;
        self.as_mut().modes_changed(&key);
    }

    /// Gives the session its modes and tells views.
    fn modes_changed(mut self: Pin<&mut Self>, key: &str) {
        // Claude switches its permission mode on the spot; Harlo takes the
        // modes itself.
        if self.live.get(key).is_some_and(|e| matches!(e.info.harness.as_str(), "claude" | "harlo"))
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
            .map(|a| a.iter().map(|l| (l["harness"].as_str().unwrap_or("harlo").to_owned(), l["model"].as_str().unwrap_or_default().to_owned())).collect())
            .unwrap_or_default();
        if lanes.len() < 2 || (prompt.is_empty() && starter.is_empty()) {
            self.set_notice_text("A run needs a prompt and at least two lanes.", true);
            return QString::default();
        }
        // The message every lane gets (with the starter's instructions).
        let mut message = prompt.clone();
        let starter_info = harlo_core::starters::find(&starter);
        if let Some(st) = starter_info {
            message = harlo_core::starters::wrap(st, &message);
        }
        let attachments: Vec<PathBuf> = form["attachments"].as_array().map(|a| a.iter().filter_map(|p| p.as_str().map(|p| PathBuf::from(url_to_path(p)))).collect()).unwrap_or_default();
        let first = FirstPrompt { text: message, attachments, server: None };
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
            // (Its model's server, started first if stopped.)
            let server = Some((harness.clone(), model.clone()));
            let first = FirstPrompt { server, ..first.clone() };
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
                    WorktreeStart { first: Some(first.clone()), model: model.clone(), lane: Some(lane_ref), ..Default::default() },
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
                    // (Another machine's: its copy here, fetched as its turns end.)
                    let file = if harlo_core::remote::is_remote(&l.session_file) { harlo_core::remote::copy_of(&l.session_file) } else { Some(l.session_file.clone()) };
                    file.and_then(|f| harlo_harness::store::read_saved(&f).ok())
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
                // (A remote lane's worktree: its record says whether it's there; its
                // changes are the sidebar's count, not asked over SSH while the page draws.)
                let remote = harlo_core::remote::is_remote(&l.cwd);
                let gone = !l.branch.is_empty() && if remote { !self.worktrees.iter().any(|w| w.path == l.cwd && !w.removed) } else { !l.cwd.is_dir() };
                let changes = match (l.branch.is_empty() || gone, remote) {
                    (true, _) => -1,
                    (false, true) => self.change_counts.get(&l.cwd).map_or(-1, |n| *n as i64),
                    (false, false) => git::status(&l.cwd).map(|s| s.files.len() as i64).unwrap_or(-1),
                };
                serde_json::json!({
                    "idx": l.idx,
                    "letter": ((b'A' + l.idx as u8) as char).to_string(),
                    "harness": l.harness,
                    "model": l.model,
                    "branch": l.branch,
                    "worktreeGone": gone,
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
                        let there = if harlo_core::remote::is_remote(&lane.cwd) { harlo_core::remote::exists(&lane.cwd, true) } else { lane.cwd.is_dir() };
                        if there {
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
        // Each machine's under its name (this computer's first) once there's
        // more than one; the sidebar shows only one machine's titles.
        let mut list: Vec<(String, u8, serde_json::Value)> = self
            .live
            .iter()
            .filter(|(_, e)| e.info.status != SessionStatus::Exited)
            // Helpers are their sessions' business: only one waiting for you.
            .filter(|(key, e)| e.info.needs_input || !self.is_helper(key))
            .map(|(key, e)| {
                let saved = PathBuf::from(key);
                let title = self
                    .rows
                    .iter()
                    .find(|r| &r.key == key)
                    .map(|r| r.title.clone())
                    .or_else(|| (!e.info.name.is_empty()).then(|| e.info.name.clone()))
                    .or_else(|| e.info.session_file.as_ref().or(Some(&saved)).and_then(|f| self.sessions.get(f)).map(|s| s.display_title().to_owned()))
                    .or_else(|| (!e.info.first_message.is_empty()).then(|| e.info.first_message.chars().take(60).collect()))
                    .unwrap_or_else(|| "New session".into());
                let status = sidebar::live_status(&e.info);
                let rank = if e.info.needs_input { 0 } else if status == "busy" { 1 } else { 2 };
                let machine = harlo_core::remote::host_of(&e.info.cwd).unwrap_or_default();
                (machine, rank, serde_json::json!({ "key": key, "title": title, "status": status }))
            })
            .collect();
        list.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
        let several = list.iter().any(|(m, ..)| *m != list[0].0);
        let mut out = Vec::new();
        for (i, (machine, _, v)) in list.iter().enumerate() {
            if several && (i == 0 || list[i - 1].0 != *machine) {
                out.push(serde_json::json!({ "header": if machine.is_empty() { "This computer" } else { machine.as_str() } }));
            }
            out.push(v.clone());
        }
        QString::from(&serde_json::Value::Array(out).to_string())
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
        // (A remote one: its copy here, as `request_session_branches` last
        // brought it up to date.)
        let p = PathBuf::from(path.to_string());
        let file = if harlo_core::remote::is_remote(&p) { harlo_core::remote::copy_of(&p) } else { Some(p) };
        let list = file.and_then(|f| harlo_harness::branches::branches(&f).ok()).unwrap_or_default();
        QString::from(&branches_json(list))
    }

    pub fn request_session_branches(&self, path: &QString) {
        let (path, thread, reads) = (path.to_string(), self.qt_thread(), Arc::clone(&self.branch_reads));
        crate::runtime().spawn_blocking(move || {
            let p = Path::new(&path);
            // (A remote one: from its copy here, brought up to date.)
            let json = if harlo_core::remote::is_remote(p) {
                branches_json(harlo_core::remote::local_copy(p).ok().and_then(|copy| harlo_harness::branches::branches(&copy).ok()).unwrap_or_default())
            } else {
                if let Some(again) = reads.lock().unwrap().running.get_mut(&path) {
                    *again = true; // answered when that read ends
                    return;
                }
                reads.lock().unwrap().running.insert(path.clone(), false);
                loop {
                    let stamp = std::fs::metadata(p).ok().and_then(|m| Some((m.modified().ok()?, m.len())));
                    let known = reads.lock().unwrap().known.get(&path).filter(|(at, _)| Some(*at) == stamp).map(|(_, json)| json.clone());
                    let json = known.unwrap_or_else(|| {
                        let json = branches_json(harlo_harness::branches::branches(p).unwrap_or_default());
                        if let Some(stamp) = stamp {
                            reads.lock().unwrap().known.insert(path.clone(), (stamp, json.clone()));
                            harlo_harness::store::after_reading(stamp.1);
                        }
                        json
                    });
                    let mut reads = reads.lock().unwrap();
                    if reads.running.get(&path) != Some(&true) {
                        reads.running.remove(&path);
                        break json;
                    }
                    reads.running.insert(path.clone(), false);
                }
            };
            let _ = thread.queue(move |this| this.session_branches_ready(&QString::from(&path), &QString::from(&json)));
        });
    }

    pub fn branch_preview_file(&self, path: &QString, leaf: &QString) -> QString {
        let dir = std::env::temp_dir().join(format!("{BRANCH_PREVIEW}{}", std::process::id()));
        // (A remote session: from its copy here.)
        if harlo_core::remote::is_remote(Path::new(&path.to_string())) {
            let Some(text) = harlo_core::remote::copy_of(Path::new(&path.to_string())).and_then(|c| std::fs::read_to_string(c).ok()) else { return QString::default() };
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
            let leaf = leaf.to_string();
            self.as_mut().background(
                move || -> Result<(PathBuf, String), String> {
                    let copy = harlo_core::remote::local_copy(&original).map_err(|e| format!("{e:#}"))?;
                    let text = std::fs::read_to_string(copy).map_err(|e| e.to_string())?;
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

    pub fn session_mentions(&self, cwd: &QString, key: &QString, query: &QString) -> QString {
        let cwd = PathBuf::from(cwd.to_string());
        if cwd.as_os_str().is_empty() || harlo_core::remote::is_remote(&cwd) {
            return QString::from("[]");
        }
        let own = self.live.get(&key.to_string()).and_then(|e| e.info.session_file.clone());
        let found = harlo_core::session_ref::candidates(self.sessions.values(), &cwd, own.as_deref(), &query.to_string(), 6);
        let list: Vec<serde_json::Value> = found
            .into_iter()
            .map(|i| {
                serde_json::json!({
                    "mention": harlo_core::session_ref::token(&i.id),
                    "title": i.display_title().chars().take(100).collect::<String>(),
                    "harness": i.harness.as_str(),
                    "path": i.path.to_string_lossy(),
                    "modified": i.modified.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
                })
            })
            .collect();
        QString::from(&serde_json::Value::Array(list).to_string())
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

    pub fn recent_sessions(&self, limit: i32) -> QString {
        let mut list: Vec<_> = self.sessions.values().collect();
        list.sort_by(|a, b| b.modified.cmp(&a.modified));
        let out: Vec<serde_json::Value> = list
            .into_iter()
            .take(limit.clamp(1, 50) as usize)
            .map(|i| {
                serde_json::json!({
                    "path": i.path.to_string_lossy(),
                    "title": i.display_title(),
                    "harness": i.harness.as_str(),
                    "cwd": harlo_core::sidebar::tilde(&i.cwd),
                    "modified": i.modified.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
                })
            })
            .collect();
        QString::from(&serde_json::Value::Array(out).to_string())
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

    /// How a session in `cwd` runs: here with Settings' program, or on its
    /// remote host over SSH.
    fn launch_for(&self, kind: HarnessKind, cwd: &Path) -> (Option<PathBuf>, Option<RemoteLaunch>) {
        match harlo_core::remote::host_of(cwd).and_then(|h| harlo_core::remote::host(&h)) {
            Some(host) => (None, Some(harlo_core::remote::launch(&host))),
            None => (self.program_for(kind), None),
        }
    }

    /// The binary set in Settings for a harness, if any (else found on PATH).
    fn program_for(&self, kind: HarnessKind) -> Option<PathBuf> {
        let path = self.setting_str(&format!("{}_path", kind.as_str()));
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    /// Harness for new sessions outside a project and for new projects:
    /// the setting, else the first on and installed (Harlo when none is).
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
            .map_or("harlo", |k| k.as_str())
            .into()
    }

    /// Where new worktrees go: the setting, else harlo's data folder.
    fn worktree_base(&self) -> PathBuf {
        let set = self.setting_str("worktree_dir");
        if set.is_empty() { worktrees_dir() } else { PathBuf::from(url_to_path(&set)) }
    }

    pub fn session_repo(&self, key: &QString) -> QString {
        let key = key.to_string();
        // (Another machine's: as the change count found it; no SSH while a
        // menu opens.)
        let root = self
            .rows
            .iter()
            .find(|r| r.key == key)
            .filter(|r| !r.cwd.as_os_str().is_empty())
            .and_then(|r| if harlo_core::remote::is_remote(&r.cwd) { self.remote_repo_roots.get(&r.cwd).cloned() } else { git::repo_root(&r.cwd) });
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
    fn push_to_phones(&self, key: &str, note: &Note, quiet: &harlo_core::notifications::Quiet) {
        use crate::phone_push::What;
        use harlo_core::Notable;
        let (what, body) = match note {
            Note::Event(Notable::Completed { last_message, .. }) => (What::Done, last_message.clone()),
            Note::Event(Notable::Error { message }) => (What::Failed, message.clone()),
            Note::Event(Notable::SubagentDone { .. }) => return,
            Note::Question(text, request) => (What::Ask(request.clone()), text.clone()),
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
        // A helper's end goes to the session that started it, not to you:
        // it only notifies when it waits for you (an approval, a question).
        if self.is_helper(key) && !matches!(note, Note::Question(..)) {
            return;
        }
        self.push_to_phones(key, &note, &settings.quiet);
        let model = entry.map(|e| e.model.clone()).unwrap_or_default();
        let plan = self.modes.get(key).is_some_and(|m| m.plan);
        let title_case = |s: &str| s.split(['-', '_', ' ']).filter(|w| !w.is_empty()).map(|w| { let mut c = w.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }).collect::<Vec<_>>().join(" ");
        let mut agent = if plan { "Plan".to_owned() } else { "Build".to_owned() };
        // event, last message, fallback title and body
        let (event, last, fallback) = match note {
            Note::Event(Notable::Completed { last_message, .. }) => ("completion", last_message, (format!("{agent} agent is ready"), format!("{} completed the task", if model.is_empty() { "The agent".to_owned() } else { model.clone() }))),
            Note::Event(Notable::Error { message }) => ("error", message, ("Tool error".to_owned(), "An error occurred".to_owned())),
            Note::Event(Notable::SubagentDone { agent: sub, status, task, .. }) => {
                agent = title_case(&sub);
                let failed = status != "completed";
                let last = if failed { format!("{agent} {status}: {task}") } else { task };
                (if failed { "error" } else { "subagent" }, last, (format!("{agent} is ready"), format!("{agent} finished")))
            }
            Note::Question(text, _) => ("question", text, ("Input needed".to_owned(), "The agent is waiting for your answer".to_owned())),
        };
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
            let (title, body) = settings.render_event(event, &vars, (&fallback.0, &fallback.1));
            let _ = thread.queue(move |mut this| this.as_mut().deliver(&tag, event, &key, &title, &body, settings.sound));
        });
    }

    /// Shows a notification (self-tests only record it).
    fn deliver(self: Pin<&mut Self>, tag: &str, kind: &str, key: &str, title: &str, body: &str, sound: bool) {
        self.deliver_with(tag, kind, key, title, body, sound.then_some("message-new-instant"));
    }

    /// [`Self::deliver`] with the sound theme's name for its sound.
    fn deliver_with(mut self: Pin<&mut Self>, tag: &str, kind: &str, key: &str, title: &str, body: &str, sound: Option<&str>) {
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
        let sounds: Vec<serde_json::Value> = harlo_core::notifications::RESET_SOUNDS.iter().map(|(id, label)| json!({ "id": id, "label": label })).collect();
        QString::from(&json!({ "variables": VARIABLES, "defaults": defaults, "resetSounds": sounds }).to_string())
    }

    pub fn play_sound(&self, name: &QString) {
        crate::notify::play_sound(&name.to_string());
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
        let mut cost: Option<f64> = None;
        let mut notes: Vec<Note> = Vec::new();
        let mut answered = false;
        // Approvals the agent asked for with harlo's tools, answered.
        let mut gates: Vec<(String, String, serde_json::Value)> = Vec::new();
        let mut stopped = false;
        // harlo's tools, called on the session's pipe.
        let mut harlo_calls: Vec<(String, String, serde_json::Value)> = Vec::new();
        // Claude's limits as it just said (a host's may be another account's).
        let mut limits: Option<serde_json::Value> = None;
        let needed_input = entry.info.needs_input;
        let file_before = entry.info.session_file.clone();
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
                    entry.model_id = m.model_id;
                    entry.thinking = m.thinking;
                }
                SessionUpdate::UiRequest(r) => {
                    if !entry.requests.iter().any(|(q, _)| q.id == r.id) {
                        notes.push(Note::Question(question_text(&r), crate::phone_server::request_json(&r)));
                        let asked = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
                        entry.requests.push((r, asked));
                    }
                }
                SessionUpdate::Notable(n) => notes.push(Note::Event(n)),
                SessionUpdate::UiCleared(id) => {
                    let before = entry.requests.len();
                    entry.requests.retain(|(q, _)| q.id != id);
                    answered |= entry.requests.len() != before;
                }
                SessionUpdate::Limits(info) => limits = Some(info),
                SessionUpdate::GateAnswered { id, question, answer } => gates.push((id, question, answer)),
                SessionUpdate::Stopped => stopped = true,
                SessionUpdate::HarloCall { id, tool, args } => harlo_calls.push((id, tool, args)),
                _ => {}
            }
        }
        entry.info.needs_input = !entry.requests.is_empty();
        let _ = needed_input;
        let file_changed = entry.info.session_file != file_before;
        // Whose limits it's using: Claude's sign-in, or Harlo's plan
        // through the Codex sign-in ("codex/…"; the official sign-in, API
        // keys and other servers say no limits).
        let limited_by = Some(harlo_core::quota::session_provider(&entry.info.harness, &entry.model_id)).filter(|p| !p.is_empty());
        // (The machine it runs on: its meter.)
        let on_host = harlo_core::remote::host_of(&entry.info.cwd).unwrap_or_default();
        let remote_file = entry.info.session_file.clone().filter(|f| harlo_core::remote::is_remote(f));
        let used = limited_by.clone().filter(|_| became_busy);
        if became_idle && !active {
            entry.info.unread = true;
        }
        if became_idle {
            // The agent may have edited files.
            this.change_kick.notify_one();
            // A lane on another machine: its file copied here for the run
            // page's answer.
            if let Some(file) = remote_file.filter(|_| this.lane_keys.contains_key(key)) {
                crate::runtime().spawn_blocking(move || {
                    let _ = harlo_core::remote::local_copy(&file);
                });
            }
        }
        if file_changed {
            self.as_mut().remember_modes(key);
            self.as_mut().remember_parent(key);
            // (Its helpers started before its file was known.)
            let kids: Vec<String> = self.orch_children.iter().filter(|(_, c)| c.parent == key).map(|(k, _)| k.clone()).collect();
            for k in kids {
                self.as_mut().remember_parent(&k);
            }
            // (A helper that finished before its file was known.)
            if self.settle_later.contains(key) && self.active_key.to_string() != key {
                self.as_mut().settle_helper(key);
            }
        }
        // A run lane's session file: remember it for the run.
        if let Some(file) = lane_file
            && let (Some((run, idx)), Some(db)) = (self.lane_keys.get(key), &self.db) {
                let _ = db.set_lane_file(run, *idx, &file);
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
        if let Some(used) = used.filter(|u| *u != self.setting_str(&meter_setting(&on_host))) {
            self.save_setting(&meter_setting(&on_host), &used);
            if on_host == self.instance.to_string() {
                self.as_mut().set_meter_last(QString::from(&used));
            }
        }
        self.as_mut().rebuild();
        if let Some(info) = limits {
            // (Another machine's Claude: its own sign-in, its own limits.)
            if on_host.is_empty() {
                self.as_mut().apply_live_limits(&info);
            } else {
                self.as_mut().apply_host_limits(&on_host, &info);
            }
        }
        // A session an agent started: its state, and its report back; its
        // parent told when it waits for the user.
        if became_busy || became_idle {
            self.as_mut().child_progress(key, became_idle, &notes);
        }
        for note in &notes {
            if let Note::Question(what, request) = note {
                let approval = request["approval"] == true;
                let what = harlo_core::orchestration::waiting_what(request["title"].as_str().unwrap_or_default(), what, approval);
                self.as_mut().child_waiting(key, &what, approval);
            }
        }
        if answered {
            self.as_mut().child_answered(key);
        }
        if stopped {
            self.as_mut().stop_helpers(key);
        }
        // The user's answers to its approval requests (and split offers):
        // to the agent.
        for (id, question, answer) in gates {
            let message = match self.as_mut().rust_mut().proposals.remove(&id) {
                Some((_, helpers)) => self.as_mut().answer_proposal(key, helpers, &answer),
                None => harlo_core::orchestration::approval_answer(&question, &answer),
            };
            if let Some(live) = registry::get(key) {
                let busy = self.live.get(key).is_some_and(|e| e.info.status == SessionStatus::Busy);
                live.handle.prompt(&message, Vec::new(), if busy { harlo_harness::SendMode::FollowUp } else { harlo_harness::SendMode::Normal });
            }
        }
        // Its calls of harlo's tools: the caller is the session itself.
        for (call, tool, args) in harlo_calls {
            let result = self.as_mut().orchestrate_as(Some(key.to_owned()), &tool, &args);
            crate::orchestration::answer_pipe(self.qt_thread(), key.to_owned(), call, tool, args, result);
        }
        // Its reply's diagrams: ones that don't parse go back to it.
        for note in &notes {
            if let Note::Event(harlo_core::Notable::Completed { last_message, .. }) = note {
                self.as_mut().check_diagrams(key, last_message);
            }
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

    /// A finished reply's diagrams, options and mockups, checked off this
    /// thread: when one can't be shown (a diagram that doesn't parse, a
    /// mockup of no file), the agent is told what's wrong and sends it
    /// again, once; not for a helper (its report has
    /// gone to its parent) nor when you've written to it since.
    fn check_diagrams(mut self: Pin<&mut Self>, key: &str, reply: &str) {
        if self.as_mut().rust_mut().diagram_asked.remove(key) || !self.orchestration_on || !["```", "~~~"].iter().any(|f| reply.contains(f)) {
            return;
        }
        if self.orch_children.get(key).is_some_and(|c| c.notify) {
            return;
        }
        let cwd = self.live.get(key).map(|e| e.info.cwd.clone());
        let (thread, key, reply) = (self.qt_thread(), key.to_owned(), reply.to_owned());
        crate::runtime().spawn_blocking(move || {
            let Some(message) = crate::diagrams::fix_message(&reply, cwd.as_deref()) else { return };
            let _ = thread.queue(move |mut this| {
                if this.live.get(&key).is_none_or(|e| e.info.status == SessionStatus::Busy) {
                    return;
                }
                if let Some(live) = registry::get(&key) {
                    this.as_mut().rust_mut().diagram_asked.insert(key.clone());
                    live.handle.prompt(&message, Vec::new(), harlo_harness::SendMode::Normal);
                }
            });
        });
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
            // (A session's folder, for its mockups.)
            "sessionCwd" => {
                let cwd = self.live.get(&key).map(|e| e.info.cwd.clone()).or_else(|| self.sessions.get(Path::new(&key)).map(|s| s.cwd.clone()));
                json!({ "cwd": cwd.map(|c| c.to_string_lossy().into_owned()).unwrap_or_default() })
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
                                     "agents": self.agents_at(&p.path),
                                     // Its own model per agent (Settings → Projects), where it has one.
                                     "ownModels": self.agents_at(&p.path).iter()
                                         .map(|a| (a.to_string(), self.setting_str(&format!("project_model_{}_{a}", p.id))))
                                         .filter(|(_, m)| !m.is_empty()).map(|(a, m)| (a, json!(m))).collect::<serde_json::Map<_, _>>() }))
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
                // Your other machines: connected (and looked at) or not, and
                // the agents found there.
                let machines: Vec<serde_json::Value> = harlo_core::remote::hosts().iter().map(|h| {
                    let connected = !harlo_core::remote::is_disconnected(&h.name) && harlo_core::remote::info_of(&h.name).is_some();
                    let home = harlo_core::remote::info_of(&h.name).map(|i| i.home).unwrap_or_default();
                    json!({ "host": h.name, "connected": connected, "home": home,
                            "agents": if connected { self.agents_at(&self.home_on(&h.name)) } else { Vec::new() } })
                }).collect();
                json!({ "projects": projects, "agents": agents, "defaultAgent": self.default_harness(), "models": models, "agentModels": all, "approvals": approvals(-1),
                        "templates": templates, "machines": machines })
            }
            "new" => {
                let project = msg["project"].as_i64().unwrap_or(-1);
                let project = self.projects.iter().find(|p| p.id == project).cloned();
                let agent = match text("agent") {
                    a if a.is_empty() => project.as_ref().map(|p| p.harness.clone()).unwrap_or_else(|| self.default_harness()),
                    a => a,
                };
                // (No project: the home folder of the machine asked for, this
                // computer's by default, whichever machine the window shows.)
                let host = text("host");
                if project.is_none() && !host.is_empty() && (harlo_core::remote::is_disconnected(&host) || harlo_core::remote::info_of(&host).is_none()) {
                    return fail(&format!("Connect to {host} first."));
                }
                let cwd = match &project {
                    Some(p) => p.path.clone(),
                    None if !host.is_empty() => self.home_on(&host),
                    None => PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/".into())),
                };
                // (On the project's own machine: its agents.)
                let Some(kind) = HarnessKind::parse(&agent).filter(|k| self.agents_at(&cwd).contains(&k.as_str())) else {
                    return fail(&format!("harlo can't start {agent} sessions there."));
                };
                let (first, images) = (text("text"), phone_images(&msg["images"]));
                // The model picked, else the project's own for that agent
                // (Settings → Projects), else the agent's default from
                // Settings (the setup guide; not on another machine: its
                // agent's own).
                let remote = harlo_core::remote::host_of(&cwd).is_some();
                let own = project.as_ref().map(|p| self.setting_str(&format!("project_model_{}_{}", p.id, kind.as_str()))).unwrap_or_default();
                let model = match text("model") {
                    m if m.is_empty() && !own.is_empty() => own,
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
                    return json!({ "key": key, "harness": kind.as_str(), "model": model, "error": live.handle.held().unwrap_or_default() });
                }
                json!({ "key": key, "harness": kind.as_str(), "model": model })
            }
            // A session's plan limit, as the line above the composer says it
            // (`limit` null: under 90%), and when it continues (`resumesAt`,
            // unix ms, 0 if it isn't going to).
            "limitOf" => json!({ "limit": self.limit_of(&key), "resumesAt": self.resume_at_for(&QString::from(&key)) as i64 }),
            // The `@` list's sessions: the open session's folder's others.
            "sessionMentions" => match self.live.get(&key) {
                Some(e) => {
                    let cwd = QString::from(&e.info.cwd.to_string_lossy().into_owned());
                    json!({ "sessions": serde_json::from_str::<serde_json::Value>(&self.session_mentions(&cwd, &QString::from(&key), &QString::from(&text("query"))).to_string()).unwrap_or_default() })
                }
                None => fail("That session isn't running."),
            },
            // Stop (when it works) and continue once its used-up limit resets.
            "resumeAtReset" => {
                let at = self.limit_of(&key).and_then(|l| l["resetAt"].as_i64()).unwrap_or(0);
                if at <= chrono::Local::now().timestamp_millis() {
                    return fail("This session's limit isn't used up.");
                }
                if self.live.get(&key).is_some_and(|e| e.info.status == harlo_core::SessionStatus::Busy)
                    && let Some(live) = registry::get(&key)
                {
                    live.handle.abort();
                }
                self.as_mut().resume_at_reset(&QString::from(&key), at as f64);
                json!({ "ok": true, "resumesAt": at })
            }
            "cancelResume" => {
                self.as_mut().cancel_resume(&QString::from(&key));
                json!({ "ok": true })
            }
            // A running session's agent, model (`provider/id`, "" not known
            // yet) and machine ("" this one), for its model's server.
            // A phone connected a machine (see `phone_server`): its
            // sessions and limits, and what it can start a session with.
            "hostConnected" => {
                let host = text("host");
                self.as_mut().host_connected(&QString::from(&host));
                self.remote_kick.notify_one();
                self.as_mut().refresh_host_limits(&host, true, false);
                let home = self.home_on(&host);
                let shown = harlo_core::remote::info_of(&host).map(|i| i.home).unwrap_or_default();
                json!({ "host": host, "home": shown, "agents": self.agents_at(&home) })
            }
            "serverOf" => match self.live.get(&key) {
                Some(e) => json!({ "agent": e.info.harness, "model": e.model_id,
                                   "host": harlo_core::remote::host_of(&e.info.cwd).unwrap_or_default() }),
                None => fail("That session isn't running."),
            },
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
                // Sessions it mentions (picked from the `@` list): read in
                // before it, off this thread, as the desktop sends them.
                let mentioned: Vec<harlo_harness::SessionInfo> = harlo_core::session_ref::tokens(&text)
                    .iter()
                    .filter_map(|m| harlo_core::session_ref::find(self.sessions.values(), m).cloned())
                    .collect();
                if !mentioned.is_empty() {
                    if let Some(held) = live.handle.held() {
                        return fail(&held);
                    }
                    crate::runtime().spawn_blocking(move || {
                        let text = harlo_core::session_ref::expand(&text, |m| harlo_core::session_ref::find(&mentioned, m).cloned());
                        live.handle.prompt(&text, images, mode);
                    });
                    return json!({ "ok": true });
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
            // A session's shared plan (as the dock above the desktop's
            // composer), the agents that can review a step, and its edits.
            // A session's helpers (the sessions its agent, or its plan's run,
            // started), as its Subagents box shows them.
            "helpers" => json!({ "helpers": serde_json::from_str::<serde_json::Value>(&self.orchestrated_json(&QString::from(&key)).to_string()).unwrap_or_default() }),
            // A helper's request (a tool's approval) answered from the
            // phone, as from the Subagents box: the helpers after it.
            "answerHelper" => {
                let helper = text("helper");
                if !self.orchestrated(&key, &helper) {
                    return fail("That isn't one of this session's helpers.");
                }
                let id = text("id");
                self.as_mut().answer_helper(&QString::from(&helper), &QString::from(&id), &QString::from(&text("value")));
                // (Without the request just answered: it's gone once the
                // helper hears.)
                let mut list: serde_json::Value = serde_json::from_str(&self.orchestrated_json(&QString::from(&key)).to_string()).unwrap_or_default();
                for h in list.as_array_mut().into_iter().flatten().filter(|h| h["id"] == helper.as_str() && h["asks"]["id"] == id.as_str()) {
                    h["asks"] = serde_json::Value::Null;
                }
                json!({ "helpers": list })
            }
            "plan" | "planEdit" | "planRun" | "planStop" => {
                if msg["op"] == "planEdit" {
                    self.as_mut().plan_edit(&QString::from(&key), &QString::from(&msg["edit"].to_string()));
                }
                if msg["op"] == "planStop" {
                    self.as_mut().stop_plan_run(&QString::from(&key));
                }
                if msg["op"] == "planRun" {
                    let problem = self.as_mut().run_plan(&QString::from(&key), &QString::from(&text("agent")), &QString::from(&text("model"))).to_string();
                    if !problem.is_empty() {
                        return fail(&problem);
                    }
                }
                let mut plan: serde_json::Value = serde_json::from_str(&self.plan_json(&QString::from(&key)).to_string()).unwrap_or_default();
                plan["reviewers"] = HarnessKind::PROVIDERS.iter().filter(|&&k| self.usable_harness(k)).map(|k| json!({ "id": k.as_str(), "label": k.label() })).collect();
                // Who "the team's helpers" are (its project's Team, else the
                // session's agent).
                plan["team"] = serde_json::from_str(&self.helpers_json(&QString::from(&key)).to_string()).unwrap_or_default();
                plan
            }
            "reviewStep" => {
                let id = msg["id"].as_i64().unwrap_or(-1) as i32;
                match self.as_mut().review_step(&QString::from(&key), id, &QString::from(&text("agent"))).to_string() {
                    e if !e.is_empty() => fail(&e),
                    _ => json!({ "ok": true }),
                }
            }
            // A settled session back among the others.
            "putBack" => {
                self.as_mut().put_back(&QString::from(&key));
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
        let approvals: Vec<&str> = if approval.is_empty() { Vec::new() } else { harlo_core::session::APPROVAL_MODES.to_vec() };
        json!({ "approval": approval, "approvals": approvals, "plan": self.plan_mode(&QString::from(key)) })
    }

    /// Every session (projects expanded, no "show more"), for phones.
    /// A running session's plan limit (see `quota::session_limit`), from its
    /// machine's accounts.
    fn limit_of(&self, key: &str) -> Option<serde_json::Value> {
        let e = self.live.get(key)?;
        let parse = |v: &QString| serde_json::from_str::<serde_json::Value>(&v.to_string()).unwrap_or_default();
        let accounts = match harlo_core::remote::host_of(&e.info.cwd) {
            None => parse(&self.quota_json)["accounts"].clone(),
            Some(h) => parse(&self.host_limits_json)[h.as_str()]["accounts"].clone(),
        };
        harlo_core::quota::session_limit(accounts.as_array().map(Vec::as_slice).unwrap_or_default(), &e.info.harness, &e.model_id)
    }

    /// The plan limits for phones (this computer's and each machine's), as
    /// Settings → Plan limits shows them.
    fn publish_phone_limits(&self) {
        publish_phone_limits(&self.quota_json, &self.host_limits_json);
    }

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
            // (Settled sessions too: a phone that shows them, as a shelf.)
            shelves_open: &expanded,
        });
        // Helpers under the sessions that started them, as here.
        let parents = self.helper_parents();
        let mut rows = rows;
        sidebar::nest_helpers(&mut rows, &parents);
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
                        // Its machine ("" this one), and why it's settled ("" not).
                        "host": harlo_core::remote::host_of(&r.cwd).unwrap_or_default(), "settled": match harlo_core::shelf::why_text(&r.settled) { "" => r.settled.as_str(), t => t },
                        "requests": requests,
                        // A helper: the session that started it (shown under it).
                        "parent": if r.nest > 0 { parents.get(&r.key).cloned() } else { None }, "helperOf": r.helper_of, "nest": r.nest,
                    }))
                }
                SidebarKind::More | SidebarKind::Shelf => {}
            }
        }
        crate::phone_bridge::publish_sessions(serde_json::Value::Array(list));
    }

    fn rebuild(mut self: Pin<&mut Self>) {
        if crate::phone_bridge::active() {
            self.publish_phone_sessions();
        }
        if self.rows_held {
            self.as_mut().rust_mut().rows_pending = true;
            return;
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
            shelves_open: &self.shelves_open,
        });
        let mut new_rows = new_rows;
        // Sessions agents started: under the session that started them.
        let parents = self.helper_parents();
        sidebar::nest_helpers(&mut new_rows, &parents);
        let mut unchecked = false;
        // A project's uncommitted changes show once, on its header; a session
        // shows its own only when it works somewhere else (a worktree, another
        // folder), so the same count isn't repeated down the list.
        let project_paths: HashMap<i64, PathBuf> =
            new_rows.iter().filter(|r| r.kind == SidebarKind::Project).map(|r| (r.project_id, PathBuf::from(&r.path))).collect();
        let mut project_changes: HashMap<i64, usize> = HashMap::new();
        for r in new_rows.iter_mut().filter(|r| r.kind == SidebarKind::Session) {
            let n = self.change_counts.get(&r.cwd).copied().unwrap_or(0);
            if r.branch.is_empty() && project_paths.get(&r.project_id) == Some(&r.cwd) {
                project_changes.insert(r.project_id, n);
                r.changes = 0;
            } else {
                r.changes = n;
            }
            unchecked |= !r.cwd.as_os_str().is_empty() && !self.change_checked.contains(&r.cwd);
        }
        for r in new_rows.iter_mut().filter(|r| r.kind == SidebarKind::Project) {
            r.changes = project_changes.get(&r.project_id).copied().unwrap_or(0);
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
            ROLE_SETTLED => s(harlo_core::shelf::why_text(&r.settled)),
            ROLE_HELPER_OF => s(&r.helper_of),
            ROLE_NEST => QVariant::from(&(r.nest as i32)),
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
            (ROLE_SETTLED, "settled"),
            (ROLE_HELPER_OF, "helperOf"),
            (ROLE_NEST, "nest"),
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
    /// Its thinking level ("" the agent's default).
    thinking: String,
    lane: Option<(String, i64)>,
    /// Started by an agent (orchestration): its session, the title, notify.
    parent: Option<(String, String, bool)>,
    /// The shield's mode to start in.
    approval: Option<String>,
    /// Starts in Plan.
    plan: bool,
    /// The step of its parent's plan it does (a run with helpers).
    plan_step: Option<u32>,
    /// A Harlo helper's token budget.
    budget: Option<u64>,
}

/// Who helpers are (an agent's, or a plan's run): Settings → Projects → Team.
#[derive(Clone, Debug)]
struct HelperSetup {
    agent: HarnessKind,
    /// "" the agent's default.
    model: String,
    thinking: String,
    /// The project's team (else the same as the session).
    team: bool,
    /// Who reviews a plan's steps, when the team says (else the helpers'
    /// agent and model): an agent and its model ("" its default).
    reviewer: Option<(HarnessKind, String)>,
}

/// A session's plan running with helpers: harlo hands each stage's steps
/// to helper sessions (`harlo_core::plan::Plan::stages`).
#[derive(Clone)]
struct PlanRun {
    agent: HarnessKind,
    model: String,
    /// The helpers' thinking level ("" their default).
    thinking: String,
    /// Why it stopped handing out steps ("" while it runs).
    paused: String,
    /// The session's file when it started (its plan's, should it close).
    file: Option<PathBuf>,
    /// Steps tried again after a failure (once each, then the run pauses).
    retried: HashSet<u32>,
    /// Each finished step is reviewed before the next (Settings →
    /// Orchestration, on by default).
    review: bool,
    /// Steps under review: who did it and what it said.
    reviews: HashMap<u32, StepReview>,
    /// Steps whose review's problems went back to their helper (once each:
    /// problems after that pause the run).
    fixes: HashSet<u32>,
    /// The commit a step in the session's own folder started from (for its
    /// review).
    bases: HashMap<u32, String>,
    /// Who reviews the steps, when the team says (else the run's helpers).
    reviewer: Option<(HarnessKind, String)>,
}

/// A plan's step under review: its helper and what it reported.
#[derive(Clone)]
struct StepReview {
    worker: String,
    summary: String,
    /// The session reviewing it.
    reviewer: String,
}

/// A session an agent started (orchestration).
#[derive(Clone)]
struct OrchChild {
    parent: String,
    title: String,
    agent: String,
    /// Its report (or last reply) goes back to the parent when it
    /// finishes, and its questions.
    notify: bool,
    /// running | asking (its parent) | waiting (for the user) | completed |
    /// failed
    status: String,
    /// What it reported this turn (`report_done`).
    report: Option<harlo_core::orchestration::Report>,
    /// It asked its parent and waits for the answer (until it works again).
    asking: bool,
    /// Its parent was told it waits for the user (once a run).
    told_waiting: bool,
    /// The step of its parent's plan it does (a run with helpers): its
    /// result goes to the run, not to the parent's agent.
    plan_step: Option<u32>,
    /// You stopped it (with its parent): it ends without reporting, and is
    /// settled.
    stopped: bool,
    /// It reviews this step of its parent's plan (a run with helpers): its
    /// verdict goes to the run.
    review_of: Option<u32>,
}

/// The first message of a session started from the new-session page.
#[derive(Clone, Default)]
struct FirstPrompt {
    text: String,
    attachments: Vec<PathBuf>,
    /// Its agent and model (`provider/id`): their server, when harlo can
    /// start it, started first if it's stopped.
    server: Option<(String, String)>,
}

/// Sends a new session's first message (attachments are read off the UI
/// thread). The harness holds it until it has started. `notify`: where a
/// model server that doesn't start is told of.
fn send_first_prompt(key: &str, first: FirstPrompt, notify: Option<CxxQtThread<Workspace>>) {
    let Some(live) = registry::get(key) else { return };
    crate::runtime().spawn_blocking(move || {
        let host = harlo_core::remote::host_of(&live.cwd);
        let server = first.server.as_ref().and_then(|(agent, model)| match &host {
            Some(h) => crate::accounts::host_server(h, agent, model),
            None => crate::accounts::managed_server(agent, model),
        });
        if let Some(server) = server.filter(|s| s.needs_start()) {
            let name = server.name();
            let say = |text: String, error: bool| {
                if let Some(thread) = &notify {
                    let _ = thread.queue(move |this| this.set_notice_text(&text, error));
                }
            };
            say(format!("Starting {name}…"), false);
            match server.start() {
                Ok(()) => say(String::new(), false),
                Err(e) => return say(format!("{name} didn't start, so your message wasn't sent: {e}"), true),
            }
        }
        let prepared = harlo_core::attach::prepare_for(&first.attachments, host.as_deref());
        let text = format!("{}{}", prepared.prefix, first.text);
        live.handle.prompt(&text, prepared.images, harlo_harness::SendMode::Normal);
    });
}

/// The plan limits for phones, from this computer's (`quota_json`) and each
/// machine's (`host_limits_json`).
fn publish_phone_limits(quota: &QString, hosts: &QString) {
    let parse = |v: &QString| serde_json::from_str::<serde_json::Value>(&v.to_string()).unwrap_or_default();
    let (here, hosts) = (parse(quota), parse(hosts));
    crate::phone_bridge::publish_limits(json!({
        "here": { "accounts": here["accounts"].as_array().cloned().unwrap_or_default(), "updatedAt": here["updatedAt"] },
        "hosts": if hosts.is_object() { hosts } else { json!({}) },
    }));
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

/// What a question asks, for its notification: an ask's first question,
/// else the dialog's message or title.
fn question_text(r: &harlo_core::UiRequest) -> String {
    let asked = r.raw["questions"][0]["question"].as_str().unwrap_or_default();
    [asked, r.message.as_str(), r.title.as_str()].into_iter().map(str::trim).find(|t| !t.is_empty()).unwrap_or_default().to_owned()
}

/// Pasted images from before last week: they've been sent or dropped by now.
fn prune_pasted(dir: &Path) {
    let week = std::time::Duration::from_secs(7 * 24 * 3600);
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > week) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
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
/// writes them, here or (over SSH) on its host.
fn session_messages(path: &Path, kind: HarnessKind) -> Vec<serde_json::Value> {
    let remote = harlo_core::remote::host_of(path).and_then(|h| harlo_core::remote::host(&h));
    match remote {
        Some(_) => harlo_core::remote::local_copy(path).ok().and_then(|copy| harlo_harness::store::read_saved_for(kind, &copy).ok()).map(|s| s.messages).unwrap_or_default(),
        None => harlo_harness::store::read_saved_for(kind, path).map(|s| s.messages).unwrap_or_default(),
    }
}

/// What the search index needs of a session: a local Claude transcript only
/// grows, so it's read on from `resume`. (Harlo's are trees: what was added
/// can change which messages came before.)
fn session_part(path: &Path, kind: HarnessKind, resume: Option<&harlo_harness::claude::Resume>) -> harlo_harness::claude::Part {
    if kind == HarnessKind::Claude && !harlo_core::remote::is_remote(path) {
        return harlo_harness::claude::read_part(path, resume).unwrap_or_default();
    }
    harlo_harness::claude::Part { messages: session_messages(path, kind), ..Default::default() }
}

impl Workspace {
    /// An agent's tool call (orchestration): `token` is its session's
    /// `HARLO_SESSION`.
    pub(crate) fn orchestrate(self: Pin<&mut Self>, token: &str, tool: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
        let caller = self.orch_tokens.get(token).cloned();
        self.orchestrate_as(caller, tool, args)
    }

    /// A tool call from `caller` (None: not a session harlo knows).
    fn orchestrate_as(mut self: Pin<&mut Self>, caller: Option<String>, tool: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
        use serde_json::json;
        if !self.orchestration_on {
            return Err("Orchestration is turned off in harlo (Settings → Orchestration).".into());
        }
        // (A session working again: its helpers start again.)
        if let Some(c) = &caller {
            self.as_mut().rust_mut().halted.remove(c);
        }
        if tool == harlo_core::orchestration::ENABLED {
            return Ok(json!(true));
        }
        // The caller's tools, and what they're for: harlo's, and its
        // project's wiki and plan when they're on.
        let features = caller.as_deref().and_then(|c| self.features_of(c));
        // (A helper: it reports to its parent, and offers no splits.)
        let helper = caller.as_ref().and_then(|c| self.orch_children.get(c)).is_some_and(|c| c.notify);
        let starting = self.agents_starting().for_session(helper);
        if tool == harlo_core::orchestration::TOOLS {
            let mut tools = harlo_core::orchestration::tools(starting).as_array().cloned().unwrap_or_default();
            // A helper: its report and its questions for its parent.
            if helper {
                tools.extend(harlo_core::orchestration::helper_tools().as_array().cloned().unwrap_or_default());
            }
            if let Some((cwd, wiki, plan, commits)) = &features {
                if *wiki && harlo_core::openwiki::has_wiki(cwd) {
                    tools.extend(harlo_core::openwiki::reading_tools().as_array().cloned().unwrap_or_default());
                }
                if *plan {
                    tools.extend(harlo_core::plan::tools(*commits).as_array().cloned().unwrap_or_default());
                }
            }
            return Ok(json!(tools));
        }
        if tool == harlo_core::orchestration::INSTRUCTIONS {
            let mut text = format!("{} {}\n\n{}", harlo_core::orchestration::BASE_INSTRUCTIONS, harlo_core::orchestration::when_to_start(starting), harlo_core::orchestration::DIAGRAMS);
            if let Some((cwd, wiki, plan, commits)) = &features {
                if *wiki && harlo_core::openwiki::has_wiki(cwd) {
                    text = format!("{text}\n\n{}", harlo_core::openwiki::READ_INSTRUCTIONS);
                }
                if *plan {
                    // (Claude Code plans in its own flow: its plan question.)
                    let own_flow = caller.as_ref().and_then(|c| self.live.get(c)).is_some_and(|e| e.info.harness == "claude");
                    text = format!("{text}\n\n{}", harlo_core::plan::instructions(*commits, !own_flow));
                }
            }
            return Ok(json!(text));
        }
        // The wiki: OpenWiki answers, off this thread (crate::orchestration).
        if tool.starts_with("openwiki_") {
            let Some((cwd, wiki, ..)) = &features else {
                return Err("The wiki is for sessions in a harlo project.".into());
            };
            if !*wiki || !harlo_core::openwiki::OFFERED.contains(&tool) {
                return Err("The wiki is off for this project (its settings in harlo).".into());
            }
            return Ok(json!({ "_defer": true, "cwd": cwd.to_string_lossy() }));
        }
        // The plan: done off this thread (crate::orchestration), with what
        // it needs from here.
        if tool.starts_with("plan_") {
            let (Some(caller), Some((cwd, _wiki, plan, commits))) = (caller.as_deref(), &features) else {
                return Err("The shared plan is for sessions in a harlo project on this computer.".into());
            };
            if !plan {
                return Err("The shared plan is off for this project (its settings in harlo).".into());
            }
            // A plan running with helpers is harlo's to hand out.
            if matches!(tool, "plan_set" | "plan_start" | "plan_done") && self.plan_runs.get(caller).is_some_and(|r| r.paused.is_empty()) {
                return Err("This plan is running with helpers: harlo hands its steps to them, so leave the steps alone until the run ends (the user can stop it).".into());
            }
            // Other sessions' last work (now, while busy), for telling a
            // step's changes apart.
            let unix = |t: SystemTime| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
            let others: Vec<serde_json::Value> = self
                .live
                .values()
                .filter(|e| e.info.key != caller)
                .map(|e| {
                    let busy = matches!(e.info.status, SessionStatus::Busy | SessionStatus::Starting);
                    let last = e.info.session_file.as_ref().and_then(|f| self.sessions.get(f)).map_or(unix(e.info.started_at), |s| unix(s.modified));
                    json!({ "cwd": e.info.cwd.to_string_lossy(), "last": if busy { i64::MAX } else { last } })
                })
                .collect();
            let file = self.file_for_key(caller).map(|f| f.to_string_lossy().into_owned());
            return Ok(json!({ "_defer": true, "cwd": cwd.to_string_lossy(), "key": caller, "file": file, "commits": commits, "others": others }));
        }
        let text = |k: &str| args[k].as_str().map(str::trim).unwrap_or_default().to_owned();
        let session = |this: &Self| -> Result<String, String> {
            let key = text("session");
            this.live.contains_key(&key).then_some(key.clone()).ok_or_else(|| format!("There's no open session {key}."))
        };
        // The caller's folder, and its machine (None: this one).
        let caller_cwd = caller.as_ref().and_then(|c| self.live.get(c)).map(|e| e.info.cwd.clone());
        let caller_host = caller_cwd.as_deref().and_then(harlo_core::remote::host_of);
        match tool {
            "list_agents" => {
                let list: Vec<serde_json::Value> = HarnessKind::PROVIDERS
                    .iter()
                    .filter(|k| self.usable_harness(**k))
                    .map(|k| {
                        // (A caller on another machine: what's installed there.)
                        let installed = match &caller_host {
                            Some(h) => harlo_core::remote::found_on(h, k.as_str()),
                            None => harlo_harness::resolve_program(*k).is_some() || !self.setting_str(&format!("{}_path", k.as_str())).is_empty(),
                        };
                        json!({ "id": k.as_str(), "name": k.label(), "installed": installed })
                    })
                    .collect();
                Ok(json!({ "agents": list }))
            }
            // Past sessions whose messages match: this folder's (or all).
            "list_sessions" if !text("query").is_empty() => {
                let Some(index) = &self.search else { return Err("Searching sessions isn't available here.".into()) };
                let hits = index.lock().unwrap().search(&text("query"), 300).unwrap_or_default();
                let everywhere = args["everywhere"] == true;
                let mut order: Vec<PathBuf> = Vec::new();
                let mut passages: HashMap<PathBuf, Vec<String>> = HashMap::new();
                for h in hits {
                    let Some(info) = self.sessions.get(&h.path) else { continue };
                    if !everywhere && caller_cwd.as_ref().is_some_and(|c| *c != info.cwd) {
                        continue;
                    }
                    if caller.as_ref().and_then(|c| self.live.get(c)).and_then(|e| e.info.session_file.as_ref()) == Some(&h.path) {
                        continue;
                    }
                    let list = passages.entry(h.path.clone()).or_default();
                    if list.is_empty() {
                        order.push(h.path.clone());
                    }
                    if list.len() < 2 {
                        list.push(format!("{}: {}", h.role, harlo_core::session_ref::plain_snippet(&h.snippet)));
                    }
                }
                let found: Vec<serde_json::Value> = order
                    .iter()
                    .take(8)
                    .filter_map(|p| {
                        let i = self.sessions.get(p)?;
                        let when = chrono::DateTime::<chrono::Local>::from(i.modified).format("%Y-%m-%d %H:%M").to_string();
                        Some(json!({
                            "session": harlo_core::session_ref::token(&i.id), "title": i.display_title().chars().take(100).collect::<String>(),
                            "agent": i.harness.as_str(), "lastActive": when, "folder": everywhere.then(|| i.cwd.to_string_lossy().into_owned()),
                            "matches": passages.remove(p).unwrap_or_default(),
                        }))
                    })
                    .collect();
                Ok(json!({ "sessions": found, "note": if found.is_empty() { "No past session matches." } else { "read_session gives one's latest messages." } }))
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
                    return Err(format!("Unknown agent \"{agent}\": use harlo or claude (list_agents)."));
                };
                if !self.usable_harness(kind) {
                    return Err(format!("{} is turned off in harlo (Settings → Providers).", kind.label()));
                }
                if caller.as_deref().is_some_and(|c| self.helper_depth(c) >= MAX_HELPER_DEPTH) {
                    return Err(format!("Helpers go {MAX_HELPER_DEPTH} deep at most, and you're that deep: do this yourself, or ask your parent (ask_parent)."));
                }
                let prompt = text("prompt");
                if prompt.is_empty() {
                    return Err("Give it a prompt.".into());
                }
                let folder = text("folder");
                // A folder is the caller's machine's (relative: to its folder).
                let cwd = if folder.is_empty() {
                    caller_cwd.clone().unwrap_or_else(|| self.instance_home())
                } else if harlo_core::remote::is_remote(Path::new(&folder)) {
                    PathBuf::from(&folder)
                } else if Path::new(&folder).is_absolute() {
                    match &caller_host {
                        Some(h) => harlo_core::remote::join(h, &folder),
                        None => PathBuf::from(&folder),
                    }
                } else {
                    match &caller_cwd {
                        Some(c) => c.join(&folder),
                        None => PathBuf::from(&folder),
                    }
                };
                if let Some(h) = harlo_core::remote::host_of(&cwd)
                    && !harlo_core::remote::found_on(&h, kind.as_str())
                    && harlo_core::remote::info_of(&h).is_some()
                {
                    return Err(format!("{} isn't installed on {h}.", kind.label()));
                }
                if !harlo_core::remote::is_remote(&cwd) && !cwd.is_dir() {
                    return Err(format!("{} isn't a folder.", cwd.display()));
                }
                let title = { let t = text("title"); if t.is_empty() { prompt.lines().next().unwrap_or_default().chars().take(60).collect() } else { t } };
                let notify = args["notify"].as_bool().unwrap_or(true) && caller.is_some();
                let model = text("model");
                let thinking = text("thinking");
                // Its token budget: asked for, else a helper's from Settings.
                let asked = match &args["budget"] {
                    serde_json::Value::Number(n) => n.as_u64().filter(|n| *n > 0).map(Some).ok_or("Give the budget as a number of tokens, as 400k or 2m.")?,
                    serde_json::Value::String(b) if !b.trim().is_empty() => Some(harlo_core::orchestration::parse_tokens(b).ok_or("Give the budget as a number of tokens, as 400k or 2m.")?),
                    _ => None,
                };
                if asked.is_some() && kind != HarnessKind::Harlo {
                    return Err("A token budget is for Harlo sessions only.".into());
                }
                let budget = asked.or_else(|| (kind == HarnessKind::Harlo && caller.is_some()).then(|| harlo_core::orchestration::parse_tokens(&self.setting_str("harlo_helper_budget"))).flatten());
                let parent = caller.clone().unwrap_or_default();
                // A helper works as its parent may: the same permission mode.
                let approval = caller.as_deref().and_then(|c| self.approval_of(c)).filter(|_| has_shield(kind.as_str(), &cwd));
                // A helper: told what it is and how its result goes back.
                let branch = text("worktree");
                // (The closest: a project can be inside another's folder.)
                let project = self.projects.iter().filter(|p| cwd.starts_with(&p.path)).max_by_key(|p| p.path.components().count()).cloned();
                // A name that's taken (an earlier helper's): the next free one.
                let branch = match &project {
                    Some(p) if !branch.is_empty() => self.free_branch(p, &branch),
                    _ => branch,
                };
                // In a worktree: told where it works (its task names the
                // checkout's paths).
                let prompt = match (&project, branch.is_empty()) {
                    (Some(p), false) => format!("{prompt}{}", harlo_core::orchestration::worktree_note(&branch, &p.path.to_string_lossy())),
                    _ => prompt,
                };
                let prompt = if notify { harlo_core::orchestration::helper_brief(&self.title_of(&parent), &prompt) } else { prompt };
                // In a new worktree: made in the background; it shows when ready.
                if !branch.is_empty() {
                    let Some(project) = project else {
                        return Err("Worktrees are for a project's folder (that one isn't a harlo project).".into());
                    };
                    let first = FirstPrompt { text: prompt, ..Default::default() };
                    self.as_mut().create_worktree_session(
                        project.id as i32,
                        &QString::from(&branch),
                        &QString::default(),
                        &QString::from(kind.as_str()),
                        WorktreeStart { first: Some(first), plan: false, model, thinking: thinking.clone(), lane: None, parent: caller.as_ref().map(|_| (parent, title.clone(), notify)), approval, plan_step: None, budget },
                    );
                    return Ok(json!({ "status": "creating the worktree; the session starts in it shortly (see list_sessions)", "branch": branch, "title": title }));
                }
                let key = self.as_mut().start_session_budget(kind, cwd.clone(), &model, &thinking, false, budget);
                // (Named in the background: renaming would open it.)
                if let Some(live) = registry::get(&key) {
                    live.handle.send_raw(json!({ "type": "set_session_name", "name": title }));
                }
                if caller.is_some() {
                    self.as_mut().adopt(&key, &parent, &title, kind.as_str(), notify, None);
                }
                if let Some(mode) = &approval {
                    self.as_mut().set_approval_mode(&QString::from(&key), &QString::from(mode));
                }
                self.as_mut().apply_thinking(&key, &thinking);
                send_first_prompt(&key, FirstPrompt { text: prompt, ..Default::default() }, Some(self.qt_thread()));
                Ok(json!({ "session": key, "title": title, "agent": kind.as_str(), "folder": cwd.to_string_lossy(), "notify": notify }))
            }
            "send_message" => {
                // One session, or all the caller's open helpers at once.
                let keys: Vec<String> = if text("session") == "helpers" {
                    let me = caller.clone().unwrap_or_default();
                    let mut keys: Vec<String> = self.orch_children.iter().filter(|(k, c)| c.parent == me && self.live.contains_key(*k)).map(|(k, _)| k.clone()).collect();
                    keys.sort();
                    if keys.is_empty() {
                        return Err("None of the helpers you started is open.".into());
                    }
                    keys
                } else {
                    vec![session(&self)?]
                };
                let message = text("message");
                if message.is_empty() {
                    return Err("Give it a message.".into());
                }
                let mut sent = Vec::new();
                for key in keys {
                    let busy = self.live.get(&key).is_some_and(|e| e.info.status == SessionStatus::Busy);
                    if let Some(live) = registry::get(&key) {
                        live.handle.prompt(&message, Vec::new(), if busy { harlo_harness::SendMode::FollowUp } else { harlo_harness::SendMode::Normal });
                    }
                    // (A helper that asked: this is its answer.)
                    if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(&key) {
                        c.status = "running".into();
                        c.asking = false;
                    }
                    sent.push(json!({ "session": key, "queued": busy }));
                }
                self.as_mut().bump_orchestration();
                Ok(if sent.len() == 1 { sent.remove(0) } else { json!({ "sent": sent }) })
            }
            // A past one: what it said last, from its file.
            "read_session" if !self.live.contains_key(&text("session")) && harlo_core::session_ref::find(self.sessions.values(), &text("session")).is_some() => {
                let i = harlo_core::session_ref::find(self.sessions.values(), &text("session")).expect("found");
                let wanted = args["messages"].as_u64().unwrap_or(6).clamp(1, 50) as usize;
                let messages = harlo_core::session_ref::latest(&harlo_core::session_ref::messages_of(i.harness, &i.path), wanted);
                let when = chrono::DateTime::<chrono::Local>::from(i.modified).format("%Y-%m-%d %H:%M").to_string();
                Ok(json!({ "session": harlo_core::session_ref::token(&i.id), "title": i.display_title(), "agent": i.harness.as_str(), "status": "closed", "lastActive": when, "messages": messages }))
            }
            "read_session" => {
                let key = session(&self)?;
                let e = &self.live[&key];
                let title = if !e.info.name.is_empty() { e.info.name.clone() } else { e.info.first_message.chars().take(80).collect() };
                Ok(json!({ "session": key, "title": title, "agent": e.info.harness, "status": if e.info.needs_input { "waiting for input" } else { e.info.status.as_str() } }))
            }
            "bring_over" => {
                let key = session(&self)?;
                let me = caller.clone().unwrap_or_default();
                if self.orch_children.get(&key).is_none_or(|c| c.parent != me) {
                    return Err(format!("{key} isn't your helper: you can bring over only what a helper you started did."));
                }
                let (Some(helper), Some(here)) = (self.live.get(&key).map(|e| e.info.cwd.clone()), caller_cwd.clone()) else {
                    return Err("Its folder or yours isn't known.".into());
                };
                // Its worktree: where it started.
                let tree = self.worktrees.iter().filter(|w| helper.starts_with(&w.path)).max_by_key(|w| w.path.components().count());
                let Some(tree) = tree else {
                    return Err("That helper doesn't work in a worktree harlo made: its changes are in its folder already.".into());
                };
                Ok(json!({ "_defer": true, "helper": tree.path.to_string_lossy(), "here": here.to_string_lossy(), "start": tree.base_commit, "base": tree.base }))
            }
            "stop_session" => {
                let key = session(&self)?;
                self.stop_session(&QString::from(&key));
                Ok(json!({ "session": key, "stopped": true }))
            }
            "request_approval" => {
                let Some(live) = caller.as_deref().and_then(registry::get) else {
                    return Err("Only a session harlo started can ask the user here.".into());
                };
                let question = text("question");
                if question.is_empty() {
                    return Err("Give the question.".into());
                }
                let mut options: Vec<String> = args["options"].as_array().into_iter().flatten().filter_map(|o| o.as_str().map(str::trim)).filter(|o| !o.is_empty()).map(str::to_owned).collect();
                if options.is_empty() {
                    options = vec!["Approve".into(), "Reject".into()];
                }
                live.handle.open_gate("", &question, options);
                Ok(json!({ "asked": true, "next": "End your turn now: harlo sends you the user's answer as a message." }))
            }
            "propose_helpers" => {
                let Some((caller, live)) = caller.as_ref().and_then(|c| registry::get(c).map(|l| (c.clone(), l))) else {
                    return Err("Only a session harlo started can offer a split.".into());
                };
                if helper {
                    return Err("You're a helper: do your task yourself. If it's too big for one session, say so to your parent with ask_parent.".into());
                }
                if self.agents_starting() != harlo_core::orchestration::Starting::Offer {
                    return Err("harlo isn't set to have agents offer splits now (Settings → Orchestration): start helpers only when the user asks.".into());
                }
                if self.helper_depth(&caller) >= MAX_HELPER_DEPTH {
                    return Err(format!("Helpers go {MAX_HELPER_DEPTH} deep at most, and you're that deep: do this yourself."));
                }
                let (why, helpers) = harlo_core::orchestration::parse_proposal(args)?;
                // (Unnamed: the team's helpers, else as the caller.)
                let team = self.helpers_for(&caller);
                let mut lines = Vec::new();
                let mut starts = Vec::new();
                for mut h in helpers {
                    let mut thinking = String::new();
                    if h.agent.is_empty()
                        && let Some(t) = &team
                    {
                        h.agent = t.agent.as_str().to_owned();
                        if h.model.is_empty() {
                            h.model = t.model.clone();
                        }
                        thinking = t.thinking.clone();
                    }
                    let agent = h.agent.clone();
                    let Some(kind) = HarnessKind::parse(&agent).filter(|k| *k != HarnessKind::Demo || agent == "demo") else {
                        return Err(format!("Unknown agent \"{agent}\" for \"{}\": use harlo or claude (list_agents).", h.title));
                    };
                    if !self.usable_harness(kind) {
                        return Err(format!("{} is turned off in harlo (Settings → Providers): pick another agent for \"{}\".", kind.label(), h.title));
                    }
                    lines.push((h.title.clone(), kind.label().to_owned(), h.model.clone(), h.worktree.clone()));
                    starts.push(json!({ "agent": kind.as_str(), "prompt": h.prompt, "title": h.title, "model": h.model, "thinking": thinking, "worktree": h.worktree }));
                }
                let options = harlo_core::orchestration::PROPOSAL_OPTIONS.iter().map(|o| o.to_string()).collect();
                let id = live.handle.open_gate(harlo_core::orchestration::PROPOSAL_TITLE, &harlo_core::orchestration::proposal_question(&why, &lines), options);
                self.as_mut().rust_mut().proposals.insert(id, (caller, starts));
                Ok(json!({ "asked": true, "next": "End your turn now: harlo tells you the user's answer as a message (and starts the helpers if they agree)." }))
            }
            "report_done" | "ask_parent" => {
                let Some((key, child)) = caller.as_ref().and_then(|c| self.orch_children.get(c).map(|ch| (c.clone(), ch.clone()))).filter(|(_, c)| c.notify) else {
                    return Err("You weren't started by another session, so there's no one to report to or ask.".into());
                };
                if tool == "report_done" {
                    let report = harlo_core::orchestration::Report::parse(args)?;
                    if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(&key) {
                        c.report = Some(report);
                    }
                    return Ok(json!({ "reported": true, "next": "End your turn now: harlo sends your report when it ends." }));
                }
                let question = text("question");
                if question.is_empty() {
                    return Err("Give the question.".into());
                }
                let Some(parent) = registry::get(&child.parent) else {
                    return Err("The session that started you is closed. Ask the user instead (end your turn with the question).".into());
                };
                let message = harlo_core::orchestration::question_message(agent_label(&child.agent), &child.title, &key, &question);
                let busy = self.live.get(&child.parent).is_some_and(|e| e.info.status == SessionStatus::Busy);
                parent.handle.prompt(&message, Vec::new(), if busy { harlo_harness::SendMode::FollowUp } else { harlo_harness::SendMode::Normal });
                if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(&key) {
                    c.asking = true;
                    c.status = "asking".into();
                }
                self.as_mut().bump_orchestration();
                Ok(json!({ "asked": true, "next": "End your turn now: harlo sends you the answer as a message." }))
            }
            _ => Err(format!("harlo has no tool {tool}.")),
        }
    }

    /// `key` was started by `parent`'s agent: it's that session's subagent.
    fn adopt(mut self: Pin<&mut Self>, key: &str, parent: &str, title: &str, agent: &str, notify: bool, plan_step: Option<u32>) {
        self.as_mut().rust_mut().orch_children.insert(key.to_owned(), OrchChild { parent: parent.to_owned(), title: title.to_owned(), agent: agent.to_owned(), notify, status: "running".into(), report: None, asking: false, told_waiting: false, plan_step, stopped: false, review_of: None });
        // Named for its part ("Helper"), not by its first message (harlo's
        // briefing).
        if !title.trim().is_empty()
            && self.live.get(key).is_some_and(|e| e.info.name.is_empty())
            && let Some(live) = registry::get(key)
        {
            live.handle.send_raw(serde_json::json!({ "type": "set_session_name", "name": title.trim() }));
        }
        // (A plan's step: who does it.)
        if let Some(id) = plan_step {
            crate::plans::update(self.plan_file(parent).as_deref(), parent, |p| {
                if let Some(s) = p.steps.iter_mut().find(|s| s.id == id) {
                    s.helper = key.to_owned();
                }
            });
            self.as_mut().plans_changed();
        }
        self.as_mut().bump_orchestration();
        // (Shown under its parent in the sidebar.)
        self.rebuild();
    }

    fn bump_orchestration(mut self: Pin<&mut Self>) {
        let next = self.orchestration_revision + 1;
        self.as_mut().set_orchestration_revision(next);
    }

    /// A started session got busy or finished: its state, and (finished)
    /// its report, or last reply, to the session that started it. (Not
    /// while it waits: on its parent's answer, or the user's.)
    fn child_progress(mut self: Pin<&mut Self>, key: &str, finished: bool, notes: &[Note]) {
        let Some(child) = self.orch_children.get(key).cloned() else { return };
        self.as_mut().remember_parent(key);
        if !finished {
            self.as_mut().rust_mut().settle_later.remove(key);
            self.as_mut().rust_mut().settle_once_seen.remove(key);
            if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(key) {
                c.status = "running".into();
                c.asking = false;
                c.told_waiting = false;
                c.stopped = false;
            }
            return self.as_mut().bump_orchestration();
        }
        // Stopped with its parent: settled, and nobody's told.
        if child.stopped {
            if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(key) {
                c.status = "stopped".into();
                c.report = None;
            }
            self.as_mut().settle_helper(key);
            return self.as_mut().bump_orchestration();
        }
        let waiting = self.live.get(key).is_some_and(|e| !e.requests.is_empty());
        if child.asking || waiting {
            if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(key) {
                c.status = if child.asking { "asking" } else { "waiting" }.into();
            }
            return self.as_mut().bump_orchestration();
        }
        // Its turn ended with commands still running in the background:
        // it goes on when they finish (Claude Code), unless it reported.
        let background = notes.iter().any(|n| matches!(n, Note::Event(harlo_core::Notable::Completed { background, .. }) if *background > 0));
        if background && child.report.is_none() {
            if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(key) {
                c.status = "running".into();
            }
            return self.as_mut().bump_orchestration();
        }
        let reply = notes.iter().find_map(|n| match n {
            Note::Event(harlo_core::Notable::Completed { last_message, .. }) => Some(last_message.clone()),
            _ => None,
        });
        let error = notes.iter().find_map(|n| match n {
            Note::Event(harlo_core::Notable::Error { message }) => Some(message.clone()),
            _ => None,
        });
        let status = match &child.report {
            // (A review that found problems did its job.)
            Some(_) if child.review_of.is_some() => "completed",
            Some(r) => if r.succeeded { "completed" } else { "failed" },
            None if error.is_some() && reply.as_deref().is_none_or(|r| r.trim().is_empty()) => "failed",
            None => "completed",
        };
        if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(key) {
            c.status = status.into();
            c.report = None;
            c.told_waiting = false;
        }
        self.as_mut().bump_orchestration();
        // A step's review: its verdict goes to the run.
        if let Some(id) = child.review_of {
            let verdict = match &child.report {
                Some(r) => Some((r.succeeded, r.summary.clone())),
                None => reply.as_deref().and_then(|r| harlo_core::plan::outcome_in(r).map(|ok| (ok, harlo_core::notifications::summary(r, 600)))),
            };
            self.as_mut().settle_helper(key);
            return self.as_mut().plan_step_reviewed(&child.parent, id, verdict, error);
        }
        // A plan's step: its result goes to the run.
        if let Some(id) = child.plan_step {
            let said = match &child.report {
                Some(r) => Some((r.succeeded, r.summary.clone())),
                None => reply.as_deref().and_then(|r| harlo_core::plan::outcome_in(r).map(|ok| (ok, harlo_core::notifications::summary(r, 300)))),
            };
            // Done: reviewed before the run goes on (when the run reviews).
            if let Some((true, summary)) = &said
                && self.plan_runs.get(&child.parent).is_some_and(|r| r.review && r.paused.is_empty())
            {
                let files = child.report.as_ref().map(|r| r.files.clone()).unwrap_or_default();
                return self.as_mut().review_plan_step(&child.parent, id, key, summary, &files);
            }
            if said.as_ref().is_some_and(|(ok, _)| *ok) {
                self.as_mut().settle_helper(key);
            } else {
                self.as_mut().settle_after_look(key);
            }
            let result = match said {
                Some((true, summary)) => Ok(summary),
                Some((false, summary)) => Err(format!("it failed: {summary}")),
                None => Err(match error {
                    Some(e) => format!("it stopped with an error: {e}"),
                    None => "it ended without saying whether it worked (no report)".into(),
                }),
            };
            return self.as_mut().plan_step_ended(&child.parent, id, result);
        }
        if child.report.as_ref().is_some_and(|r| r.succeeded) {
            self.as_mut().settle_helper(key);
        } else {
            self.as_mut().settle_after_look(key);
        }
        if !child.notify {
            return;
        }
        let Some(parent) = registry::get(&child.parent) else { return };
        let busy = self.live.get(&child.parent).is_some_and(|e| e.info.status == SessionStatus::Busy);
        let mode = if busy { harlo_harness::SendMode::FollowUp } else { harlo_harness::SendMode::Normal };
        // In a worktree of this computer's: whether its base moved on.
        let cwd = self.live.get(key).map(|e| e.info.cwd.clone());
        let tree = cwd.as_ref().and_then(|cwd| {
            self.worktrees.iter().filter(|w| !w.removed && !w.base.is_empty() && cwd.starts_with(&w.path) && !harlo_core::remote::is_remote(&w.path)).max_by_key(|w| w.path.components().count()).map(|w| (w.path.clone(), w.branch.clone(), w.base.clone()))
        });
        // (In any worktree: how its work comes over.)
        let in_tree = cwd.as_ref().and_then(|cwd| self.worktrees.iter().filter(|w| !w.removed && cwd.starts_with(&w.path)).max_by_key(|w| w.path.components().count()).map(|w| w.branch.clone()));
        // What it used: from its file when its agent is Harlo (and
        // its subagents' beside it), else its cost as it said.
        let model = self.live.get(key).map(|e| e.model_id.clone()).unwrap_or_default();
        let meta_cost = self.spend.get(key).map_or(0.0, |s| s.cost);
        let file = matches!(HarnessKind::parse(&child.agent), Some(HarnessKind::Harlo)).then(|| self.file_for_key(key)).flatten();
        let (agent, title, key) = (agent_label(&child.agent).to_owned(), child.title.clone(), key.to_owned());
        // (Files and git in the background, then the message.)
        crate::runtime().spawn_blocking(move || {
            let used = match file.and_then(|f| harlo_core::context::spent_in_file(&f)).filter(|s| s.requests > 0) {
                Some(s) => Some(s.line(&model)),
                None if meta_cost > 0.0 => Some(if model.is_empty() { format!("${meta_cost:.2}") } else { format!("{model}: ${meta_cost:.2}") }),
                None => None,
            };
            let mut message = harlo_core::orchestration::finished_message(&agent, &title, &key, child.report.as_ref(), reply.as_deref(), error.as_deref(), used.as_deref());
            if let Some((path, branch, base)) = tree {
                let behind = harlo_core::git::commits_behind(&path, &base).unwrap_or(0);
                message.push_str(&harlo_core::orchestration::drift_note(&branch, &base, behind));
            }
            if let Some(branch) = in_tree {
                message.push_str(&harlo_core::orchestration::bring_over_note(&branch, &key));
            }
            let _ = parent.handle.prompt(&message, Vec::new(), mode);
        });
    }

    /// A started session waits for the user (an approval, its question):
    /// its parent is told (once a run: approvals come one after another).
    fn child_waiting(mut self: Pin<&mut Self>, key: &str, what: &str, approval: bool) {
        let Some(child) = self.orch_children.get(key).cloned() else { return };
        if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(key) {
            c.status = "waiting".into();
            c.told_waiting = true;
        }
        self.as_mut().bump_orchestration();
        // (A plan's step: you're told, as for any session; the run waits.)
        // (A tool's approval: yours alone, and you're notified; the parent
        // isn't woken just to say so.)
        let Some(parent) = registry::get(&child.parent).filter(|_| child.notify && !child.told_waiting && child.plan_step.is_none() && !approval) else { return };
        let message = harlo_core::orchestration::waiting_message(agent_label(&child.agent), &child.title, key, what);
        let busy = self.live.get(&child.parent).is_some_and(|e| e.info.status == SessionStatus::Busy);
        parent.handle.prompt(&message, Vec::new(), if busy { harlo_harness::SendMode::FollowUp } else { harlo_harness::SendMode::Normal });
    }

    /// Its answers: a started session that waited, working again.
    fn child_answered(mut self: Pin<&mut Self>, key: &str) {
        let working = self.live.get(key).is_some_and(|e| e.requests.is_empty() && e.info.status == SessionStatus::Busy);
        if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(key) {
            if working && c.status == "waiting" {
                c.status = "running".into();
            }
            // (Its parent's Subagents box shows what it asks.)
            self.as_mut().bump_orchestration();
        }
    }

    /// A session's permission mode (its shield's), if it has one.
    fn approval_of(&self, key: &str) -> Option<String> {
        let entry = self.live.get(key)?;
        if !has_shield(&entry.info.harness, &entry.info.cwd) {
            return None;
        }
        let mode = self.modes.get(key).and_then(|m| m.approval.clone()).unwrap_or_else(|| self.starting_approval(&entry.info.harness, &entry.info.cwd).to_string());
        harlo_core::session::APPROVAL_MODES.contains(&mode.as_str()).then_some(mode)
    }

    /// The user answered `key`'s split offer: its helpers started (if they
    /// agreed); what to tell it.
    fn answer_proposal(mut self: Pin<&mut Self>, key: &str, helpers: Vec<serde_json::Value>, answer: &serde_json::Value) -> String {
        if answer["value"].as_str() != Some(harlo_core::orchestration::PROPOSAL_OPTIONS[0]) {
            return harlo_core::orchestration::proposal_answer(None);
        }
        let started: Vec<(String, Result<String, String>)> = helpers
            .iter()
            .map(|args| {
                let title = args["title"].as_str().unwrap_or_default().to_owned();
                let result = self.as_mut().orchestrate_as(Some(key.to_owned()), "start_session", args).map(|r| match r["session"].as_str() {
                    Some(session) => session.to_owned(),
                    None => format!("starting in a new worktree on {}; see list_sessions", r["branch"].as_str().unwrap_or_default()),
                });
                (title, result)
            })
            .collect();
        harlo_core::orchestration::proposal_answer(Some(&started))
    }

    /// Settings → Orchestration: when agents start helpers.
    fn agents_starting(&self) -> harlo_core::orchestration::Starting {
        harlo_core::orchestration::Starting::from_setting(&self.setting_str(ORCH_ON_THEIR_OWN))
    }

    /// A session's title, as the sidebar shows it.
    fn title_of(&self, key: &str) -> String {
        if let Some(row) = self.rows.iter().find(|r| r.key == key) {
            return row.title.clone();
        }
        self.live.get(key).map(|e| if !e.info.name.is_empty() { e.info.name.clone() } else { e.info.first_message.chars().take(80).collect() }).unwrap_or_default()
    }

    /// Runs a session's plan with helpers (`agent`: "" for the session's
    /// own, `model`: "" for its default), or goes on with a paused run. The
    /// problem, if any.
    pub fn run_plan(mut self: Pin<&mut Self>, key: &QString, agent: &QString, model: &QString) -> QString {
        let key = key.to_string();
        if !self.orchestration_on {
            return QString::from("Running a plan with helpers needs orchestration (Settings → Orchestration).");
        }
        let Some(entry) = self.live.get(&key) else { return QString::from("Open the session first: its plan runs from it.") };
        let status = entry.info.status;
        // The team's helpers, or (this run) another agent and model.
        let (kind, model, thinking) = match agent.to_string().trim() {
            "" => match self.helpers_for(&key) {
                Some(h) => (Some(h.agent), h.model, h.thinking),
                None => (None, String::new(), String::new()),
            },
            a => (HarnessKind::parse(a), model.to_string().trim().to_owned(), String::new()),
        };
        // (The team's reviewer, for this run's helpers too.)
        let reviewer = self.helpers_for(&key).and_then(|h| h.reviewer);
        let Some(kind) = kind.filter(|k| self.usable_harness(*k)) else { return QString::from("That agent isn't available.") };
        if !self.features_of(&key).is_some_and(|(_, _, plan, _)| plan) {
            return QString::from("The shared plan is off for this session's project (its settings).");
        }
        // Not with the agent at work on it too.
        if matches!(status, SessionStatus::Busy | SessionStatus::Starting) {
            return QString::from("The agent is working: run the plan with helpers once it stops (or stop it first).");
        }
        let file = self.file_for_key(&key);
        // A step the agent left started (stopped midway): a helper's now.
        crate::plans::update(file.as_deref(), &key, |p| {
            for step in p.steps.iter_mut().filter(|s| s.status == harlo_core::plan::Status::Active && s.helper.is_empty()) {
                step.status = harlo_core::plan::Status::Pending;
            }
        });
        let plan = crate::plans::get(file.as_deref(), &key);
        if plan.current_stage().is_none() {
            return QString::from("Every step of the plan has finished.");
        }
        let review = self.setting_str(PLAN_REVIEW) != "0";
        self.as_mut().rust_mut().plan_runs.insert(key.clone(), PlanRun {
                agent: kind,
                model,
                thinking,
                paused: String::new(),
                file,
                retried: HashSet::new(),
                review,
                reviews: HashMap::new(),
                fixes: HashSet::new(),
                bases: HashMap::new(),
                reviewer,
            });
        self.as_mut().plans_changed();
        self.as_mut().advance_plan(&key);
        QString::default()
    }

    /// Stops handing out a plan's steps (helpers at work go on; their steps
    /// are marked as they finish).
    pub fn stop_plan_run(mut self: Pin<&mut Self>, key: &QString) {
        if self.as_mut().rust_mut().plan_runs.remove(&key.to_string()).is_some() {
            self.as_mut().plans_changed();
        }
    }

    /// A run's next steps: the current stage's that haven't started, each
    /// to a helper (more than one: each in a worktree of its own); or, with
    /// every step finished, the run's end.
    fn advance_plan(mut self: Pin<&mut Self>, owner: &str) {
        let Some(run) = self.plan_runs.get(owner).cloned().filter(|r| r.paused.is_empty()) else { return };
        let file = self.plan_file(owner);
        let plan = crate::plans::get(file.as_deref(), owner);
        let Some(stage) = plan.current_stage() else {
            self.as_mut().rust_mut().plan_runs.remove(owner);
            self.as_mut().plans_changed();
            let done = plan.steps.iter().filter(|s| s.status == harlo_core::plan::Status::Done).count();
            let skipped = plan.steps.iter().filter(|s| s.status == harlo_core::plan::Status::Skipped).count();
            let said = format!("{done} step{} done{}", if done == 1 { "" } else { "s" }, if skipped > 0 { format!(", {skipped} skipped") } else { String::new() });
            self.as_mut().tell_about_run(owner, "Plan finished", &said, false);
            // Its agent sums it up (the work on the last step's branch,
            // when the steps ran in worktrees).
            if let Some(live) = registry::get(owner) {
                let busy = self.live.get(owner).is_some_and(|e| e.info.status == SessionStatus::Busy);
                let branch = plan.where_work_is();
                let text = format!("[harlo] Your plan ran with helpers and has finished: {said}.{branch} Read each step's summary with plan_read, check the work fits together, and tell the user what was done and what's still open.");
                live.handle.prompt(&text, Vec::new(), if busy { harlo_harness::SendMode::FollowUp } else { harlo_harness::SendMode::Normal });
            }
            return;
        };
        let ready = plan.ready();
        if ready.is_empty() {
            return;
        }
        let Some((cwd, ..)) = self.features_of(owner) else {
            return self.as_mut().pause_plan(owner, "the session isn't in a harlo project any more");
        };
        let commits = self.features_of(owner).is_some_and(|f| f.3);
        // At the same time: each in a worktree of the project's (from the
        // session's own branch, when it's in one).
        let project = self.projects.iter().filter(|p| cwd.starts_with(&p.path)).max_by_key(|p| p.path.components().count()).cloned();
        let tree = self.worktrees.iter().filter(|w| !w.removed && cwd.starts_with(&w.path)).max_by_key(|w| w.path.components().count()).cloned();
        let project = project.or_else(|| tree.as_ref().and_then(|t| self.projects.iter().find(|p| p.id == t.project_id).cloned()));
        let merge = plan.branches_before(&stage);
        // In worktrees: steps at the same time, and every step after one
        // (it merges theirs: never into your checkout).
        let together = (stage.len() > 1 || !merge.is_empty()) && project.is_some();
        // (Without a project's worktrees: one at a time.)
        let ready: Vec<u32> = if together { ready } else { ready.into_iter().take(1).collect() };
        if !together && plan.steps.iter().any(|s| stage.contains(&s.id) && s.status == harlo_core::plan::Status::Active) {
            return;
        }
        let owner_title = self.title_of(owner);
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() % 100_000);
        for id in ready {
            let Some(step) = plan.step(id).cloned() else { continue };
            let n = plan.steps.iter().position(|s| s.id == id).map_or(0, |i| i + 1);
            // Who does it: the agent and model you chose for the step, else
            // the run's (the run's thinking only with the run's agent).
            let (agent, model, thinking) = match HarnessKind::parse(&step.agent).filter(|k| self.usable_harness(*k)) {
                Some(k) => (k, step.model.clone(), if k == run.agent { run.thinking.clone() } else { String::new() }),
                None => (run.agent, run.model.clone(), run.thinking.clone()),
            };
            let approval = self.approval_of(owner).filter(|_| has_shield(agent.as_str(), &cwd));
            let title = format!("Step {n}: {}", step.title);
            let branch = if together { format!("plan/{}-{id}-{stamp}", git::branch_slug(&step.title).chars().take(30).collect::<String>().trim_matches('-')) } else { String::new() };
            let task = harlo_core::plan::step_prompt(&plan, id, &merge, commits, together);
            let prompt = harlo_core::orchestration::helper_brief(&owner_title, &task);
            if let Err(e) = crate::plans::update(file.as_deref(), owner, |p| p.hand_out(id, "", &branch)) {
                tracing::warn!("plan run: {e}");
                continue;
            }
            // (Harlo's: the budget per helper, if set.)
            let budget = (agent == HarnessKind::Harlo).then(|| harlo_core::orchestration::parse_tokens(&self.setting_str("harlo_helper_budget"))).flatten();
            if together && let Some(project) = &project {
                let base = tree.as_ref().map(|t| t.branch.clone()).unwrap_or_default();
                let first = FirstPrompt { text: prompt, ..Default::default() };
                self.as_mut().create_worktree_session(
                    project.id as i32,
                    &QString::from(&branch),
                    &QString::from(&base),
                    &QString::from(agent.as_str()),
                    WorktreeStart { first: Some(first), plan: false, model: model.clone(), thinking: thinking.clone(), lane: None, parent: Some((owner.to_owned(), title.clone(), true)), approval: approval.clone(), plan_step: Some(id), budget },
                );
                continue;
            }
            // (Where it starts from: its review looks at what changed since.)
            if run.review
                && let Some(head) = harlo_core::plan::snapshot(&cwd, 0).map(|s| s.head).filter(|h| !h.is_empty())
                && let Some(r) = self.as_mut().rust_mut().plan_runs.get_mut(owner)
            {
                r.bases.insert(id, head);
            }
            let key = self.as_mut().start_session_budget(agent, cwd.clone(), &model, &thinking, false, budget);
            if let Some(live) = registry::get(&key) {
                live.handle.send_raw(json!({ "type": "set_session_name", "name": title }));
            }
            self.as_mut().adopt(&key, owner, &title, agent.as_str(), true, Some(id));
            if let Some(mode) = &approval {
                self.as_mut().set_approval_mode(&QString::from(&key), &QString::from(mode));
            }
            self.as_mut().apply_thinking(&key, &thinking);
            send_first_prompt(&key, FirstPrompt { text: prompt, ..Default::default() }, Some(self.qt_thread()));
        }
        self.as_mut().plans_changed();
    }

    /// A plan's step ended: done with its summary (`Ok`), or (`Err`, why)
    /// back to waiting with the run paused for you. Then the run goes on.
    fn plan_step_ended(mut self: Pin<&mut Self>, owner: &str, id: u32, result: Result<String, String>) {
        let file = self.plan_file(owner);
        let plan = crate::plans::get(file.as_deref(), owner);
        let n = plan.steps.iter().position(|s| s.id == id).map_or(0, |i| i + 1);
        match result {
            Ok(summary) => {
                let _ = crate::plans::update(file.as_deref(), owner, |p| p.finish(id, &summary, false).map(|_| ()));
                self.as_mut().plans_changed();
                self.as_mut().advance_plan(owner);
            }
            Err(why) => {
                let _ = crate::plans::update(file.as_deref(), owner, |p| p.put_back(id, &format!("Not done: {why}")));
                self.as_mut().plans_changed();
                // Once more with a new helper (told what went wrong), then
                // the run waits for you.
                let again = self.as_mut().rust_mut().plan_runs.get_mut(owner).is_some_and(|r| r.paused.is_empty() && r.retried.insert(id));
                if again {
                    return self.as_mut().advance_plan(owner);
                }
                self.as_mut().pause_plan(owner, &format!("step {n} ({}) {why}", plan.step(id).map(|s| s.title.as_str()).unwrap_or_default()));
            }
        }
    }

    /// A step's helper reported it done: another session of the run's agent
    /// reviews it, in the same folder, before the run goes on.
    fn review_plan_step(mut self: Pin<&mut Self>, owner: &str, id: u32, worker: &str, summary: &str, files: &[String]) {
        let Some(run) = self.plan_runs.get(owner).cloned() else { return self.as_mut().plan_step_ended(owner, id, Ok(summary.to_owned())) };
        let Some(cwd) = self.live.get(worker).map(|e| e.info.cwd.clone()) else {
            self.as_mut().settle_helper(worker);
            return self.as_mut().plan_step_ended(owner, id, Ok(summary.to_owned()));
        };
        let plan = crate::plans::get(self.plan_file(owner).as_deref(), owner);
        let n = plan.steps.iter().position(|s| s.id == id).map_or(0, |i| i + 1);
        let step = plan.step(id).cloned().unwrap_or_default();
        let mut task = format!("Step {n} of {}: {}", plan.steps.len(), step.title);
        if !step.detail.is_empty() {
            task.push_str(&format!("\n\n{}", step.detail));
        }
        // Where it started: the session's folder then, or its worktree's base.
        let base = run.bases.get(&id).cloned().or_else(|| {
            self.worktrees.iter().filter(|w| !w.removed && !w.base_commit.is_empty() && cwd.starts_with(&w.path)).max_by_key(|w| w.path.components().count()).map(|w| w.base_commit.clone())
        });
        let mut brief = harlo_core::orchestration::review_brief(&self.title_of(owner), &task, summary, files, base.as_deref().unwrap_or_default());
        if !step.branch.is_empty() && !plan.branches_before(&[id]).is_empty() {
            brief.push_str("\n\n(Earlier steps' branches were merged into this one first: review only this step's own changes.)");
        }
        if let Some(r) = self.as_mut().rust_mut().plan_runs.get_mut(owner) {
            r.reviews.insert(id, StepReview { worker: worker.to_owned(), summary: summary.to_owned(), reviewer: String::new() });
        }
        let title = format!("Review: Step {n}: {}", step.title);
        // Who reviews: the team's reviewer, else the run's helpers.
        let (agent, model, thinking) = match &run.reviewer {
            Some((k, m)) => (*k, m.clone(), if *k == run.agent && *m == run.model { run.thinking.clone() } else { String::new() }),
            None => (run.agent, run.model.clone(), run.thinking.clone()),
        };
        let approval = self.approval_of(owner).filter(|_| has_shield(agent.as_str(), &cwd));
        let key = self.as_mut().start_session_budget(agent, cwd, &model, &thinking, false, None);
        if let Some(r) = self.as_mut().rust_mut().plan_runs.get_mut(owner).and_then(|r| r.reviews.get_mut(&id)) {
            r.reviewer = key.clone();
        }
        if let Some(live) = registry::get(&key) {
            live.handle.send_raw(json!({ "type": "set_session_name", "name": title }));
        }
        self.as_mut().adopt(&key, owner, &title, agent.as_str(), true, None);
        if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(&key) {
            c.review_of = Some(id);
        }
        if let Some(mode) = &approval {
            self.as_mut().set_approval_mode(&QString::from(&key), &QString::from(mode));
        }
        self.as_mut().apply_thinking(&key, &thinking);
        send_first_prompt(&key, FirstPrompt { text: brief, ..Default::default() }, Some(self.qt_thread()));
        self.as_mut().plans_changed();
    }

    /// A step's review ended (`verdict`: right, or the problems it found;
    /// None: no verdict). Right: the step is done and the run goes on.
    /// Problems: back to its helper once, then the run pauses for you.
    fn plan_step_reviewed(mut self: Pin<&mut Self>, owner: &str, id: u32, verdict: Option<(bool, String)>, error: Option<String>) {
        let review = self.as_mut().rust_mut().plan_runs.get_mut(owner).and_then(|r| r.reviews.remove(&id));
        let (worker, summary) = review.map(|r| (r.worker, r.summary)).unwrap_or_default();
        let file = self.plan_file(owner);
        let plan = crate::plans::get(file.as_deref(), owner);
        let n = plan.steps.iter().position(|s| s.id == id).map_or(0, |i| i + 1);
        let step = format!("step {n} ({})", plan.step(id).map(|s| s.title.as_str()).unwrap_or_default());
        let problems = match verdict {
            Some((true, said)) => {
                self.as_mut().settle_helper(&worker);
                let said = said.trim();
                let summary = if said.is_empty() { summary } else { format!("{}\n\nReviewed: {said}", summary.trim()) };
                return self.as_mut().plan_step_ended(owner, id, Ok(summary));
            }
            Some((false, problems)) => problems,
            None => {
                self.as_mut().settle_after_look(&worker);
                let why = match error {
                    Some(e) => format!("its review stopped with an error: {e}"),
                    None => "its review ended without a verdict".to_owned(),
                };
                let _ = crate::plans::update(file.as_deref(), owner, |p| p.put_back(id, &format!("Not done: {why}")));
                self.as_mut().plans_changed();
                return self.as_mut().pause_plan(owner, &format!("{step}: {why}"));
            }
        };
        // Back to its helper, once: it fixes them and reports again, and
        // that's reviewed again.
        let again = self.as_mut().rust_mut().plan_runs.get_mut(owner).is_some_and(|r| r.paused.is_empty() && r.fixes.insert(id));
        if again && let Some(live) = registry::get(&worker) {
            if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(&worker) {
                c.report = None;
            }
            let busy = self.live.get(&worker).is_some_and(|e| e.info.status == SessionStatus::Busy);
            live.handle.prompt(&harlo_core::orchestration::review_fix_message(&problems), Vec::new(), if busy { harlo_harness::SendMode::FollowUp } else { harlo_harness::SendMode::Normal });
            return self.as_mut().plans_changed();
        }
        self.as_mut().settle_after_look(&worker);
        let short = harlo_core::notifications::summary(&problems, 300);
        let _ = crate::plans::update(file.as_deref(), owner, |p| p.put_back(id, &format!("Not done: its review found problems: {short}")));
        self.as_mut().plans_changed();
        self.as_mut().pause_plan(owner, &format!("{step}: its review still found problems after a round of fixes: {short}"));
    }

    /// The file a session's plan is kept by: its session file (the one its
    /// run started with, when it's closed since).
    fn plan_file(&self, owner: &str) -> Option<PathBuf> {
        self.file_for_key(owner).or_else(|| self.plan_runs.get(owner).and_then(|r| r.file.clone()))
    }

    /// A run stops handing out steps until you run it again; you're told.
    fn pause_plan(mut self: Pin<&mut Self>, owner: &str, why: &str) {
        {
            let mut this = self.as_mut().rust_mut();
            let Some(run) = this.plan_runs.get_mut(owner) else { return };
            run.paused = why.to_owned();
        }
        self.as_mut().plans_changed();
        let text = format!("Paused: {why}. Run it again to retry the step, or skip or change it first.");
        self.as_mut().tell_about_run(owner, "Plan run paused", &text, true);
    }

    /// A notification about a plan's run (desktop and phone).
    fn tell_about_run(mut self: Pin<&mut Self>, owner: &str, title: &str, body: &str, failed: bool) {
        let settings = harlo_core::notifications::Settings::load(&self.setting_str("notify"), &self.setting_str("notifications"));
        let session = self.title_of(owner);
        crate::phone_push::send(crate::phone_push::Alert {
            what: if failed { crate::phone_push::What::Failed } else { crate::phone_push::What::Done },
            key: owner.to_owned(),
            title: session.clone(),
            body: format!("{title}: {body}"),
            badge: self.waiting_count(),
            quiet: settings.quiet.hushes_now(false),
        });
        if settings.enabled && !settings.quiet.hushes_now(false) {
            let tag = format!("plan-run-{owner}");
            self.as_mut().deliver(&tag, if failed { "error" } else { "completion" }, owner, &format!("{title} · {session}"), body, settings.sound);
        }
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

    pub fn graphics_drawn(&self) {
        crate::graphics::drawn();
    }

    pub fn graphics_started_with(&self) -> QString {
        QString::from(&crate::graphics::started_with())
    }

    pub fn restart_app(&self) {
        crate::graphics::restart();
    }

    pub fn set_orchestration_enabled(mut self: Pin<&mut Self>, on: bool) {
        if on == self.orchestration_on {
            return;
        }
        self.save_setting("orchestration", if on { "1" } else { "0" });
        harlo_harness::set_orchestration(on);
        self.as_mut().set_orchestration_on(on);
        self.as_mut().bump_orchestration();
    }

    /// Answers a helper's request (a tool's approval: `value` Approve,
    /// Deny or Always allow) from its parent's Subagents box.
    pub fn answer_helper(mut self: Pin<&mut Self>, key: &QString, request_id: &QString, value: &QString) {
        if let Some(live) = registry::get(&key.to_string()) {
            live.handle.answer_ui(&request_id.to_string(), json!({ "value": value.to_string() }));
        }
        self.as_mut().bump_orchestration();
    }

    /// Whether `helper` is a helper `parent` started (or its plan's run).
    fn orchestrated(&self, parent: &str, helper: &str) -> bool {
        self.orch_children.get(helper).is_some_and(|c| c.parent == parent)
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
                // What it waits for you to answer (the oldest): answered here
                // when it's a tool's approval.
                let asks = live.and_then(|e| e.requests.first()).map_or(serde_json::Value::Null, |(r, _)| {
                    serde_json::json!({
                        "id": r.id, "approval": r.is_approval, "options": r.options,
                        "text": harlo_core::orchestration::waiting_what(&r.title, &question_text(r), r.is_approval),
                    })
                });
                // Its model, and what it used so far.
                let model = live.map(|e| e.model_id.clone()).unwrap_or_default();
                let used = self.used_by(k, &c.agent);
                serde_json::json!({
                    "asks": asks,
                    "id": k, "name": c.title, "agent": c.agent, "status": status, "openKey": k,
                    "canCancel": live.is_some(), "canSteer": false, "startedAt": live.map(|e| e.info.started_at.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())),
                    "description": c.report.as_ref().map_or("", |r| r.summary.as_str()), "step": c.plan_step, "review": c.review_of,
                    "model": model,
                    "used": used.map_or(serde_json::Value::Null, |u| json!({ "tokens": u.input + u.output, "cost": u.cost, "requests": u.requests })),
                })
            })
            .collect();
        let mut list = list;
        list.sort_by_key(|e| e["startedAt"].as_u64().unwrap_or(0));
        QString::from(&serde_json::Value::Array(list).to_string())
    }

    /// What a session an agent started has used so far: from its file when
    /// it's Harlo's (its subagents' included; read again only when the
    /// file changed), else its cost as its agent said.
    fn used_by(&self, key: &str, agent: &str) -> Option<harlo_core::context::Spent> {
        static READ: OnceLock<Mutex<HashMap<PathBuf, (u64, SystemTime, harlo_core::context::Spent)>>> = OnceLock::new();
        if HarnessKind::parse(agent) == Some(HarnessKind::Harlo)
            && let Some(file) = self.file_for_key(key)
            && let Ok(meta) = std::fs::metadata(&file)
        {
            let stamp = (meta.len(), meta.modified().unwrap_or(UNIX_EPOCH));
            let cache = READ.get_or_init(Default::default);
            if let Some((len, at, spent)) = cache.lock().unwrap().get(&file)
                && (*len, *at) == stamp
            {
                return Some(spent.clone()).filter(|s| s.requests > 0);
            }
            let spent = harlo_core::context::spent_in_file(&file).unwrap_or_default();
            cache.lock().unwrap().insert(file, (stamp.0, stamp.1, spent.clone()));
            return Some(spent).filter(|s| s.requests > 0);
        }
        let cost = self.spend.get(key).map_or(0.0, |s| s.cost);
        (cost > 0.0).then(|| harlo_core::context::Spent { cost, ..Default::default() })
    }

    /// A helper that did its task goes on its project's Settled shelf
    /// ("Done as a helper"), unless you're looking at it; it's back when
    /// it works again.
    /// An agent was told its helper starts shortly (in a worktree): it
    /// hears that it didn't, and why.
    fn tell_worktree_failed(&self, parent: &str, title: &str, branch: &str, why: &str) {
        let Some(live) = registry::get(parent) else { return };
        let busy = self.live.get(parent).is_some_and(|e| e.info.status == SessionStatus::Busy);
        let message = harlo_core::orchestration::worktree_failed(title, branch, why);
        live.handle.prompt(&message, Vec::new(), if busy { harlo_harness::SendMode::FollowUp } else { harlo_harness::SendMode::Normal });
    }

    /// Whether a session is a helper (another session started it): this
    /// run's, or one saved with its parent.
    fn is_helper(&self, key: &str) -> bool {
        self.orch_children.contains_key(key)
            || self.live.get(key).and_then(|e| e.info.session_file.as_ref()).and_then(|f| self.meta.get(f)).is_some_and(|m| !m.parent.is_empty())
    }

    /// A helper that failed, or ended without saying: settled once you've
    /// looked at it (now, if you are).
    fn settle_after_look(mut self: Pin<&mut Self>, key: &str) {
        if key.is_empty() {
            return;
        }
        if self.active_key.to_string() == key {
            self.as_mut().rust_mut().settle_later.insert(key.to_owned());
        } else {
            self.as_mut().rust_mut().settle_once_seen.insert(key.to_owned());
        }
    }

    fn settle_helper(mut self: Pin<&mut Self>, key: &str) {
        if key.is_empty() || self.setting_str("settle_sessions") == "0" {
            return;
        }
        if self.active_key.to_string() == key || self.file_for_key(key).is_none() {
            self.as_mut().rust_mut().settle_later.insert(key.to_owned());
            return;
        }
        self.as_mut().rust_mut().settle_later.remove(key);
        // (A little after now: its last writes may land a moment later.)
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) + 5;
        self.as_mut().update_meta(key, |m| {
            if !m.pinned && !m.archived {
                m.settled_why = "helper".into();
                m.settled_at = at;
            }
        });
    }

    /// Each helper's parent, by their rows' keys: this run's, and those
    /// saved with the sessions (helpers from before harlo restarted).
    fn helper_parents(&self) -> HashMap<String, String> {
        let live_by_file: HashMap<&Path, &str> = self.live.values().filter_map(|e| e.info.session_file.as_deref().map(|f| (f, e.info.key.as_str()))).collect();
        let key_of = |file: &Path| live_by_file.get(file).map_or_else(|| file.to_string_lossy().into_owned(), |k| (*k).to_owned());
        let mut parents: HashMap<String, String> = self.meta.iter().filter(|(_, m)| !m.parent.is_empty()).map(|(f, m)| (key_of(f), key_of(Path::new(&m.parent)))).collect();
        parents.extend(self.orch_children.iter().map(|(k, c)| (k.clone(), c.parent.clone())));
        parents
    }

    /// Saves a helper's parent with it, once both their files are known.
    fn remember_parent(mut self: Pin<&mut Self>, key: &str) {
        let Some(parent) = self.orch_children.get(key).map(|c| c.parent.clone()) else { return };
        let (Some(file), Some(parent_file)) = (self.file_for_key(key), self.file_for_key(&parent)) else { return };
        let parent_file = parent_file.to_string_lossy().into_owned();
        if self.meta.get(&file).is_some_and(|m| m.parent == parent_file) {
            return;
        }
        self.as_mut().update_meta(key, |m| m.parent = parent_file);
    }

    /// How many sessions up a session's chain of parents goes (0: one
    /// nobody started).
    fn helper_depth(&self, key: &str) -> usize {
        let mut at = key;
        let mut depth = 0;
        while let Some(c) = self.orch_children.get(at) {
            depth += 1;
            at = &c.parent;
            if depth > 16 {
                break;
            }
        }
        depth
    }

    pub fn stop_session(&self, key: &QString) {
        if let Some(live) = registry::get(&key.to_string()) {
            live.handle.abort();
        }
    }

    /// The Stop button (with the session's own stop): the helpers it
    /// started (theirs too), working or waiting.
    pub fn stop_helpers_of(self: Pin<&mut Self>, key: &QString) {
        self.stop_helpers(&key.to_string());
    }

    /// Whether helpers `key` started (or theirs) are still at work.
    pub fn helpers_working(&self, key: &QString) -> bool {
        !self.working_helpers(&key.to_string()).is_empty()
    }

    /// The helpers under `key`, at any depth, still at work.
    fn working_helpers(&self, key: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut parents = vec![key.to_owned()];
        while let Some(p) = parents.pop() {
            for (k, c) in self.orch_children.iter().filter(|(_, c)| c.parent == p) {
                parents.push(k.clone());
                let busy = self.live.get(k).is_some_and(|e| matches!(e.info.status, SessionStatus::Busy | SessionStatus::Starting));
                if busy || matches!(c.status.as_str(), "running" | "asking" | "waiting") && self.live.contains_key(k) {
                    out.push(k.clone());
                }
            }
        }
        out
    }

    /// A session was stopped: so are its helpers (whatever their agent),
    /// and its plan's run. They don't report back; they're settled.
    fn stop_helpers(mut self: Pin<&mut Self>, key: &str) {
        if self.as_mut().rust_mut().plan_runs.remove(key).is_some() {
            self.as_mut().plans_changed();
        }
        self.as_mut().rust_mut().halted.insert(key.to_owned());
        let helpers = self.working_helpers(key);
        if helpers.is_empty() {
            return;
        }
        for k in &helpers {
            self.as_mut().rust_mut().halted.insert(k.clone());
            let busy = self.live.get(k).is_some_and(|e| matches!(e.info.status, SessionStatus::Busy | SessionStatus::Starting));
            let asked: Vec<String> = self.live.get(k).map(|e| e.requests.iter().map(|(r, _)| r.id.clone()).collect()).unwrap_or_default();
            if let Some(c) = self.as_mut().rust_mut().orch_children.get_mut(k) {
                c.stopped = true;
                c.status = "stopped".into();
                c.asking = false;
            }
            if let Some(live) = registry::get(k) {
                // (What it asked you: dismissed, so it isn't left waiting.)
                for id in asked {
                    live.handle.answer_ui(&id, serde_json::json!({ "cancelled": true }));
                }
                live.handle.abort();
            }
            // Idle (it waited on its parent): settled now; else as it ends.
            if !busy {
                self.as_mut().settle_helper(k);
            }
        }
        self.as_mut().bump_orchestration();
    }
}

/// How deep helpers go: a helper's helper's helper can't start another.
const MAX_HELPER_DEPTH: usize = 3;

/// The setting: agents may start sessions on their own ("1"), else only
/// when the user asks.
const ORCH_ON_THEIR_OWN: &str = "orchestration_on_their_own";
/// Whether a plan's run reviews each step ("0" off; on by default).
const PLAN_REVIEW: &str = "plan_review";

/// An agent's name ("claude" → "Claude").
fn agent_label(agent: &str) -> &str {
    HarnessKind::parse(agent).map_or(agent, |k| k.label())
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

/// The setting with what was used last on `host` ("" this computer).
fn meter_setting(host: &str) -> String {
    if host.is_empty() { "meter_last".to_owned() } else { format!("meter_last@{host}") }
}

#[cfg(test)]
mod tests {
    #[test]
    fn answered_approvals_reach_the_workspace() {
        // (It sends the answer to the agent; filtered out, the agent never heard it.)
        let answered = harlo_core::session::SessionUpdate::GateAnswered { id: "harlo-gate-1".into(), question: "Delete it?".into(), answer: serde_json::json!({ "value": "Reject" }) };
        assert!(super::workspace_wants(&answered));
    }

    #[test]
    fn a_hosts_projects_are_looked_in_from_the_start() {
        let paths = ["ssh://nas/a", "/local/b", "ssh://pi/c", "ssh://nas/d"].map(std::path::PathBuf::from);
        let roots = super::remote_roots_of(paths.iter());
        assert_eq!(roots.iter().map(|(h, l)| (h.as_str(), l.len())).collect::<Vec<_>>(), [("nas", 2), ("pi", 1)]);
    }

    #[test]
    fn a_session_continues_a_minute_after_its_limit_resets() {
        let resumes = super::parse_resumes(r#"{"/a.jsonl": 1000000, "/b.jsonl": 2000000}"#);
        assert!(super::due_resumes(&resumes, 1_000_000).is_empty());
        assert_eq!(super::due_resumes(&resumes, 1_060_000), [("/a.jsonl".to_owned(), false)]);
        assert_eq!(super::due_resumes(&resumes, 90_000_000).iter().filter(|(_, give_up)| *give_up).count(), 2);
        assert!(super::parse_resumes("").is_empty());
    }

    #[test]
    fn harlos_sandbox_goes_on_its_command_line() {
        use super::{HarnessKind, harlo_args, model_args};
        assert_eq!(harlo_args(HarnessKind::Harlo, true, true), ["--harlo-tools"]);
        assert_eq!(harlo_args(HarnessKind::Harlo, true, false), ["--harlo-tools", "--sandbox", "no-network"]);
        assert_eq!(harlo_args(HarnessKind::Harlo, false, true), ["--harlo-tools", "--sandbox", "off"]);
        assert!(harlo_args(HarnessKind::Claude, false, false).is_empty());
        assert_eq!(model_args(" codex/gpt-6.1-mini ", ""), ["--task-model", "codex/gpt-6.1-mini"]);
        assert_eq!(model_args("", "local/qwen"), ["--fallback", "local/qwen"]);
        assert!(model_args("", "not a model").is_empty());
    }

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
