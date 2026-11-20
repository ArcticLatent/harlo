//! `Workspace`: the QML singleton that owns projects, the on-disk session
//! list, live sessions and the sidebar model built from them.

// Rust guideline compliant 2026-10-09

mod workflows;

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
        /// Whose limits the sidebar meter shows: the account the last busy
        /// session on the machine shown used ("Claude", or a model provider
        /// such as "openai-codex"), unless one is pinned.
        #[qproperty(QString, meter_last, cxx_name = "meterLast")]
        #[qproperty(QString, meter_pin, cxx_name = "meterPin")]
        /// Bumped when a remote file's local copy (`localCopy`) is ready.
        #[qproperty(i32, copies_revision, cxx_name = "copiesRevision")]
        /// Whether agents get harlo's tools (Settings → Tools for agents).
        #[qproperty(bool, orchestration_on, cxx_name = "orchestrationOn")]
        /// Bumped when a workflow run changes (`workflowFor`).
        #[qproperty(i32, workflows_revision, cxx_name = "workflowsRevision")]
        /// Bumped whenever scheduled prompts change (views re-read them).
        #[qproperty(i32, schedules_revision, cxx_name = "schedulesRevision")]
        /// Bumped whenever a session's approval mode or thinking level changes.
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
        /// Offers to update a project's wiki, as its default branch takes
        /// in work changing code the wiki describes: `[{ project, name,
        /// branch, commits, files }]`.
        #[qproperty(QString, wiki_offers_json, cxx_name = "wikiOffersJson")]
        /// Bumped when projects change (Settings → Projects follows it).
        #[qproperty(i32, projects_revision, cxx_name = "projectsRevision")]
        /// Bumped when a provider's check finishes (Settings → Agents).
        #[qproperty(i32, providers_revision, cxx_name = "providersRevision")]
        /// Bumped when `setSetting` changes a value, so bindings on a setting
        /// follow it.
        #[qproperty(i32, settings_revision, cxx_name = "settingsRevision")]
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
        /// Makes a new project folder `name` in `parent` (a path or a
        /// `file://` URL), a git repository: `{ path }` or `{ error }`.
        #[qinvokable]
        fn create_project_folder(self: &Workspace, parent: &QString, name: &QString) -> QString;
        /// Where a new project folder goes unless you pick: the folder
        /// your latest project on this computer is in, else your home.
        #[qinvokable]
        fn new_project_parent(self: &Workspace) -> QString;
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
        fn archive_shelf(self: Pin<&mut Workspace>, project_id: i32, wiki: bool);
        /// A session's title, as the sidebar shows it ("" when unknown).
        #[qinvokable]
        fn session_title(self: &Workspace, key: &QString) -> QString;
        /// Starts a session that writes the wiki of a session's repository
        /// (`key`: a session, or `project:<id>`): `mode` "create",
        /// "update", or "page:<path>" (one page checked against its code).
        /// harlo checks first with no model; an update with nothing owed
        /// starts no session. "" when it's on its way, else why not.
        #[qinvokable]
        fn start_wiki_run(self: Pin<&mut Workspace>, key: &QString, mode: &QString) -> QString;
        /// An offer to update a project's wiki taken: the update starts
        /// (as start_wiki_run's "update"), and the offer goes until its
        /// default branch moves again. "" when it's on its way, else why not.
        #[qinvokable]
        fn take_wiki_offer(self: Pin<&mut Workspace>, project_id: i32) -> QString;
        /// Not now: the offer goes until the default branch moves again.
        #[qinvokable]
        fn decline_wiki_offer(self: Pin<&mut Workspace>, project_id: i32);
        /// Looks for those offers now (harlo does every minute).
        #[qinvokable]
        fn check_wiki_offers(self: Pin<&mut Workspace>);
        /// Marks a page of that wiki reviewed by you (read against its
        /// code): its stale footnotes count as read. Done in the
        /// background; wikiReady follows, and a notice says why not.
        #[qinvokable]
        fn mark_wiki_reviewed(self: Pin<&mut Workspace>, key: &QString, page: &QString);
        /// The wiki of a session's repository (`key`: a session, or
        /// `project:<id>`), as `wiki_view::state` reads it, with `project,
        /// enabled, loading, host, running, session, checking, recorded`
        /// (`running`: a wiki session of harlo's is open there;
        /// `recorded`: when harlo last found nothing owed). Read in the
        /// background: wikiReady says when it changed.
        #[qinvokable]
        fn wiki_json(self: &Workspace, key: &QString) -> QString;
        /// Forgets what the wikis were read as, so wiki_json reads them
        /// again now (Refresh).
        #[qinvokable]
        fn refresh_wiki(self: &Workspace);
        /// Who writes it: `{ agent, model, thinking }` (Settings → Wiki,
        /// else the project's), or {}.
        #[qinvokable]
        fn wiki_writer_json(self: &Workspace, key: &QString) -> QString;
        /// Settings → Wiki changed who writes it: open Wiki panels look again.
        #[qinvokable]
        fn wiki_writer_changed(self: Pin<&mut Workspace>);
        /// A page of it for reading (`wikiPageReady` brings it).
        #[qinvokable]
        fn request_wiki_page(self: &Workspace, key: &QString, page: &QString);
        /// Searches its pages' sections by their words (`wikiSearchReady`
        /// brings the hits); `workspace` is unused.
        #[qinvokable]
        fn request_wiki_search(self: &Workspace, key: &QString, query: &QString, workspace: &QString);
        /// A declaration's card from the code index, for a code hit of the
        /// search (`wikiLookupReady` brings it).
        #[qinvokable]
        fn request_wiki_lookup(self: &Workspace, key: &QString, name: &QString, path: &QString);
        /// Starts a workflow in a project (`form`: `{ workflow, task,
        /// criteria, testCommand, implementer, implementerModel, reviewer,
        /// reviewerModel, approval, prepare }`); the problem, if any.
        #[qinvokable]
        fn start_workflow(self: Pin<&mut Workspace>, project_id: i32, form: &QString) -> QString;
        /// Whether a project's folder can have a run's worktree: `{ state,
        /// files }`, state "ready", "no-commits" (a repository with nothing
        /// committed) or "no-git"; files, when it isn't ready, whether the
        /// folder holds anything (a run's branch would start without it).
        #[qinvokable]
        fn project_repo_state(self: &Workspace, project_id: i32) -> QString;
        /// The test command a project's workflow runs (last used, else guessed).
        #[qinvokable]
        fn workflow_test_command(self: &Workspace, project_id: i32) -> QString;
        /// The workflow last started in a project ("" none yet).
        #[qinvokable]
        fn workflow_choice(self: &Workspace, project_id: i32) -> QString;
        /// The workflows to pick from: the built-in one, then yours.
        #[qinvokable]
        fn workflow_definitions_json(self: &Workspace) -> QString;
        /// Saves one of your workflows (`id` "" a new one): `{ id, error }`.
        #[qinvokable]
        fn save_workflow_definition(self: Pin<&mut Workspace>, definition: &QString) -> QString;
        /// Deletes one of your workflows (runs keep their own copy).
        #[qinvokable]
        fn delete_workflow_definition(self: Pin<&mut Workspace>, id: &QString);
        /// The workflow run a session is part of, as its card shows it ("" none).
        #[qinvokable]
        fn workflow_for(self: &Workspace, key: &QString) -> QString;
        /// Every workflow run, newest first, as their cards show them.
        #[qinvokable]
        fn workflows_json(self: &Workspace) -> QString;
        /// Workflow runs under way (running, not paused): harlo keeps going
        /// without its window while there are any.
        #[qinvokable]
        fn workflows_under_way(self: &Workspace) -> i32;
        /// The window closed while runs are under way: harlo goes on without
        /// it, and says so (once).
        #[qinvokable]
        fn workflows_backgrounded(self: Pin<&mut Workspace>);
        /// Pauses, resumes or stops a workflow run (`action`: pause | resume | stop).
        #[qinvokable]
        fn workflow_action(self: Pin<&mut Workspace>, id: &QString, action: &QString);
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
        /// Saves a setting; a changed value bumps `settingsRevision`.
        #[qinvokable]
        fn set_setting(self: Pin<&mut Workspace>, key: &QString, value: &QString);
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
        /// Markdown as harlo draws a reply's: `[{ kind: "prose"|"code",
        /// html, code, lang }]` (a plan to approve, say).
        #[qinvokable]
        fn markdown_blocks(self: &Workspace, text: &QString) -> QString;
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
        /// What's on the clipboard to attach: its local files, or its image
        /// saved to a file. Empty when it's text.
        #[qinvokable]
        fn paste_files(self: &Workspace) -> QStringList;
        /// A provider's logo as a data URL in `color` (`#rrggbb`), or "".
        #[qinvokable]
        fn provider_logo(self: &Workspace, provider: &QString, color: &QString) -> QString;
        /// Who made a model (`provider/model`): its logo's id, or "".
        #[qinvokable]
        fn model_maker(self: &Workspace, model: &QString) -> QString;
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
        /// Full-text search over every session's messages, off the UI
        /// thread: the hits come with `messagesFound(generation, json)`, a
        /// JSON array of `{ path, index, role, snippet, title, harness, cwd,
        /// modified }` (the caller drops a generation that isn't its latest).
        #[qinvokable]
        fn find_messages(self: Pin<&mut Workspace>, query: &QString, limit: i32, generation: i32);
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
        /// Stops what a session is doing (it stays open).
        #[qinvokable]
        fn stop_session(self: &Workspace, key: &QString);
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
        /// Turns harlo's tools for agents on or off: off, sessions don't get
        /// them.
        #[qinvokable]
        fn set_orchestration_enabled(self: Pin<&mut Workspace>, on: bool);
        /// Starts a session from the new-session page and sends its first
        /// message. `form` is JSON: `{ projectId (-1 = home), harness ("" =
        /// the project's), worktreeBranch ("" = current checkout), base,
        /// text, attachments: [paths], approval ("" = the agent's), model }`.
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
        /// Settings → Agents: `[{ id, label, enabled, canChat, state
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
        /// findMessages' hits.
        #[qsignal]
        #[cxx_name = "messagesFound"]
        fn messages_found(self: Pin<&mut Workspace>, generation: i32, json: &QString);
        /// The message search index took in more sessions (a search open
        /// may have more hits now).
        #[qsignal]
        #[cxx_name = "searchIndexGrew"]
        fn search_index_grew(self: Pin<&mut Workspace>);
        /// Another machine's worktree, asked there (`worktree_info` gave its
        /// record first).
        #[qsignal]
        #[cxx_name = "worktreeInfoReady"]
        fn worktree_info_ready(self: Pin<&mut Workspace>, key: &QString, json: &QString);
        /// What wikiJson says was read again, and changed.
        #[qsignal]
        #[cxx_name = "wikiReady"]
        fn wiki_ready(self: Pin<&mut Workspace>);
        /// A page asked for with requestWikiPage (`wiki_view::page_view`),
        /// or `{ path, error }`.
        #[qsignal]
        #[cxx_name = "wikiPageReady"]
        fn wiki_page_ready(self: Pin<&mut Workspace>, page: &QString, json: &QString);
        /// A search's hits: `{ hits, workspaces }`.
        #[qsignal]
        #[cxx_name = "wikiSearchReady"]
        fn wiki_search_ready(self: Pin<&mut Workspace>, query: &QString, json: &QString);
        /// A declaration's card: `{ text }` or `{ error }`.
        #[qsignal]
        #[cxx_name = "wikiLookupReady"]
        fn wiki_lookup_ready(self: Pin<&mut Workspace>, name: &QString, path: &QString, json: &QString);
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
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QStringList, QVariant, QVector};
use harlo_core::db::{Db, Project, SessionMeta, WorktreeRecord};
use harlo_core::git;
use harlo_core::{agents, tool_calls};
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
/// A page of the wiki of the repository `cwd` is in, for reading (`.okf/x.md`
/// or `x.md`). Blocking: it works out what the page owes the code (on
/// another machine, there).
pub(crate) fn wiki_page_at(cwd: &Path, page: &str) -> serde_json::Value {
    harlo_core::wiki_view::page_view(cwd, page)
}

/// A search of that wiki's pages, by their words. Blocking.
pub(crate) fn wiki_search_at(cwd: &Path, query: &str, _workspace: &str) -> serde_json::Value {
    harlo_core::wiki_view::search(cwd, query, 12)
}

/// How long a project's sessions stay quiet before harlo checks a wiki kept
/// current as you work (`HARLO_TEST_WIKI_KEEP_MS` in tests).
fn wiki_keep_delay() -> Duration {
    std::env::var("HARLO_TEST_WIKI_KEEP_MS").ok().and_then(|v| v.parse().ok()).map_or(Duration::from_secs(90), Duration::from_millis)
}

/// What each folder's wiki was read as, and when (`wiki_json`'s, read in
/// the background).
type WikiRead = (Instant, Option<serde_json::Value>);
static WIKI_READS: OnceLock<Mutex<HashMap<PathBuf, WikiRead>>> = OnceLock::new();

/// Forgets what the wikis were read as (one just changed), so the next
/// look reads them again.
pub(crate) fn forget_wiki_reads() {
    WIKI_READS.get_or_init(Default::default).lock().unwrap().clear();
}

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
            e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > std::time::Duration::from_hours(24))
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
const ROLE_MODEL_NAME: i32 = 0x113;
const ROLE_MAKER: i32 = 0x114;

const SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// Remote hosts' session folders are listed this often (over SSH).
const REMOTE_SCAN_INTERVAL: Duration = Duration::from_secs(10);
/// A host that didn't answer is tried again after this, doubling up to
/// `REMOTE_RETRY_MAX` while it keeps not answering.
const REMOTE_RETRY: Duration = Duration::from_secs(60);
const REMOTE_RETRY_MAX: Duration = Duration::from_mins(30);
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

/// What a batch of a live session's updates changed, for acting on.
#[derive(Default)]
struct Changes {
    became_idle: bool,
    became_busy: bool,
    cost: Option<(f64, String)>,
    notes: Vec<Note>,
    /// A question of its was answered.
    answered: bool,
    /// Approvals the agent asked for with harlo's tools, answered.
    gates: Vec<(String, String, serde_json::Value)>,
    /// It's on another model than it was.
    model_moved: bool,
}

impl Changes {
    /// Applies `updates` to the session's entry, and says what they changed.
    fn of(entry: &mut LiveEntry, updates: Vec<SessionUpdate>) -> Self {
        let mut c = Changes::default();
        for u in updates {
            match u {
                SessionUpdate::Status(s) => {
                    c.became_idle |= entry.info.status == SessionStatus::Busy && s == SessionStatus::Idle;
                    c.became_busy |= entry.info.status != SessionStatus::Busy && s == SessionStatus::Busy;
                    entry.info.status = s;
                }
                SessionUpdate::Meta(m) => {
                    c.cost = Some((m.cost, m.cost_kind.clone()));
                    entry.info.session_file = m.session_file.or(entry.info.session_file.take());
                    entry.info.name = m.name;
                    entry.info.first_message = m.first_message;
                    // (Its model: in the sidebar's icon, and kept for when
                    // it isn't running.)
                    c.model_moved |= !m.model_id.is_empty() && m.model_id != entry.info.model;
                    entry.info.model.clone_from(&m.model_id);
                    entry.info.model_name.clone_from(&m.model);
                    entry.model = m.model;
                    entry.model_id = m.model_id;
                    entry.thinking = m.thinking;
                }
                SessionUpdate::UiRequest(r) => {
                    if !entry.requests.iter().any(|(q, _)| q.id == r.id) {
                        c.notes.push(Note::Question(r.question().to_owned(), crate::phone_server::request_json(&r)));
                        let asked = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
                        entry.requests.push((r, asked));
                    }
                }
                SessionUpdate::Notable(n) => c.notes.push(Note::Event(n)),
                SessionUpdate::UiCleared(id) => {
                    let before = entry.requests.len();
                    entry.requests.retain(|(q, _)| q.id != id);
                    c.answered |= entry.requests.len() != before;
                }
                SessionUpdate::GateAnswered { id, question, answer } => c.gates.push((id, question, answer)),
                _ => {}
            }
        }
        entry.info.needs_input = !entry.requests.is_empty();
        c
    }
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
    settings_revision: i32,
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
    wiki_offers_json: QString,
    /// Those offers, by project.
    wiki_offers: Vec<(i64, harlo_core::wiki_view::MergeOffer)>,
    /// Looking for them now.
    wiki_offer_checking: bool,
    /// Each wiki project's branches and HEAD as last looked at: when they
    /// move (a commit, here or elsewhere), an open Wiki panel reads how far
    /// behind it is again.
    wiki_heads: HashMap<i64, String>,
    /// Updates harlo recorded without a model (nothing stale), by wiki
    /// folder: the last update's time it wrote and the commits it looked
    /// at, so the panel can say so while that's the last update.
    wiki_quiet: HashMap<PathBuf, (String, u64)>,
    /// Wiki folders whose Update is checking what changed (no model yet).
    wiki_checking: HashSet<PathBuf>,
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
    orchestration_on: bool,
    /// Each started session's `HARLO_SESSION` token → its key (who calls
    /// harlo's tools).
    orch_tokens: HashMap<String, String>,
    /// Workflow runs, and the sessions they wait on.
    workflows: workflows::Workflows,
    workflows_revision: i32,
    /// Wiki runs that finished while you were looking at them (or before
    /// their file was known): settled once you're not.
    settle_later: HashSet<String>,
    /// harlo's tools each session's agent was given (their names), as it
    /// last listed them: an agent that lists them once, as it starts, is
    /// restarted when they change (a session a workflow takes on).
    listed: HashMap<String, Vec<String>>,
    /// Sessions harlo started to write a wiki: they get its check
    /// (wiki::CHECK).
    wiki_sessions: HashSet<String>,
    /// Projects keeping their wiki current as you work: the number of
    /// their latest turn (the check waits for a quiet spell after it).
    wiki_keep_turns: HashMap<i64, u64>,
    /// What harlo's check found when it last updated a repository's wiki
    /// on its own: not again until it finds something else.
    wiki_kept: HashMap<PathBuf, String>,
    /// Sessions harlo asked to send a diagram again, fixed: their next
    /// reply isn't checked (once, never a loop).
    diagram_asked: HashSet<String>,
    /// Remote files being copied here (`localCopy`).
    copying: std::cell::RefCell<HashSet<PathBuf>>,
    quota_loading: bool,
    /// When limits were last asked for (a turn on a provider with limits
    /// asks again, a minute apart at most).
    limits_asked: Option<std::time::Instant>,
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
    /// The index's second connection, for searches: they never wait for
    /// the indexer.
    search_reader: Option<Arc<std::sync::Mutex<harlo_core::search::SearchReader>>>,
    /// The search dialog's newest query (its generation): older ones still
    /// waiting for the reader aren't run.
    find_latest: Arc<std::sync::atomic::AtomicI32>,
    /// Wakes the search indexer (new or changed session files).
    search_kick: Arc<tokio::sync::Notify>,
    /// Remote project folders per host, for the remote session scan.
    remote_roots: Arc<std::sync::Mutex<harlo_core::remote::HostRoots>>,
    /// Wakes the remote session scan (a remote project was added).
    remote_kick: Arc<tokio::sync::Notify>,
    /// Restored tabs of remote sessions not found yet (their host is
    /// scanned after the local folders).
    pending_remote_tabs: Vec<PathBuf>,
    last_notification: QString,
    /// When each notification tag (event + session) last went out.
    notified_at: HashMap<String, std::time::Instant>,
    /// Sessions' approval modes and thinking levels, by live key.
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

/// A session's approval mode (None: the one it starts in) and thinking level.
#[derive(Default, Clone)]
struct SessionModes {
    approval: Option<String>,
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
            // (The user's answer to an approval request: to the agent.)
            | SessionUpdate::GateAnswered { .. }
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
        self.as_mut().load_saved();

        // The interface font for the whole app: the chosen one, else harlo's.
        crate::fonts::set_application_font_family(&QString::from(&crate::fonts::ui_family(&self.ui_font.to_string())));
        self.as_mut().rebuild();

        self.start_timers();

        self.count_changes();

        self.as_mut().keep_search_index();

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

        // Poll the session directories in the background (pi's when it's on).
        harlo_core::scanner::set_pi_listed(self.provider_enabled(HarnessKind::Pi));
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            let mut scanner = Scanner::default();
            let mut first = true;
            loop {
                let (s, delta) = tokio::task::spawn_blocking(move || {
                    let delta = scanner.poll();
                    (scanner, delta)
                })
                .await
                .expect("scanner task");
                scanner = s;
                // (The first look: workflow runs on this machine go on.)
                let listed = std::mem::take(&mut first);
                if (listed || !delta.is_empty())
                    && thread
                        .queue(move |mut this| {
                            this.as_mut().apply_scan(delta);
                            if listed {
                                this.workflows_listed("");
                            }
                        })
                        .is_err()
                {
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

        self.scan_hosts();
    }
}

impl Workspace {

    /// Looks for remote hosts' saved sessions over SSH, in the background.
    fn scan_hosts(&self) {
        // Remote hosts with projects: their saved sessions, over SSH.
        let thread = self.qt_thread();
        let (roots, kick) = (Arc::clone(&self.remote_roots), Arc::clone(&self.remote_kick));
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
                                tracing::event!(name: "remote.sessions.reachable", tracing::Level::DEBUG, server.address = %name, "remote sessions on {{server.address}}: reachable again");
                            }
                            let first = scanned.insert(name.clone());
                            if (first || !delta.is_empty())
                                && thread
                                    .queue(move |mut this| {
                                        this.as_mut().apply_scan(delta);
                                        // (Its workflow runs go on.)
                                        if first {
                                            this.workflows_listed(&name);
                                        }
                                    })
                                    .is_err()
                            {
                                return;
                            }
                        }
                        Err(e) => {
                            let failures = retry_at.get(&name).map_or(0, |(_, n)| *n) + 1;
                            // (Background work nobody asked for: the debug log only.)
                            if failures == 1 {
                                tracing::event!(name: "remote.sessions.unreachable", tracing::Level::DEBUG, server.address = %name, error.message = %format_args!("{e:#}"), "remote sessions on {{server.address}}: {{error.message}} (trying again later, less and less often)");
                            }
                            let wait = REMOTE_RETRY.saturating_mul(1 << (failures - 1).min(5)).min(REMOTE_RETRY_MAX);
                            retry_at.insert(name, (std::time::Instant::now() + wait, failures));
                        }
                    }
                }
                tokio::select! {
                    // Asked for (a host connected, its instance shown): now.
                    () = kick.notified() => {
                        for (t, _) in retry_at.values_mut() {
                            *t = std::time::Instant::now();
                        }
                    }
                    () = tokio::time::sleep(REMOTE_SCAN_INTERVAL) => {}
                }
            }
        });
    }

    /// Opens the message search index and keeps it current in the background.
    fn keep_search_index(mut self: Pin<&mut Self>) {
        // Keep the message search index current.
        match harlo_core::search::SearchIndex::open(&harlo_core::search::default_path()) {
            Ok(index) => {
                let index = Arc::new(std::sync::Mutex::new(index));
                self.as_mut().rust_mut().search = Some(Arc::clone(&index));
                match harlo_core::search::SearchReader::open(&harlo_core::search::default_path()) {
                    Ok(r) => self.as_mut().rust_mut().search_reader = Some(Arc::new(std::sync::Mutex::new(r))),
                    Err(e) => tracing::event!(name: "search.reader.unavailable", tracing::Level::WARN, error.message = %format_args!("{e:#}"), "search reader unavailable: {{error.message}}"),
                }
                let s_thread = self.qt_thread();
                let kick = Arc::clone(&self.search_kick);
                crate::runtime().spawn(async move {
                    loop {
                        // Wait for the first scan (or a later change) before reading files.
                        tokio::select! {
                            () = kick.notified() => {}
                            () = tokio::time::sleep(SEARCH_INTERVAL) => {}
                        }
                        // Every session the sidebar knows, here or on a remote
                        // host, with its stamp (modified, size) and agent.
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        if s_thread
                            .queue(move |this| {
                                let list: Vec<(PathBuf, i64, i64, HarnessKind)> = this
                                    .sessions
                                    .values()
                                    .map(|s| (s.path.clone(), s.modified.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX)), i64::try_from(s.size).unwrap_or(i64::MAX), s.harness))
                                    .collect();
                                let _ = tx.send(list);
                            })
                            .is_err()
                        {
                            break;
                        }
                        let Ok(list) = rx.await else { break };
                        let index = Arc::clone(&index);
                        let grew = s_thread.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            let files: Vec<(PathBuf, i64, i64)> = list.into_iter().map(|(p, m, s, _)| (p, m, s)).collect();
                            // (Tests: a slow disk or host, each file taking this long.)
                            let slow = std::env::var("HARLO_TEST_SLOW_INDEX_MS").ok().and_then(|v| v.parse::<u64>().ok()).map(Duration::from_millis);
                            let read = |p: &Path, resume: Option<&harlo_harness::store::Resume>| {
                                if let Some(d) = slow {
                                    std::thread::sleep(d);
                                }
                                harlo_core::search::read_session(p, resume)
                            };
                            let told = || {
                                let _ = grew.queue(qobject::Workspace::search_index_grew);
                            };
                            if let Err(e) = index.lock().unwrap().update_batched(&files, read, told) {
                                tracing::event!(name: "search.index.failed", tracing::Level::WARN, error.message = %format_args!("{e:#}"), "search index: {{error.message}}");
                            }
                        })
                        .await;
                    }
                });
            }
            Err(e) => tracing::event!(name: "search.index.unavailable", tracing::Level::WARN, error.message = %format_args!("{e:#}"), "search index unavailable: {{error.message}}"),
        }
    }

    /// Counts uncommitted changes in listed sessions' repositories, in the background.
    fn count_changes(&self) {
        // Uncommitted changes in the repositories of listed sessions.
        let ch_thread = self.qt_thread();
        let kick = Arc::clone(&self.change_kick);
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
                    () = tokio::time::sleep(CHANGES_INTERVAL) => {}
                    () = kick.notified() => {}
                }
                // Let bursts of triggers settle into one count.
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        });
    }

    /// Starts the checks that run now and then, and takes later launches' commands.
    fn start_timers(&self) {
        // Worktrees can also be made outside harlo; look for them now and then.
        let wt_thread = self.qt_thread();
        crate::runtime().spawn(async move {
            loop {
                if wt_thread.queue(qobject::Workspace::refresh_worktrees).is_err() {
                    break;
                }
                tokio::time::sleep(WORKTREE_INTERVAL).await;
            }
        });

        // Offers to update a wiki, as merged work lands: now and then.
        let offer_thread = self.qt_thread();
        crate::runtime().spawn(async move {
            tokio::time::sleep(Duration::from_secs(20)).await;
            loop {
                if offer_thread.queue(qobject::Workspace::check_wiki_offers).is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        });

        // The Settled shelf: a minute after starting, then every half hour.
        let settle_thread = self.qt_thread();
        crate::runtime().spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            loop {
                if settle_thread.queue(qobject::Workspace::settle_sessions).is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_mins(30)).await;
            }
        });

        // Commands from later launches (`harlo`, `harlo --toggle-dropdown`).
        let r_thread = self.qt_thread();
        crate::instance::set_handler(move |cmd| {
            let _ = r_thread.queue(move |this| this.remote_command(&QString::from(&cmd)));
        });
    }

    /// Reads what harlo saved: projects, tabs, settings, worktrees, schedules.
    fn load_saved(mut self: Pin<&mut Self>) {
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
                publish_phone_limits(&this.quota_json);
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
                *this.remote_roots.lock().unwrap() = harlo_core::remote::roots_by_host(projects.iter().map(|p| &p.path));
                this.projects = projects;
                this.meta = meta;
                this.other_collapsed = other_collapsed;
                this.workflows = workflows::Workflows::load(&db);
                this.db = Some(db);
            }
            Err(e) => {
                self.as_mut().set_theme_family(QString::from(crate::themes::DEFAULT_FAMILY));
                self.as_mut().set_theme_mode(QString::from("dark"));
                self.as_mut().set_text_scale(1.0);
                self.as_mut().set_glass(true);
                self.as_mut().set_glass_opacity(GLASS_OPACITY);
                self.as_mut().set_error_text(QString::from(&format!("Could not open the database: {e:#}")));
            }
        }
    }
}

/// A machine's home folder ("" this one's).
fn home_on(inst: &str) -> PathBuf {
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

/// The shield's mode a session starts in, before it's switched: ask first
/// (Claude's own settings may differ).
const STARTING_APPROVAL: &str = "always-ask";

#[expect(clippy::unused_self, reason = "QML calls these as methods of their object, so they take self")]
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
        let projects = self.db().map(harlo_core::db::Db::projects).transpose();
        if let Some(projects) = self.as_mut().report(projects) {
            self.as_mut().rust_mut().projects = projects.unwrap_or_default();
        }
        let n = self.projects_revision + 1;
        self.as_mut().set_projects_revision(n);
        let roots = harlo_core::remote::roots_by_host(self.projects.iter().map(|p| &p.path));
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

    pub fn create_project_folder(&self, parent: &QString, name: &QString) -> QString {
        let parent = PathBuf::from(url_to_path(&parent.to_string()));
        let answer = match git::new_repo_folder(&parent, &name.to_string()) {
            Ok(path) => json!({ "path": path.to_string_lossy() }),
            Err(e) => json!({ "error": format!("{e:#}") }),
        };
        QString::from(&answer.to_string())
    }

    pub fn new_project_parent(&self) -> QString {
        let latest = self
            .db()
            .and_then(|db| db.projects().ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|p| !harlo_core::remote::is_remote(&p.path))
            .max_by_key(|p| p.id)
            .and_then(|p| p.path.parent().map(Path::to_path_buf))
            .filter(|d| d.is_dir());
        let home = || PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()));
        QString::from(&latest.unwrap_or_else(home).to_string_lossy().into_owned())
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
            // What the host has; pi when nothing's known.
            Some(host) => {
                harlo_core::remote::AGENTS
                    .iter()
                    .find(|a| harlo_core::remote::found_on(host, a) && HarnessKind::parse(a).is_some_and(|k| self.usable_harness(k)))
                    .map_or("pi", |a| *a)
                    .to_owned()
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
        let r = self.db().map(|db| db.remove_project(i64::from(id))).transpose();
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
        self.move_project(id, i32::try_from(before).unwrap_or(i32::MAX));
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
        let r = self.db().map(|db| db.set_project_order(&ids).and_then(|()| db.set(PLACED, "1"))).transpose();
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
        let list = self.sessions_of_project(i64::from(id));
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
        let list = self.sessions_of_project(i64::from(id));
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
                        let done = harlo_core::remote::trash(&file).map_err(|e| anyhow::anyhow!("{e:#}"));
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
        // (On until turned off in Settings → Agents.)
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
        if kind == HarnessKind::Pi {
            harlo_core::scanner::set_pi_listed(on);
            // (Off, it wasn't checked: check it now.)
            if on {
                self.as_mut().refresh_providers();
            }
        }
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
    /// turned on (Settings → Agents); on a remote host, those also found
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
            let Some(p) = self.projects.iter().find(|p| p.id == i64::from(id)) else { return };
            p.path.file_name().map_or_else(|| p.path.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned())
        } else {
            name
        };
        let r = self.db().map(|db| db.set_project_name(i64::from(id), &name)).transpose();
        self.as_mut().report(r);
        self.reload_projects();
    }

    pub fn set_project_look(mut self: Pin<&mut Self>, id: i32, icon: &QString, color: &QString) {
        let (icon, color) = (icon.to_string(), color.to_string());
        if !icon.is_empty() && !harlo_core::project_icons::PRESETS.contains(&icon.as_str()) {
            return;
        }
        let r = self.db().map(|db| db.set_project_look(i64::from(id), &icon, &color)).transpose();
        self.as_mut().report(r);
        self.reload_projects();
    }

    pub fn set_project_harness(mut self: Pin<&mut Self>, id: i32, harness: &QString) {
        let harness = harness.to_string();
        if !HarnessKind::parse(&harness).is_some_and(harlo_harness::HarnessKind::can_chat) {
            return;
        }
        let r = self.db().map(|db| db.set_project_harness(i64::from(id), &harness)).transpose();
        self.as_mut().report(r);
        self.reload_projects();
    }

    pub fn toggle_collapsed(mut self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        let shelf = key.strip_prefix("shelf:").and_then(|s| s.parse::<i64>().ok());
        let wiki_shelf = key.strip_prefix("wikishelf:").and_then(|s| s.parse::<i64>().ok()).map(|id| id + sidebar::WIKI_SHELF);
        if let Some(id) = shelf.or(wiki_shelf) {
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
        self.as_mut().rust_mut().expanded.push(i64::from(project_id));
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
            let token = agents::session_token();
            let cfg = HarnessConfig {
                kind: info.harness,
                program,
                cwd: info.cwd.clone(),
                session: SessionTarget::Resume(info.path.clone()),
                extra_args: Vec::new(),
                // (The agent's harlo tools know who's calling.)
                env: self.session_env(info.harness, &token),
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
        let project = self.projects.iter().find(|p| p.id == i64::from(project_id)).cloned();
        let harness = if harness.is_empty() {
            project.as_ref().map_or_else(|| self.default_harness(), |p| p.harness.clone())
        } else {
            harness.to_owned()
        };
        let Some(kind) = HarnessKind::parse(&harness) else { return String::new() };
        let cwd = project.map_or_else(|| self.instance_home(), |p| p.path);
        self.start_session_with(kind, cwd, model, true)
    }

    /// Spawns a session; `model` (`provider/model`) overrides the harness's
    /// default; `activate` makes it the active tab.
    fn start_session_with(mut self: Pin<&mut Self>, kind: HarnessKind, cwd: PathBuf, model: &str, activate: bool) -> String {
        let (program, remote) = self.launch_for(kind, &cwd);
        let token = agents::session_token();
        let cfg = HarnessConfig {
            kind,
            program,
            cwd: cwd.clone(),
            // `--ephemeral` (testing): new sessions are not saved.
            session: if std::env::args().any(|a| a == "--ephemeral") { SessionTarget::Ephemeral } else { SessionTarget::New },
            extra_args: if !model.trim().is_empty() && kind != HarnessKind::Demo { vec!["--model".into(), model.trim().to_owned()] } else { Vec::new() },
            // (The agent's harlo tools know who's calling.)
            env: self.session_env(kind, &token),
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
            model: String::new(),
            model_name: String::new(),
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
        // A wiki run that finished while you looked at it: settled as you go.
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
            self.as_mut().settle_wiki_run(&previous);
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
            self.as_mut().rust_mut().tab_keys[i].clone_from(&key);
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

    /// The folder a wiki action is for: a session's, or a project's
    /// (`project:<id>`), with its project.
    fn wiki_folder(&self, key: &str) -> Option<(PathBuf, i64)> {
        match key.strip_prefix("project:").and_then(|id| id.parse::<i64>().ok()) {
            Some(id) => self.projects.iter().find(|p| p.id == id).map(|p| (p.path.clone(), id)),
            None => self.project_of(key),
        }
    }

    pub fn wiki_json(&self, key: &QString) -> QString {
        let key = key.to_string();
        let Some((cwd, project)) = self.wiki_folder(&key) else {
            return QString::from(&json!({ "root": "", "project": -1, "enabled": false, "exists": false, "pages": [] }).to_string());
        };
        let enabled = self.wiki_on(project);
        let writer = self.wiki_writing(&cwd);
        let running = writer.is_some();
        let writer = writer.unwrap_or_default();
        let with = |mut v: serde_json::Value, loading: bool| {
            v["project"] = json!(project);
            v["enabled"] = json!(enabled);
            v["running"] = json!(running);
            v["session"] = json!(writer);
            v["loading"] = json!(loading);
            v["host"] = json!(harlo_core::remote::host_of(&cwd).unwrap_or_default());
            v["checking"] = json!(self.wiki_checking.contains(&cwd));
            // (When harlo last checked and found nothing owed, with no model.)
            if let Some((at, _)) = self.wiki_quiet.get(&cwd) {
                v["recorded"] = json!(at);
            }
            QString::from(&v.to_string())
        };
        // What was read of it, read again in the background when it's a
        // few seconds old (the work list reads every file in scope).
        let reads = WIKI_READS.get_or_init(Default::default);
        // (Another machine's less often: each read is a trip over SSH.)
        let max_age = Duration::from_secs(if harlo_core::remote::is_remote(&cwd) { 15 } else { 3 });
        let (fresh, cached) = reads.lock().unwrap().get(&cwd).map_or((false, None), |(at, v)| (at.elapsed() < max_age, v.clone()));
        if !fresh {
            reads.lock().unwrap().insert(cwd.clone(), (Instant::now(), cached.clone()));
            let (thread, cwd) = (self.qt_thread(), cwd.clone());
            crate::runtime().spawn_blocking(move || {
                let found = harlo_core::wiki_view::state(&cwd);
                let changed = reads.lock().unwrap().insert(cwd, (Instant::now(), Some(found.clone()))).and_then(|(_, v)| v).as_ref() != Some(&found);
                if changed {
                    let _ = thread.queue(|mut this| this.as_mut().wiki_ready());
                }
            });
        }
        let loading = cached.is_none();
        with(cached.unwrap_or_else(|| json!({ "root": cwd.to_string_lossy(), "exists": !harlo_core::remote::is_remote(&cwd) && harlo_core::wiki::has_wiki(&cwd), "pages": [] })), loading)
    }

    pub fn refresh_wiki(&self) {
        forget_wiki_reads();
    }

    pub fn request_wiki_page(&self, key: &QString, page: &QString) {
        let page = page.to_string();
        let Some((cwd, _)) = self.wiki_folder(&key.to_string()) else { return };
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let v = wiki_page_at(&cwd, &page);
            let _ = thread.queue(move |mut this| this.as_mut().wiki_page_ready(&QString::from(&page), &QString::from(&v.to_string())));
        });
    }

    pub fn request_wiki_search(&self, key: &QString, query: &QString, workspace: &QString) {
        let (query, workspace) = (query.to_string(), workspace.to_string());
        let Some((cwd, _)) = self.wiki_folder(&key.to_string()) else { return };
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let v = wiki_search_at(&cwd, &query, &workspace);
            let _ = thread.queue(move |mut this| this.as_mut().wiki_search_ready(&QString::from(&query), &QString::from(&v.to_string())));
        });
    }

    pub fn request_wiki_lookup(&self, key: &QString, name: &QString, path: &QString) {
        let (name, path) = (name.to_string(), path.to_string());
        let Some((cwd, _)) = self.wiki_folder(&key.to_string()) else { return };
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let v = harlo_core::wiki_view::lookup(&cwd, &name, &path);
            let _ = thread.queue(move |mut this| this.as_mut().wiki_lookup_ready(&QString::from(&name), &QString::from(&path), &QString::from(&v.to_string())));
        });
    }

    /// The model a session started for a project gets when none is picked:
    /// the project's own for that agent (Settings → Projects), else the
    /// agent's default from Settings (not on another machine: its agent's
    /// own there); "" the agent's own.
    fn model_for(&self, project: Option<i64>, kind: HarnessKind, cwd: &Path) -> String {
        let own = project.map(|p| self.setting_str(&format!("project_model_{p}_{}", kind.as_str()))).unwrap_or_default();
        if !own.is_empty() || harlo_core::remote::host_of(cwd).is_some() {
            return own;
        }
        self.setting_str(&format!("default_model_{}", kind.as_str()))
    }

    /// Who writes a project's wiki: Settings → Wiki's agent, model and
    /// thinking for every project, else the project's own (its agent, and
    /// the model a new session of it gets). On another machine its model
    /// stays the project's: the one chosen here may not be there.
    /// Who writes a project's wiki for `op` ("create": the first, whole
    /// repository; "update": the pages whose code changed), from Settings →
    /// Wiki (`wiki_writer`: `{ create: { agent, model, thinking }, update:
    /// … }`), else the project's agent and model.
    fn wiki_writer(&self, project: i64, cwd: &Path, op: &str) -> Option<(HarnessKind, String, String)> {
        let set: serde_json::Value = serde_json::from_str(&self.setting_str("wiki_writer")).unwrap_or_default();
        let field = |k: &str| set[op][k].as_str().unwrap_or_default().to_owned();
        let chosen = HarnessKind::parse(&field("agent")).filter(|k| self.usable_harness(*k));
        let kind = match chosen {
            Some(k) => k,
            None => HarnessKind::parse(&self.projects.iter().find(|p| p.id == project).map(|p| p.harness.clone()).filter(|h| !h.is_empty()).unwrap_or_else(|| self.default_harness()))?,
        };
        // (Another machine's project: its own model; this one's may not be there.)
        let model = match field("model") {
            m if chosen.is_some() && !m.is_empty() && harlo_core::remote::host_of(cwd).is_none() => m,
            _ => self.model_for(Some(project), kind, cwd),
        };
        Some((kind, model, if chosen.is_some() { field("thinking") } else { String::new() }))
    }

    /// Who writes the wiki of a session's repository (as wiki_json's key):
    /// `{ create: { agent, model, thinking }, update: … }`, each {} when no
    /// agent can.
    pub fn wiki_writer_json(&self, key: &QString) -> QString {
        let folder = self.wiki_folder(&key.to_string());
        let one = |op: &str| {
            folder.as_ref().and_then(|(cwd, project)| self.wiki_writer(*project, cwd, op)).map_or_else(|| json!({}), |(kind, model, thinking)| json!({ "agent": kind.as_str(), "model": model, "thinking": thinking }))
        };
        QString::from(&json!({ "create": one("create"), "update": one("update") }).to_string())
    }

    pub fn wiki_writer_changed(self: Pin<&mut Self>) {
        self.wiki_ready();
    }

    pub fn start_wiki_run(self: Pin<&mut Self>, key: &QString, mode: &QString) -> QString {
        let Some((cwd, project)) = self.wiki_folder(&key.to_string()) else {
            return QString::from("The wiki is for a harlo project's sessions.");
        };
        self.run_wiki(cwd, project, mode.to_string(), false)
    }

    /// A wiki session at work in the repository at `cwd` (busy, or asking).
    fn wiki_writing(&self, cwd: &Path) -> Option<String> {
        self.wiki_sessions
            .iter()
            .find(|k| {
                self.live.get(*k).is_some_and(|e| {
                    (e.info.cwd.starts_with(cwd) || cwd.starts_with(&e.info.cwd))
                        && (matches!(e.info.status, harlo_core::session::SessionStatus::Busy | harlo_core::session::SessionStatus::Starting) || e.info.needs_input)
                })
            })
            .cloned()
    }

    /// After a turn in a project that keeps its wiki current as you work
    /// (`wiki_keep_<id>`, off unless switched on): once its sessions have
    /// been quiet for a while, harlo's check, and an update in a session in
    /// the background when the code has moved past the wiki.
    fn keep_wiki_current(mut self: Pin<&mut Self>, key: &str) {
        let Some((cwd, project)) = self.wiki_folder(key) else { return };
        if !self.wiki_on(project) || self.setting_str(&format!("wiki_keep_{project}")) != "1" {
            return;
        }
        let turn = self.wiki_keep_turns.get(&project).copied().unwrap_or_default() + 1;
        self.as_mut().rust_mut().wiki_keep_turns.insert(project, turn);
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            tokio::time::sleep(wiki_keep_delay()).await;
            let _ = thread.queue(move |this| {
                // (Another turn since, or a session in it still at work:
                // the check waits for that one's end.)
                let busy = this.live.values().any(|e| {
                    e.info.cwd.starts_with(&cwd) && matches!(e.info.status, harlo_core::session::SessionStatus::Busy | harlo_core::session::SessionStatus::Starting)
                });
                if this.wiki_keep_turns.get(&project) == Some(&turn) && !busy && this.wiki_writing(&cwd).is_none() {
                    let _ = this.run_wiki(cwd, project, "update".to_owned(), true);
                }
            });
        });
    }

    /// Starts a wiki run: harlo's check first, with no model, then a
    /// session when there's something to do. `mode` is "create", "update"
    /// or "page:<path>"; `kept` is an update harlo starts on its own (in the
    /// background, never a first wiki, and not twice for the same
    /// findings). Why it can't start ("" when it does, or needn't).
    fn run_wiki(mut self: Pin<&mut Self>, cwd: PathBuf, project: i64, mode: String, kept: bool) -> QString {
        let (Some(create), Some(update)) = (self.wiki_writer(project, &cwd, "create"), self.wiki_writer(project, &cwd, "update")) else {
            return QString::from("Pick a default agent first (Settings → General).");
        };
        let thread = self.qt_thread();
        if !self.as_mut().rust_mut().wiki_checking.insert(cwd.clone()) {
            return QString::default();
        }
        self.as_mut().wiki_ready();
        crate::runtime().spawn_blocking(move || {
            let root = harlo_core::wiki::root_of(&cwd);
            let done = |thread: &CxxQtThread<Workspace>, cwd: PathBuf, quiet: Option<String>| {
                let _ = thread.queue(move |mut this| {
                    this.as_mut().rust_mut().wiki_checking.remove(&cwd);
                    if let Some(at) = quiet {
                        this.as_mut().rust_mut().wiki_quiet.insert(cwd, (at, 0));
                    }
                    this.as_mut().wiki_ready();
                });
            };
            // First a check with no model: it records what needs none, and
            // when nothing's owed an update needs no session.
            let outcome = match harlo_core::wiki::writer_check(&root) {
                Ok(o) => o,
                Err(e) => {
                    done(&thread, cwd, None);
                    let text = format!("The wiki couldn't be checked: {e:#}");
                    let _ = thread.queue(move |mut this| this.as_mut().set_notice_text(&text, true));
                    return;
                }
            };
            forget_wiki_reads();
            let fresh = outcome.pages == 0;
            if kept && fresh {
                done(&thread, cwd, None);
                return;
            }
            let page = mode.strip_prefix("page:").map(str::to_owned);
            let mode = if page.is_some() { mode.clone() } else if mode == "create" || fresh { "create".to_owned() } else { "update".to_owned() };
            if mode == "update" && outcome.current {
                done(&thread, cwd, Some(chrono::Utc::now().to_rfc3339()));
                return;
            }
            done(&thread, cwd, None);
            let report = match &page {
                Some(p) => harlo_core::wiki_view::page_report(&root, p),
                None => outcome.text,
            };
            let prompt = harlo_core::wiki::run_prompt(&mode, &report);
            let _ = thread.queue(move |mut this| {
                if kept {
                    // (Not again for what it found last time: the session
                    // left it, or you will.)
                    if this.wiki_kept.get(&root).is_some_and(|r| *r == report) || this.wiki_writing(&root).is_some() {
                        return;
                    }
                    this.as_mut().rust_mut().wiki_kept.insert(root.clone(), report);
                    this.as_mut().set_notice_text("The code moved past the wiki: a session is updating it in the background.", false);
                }
                let (kind, model, thinking) = if mode == "create" { create } else { update };
                let title = if mode == "create" { "Wiki: create" } else if page.is_some() { "Wiki: check a page" } else { "Wiki: update" };
                let key = this.as_mut().start_session_with(kind, root, &model, !kept);
                this.as_mut().rust_mut().wiki_sessions.insert(key.clone());
                this.as_mut().apply_thinking(&key, &thinking);
                if let Some(live) = registry::get(&key) {
                    live.handle.send_raw(json!({ "type": "set_session_name", "name": title }));
                }
                // (It writes pages in .okf/: its writes go ahead and land in
                // Changes; commands still ask.)
                if agents::has_shield(kind.as_str()) {
                    this.as_mut().set_approval_mode(&QString::from(&key), &QString::from("write"));
                }
                send_first_prompt(&key, FirstPrompt { text: prompt, ..Default::default() }, Some(this.qt_thread()));
            });
        });
        QString::default()
    }

    /// Looks for merged work a project's wiki should take in (projects
    /// on this computer with the wiki on): an offer for each, until it's
    /// taken or declined or the default branch moves again.
    pub fn check_wiki_offers(mut self: Pin<&mut Self>) {
        if self.wiki_offer_checking {
            return;
        }
        let projects: Vec<(i64, PathBuf, String)> = self
            .projects
            .iter()
            .filter(|p| !harlo_core::remote::is_remote(&p.path) && self.wiki_on(p.id))
            .map(|p| (p.id, p.path.clone(), self.setting_str(&format!("wiki_seen_{}", p.id))))
            .collect();
        if projects.is_empty() && self.wiki_offers.is_empty() {
            return;
        }
        self.as_mut().rust_mut().wiki_offer_checking = true;
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let found: Vec<(i64, String, Option<harlo_core::wiki_view::MergeOffer>, String)> = projects
                .into_iter()
                .map(|(id, path, seen)| {
                    let root = harlo_core::wiki::root_of(&path);
                    let (tip, offer) = harlo_core::wiki_view::merge_offer(&root, &seen);
                    // (Branch tips: a worktree's commits move its branch here too.)
                    let heads = harlo_core::git::output_in(&root, &["for-each-ref", "--format=%(objectname)", "refs/heads"])
                        + &harlo_core::git::output_in(&root, &["rev-parse", "HEAD"]);
                    (id, tip, offer, heads)
                })
                .collect();
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().wiki_offer_checking = false;
                let mut moved = false;
                for (id, _, _, heads) in &found {
                    if let Some(before) = this.as_mut().rust_mut().wiki_heads.insert(*id, heads.clone()) {
                        moved |= before != *heads;
                    }
                }
                if moved {
                    this.as_mut().wiki_ready();
                }
                let mut offers: Vec<(i64, harlo_core::wiki_view::MergeOffer)> = Vec::new();
                for (id, tip, offer, _) in found {
                    match offer {
                        Some(o) => offers.push((id, o)),
                        // (Nothing to offer at this tip: seen.)
                        None if !tip.is_empty() => this.save_setting(&format!("wiki_seen_{id}"), &tip),
                        None => {}
                    }
                }
                this.as_mut().set_wiki_offers(offers);
            });
        });
    }

    fn set_wiki_offers(mut self: Pin<&mut Self>, offers: Vec<(i64, harlo_core::wiki_view::MergeOffer)>) {
        let list: Vec<serde_json::Value> = offers
            .iter()
            .map(|(id, o)| {
                let name = self.projects.iter().find(|p| p.id == *id).map(|p| p.name.clone()).unwrap_or_default();
                json!({ "project": id, "name": name, "branch": o.branch, "commits": o.commits, "owed": o.owed, "new": o.new })
            })
            .collect();
        self.as_mut().rust_mut().wiki_offers = offers;
        self.as_mut().set_wiki_offers_json(QString::from(&serde_json::Value::Array(list).to_string()));
    }

    /// An offer gone (taken or declined): its tip is seen.
    fn settle_wiki_offer(self: Pin<&mut Self>, project: i64) {
        let mut offers = self.wiki_offers.clone();
        if let Some(i) = offers.iter().position(|(id, _)| *id == project) {
            let (_, o) = offers.remove(i);
            self.save_setting(&format!("wiki_seen_{project}"), &o.tip);
        }
        self.set_wiki_offers(offers);
    }

    pub fn mark_wiki_reviewed(self: Pin<&mut Self>, key: &QString, page: &QString) {
        let page = page.to_string();
        let Some((cwd, _)) = self.wiki_folder(&key.to_string()) else { return };
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let said = harlo_core::wiki::mark_reviewed(&cwd, &page).map_err(|e| format!("{e:#}"));
            forget_wiki_reads();
            let _ = thread.queue(move |mut this| {
                match said {
                    Ok(why) if why.is_empty() => {}
                    Ok(why) => this.as_mut().set_notice_text(&why, true),
                    Err(e) => this.as_mut().set_notice_text(&format!("The page couldn't be marked reviewed: {e}"), true),
                }
                this.as_mut().wiki_ready();
            });
        });
    }

    pub fn take_wiki_offer(mut self: Pin<&mut Self>, project_id: i32) -> QString {
        let project = i64::from(project_id);
        self.as_mut().settle_wiki_offer(project);
        self.start_wiki_run(&QString::from(&format!("project:{project}")), &QString::from("update"))
    }

    pub fn decline_wiki_offer(self: Pin<&mut Self>, project_id: i32) {
        self.settle_wiki_offer(i64::from(project_id));
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

    #[expect(clippy::cast_possible_truncation, reason = "whole pixels, sizes and times from floats: `as` rounds toward zero and saturates, as wanted here")]
    pub fn resume_at_reset(self: Pin<&mut Self>, key: &QString, at: f64) {
        let id = self.resume_id(&key.to_string());
        let mut resumes = harlo_core::quota::resumes(&self.limit_resumes.to_string());
        resumes.insert(id, json!(at as i64));
        self.store_resumes(&resumes);
    }

    pub fn cancel_resume(self: Pin<&mut Self>, key: &QString) {
        let (key, id) = (key.to_string(), self.resume_id(&key.to_string()));
        let mut resumes = harlo_core::quota::resumes(&self.limit_resumes.to_string());
        // (Asked before it had a file: under its key.)
        if resumes.remove(&id).is_some() | resumes.remove(&key).is_some() {
            self.store_resumes(&resumes);
        }
    }

    pub fn resume_at_for(&self, key: &QString) -> f64 {
        let (key, resumes) = (key.to_string(), harlo_core::quota::resumes(&self.limit_resumes.to_string()));
        resumes.get(&self.resume_id(&key)).or_else(|| resumes.get(&key)).and_then(serde_json::Value::as_i64).unwrap_or(0) as f64
    }

    /// The sessions whose limit has reset get their message.
    fn continue_after_limits(mut self: Pin<&mut Self>) {
        let mut resumes = harlo_core::quota::resumes(&self.limit_resumes.to_string());
        let due = harlo_core::quota::due_resumes(&resumes, chrono::Local::now().timestamp_millis());
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
                tracing::event!(name: "app.keep_above.failed", tracing::Level::DEBUG, window.title = ?title, error.message = %e, "keeping {{window.title}} on top: {{error.message}}");
            }
        });
    }

    pub fn check_remote_sessions(self: Pin<&mut Self>) {
        self.remote_kick.notify_one();
    }

    pub fn working_on_host(&self, name: &QString) -> i32 {
        let name = name.to_string();
        i32::try_from(self.live
            .values()
            .filter(|e| harlo_core::remote::host_of(&e.info.cwd).as_deref() == Some(name.as_str()))
            .filter(|e| e.info.needs_input || matches!(e.info.status, SessionStatus::Busy | SessionStatus::Starting))
            .count()).unwrap_or(i32::MAX)
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
        home_on(&self.instance.to_string())
    }


    pub fn draft_folder(&self, project_id: i32) -> QString {
        let inst = self.instance.to_string();
        let path = match self.projects.iter().find(|p| p.id == i64::from(project_id)) {
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
        crate::runtime().spawn(async move {
            let answer = match harlo_core::providers::host_models(kind, remote, cwd).await {
                Ok(list) => serde_json::json!({ "models": list.into_iter().map(|(id, name, local)| serde_json::json!({ "id": id, "name": name, "local": local })).collect::<Vec<_>>() }),
                Err(e) if e.to_string() == "harness exited before responding" => serde_json::json!({ "error": "it quit before answering, which usually means it has no login there yet." }),
                Err(e) => serde_json::json!({ "error": format!("{e:#}") }),
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
        let key = format!("diff:{}:{path}", u8::from(staged));
        if let Some(i) = self.tab_keys.iter().position(|k| k.starts_with("diff:")) {
            self.as_mut().rust_mut().tab_keys[i].clone_from(&key);
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
        let n = i32::try_from(self.tab_keys.len()).unwrap_or(i32::MAX);
        if n == 0 {
            return;
        }
        let current = self.tab_keys.iter().position(|k| *k == self.active_key.to_string()).map_or(0, |i| i32::try_from(i).unwrap_or(i32::MAX));
        let index = (current + delta).rem_euclid(n);
        self.activate_tab(index);
    }

    pub fn activate_tab(mut self: Pin<&mut Self>, index: i32) {
        if let Some(key) = self.tab_keys.get(usize::try_from(index).unwrap_or(0)).cloned() {
            self.as_mut().activate(&key);
        }
    }

    pub fn move_tab(mut self: Pin<&mut Self>, from: i32, to: i32) {
        let n = self.tab_keys.len();
        let (from, to) = (usize::try_from(from).unwrap_or(0), usize::try_from(to).unwrap_or(0));
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
                let title = match view {
                    "stats" => "Usage",
                    "settings" => "Settings",
                    "new" => "New session",
                    "workflow" => "New workflow",
                    other => other,
                };
                list.append(QString::from(&format!("{key}\t{title}\t\tview\t")));
                continue;
            }
            if let Some(path) = key.strip_prefix("file:") {
                let name = Path::new(path).file_name().map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned());
                list.append(QString::from(&format!("{key}\t{}\t\tfile\t", name.replace(['\t', '\n'], " "))));
                continue;
            }
            if let Some((staged, path)) = diff_key(key) {
                let name = Path::new(path).file_name().map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned());
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
        if let Some(p) = self.projects.iter().find(|p| p.id == i64::from(project_id)) {
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
            return i32::try_from(w.project_id).unwrap_or(i32::MAX);
        }
        self.projects
            .iter()
            .filter(|p| cwd.starts_with(&p.path))
            .max_by_key(|p| p.path.components().count())
            .map_or(-1, |p| i32::try_from(p.id).unwrap_or(i32::MAX))
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
        let unix = |t: SystemTime| t.duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
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
        for i in order.into_iter().take(usize::try_from(limit.max(1)).unwrap_or(usize::MAX)) {
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
        meta.last_viewed_at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
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
            (_, Some("workflow")) => "New workflow".to_owned(),
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

    pub fn archive_shelf(mut self: Pin<&mut Self>, project_id: i32, wiki: bool) {
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
            shelves_open: &[id, id + sidebar::WIKI_SHELF],
        });
        // The rows on that shelf (Settled, or Wiki).
        let files: Vec<PathBuf> = rows
            .iter()
            .filter(|r| r.kind == SidebarKind::Session && !r.settled.is_empty() && (r.settled == "wiki") == wiki)
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

    /// What a session's project has on: `(cwd, wiki)` (sessions in a
    /// harlo project only).
    fn features_of(&self, key: &str) -> Option<(PathBuf, bool)> {
        let (cwd, project) = self.project_of(key)?;
        Some((cwd, self.wiki_on(project)))
    }

    /// A project's agents get the wiki's map with its note: its own Wiki map
    /// setting ("1" on, "0" off), else Settings → Wiki's (on unless "0").
    fn wiki_map_on(&self, project: i64) -> bool {
        match self.setting_str(&format!("wiki_map_{project}")).as_str() {
            "1" => true,
            "0" => false,
            _ => self.setting_str("wiki_map") != "0",
        }
    }

    /// The environment of a session: its `token` for harlo's tools, and
    /// pi's sandbox as set.
    fn session_env(&self, kind: HarnessKind, token: &str) -> Vec<(String, String)> {
        let mut env = vec![("HARLO_SESSION".to_owned(), token.to_owned())];
        // (pi: harlo's sandbox, as Settings has it, through harlo's
        // extension and harlo-helper.)
        if kind == HarnessKind::Pi {
            let mode = match (self.setting_str("harlo_sandbox") != "0", self.setting_str("harlo_sandbox_net") != "0") {
                (false, _) => "off",
                (true, false) => "no-network",
                (true, true) => "on",
            };
            env.push(("HARLO_SANDBOX".to_owned(), mode.to_owned()));
        }
        env
    }

    /// What harlo knows of a session calling its tools (None: one it doesn't).
    /// Whether the agent of session `key` was given other tools than harlo
    /// offers it now (false when it hasn't listed them).
    pub(crate) fn tools_changed(&self, key: &str) -> bool {
        let Some(listed) = self.listed.get(key) else { return false };
        let facts = self.caller_facts(Some(key));
        let now: Vec<String> = tool_calls::offered(&facts).iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect();
        &now != listed
    }

    fn caller_facts(&self, caller: Option<&str>) -> tool_calls::Caller {
        let Some(c) = caller else { return tool_calls::Caller::default() };
        tool_calls::Caller {
            harness: self.live.get(c).map(|e| e.info.harness.clone()).unwrap_or_default(),
            // (A workflow's: it reports to its run.)
            helper: self.serves_workflow(c),
            writing: self.wiki_sessions.contains(c),
            project: self.features_of(c).map(|(cwd, wiki)| {
                let map = self.project_of(c).is_some_and(|(_, p)| self.wiki_map_on(p));
                tool_calls::Project { cwd, wiki, map }
            }),
        }
    }

    /// A project's agents use its wiki (when it has one) unless its Wiki
    /// setting is switched off.
    fn wiki_on(&self, project: i64) -> bool {
        self.setting_str(&format!("wiki_{project}")) != "0"
    }

    /// A session's folder, and the project it's in (its worktree's).
    fn project_of(&self, key: &str) -> Option<(PathBuf, i64)> {
        let cwd = self.live.get(key).map(|e| e.info.cwd.clone()).or_else(|| self.sessions.get(Path::new(key)).map(|s| s.cwd.clone()))?;
        let project = self.project_at(&cwd)?;
        Some((cwd, project))
    }

    /// The project a folder is in (its worktree's).
    fn project_at(&self, cwd: &Path) -> Option<i64> {
        self.worktrees
            .iter()
            .filter(|w| cwd.starts_with(&w.path))
            .max_by_key(|w| w.path.components().count())
            .map(|w| w.project_id)
            .or_else(|| self.projects.iter().filter(|p| cwd.starts_with(&p.path)).max_by_key(|p| p.path.components().count()).map(|p| p.id))
    }

    /// Sets a new session's thinking level ("": leaves it).
    fn apply_thinking(&self, key: &str, level: &str) {
        let level = level.trim();
        if level.is_empty() {
            return;
        }
        if let Some(live) = registry::get(key) {
            live.handle.send_raw(json!({ "type": "set_thinking_level", "level": level }));
        }
    }

    pub fn session_title(&self, key: &QString) -> QString {
        QString::from(self.title_of(&key.to_string()).as_str())
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
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
        let unix = |t: SystemTime| t.duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
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
        let mut wiki_runs: Vec<(PathBuf, i64)> = Vec::new();
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
            // A wiki run from before harlo restarted (named by harlo).
            if matches!(s.name.as_str(), "Wiki: create" | "Wiki: update" | "Wiki: continue") {
                wiki_runs.push((s.path.clone(), modified));
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
        for (file, modified) in wiki_runs {
            let mut meta = self.meta.get(&file).cloned().unwrap_or_default();
            meta.settled_why = "wiki".into();
            meta.settled_at = modified;
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
                    why.clone_into(&mut meta.settled_why);
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
        self.rows.get(usize::try_from(index).unwrap_or(0)).map(|r| QString::from(&r.key)).unwrap_or_default()
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
        if let Some(Ok(list)) = self.db().map(harlo_core::db::Db::worktrees) {
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
            let scan = harlo_core::db::scan_worktrees(projects);
            let _ = thread.queue(move |mut this| {
                let mut changed = false;
                if let Some(db) = this.db() {
                    for rec in scan.changed(&this.worktrees) {
                        let _ = db.upsert_worktree(rec);
                        changed = true;
                    }
                    for w in scan.gone(&this.worktrees) {
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
        let project = self.projects.iter().find(|p| p.id == i64::from(project_id))?.clone();
        let root = git::repo_root(&project.path)?;
        Some((project, root))
    }

    pub fn request_branches(&self, project_id: i32) {
        let Some(path) = self.projects.iter().find(|p| p.id == i64::from(project_id)).map(|p| p.path.clone()) else { return };
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
        let n = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() as usize ^ usize::try_from(d.as_secs()).unwrap_or(usize::MAX));
        QString::from(&format!("harlo/{}-{}", ADJ[n % ADJ.len()], NOUN[(n / ADJ.len()) % NOUN.len()]))
    }

    pub fn new_worktree_session(self: Pin<&mut Self>, project_id: i32, branch: &QString, base: &QString, harness: &QString) {
        self.create_worktree_session(project_id, branch, base, harness, WorktreeStart::default());
    }

    pub fn start_draft(mut self: Pin<&mut Self>, form: &QString) -> QString {
        let form: serde_json::Value = serde_json::from_str(&form.to_string()).unwrap_or_default();
        let field = |k: &str| form[k].as_str().unwrap_or_default().to_owned();
        let mut project_id = i32::try_from(form["projectId"].as_i64().unwrap_or(-1)).unwrap_or(i32::MAX);
        // (The page says what it shows: never a project of another machine.)
        if let Some(p) = self.projects.iter().find(|p| p.id == i64::from(project_id))
            && !self.here(&p.path)
        {
            tracing::event!(
                name: "session.new.wrong_machine", tracing::Level::WARN,
                file.path = %p.path.display(),
                instance = %self.instance,
                "new session: project {{file.path}} isn't on the machine shown ({{instance}})",
            );
            project_id = -1;
        }
        let (harness, worktree_branch, base, text) = (field("harness"), field("worktreeBranch"), field("base"), field("text"));
        // The model picked ("" the agent's own).
        let model = field("model");
        // The shield's mode to start in ("" the agent's own).
        let approval = Some(field("approval")).filter(|m| harlo_core::session::APPROVAL_MODES.contains(&m.as_str()));
        let attachments: Vec<String> = form["attachments"].as_array().map(|a| a.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect()).unwrap_or_default();
        let message = text.trim().to_owned();
        let harness_name = if harness.is_empty() {
            self.projects.iter().find(|p| p.id == i64::from(project_id)).map_or_else(|| self.default_harness(), |p| p.harness.clone())
        } else {
            harness.clone()
        };
        let server = Some((harness_name.clone(), model.clone()));
        let first = FirstPrompt { text: message, attachments: attachments.iter().map(|p| PathBuf::from(url_to_path(p))).collect(), server };
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
        let WorktreeStart { first, model, thinking, approval } = opts;
        let Some((project, root)) = self.project_root(project_id) else {
            self.as_mut().set_notice_text("This project is not a git repository.", true);
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
                // No base named: the branch checked out now.
                let bs = if bs.trim().is_empty() { git::head_branch(&root) } else { bs };
                git::create_worktree(&root, &path, &b, &bs)?;
                // (Where it starts.)
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
                    let key = this.as_mut().start_session_with(kind, path.clone(), &model, true);
                    if let Some(mode) = approval {
                        this.as_mut().set_approval_mode(&QString::from(&key), &QString::from(&mode));
                    }
                    this.as_mut().apply_thinking(&key, &thinking);
                    if let Some(first) = first {
                        send_first_prompt(&key, first, Some(this.qt_thread()));
                    }
                    this.as_mut().set_notice_text(&format!("Worktree {branch} ready"), false);
                    this.run_setup(path, &setup);
                }
                Err(e) => {
                    this.as_mut().set_notice_text(&format!("Could not create the worktree: {e:#}"), true);
                }
            },
        );
    }

    /// Runs a project's setup commands in a new worktree, one per line.
    fn run_setup(self: Pin<&mut Self>, dir: PathBuf, setup: &str) {
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
                        Some((host, p)) => harlo_core::remote::run(host, &format!("cd {} && {cmd}", harlo_core::remote::quote(p)), Duration::from_mins(30)).map_err(|e| std::io::Error::other(format!("{e:#}"))),
                        None => std::process::Command::new("sh").arg("-c").arg(cmd).current_dir(&dir).output(),
                    };
                    match out {
                        Ok(o) if o.status.success() => {}
                        Ok(o) => {
                            let err = String::from_utf8_lossy(&o.stderr);
                            return Err(anyhow::anyhow!("Setup command failed: {cmd}\n{}", err.trim().lines().last().unwrap_or("")));
                        }
                        Err(e) => return Err(anyhow::anyhow!("Setup command failed: {cmd}: {e}")),
                    }
                }
                Ok(commands.len())
            },
            |this, result| match result {
                Ok(n) => this.set_notice_text(&format!("Worktree setup finished ({n} command{})", if n == 1 { "" } else { "s" }), false),
                Err(e) => this.set_notice_text(&format!("{e:#}"), true),
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
                let info = w2.details(project.as_deref(), !w2.removed);
                let _ = thread.queue(move |mut this| this.as_mut().worktree_info_ready(&QString::from(&k), &QString::from(&info.to_string())));
            });
            return QString::from(
                &json!({ "path": w.path.to_string_lossy(), "branch": w.branch, "base": w.base, "mainBranch": "", "exists": !w.removed,
                         "dirty": 0, "ahead": 0, "behind": 0, "loading": true })
                .to_string(),
            );
        }
        QString::from(&w.details(project.as_deref(), w.path.exists() && !w.removed).to_string())
    }

    pub fn merge_worktree(mut self: Pin<&mut Self>, key: &QString) {
        let Some(w) = self.worktree_for_key(&key.to_string()) else { return };
        // (Its repository found in the background: maybe another machine's.)
        let Some(project) = self.projects.iter().find(|p| p.id == w.project_id).map(|p| p.path.clone()) else { return };
        self.as_mut().set_notice_text(&format!("Merging {}…", w.branch), false);
        let branch = w.branch.clone();
        self.background(
            move || {
                let main = git::repo_root(&project).ok_or_else(|| anyhow::Error::msg("The project isn't a git repository."))?;
                git::merge_branch(&main, &branch).map_err(|e| anyhow::anyhow!("{e:#}"))
            },
            |this, result| match result {
                Ok(msg) => this.set_notice_text(&msg, false),
                Err(e) => this.set_notice_text(&format!("{e:#}"), true),
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
            move || -> anyhow::Result<String> {
                let main = git::repo_root(&project).ok_or_else(|| anyhow::Error::msg("The project isn't a git repository."))?;
                git::remove_worktree(&main, &rec.path, force).map_err(|e| anyhow::anyhow!("{e:#}"))?;
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
                    Err(e) => this.as_mut().set_notice_text(&format!("{e:#}"), true),
                }
                this.reload_worktrees();
            },
        );
    }

    pub fn project_setup(&self, project_id: i32) -> QString {
        self.projects.iter().find(|p| p.id == i64::from(project_id)).map(|p| QString::from(&p.setup)).unwrap_or_default()
    }

    pub fn set_project_setup(mut self: Pin<&mut Self>, project_id: i32, setup: &QString) {
        let r = self.db().map(|db| db.set_project_setup(i64::from(project_id), &setup.to_string())).transpose();
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

    pub fn set_setting(mut self: Pin<&mut Self>, key: &QString, value: &QString) {
        let (key, value) = (key.to_string(), value.to_string());
        let value = value.trim();
        let changed = self.setting_str(&key) != value;
        self.save_setting(&key, value);
        if key == "debug_protocol" {
            crate::diagnostics::set_protocol_logging(value == "1");
        }
        if changed {
            let n = self.settings_revision.wrapping_add(1);
            self.as_mut().set_settings_revision(n);
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
            let result = live.handle.export_markdown_to(path.clone()).await;
            let _ = thread.queue(move |this| match result {
                Ok(()) => this.set_notice_text(&format!("Saved as {}", sidebar::tilde(&path)), false),
                Err(e) => this.set_notice_text(&format!("Couldn't save it: {e}"), true),
            });
        });
    }

    pub fn markdown_blocks(&self, text: &QString) -> QString {
        let blocks = harlo_core::markdown::MarkdownRenderer::default().render(&text.to_string());
        let out: Vec<serde_json::Value> = blocks
            .iter()
            .map(|b| json!({ "kind": if b.kind == harlo_core::markdown::MdBlockKind::Code { "code" } else { "prose" }, "html": b.html, "code": b.code, "lang": b.lang }))
            .collect();
        QString::from(&serde_json::Value::Array(out).to_string())
    }

    pub fn copy_markdown(mut self: Pin<&mut Self>, key: &QString) {
        let Some(live) = self.as_mut().make_live(&key.to_string()).and_then(|k| registry::get(&k)) else { return };
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            let md = match live.handle.export_markdown().await {
                Ok(md) => md,
                Err(e) => return drop(thread.queue(move |this| this.set_notice_text(&format!("Couldn't copy it: {e}."), true))),
            };
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
        self.as_mut().rust_mut().ci_watch.retain(|(_, since)| since.elapsed() < Duration::from_hours(6));
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
                    let Some(checks) = checks.filter(harlo_core::git_hosting::Checks::settled) else { continue };
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
        };
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
        // Sessions whose costs the limits can't count (all or some).
        let unpriced = self.spend.values().filter(|s| matches!(s.kind.as_str(), "unknown" | "partly")).count();
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
        let text = json!({ "limits": limits.to_json(), "today": self.spend_ledger.total, "day": day.unwrap_or_default(), "held": held, "costs": costs, "unpriced": unpriced }).to_string();
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
        let form: serde_json::Value = serde_json::from_str(&json.to_string()).unwrap_or_default();
        let id = form["id"].as_i64().unwrap_or(0);
        let previous = self.schedules.iter().find(|s| s.id == id);
        let mut schedule = match harlo_core::schedules::Schedule::from_form(&form, previous, &self.instance.to_string(), chrono::Local::now()) {
            Ok(schedule) => schedule,
            Err(what) => return QString::from(what),
        };
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
            let _ = db.delete_schedule(i64::from(id));
        }
        self.as_mut().rust_mut().schedules.retain(|s| s.id != i64::from(id));
        self.bump_schedules();
    }

    pub fn set_schedule_enabled(mut self: Pin<&mut Self>, id: i32, on: bool) {
        let Some(mut s) = self.schedules.iter().find(|s| s.id == i64::from(id)).cloned() else { return };
        s.enabled = on;
        s.reschedule(chrono::Local::now());
        self.as_mut().store_schedule(s);
    }

    pub fn run_schedule_now(mut self: Pin<&mut Self>, id: i32) {
        let Some(mut s) = self.schedules.iter().find(|s| s.id == i64::from(id)).cloned() else { return };
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
        let harness = if s.harness.is_empty() {
            project.as_ref().map_or_else(|| self.default_harness(), |p| p.harness.clone())
        } else {
            s.harness.clone()
        };
        let Some(kind) = HarnessKind::parse(&harness) else { return };
        // (Without a project: the home folder of the machine it was made on.)
        let cwd = project.map_or_else(|| home_on(&s.host), |p| p.path);
        tracing::event!(name: "schedule.prompt.start", tracing::Level::INFO, schedule.title = %s.title, "scheduled prompt {{schedule.title}} starts");
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

    /// Plan limits: the Claude account's (Claude Code's own `/usage`), for
    /// pi's Claude subscription. The last numbers stay when it can't be asked.
    pub fn refresh_quota(self: Pin<&mut Self>) {
        self.refresh_limits(true);
    }

    /// [`Self::refresh_quota`]; `claude` false keeps Claude's numbers as they
    /// are (a running session keeps them current).
    fn refresh_limits(mut self: Pin<&mut Self>, ask_claude: bool) {
        if self.quota_loading {
            return;
        }
        self.as_mut().rust_mut().limits_asked = Some(std::time::Instant::now());
        // Claude's: the account pi's Claude subscription uses, while it's on.
        let claude = (self.setting_str(harlo_core::pi_bridge::SETTING) != "0")
            .then(harlo_core::claude::program)
            .flatten()
            .zip(harlo_core::claude::config_dir());
        let keep_claude = !ask_claude && claude.is_some();
        let claude = claude.filter(|_| ask_claude);
        if claude.is_none() && !keep_claude {
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
            let _ = thread.queue(move |mut this| {
                this.as_mut().rust_mut().quota_loading = false;
                let previous: serde_json::Value = serde_json::from_str(&this.quota_json.to_string()).unwrap_or_default();
                let json = harlo_core::quota::refreshed(&previous, from_claude, keep_claude, chrono::Local::now().timestamp_millis());
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

    /// The meter follows the machine shown: what was used last there.
    fn show_meter_of(mut self: Pin<&mut Self>, host: &str) {
        let last = self.setting_str(&meter_setting(host));
        if last != self.meter_last.to_string() {
            self.as_mut().set_meter_last(QString::from(&last));
        }
    }

    pub fn check_limits(mut self: Pin<&mut Self>) {
        let now = chrono::Local::now().timestamp_millis();
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
        if !agents::has_shield(&entry.info.harness) {
            return QString::default();
        }
        if let Some(mode) = self.modes.get(&key).and_then(|m| m.approval.clone()) {
            return QString::from(&mode);
        }
        QString::from(STARTING_APPROVAL)
    }

    pub fn draft_approval(&self, harness: &QString, _project_id: i32) -> QString {
        if !agents::has_shield(&harness.to_string()) {
            return QString::default();
        }
        QString::from(STARTING_APPROVAL)
    }

    pub fn set_approval_mode(mut self: Pin<&mut Self>, key: &QString, mode: &QString) {
        let (key, mode) = (key.to_string(), mode.to_string());
        if !harlo_core::session::APPROVAL_MODES.contains(&mode.as_str()) {
            return;
        }
        self.as_mut().rust_mut().modes.entry(key.clone()).or_default().approval = Some(mode.clone());
        self.as_mut().modes_changed(&key);
        self.as_mut().remember_modes(&key);
    }

    /// Keeps a session's permission mode and thinking level with its file (a
    /// new session's once it has one).
    /// Keeps the model a session is on with its file, for its icon when it
    /// isn't running.
    fn remember_model(mut self: Pin<&mut Self>, key: &str) {
        let Some(entry) = self.live.get(key) else { return };
        let (Some(file), model, name) = (entry.info.session_file.clone(), entry.info.model.clone(), entry.info.model_name.clone()) else { return };
        let mut meta = self.meta.get(&file).cloned().unwrap_or_default();
        if model.is_empty() || (meta.model == model && meta.model_name == name) {
            return;
        }
        (meta.model, meta.model_name) = (model, name);
        if let Some(db) = self.db() {
            let _ = db.set_session_meta(&file, &meta);
        }
        self.as_mut().rust_mut().meta.insert(file, meta);
    }

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

    /// Gives the session its modes and tells views.
    fn modes_changed(mut self: Pin<&mut Self>, key: &str) {
        // pi takes the modes itself (harlo's extension).
        if self.live.get(key).is_some_and(|e| e.info.harness == "pi")
            && let Some(live) = registry::get(key)
        {
            let modes = self.modes.get(key).cloned().unwrap_or_default();
            live.handle.send_raw(serde_json::json!({ "type": "harlo_set_modes", "approval": modes.approval.unwrap_or_default() }));
        }
        let next = self.modes_revision + 1;
        self.as_mut().set_modes_revision(next);
    }

    pub fn model_maker(&self, model: &QString) -> QString {
        QString::from(crate::provider_logos::maker_of(&model.to_string()).unwrap_or_default())
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
                    tracing::event!(name: "file.copy.failed", tracing::Level::WARN, file.path = %p.display(), error.message = %format_args!("{e:#}"), "copying {{file.path}}: {{error.message}}");
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

    pub fn running_sessions(&self) -> QString {
        // Each machine's under its name (this computer's first) once there's
        // more than one; the sidebar shows only one machine's titles.
        let mut list: Vec<(String, u8, serde_json::Value)> = self
            .live
            .iter()
            .filter(|(_, e)| e.info.status != SessionStatus::Exited)
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
                move || -> anyhow::Result<(PathBuf, String)> {
                    let copy = harlo_core::remote::local_copy(&original).map_err(|e| anyhow::anyhow!("{e:#}"))?;
                    let text = std::fs::read_to_string(copy)?;
                    let (name, body) = harlo_harness::branches::branch_file(&text, &leaf, &harlo_harness::on_host(&original))?;
                    let dest = original.with_file_name(name);
                    harlo_core::remote::write(&dest, body.as_bytes()).map_err(|e| anyhow::anyhow!("{e:#}"))?;
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
                    "modified": i.modified.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
                })
            })
            .collect();
        QString::from(&serde_json::Value::Array(list).to_string())
    }

    /// The message search's reader, for a search off the UI thread.
    pub(crate) fn session_search(&self) -> Option<Arc<std::sync::Mutex<harlo_core::search::SearchReader>>> {
        self.search_reader.clone()
    }

    /// `list_sessions`' answer from its search's `hits` (done off this
    /// thread): `cwd`'s sessions (or all, `everywhere`), not the caller's
    /// own (`own`), each with a couple of its matches.
    pub(crate) fn found_sessions(&self, hits: &[harlo_core::search::Hit], everywhere: bool, cwd: Option<&Path>, own: Option<&Path>) -> serde_json::Value {
        use serde_json::json;
        let mut order: Vec<PathBuf> = Vec::new();
        let mut passages: HashMap<PathBuf, Vec<String>> = HashMap::new();
        for h in hits {
            let Some(info) = self.sessions.get(&h.path) else { continue };
            if !everywhere && cwd.is_some_and(|c| c != info.cwd) {
                continue;
            }
            if own == Some(h.path.as_path()) {
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
        json!({ "sessions": found, "note": if found.is_empty() { "No past session matches." } else { "read_session gives one's latest messages." } })
    }

    pub fn find_messages(self: Pin<&mut Self>, query: &QString, limit: i32, generation: i32) {
        let Some(reader) = self.search_reader.clone() else {
            self.messages_found(generation, &QString::from("[]"));
            return;
        };
        let (query, limit) = (query.to_string(), usize::try_from(limit.clamp(1, 500)).unwrap_or(0));
        let latest = Arc::clone(&self.find_latest);
        latest.store(generation, std::sync::atomic::Ordering::SeqCst);
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            // (Typed on since: a newer one answers instead.)
            let current = move || latest.load(std::sync::atomic::Ordering::SeqCst) == generation;
            let hits = tokio::task::spawn_blocking(move || {
                let reader = reader.lock().unwrap();
                current().then(|| reader.search(&query, limit).unwrap_or_default())
            })
            .await
            .ok()
            .flatten();
            let Some(hits) = hits else { return };
            let _ = thread.queue(move |mut this| {
                let json = this.hits_json(hits);
                this.as_mut().messages_found(generation, &QString::from(&json));
            });
        });
    }

    /// Hits as the search shows them, with their sessions' titles.
    fn hits_json(&self, hits: Vec<harlo_core::search::Hit>) -> String {
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
                    "modified": info.map_or(0, |i| i.modified.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())),
                })
            })
            .collect();
        serde_json::Value::Array(list).to_string()
    }

    pub fn recent_sessions(&self, limit: i32) -> QString {
        let mut list: Vec<_> = self.sessions.values().collect();
        list.sort_by_key(|a| std::cmp::Reverse(a.modified));
        let out: Vec<serde_json::Value> = list
            .into_iter()
            .take(usize::try_from(limit.clamp(1, 50)).unwrap_or(0))
            .map(|i| {
                serde_json::json!({
                    "path": i.path.to_string_lossy(),
                    "title": i.display_title(),
                    "harness": i.harness.as_str(),
                    "cwd": harlo_core::sidebar::tilde(&i.cwd),
                    "modified": i.modified.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
                })
            })
            .collect();
        QString::from(&serde_json::Value::Array(out).to_string())
    }

    pub fn home_dir(&self) -> QString {
        QString::from(&std::env::var("HOME").unwrap_or_else(|_| "/".into()))
    }

    pub fn default_worktree_dir(&self) -> QString {
        QString::from(&harlo_core::db::worktrees_dir().to_string_lossy().into_owned())
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
    /// the setting, else the first on and installed (pi when none is).
    fn default_harness(&self) -> String {
        let set = self.setting_str("default_harness");
        if let Some(kind) = HarnessKind::parse(&set)
            && set != "demo"
            && self.usable_harness(kind)
        {
            return set;
        }
        // The first one on and installed, in Settings → Agents' order.
        let usable: Vec<HarnessKind> = HarnessKind::PROVIDERS.iter().copied().filter(|&k| self.usable_harness(k)).collect();
        usable
            .iter()
            .find(|&&k| self.program_for(k).is_some() || harlo_harness::resolve_program(k).is_some())
            .or(usable.first())
            .map_or("pi", |k| k.as_str())
            .into()
    }

    /// Where new worktrees go: the setting, else harlo's data folder.
    fn worktree_base(&self) -> PathBuf {
        let set = self.setting_str("worktree_dir");
        if set.is_empty() { harlo_core::db::worktrees_dir() } else { PathBuf::from(url_to_path(&set)) }
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
    fn push_to_phones(&self, key: &str, note: &Note, quiet: harlo_core::notifications::Quiet) {
        use crate::phone_push::What;
        use harlo_core::Notable;
        let (what, body) = match note {
            Note::Event(Notable::Completed { last_message, .. }) => (What::Done, last_message.clone()),
            Note::Event(Notable::Error { message }) => (What::Failed, message.clone()),
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
        // A workflow's reviewer (saved under the session it reviews): the
        // run tells you its end; it only notifies when it waits for you.
        let under_another = entry.and_then(|e| e.info.session_file.as_ref()).and_then(|f| self.meta.get(f)).is_some_and(|m| !m.parent.is_empty());
        if under_another && !matches!(note, Note::Question(..)) {
            return;
        }
        self.push_to_phones(key, &note, settings.quiet);
        let model = entry.map(|e| e.model.clone()).unwrap_or_default();
        let agent = entry.map_or_else(|| "pi".to_owned(), |e| e.info.harness.clone());
        // event, last message, fallback title and body
        let (event, last, fallback) = match note {
            Note::Event(Notable::Completed { last_message, .. }) => ("completion", last_message, (format!("{agent} is ready"), format!("{} completed the task", if model.is_empty() { "The agent".to_owned() } else { model.clone() }))),
            Note::Event(Notable::Error { message }) => ("error", message, ("Tool error".to_owned(), "An error occurred".to_owned())),
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
        let project = row.as_ref().and_then(|r| self.projects.iter().find(|p| p.id == r.project_id)).map_or_else(|| cwd.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), |p| p.name.clone());
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

    /// A live session's file became known: what waited on it.
    fn on_file_known(mut self: Pin<&mut Self>, key: &str) {
        self.as_mut().workflow_file_known(key);
        self.as_mut().remember_modes(key);
        // (A wiki run that finished before its file was known.)
        if self.settle_later.contains(key) && self.active_key.to_string() != key {
            self.as_mut().settle_wiki_run(key);
        }
    }

    fn on_live_updates(mut self: Pin<&mut Self>, key: &str, updates: Vec<SessionUpdate>) {
        let active = self.active_key.to_string() == key;
        let mut this = self.as_mut().rust_mut();
        let Some(entry) = this.live.get_mut(key) else { return };
        let file_before = entry.info.session_file.clone();
        let Changes { became_idle, became_busy, cost, notes, answered, gates, model_moved } = Changes::of(entry, updates);
        let file_changed = entry.info.session_file != file_before;
        // Whose limits it's using: Claude's sign-in (pi's Claude
        // subscription); other providers say no limits.
        let limited_by = Some(harlo_core::quota::session_provider(&entry.info.harness, &entry.model_id)).filter(|p| !p.is_empty());
        // (The machine it runs on: its meter.)
        let on_host = harlo_core::remote::host_of(&entry.info.cwd).unwrap_or_default();
        let used = limited_by.clone().filter(|_| became_busy);
        if became_idle && !active {
            entry.info.unread = true;
        }
        if became_idle {
            // The agent may have edited files.
            this.change_kick.notify_one();
        }
        if file_changed || model_moved {
            self.as_mut().remember_model(key);
        }
        if file_changed {
            self.as_mut().on_file_known(key);
        }
        // A wiki run whose turn ended: onto the Wiki shelf (once you've
        // left it, when you're looking at it).
        if became_idle && self.wiki_sessions.contains(key) {
            self.as_mut().settle_wiki_run(key);
        }
        // Any other session's: its project may keep its wiki current.
        if became_idle && !self.wiki_sessions.contains(key) {
            self.as_mut().keep_wiki_current(key);
        }
        // What it cost since the last report: the day's too.
        if let Some((cost, kind)) = cost {
            self.as_mut().add_cost(key, cost, kind);
        }
        if let Some(used) = used.filter(|u| *u != self.setting_str(&meter_setting(&on_host))) {
            self.save_setting(&meter_setting(&on_host), &used);
            if on_host == self.instance.to_string() {
                self.as_mut().set_meter_last(QString::from(&used));
            }
        }
        self.as_mut().rebuild();
        // A workflow's session asking you something, or answered: its
        // run's card says so.
        if (answered || notes.iter().any(|n| matches!(n, Note::Question(..)))) && self.workflows.keys.contains_key(key) {
            self.as_mut().workflows_changed();
        }
        // A workflow's session: its report to the run.
        if became_idle {
            self.as_mut().workflow_turn_ended(key, &notes);
        }
        // The user's answers to its approval requests: to the agent.
        self.as_mut().answer_gates(key, gates);
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

    /// Adds what a session cost since its last report, to it and the day.
    fn add_cost(mut self: Pin<&mut Self>, key: &str, cost: f64, kind: String) {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut this = self.as_mut().rust_mut();
        let spend = this.spend.entry(key.to_owned()).or_default();
        spend.kind = kind;
        let added = spend.observe(cost);
        if added > 0.0 {
            this.spend_ledger.add(&today, added);
            self.save_setting("spend_today", &self.spend_ledger.to_json().to_string());
        }
        self.as_mut().refresh_spending();
    }

    /// Takes the user's answers to a session's approval requests to its agent.
    fn answer_gates(&self, key: &str, gates: Vec<(String, String, serde_json::Value)>) {
        for (_, question, answer) in gates {
            let message = harlo_core::orchestration::approval_answer(&question, &answer);
            if let Some(live) = registry::get(key) {
                let busy = self.live.get(key).is_some_and(|e| e.info.status == SessionStatus::Busy);
                live.handle.prompt(&message, Vec::new(), if busy { harlo_harness::SendMode::FollowUp } else { harlo_harness::SendMode::Normal });
            }
        }
    }

    /// A finished reply's diagrams, options and mockups, checked off this
    /// thread: when one can't be shown (a diagram that doesn't parse, a
    /// mockup of no file), the agent is told what's wrong and sends it
    /// again, once; not when you've written to it since.
    fn check_diagrams(mut self: Pin<&mut Self>, key: &str, reply: &str) {
        if self.as_mut().rust_mut().diagram_asked.remove(key) || !self.orchestration_on || !["```", "~~~"].iter().any(|f| reply.contains(f)) {
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
    #[expect(clippy::cast_possible_truncation, reason = "whole pixels, sizes and times from floats: `as` rounds toward zero and saturates, as wanted here")]
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
            "projects" => self.phone_projects(),
            "new" => self.as_mut().phone_new(msg),
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
            "resumeAtReset" => self.as_mut().phone_resume_at_reset(msg),
            "cancelResume" => {
                self.as_mut().cancel_resume(&QString::from(&key));
                json!({ "ok": true })
            }
            // A running session's agent, model (`provider/id`, "" not known
            // yet) and machine ("" this one), for its model's server.
            // A phone connected a machine (see `phone_server`): its
            // sessions and limits, and what it can start a session with.
            "hostConnected" => self.as_mut().phone_host_connected(msg),
            "serverOf" => match self.live.get(&key) {
                Some(e) => json!({ "agent": e.info.harness, "model": e.model_id,
                                   "host": harlo_core::remote::host_of(&e.info.cwd).unwrap_or_default() }),
                None => fail("That session isn't running."),
            },
            "send" => self.phone_send(msg),
            "modes" => self.phone_modes(&key),
            "setModes" => self.as_mut().phone_set_modes(msg),
            "rename" => self.as_mut().phone_rename(msg),
            "archive" => self.as_mut().phone_archive(msg),
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
            // The wiki of the session's repository: its pages and how fresh
            // it is (a page and a search are read off this thread), and a
            // run started from the phone, as from the Wiki panel.
            "wiki" => serde_json::from_str(&self.wiki_json(&QString::from(&key)).to_string()).unwrap_or_default(),
            "wikiFolder" => match self.wiki_folder(&key) {
                Some((cwd, _)) => json!({ "cwd": cwd.to_string_lossy() }),
                None => fail("The wiki is for a harlo project's sessions."),
            },
            "wikiRun" => self.as_mut().phone_wiki_run(msg),
            "wikiChanged" => {
                forget_wiki_reads();
                self.as_mut().wiki_ready();
                json!({ "ok": true })
            }
            // Workflow runs: a session's run (as its card shows it), every
            // run, and Pause/Resume/Stop/Merge/Remove from the phone.
            "workflow" | "workflows" | "workflowAction" => self.as_mut().phone_workflow(msg),
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

    /// Archives a session for a phone, or takes it out of the archive.
    fn phone_archive(mut self: Pin<&mut Self>, msg: &serde_json::Value) -> serde_json::Value {
        use serde_json::json;
        let key = msg["key"].as_str().unwrap_or_default().to_owned();
        let fail = |e: &str| serde_json::json!({ "error": e });
        if self.file_for_key(&key).is_none() {
            return fail("This session isn't saved yet.");
        }
        let on = msg["archived"].as_bool().unwrap_or(true);
        self.as_mut().update_meta(&key, |m| m.archived = on);
        json!({ "ok": true })
    }

    /// Renames a session for a phone.
    fn phone_rename(mut self: Pin<&mut Self>, msg: &serde_json::Value) -> serde_json::Value {
        use serde_json::json;
        let text = |k: &str| msg[k].as_str().unwrap_or_default().to_owned();
        let key = text("key");
        let fail = |e: &str| serde_json::json!({ "error": e });
        let name = text("name");
        let Some(live_key) = self.as_mut().make_live(&key) else { return fail("That session isn't available.") };
        if let Some(live) = registry::get(&live_key) {
            live.handle.send_raw(json!({ "type": "set_session_name", "name": name.trim() }));
        }
        json!({ "ok": true })
    }

    /// A session's approval mode, as a phone set it.
    fn phone_set_modes(mut self: Pin<&mut Self>, msg: &serde_json::Value) -> serde_json::Value {
        let key = msg["key"].as_str().unwrap_or_default().to_owned();
        if let Some(mode) = msg["approval"].as_str() {
            self.as_mut().set_approval_mode(&QString::from(&key), &QString::from(mode));
        }
        self.phone_modes(&key)
    }

    /// A machine a phone connected: its home folder and the agents there.
    fn phone_host_connected(mut self: Pin<&mut Self>, msg: &serde_json::Value) -> serde_json::Value {
        use serde_json::json;
        let text = |k: &str| msg[k].as_str().unwrap_or_default().to_owned();
        let host = text("host");
        self.as_mut().host_connected(&QString::from(&host));
        self.remote_kick.notify_one();
        let home = home_on(&host);
        let shown = harlo_core::remote::info_of(&host).map(|i| i.home).unwrap_or_default();
        json!({ "host": host, "home": shown, "agents": self.agents_at(&home) })
    }

    /// Continues a session when its plan limit resets, as a phone asked.
    fn phone_resume_at_reset(mut self: Pin<&mut Self>, msg: &serde_json::Value) -> serde_json::Value {
        use serde_json::json;
        let text = |k: &str| msg[k].as_str().unwrap_or_default().to_owned();
        let key = text("key");
        let fail = |e: &str| json!({ "error": e });
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

    /// Starts a wiki run a phone asked for.
    fn phone_wiki_run(mut self: Pin<&mut Self>, msg: &serde_json::Value) -> serde_json::Value {
        use serde_json::json;
        let text = |k: &str| msg[k].as_str().unwrap_or_default().to_owned();
        let key = text("key");
        let fail = |e: &str| json!({ "error": e });
        let mode = text("mode");
        if !["create", "update"].contains(&mode.as_str()) && !mode.starts_with("page:") {
            return fail("harlo doesn't know that wiki run.");
        }
        if mode == "create"
            && let Some((_, project)) = self.wiki_folder(&key) {
                self.as_mut().set_setting(&QString::from(&format!("wiki_{project}")), &QString::from("1"));
            }
        match self.as_mut().start_wiki_run(&QString::from(&key), &QString::from(&mode)).to_string() {
            e if !e.is_empty() => fail(&e),
            _ => json!({ "ok": true }),
        }
    }

    /// Sends a phone's message to a running session.
    fn phone_send(&self, msg: &serde_json::Value) -> serde_json::Value {
        use serde_json::json;
        let text = |k: &str| msg[k].as_str().unwrap_or_default().to_owned();
        let key = text("key");
        let fail = |e: &str| json!({ "error": e });
        let Some(live) = registry::get(&key) else { return fail("That session isn't running.") };
        let mode = match msg["mode"].as_str() {
            Some("steer") => harlo_harness::SendMode::Steer,
            Some("followUp") => harlo_harness::SendMode::FollowUp,
            Some("stopAndSend") => harlo_harness::SendMode::StopAndSend,
            _ => harlo_harness::SendMode::Normal,
        };
        let images = harlo_core::phone::images(&msg["images"]);
        // (As typed: its pictures are its images.)
        let typed = Some(harlo_core::Typed { words: text("text"), files: Vec::new() });
        let text = format!("{}{}", harlo_core::phone::image_blocks(images.len()), text("text"));
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
                live.handle.prompt_typed(&text, images, mode, typed);
            });
            return json!({ "ok": true });
        }
        if !live.handle.prompt_typed(&text, images, mode, typed) {
            return fail(&live.handle.held().unwrap_or_else(|| "harlo holds this session's messages.".into()));
        }
        json!({ "ok": true })
    }

    /// Starts a session a phone asked for, with its first message and photos.
    fn phone_new(mut self: Pin<&mut Self>, msg: &serde_json::Value) -> serde_json::Value {
        use serde_json::json;
        let text = |k: &str| msg[k].as_str().unwrap_or_default().to_owned();
        let fail = |e: &str| json!({ "error": e });
        let project = msg["project"].as_i64().unwrap_or(-1);
        let project = self.projects.iter().find(|p| p.id == project).cloned();
        let agent = match text("agent") {
            a if a.is_empty() => project.as_ref().map_or_else(|| self.default_harness(), |p| p.harness.clone()),
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
            None if !host.is_empty() => home_on(&host),
            None => PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/".into())),
        };
        // (On the project's own machine: its agents.)
        let Some(kind) = HarnessKind::parse(&agent).filter(|k| self.agents_at(&cwd).contains(&k.as_str())) else {
            return fail(&format!("harlo can't start {agent} sessions there."));
        };
        let (first, images) = (text("text"), harlo_core::phone::images(&msg["images"]));
        // The model picked, else the project's own for that agent
        // (Settings → Projects), else the agent's default from
        // Settings (the setup guide; not on another machine: its
        // agent's own).
        let model = match text("model") {
            m if m.is_empty() => self.model_for(project.as_ref().map(|p| p.id), kind, &cwd),
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
            && !live.handle.prompt_typed(&format!("{}{first}", harlo_core::phone::image_blocks(images.len())), images, harlo_harness::SendMode::Normal, Some(harlo_core::Typed { words: first.clone(), files: Vec::new() }))
        {
            return json!({ "key": key, "harness": kind.as_str(), "model": model, "error": live.handle.held().unwrap_or_default() });
        }
        json!({ "key": key, "harness": kind.as_str(), "model": model })
    }

    /// The projects and agents a phone offers for a new session.
    fn phone_projects(&self) -> serde_json::Value {
        use serde_json::json;
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
                (id.to_owned(), json!(self.draft_approval(&QString::from(id), i32::try_from(project).unwrap_or(i32::MAX)).to_string()))
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
        // Every model each agent offers (Settings → Agents reads them).
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
                    "agents": if connected { self.agents_at(&home_on(&h.name)) } else { Vec::new() } })
        }).collect();
        json!({ "projects": projects, "agents": agents, "defaultAgent": self.default_harness(), "models": models, "agentModels": all, "approvals": approvals(-1),
                "templates": templates, "machines": machines })
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

    /// A session's approval modes, for phones.
    fn phone_modes(&self, key: &str) -> serde_json::Value {
        let approval = self.approval_mode(&QString::from(key)).to_string();
        let approvals: Vec<&str> = if approval.is_empty() { Vec::new() } else { harlo_core::session::APPROVAL_MODES.to_vec() };
        json!({ "approval": approval, "approvals": approvals })
    }

    /// Every session (projects expanded, no "show more"), for phones.
    /// A running session's plan limit (see `quota::session_limit`), from its
    /// machine's accounts.
    fn limit_of(&self, key: &str) -> Option<serde_json::Value> {
        let e = self.live.get(key)?;
        let parse = |v: &QString| serde_json::from_str::<serde_json::Value>(&v.to_string()).unwrap_or_default();
        // (Another machine's sessions: no limits harlo knows of.)
        if harlo_core::remote::host_of(&e.info.cwd).is_some() {
            return None;
        }
        let accounts = parse(&self.quota_json)["accounts"].clone();
        harlo_core::quota::session_limit(accounts.as_array().map(Vec::as_slice).unwrap_or_default(), &e.info.harness, &e.model_id)
    }

    /// The plan limits for phones, as Settings → Plan limits shows them.
    fn publish_phone_limits(&self) {
        publish_phone_limits(&self.quota_json);
    }

    fn publish_phone_sessions(&self) {
        let sessions: Vec<SessionInfo> = self.sessions.values().cloned().collect();
        let live: Vec<LiveInfo> = self.live.values().map(|e| e.info.clone()).collect();
        let projects: Vec<Project> = self.projects.iter().cloned().map(|mut p| { p.collapsed = false; p }).collect();
        let mut expanded: Vec<i64> = projects.iter().map(|p| p.id).collect();
        expanded.push(-1);
        let shelves: Vec<i64> = projects.iter().flat_map(|p| [p.id, p.id + sidebar::WIKI_SHELF]).collect();
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
            shelves_open: &shelves,
        });
        // What each waits on you for (a phone answers from its list).
        let requests = |key: &str| -> Vec<serde_json::Value> {
            self.live.get(key).into_iter()
                .flat_map(|e| &e.requests)
                .map(|(q, asked)| { let mut v = crate::phone_server::request_json(q); v["asked"] = json!(asked); v })
                .collect()
        };
        let mut list = sidebar::phone_list(rows, requests);
        // (Who made each one's model: its icon.)
        for s in &mut list {
            s["maker"] = json!(crate::provider_logos::maker_of(s["model"].as_str().unwrap_or_default()).unwrap_or_default());
            // A finished wiki run the desktop still has open (it settles
            // once you leave it there): settled on a phone already.
            let key = s["key"].as_str().unwrap_or_default();
            if s["settled"] == "" && self.settle_later.contains(key) {
                s["settled"] = json!(harlo_core::shelf::why_text("wiki"));
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
        let busy = i32::try_from(self.live.values().filter(|e| matches!(e.info.status, SessionStatus::Busy | SessionStatus::Starting)).count()).unwrap_or(i32::MAX);
        let input = i32::try_from(self.live.values().filter(|e| e.info.needs_input).count()).unwrap_or(i32::MAX);
        if self.busy_count != busy {
            self.as_mut().set_busy_count(busy);
        }
        if self.input_count != input {
            self.as_mut().set_input_count(input);
        }
        let (projects, count) = (i32::try_from(self.projects.len()).unwrap_or(i32::MAX), i32::try_from(sessions.len()).unwrap_or(i32::MAX));
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
                    let idx = self.index(i32::try_from(at).unwrap_or(i32::MAX), 0, &root);
                    self.as_mut().data_changed(&idx, &idx, &QVector::<i32>::default());
                }
                // SAFETY: begin/end_remove_rows are paired here, on the GUI thread;
                // diff_by made this change from `self.rows`, so `at..at + count`
                // is a non-empty range of existing rows, drained between them.
                ListChange::Remove { at, count } => unsafe {
                    self.as_mut().begin_remove_rows(&root, i32::try_from(at).unwrap_or(i32::MAX), i32::try_from(at + count - 1).unwrap_or(i32::MAX));
                    self.as_mut().rust_mut().rows.drain(at..at + count);
                    self.as_mut().end_remove_rows();
                },
                // SAFETY: begin/end_insert_rows are paired here, on the GUI thread;
                // diff_by made this change from `self.rows`: `rows` is non-empty
                // and `at` within them, and exactly these are spliced in between.
                ListChange::Insert { at, rows } => unsafe {
                    self.as_mut().begin_insert_rows(&root, i32::try_from(at).unwrap_or(i32::MAX), i32::try_from(at + rows.len() - 1).unwrap_or(i32::MAX));
                    self.as_mut().rust_mut().rows.splice(at..at, rows);
                    self.as_mut().end_insert_rows();
                },
            }
        }
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(r) = self.rows.get(usize::try_from(index.row()).unwrap_or(0)) else {
            return QVariant::default();
        };
        let s = |v: &str| QVariant::from(&QString::from(v));
        match role {
            ROLE_KIND => s(r.kind.as_str()),
            ROLE_KEY => s(&r.key),
            ROLE_PROJECT => QVariant::from(&i32::try_from(r.project_id).unwrap_or(i32::MAX)),
            ROLE_TITLE => s(&r.title),
            ROLE_SUBTITLE => s(&r.subtitle),
            ROLE_HARNESS => s(&r.harness),
            ROLE_STATUS => s(&r.status),
            ROLE_PATH => s(&r.path),
            ROLE_MODIFIED => QVariant::from(&(r.modified as f64)),
            ROLE_COLLAPSED => QVariant::from(&r.collapsed),
            ROLE_PINNED => QVariant::from(&r.pinned),
            ROLE_COUNT => QVariant::from(&i32::try_from(r.count).unwrap_or(i32::MAX)),
            ROLE_BRANCH => s(&r.branch),
            ROLE_CHANGES => QVariant::from(&i32::try_from(r.changes).unwrap_or(i32::MAX)),
            ROLE_ICON => s(harlo_core::project_icons::shown(&r.icon)),
            ROLE_COLOR => s(&r.color),
            ROLE_SETTLED => s(harlo_core::shelf::why_text(&r.settled)),
            ROLE_MODEL_NAME => s(&r.model_name),
            ROLE_MAKER => s(crate::provider_logos::maker_of(&r.model).unwrap_or_default()),
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
            (ROLE_MODEL_NAME, "modelName"),
            (ROLE_MAKER, "maker"),
        ] {
            roles.insert(role, QByteArray::from(name));
        }
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        i32::try_from(self.rows.len()).unwrap_or(i32::MAX)
    }
}

/// Where harlo creates worktrees: `$XDG_DATA_HOME/harlo/worktrees`.
/// How a worktree session starts: its first message, model and modes.
#[derive(Default)]
struct WorktreeStart {
    first: Option<FirstPrompt>,
    model: String,
    /// Its thinking level ("" the agent's default).
    thinking: String,
    /// The shield's mode to start in.
    approval: Option<String>,
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
        if let Some(server) = server.filter(super::accounts::ModelServer::needs_start) {
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
        let typed = harlo_core::Typed { words: first.text.clone(), files: first.attachments.iter().map(|p| p.to_string_lossy().into_owned()).collect() };
        live.handle.prompt_typed(&text, prepared.images, harlo_harness::SendMode::Normal, Some(typed));
    });
}

/// Pasted images from before last week: they've been sent or dropped by now.
fn prune_pasted(dir: &Path) {
    let week = std::time::Duration::from_hours(168);
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > week) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The plan limits for phones, from this computer's (`quota_json`).
fn publish_phone_limits(quota: &QString) {
    let parse = |v: &QString| serde_json::from_str::<serde_json::Value>(&v.to_string()).unwrap_or_default();
    crate::phone_bridge::publish_limits(harlo_core::phone::limits(&parse(quota), &serde_json::json!({})));
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

/// An agent's tool call, as the handlers of its tools read it.
struct ToolCall<'a> {
    /// The calling session's key (None: an agent harlo didn't start).
    caller: Option<String>,
    args: &'a serde_json::Value,
    /// The caller's folder.
    cwd: Option<PathBuf>,
}

impl ToolCall<'_> {
    /// A text argument, trimmed ("" when it's missing).
    fn text(&self, key: &str) -> String {
        self.args[key].as_str().map(str::trim).unwrap_or_default().to_owned()
    }
}

#[expect(clippy::unused_self, reason = "QML calls these as methods of their object, so they take self")]
impl Workspace {
    /// An agent's call of harlo's tools: `token` is its session's
    /// `HARLO_SESSION`.
    pub(crate) fn orchestrate(self: Pin<&mut Self>, token: &str, tool: &str, args: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let caller = self.orch_tokens.get(token).cloned();
        self.orchestrate_as(caller, tool, args)
    }

    /// A tool call from `caller` (None: not a session harlo knows).
    fn orchestrate_as(mut self: Pin<&mut Self>, caller: Option<String>, tool: &str, args: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        use serde_json::json;
        if !self.orchestration_on {
            return Err(anyhow::Error::msg("harlo's tools are turned off (Settings → Tools for agents)."));
        }
        if tool == harlo_core::orchestration::ENABLED {
            return Ok(json!(true));
        }
        let facts = self.caller_facts(caller.as_deref());
        if tool == harlo_core::orchestration::TOOLS {
            let tools = tool_calls::offered(&facts);
            if let Some(c) = caller {
                let names = tools.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect();
                self.as_mut().rust_mut().listed.insert(c, names);
            }
            return Ok(json!(tools));
        }
        if tool == harlo_core::orchestration::INSTRUCTIONS {
            return Ok(instructions_for(&facts));
        }
        // The wiki: answered off this thread (crate::orchestration).
        if tool == harlo_core::wiki::CHECK {
            let cwd = tool_calls::wiki_folder(tool, &facts)?;
            let info = caller.as_ref().and_then(|c| self.live.get(c)).map(|e| (e.info.harness.clone(), e.info.model.clone())).unwrap_or_default();
            let writer = harlo_core::wiki::actor(&info.0, &info.1);
            return Ok(tool_calls::deferred(json!({ "cwd": cwd.to_string_lossy(), "writer": writer })));
        }
        let cwd = caller.as_ref().and_then(|c| self.live.get(c)).map(|e| e.info.cwd.clone());
        let call = ToolCall { caller, args, cwd };
        match tool {
            // Past sessions whose messages match: this folder's (or all).
            // (The search itself runs off this thread: `found_sessions`.)
            "list_sessions" if !call.text("query").is_empty() => {
                if self.search_reader.is_none() {
                    return Err(anyhow::Error::msg("Searching sessions isn't available here."));
                }
                let own = call.caller.as_ref().and_then(|c| self.live.get(c)).and_then(|e| e.info.session_file.clone());
                Ok(tool_calls::deferred(json!({ "query": call.text("query"), "everywhere": call.args["everywhere"] == true, "cwd": call.cwd, "own": own })))
            }
            "list_sessions" => Ok(self.open_sessions_for(&call)),
            // A past one: what it said last, from its file.
            "read_session" if !self.live.contains_key(&call.text("session")) && harlo_core::session_ref::find(self.sessions.values(), &call.text("session")).is_some() => {
                let i = harlo_core::session_ref::find(self.sessions.values(), &call.text("session")).expect("found");
                let wanted = call.args["messages"].as_u64().unwrap_or(6).clamp(1, 50) as usize;
                let messages = harlo_core::session_ref::latest(&harlo_core::session_ref::messages_of(&i.path), wanted);
                let when = chrono::DateTime::<chrono::Local>::from(i.modified).format("%Y-%m-%d %H:%M").to_string();
                Ok(json!({ "session": harlo_core::session_ref::token(&i.id), "title": i.display_title(), "agent": i.harness.as_str(), "status": "closed", "lastActive": when, "messages": messages }))
            }
            "read_session" => {
                let key = self.open_session(&call)?;
                let e = &self.live[&key];
                Ok(json!({ "session": key, "title": tool_calls::session_title(&e.info.name, &e.info.first_message), "agent": e.info.harness, "status": tool_calls::status_word(e.info.needs_input, e.info.status.as_str()) }))
            }
            "request_approval" => {
                let Some(live) = call.caller.as_deref().and_then(registry::get) else {
                    return Err(anyhow::Error::msg("Only a session harlo started can ask the user here."));
                };
                let question = call.text("question");
                if question.is_empty() {
                    return Err(anyhow::Error::msg("Give the question."));
                }
                live.handle.open_gate("", &question, &tool_calls::approval_options(&call.args["options"]));
                Ok(json!({ "asked": true, "next": "End your turn now: harlo sends you the user's answer as a message." }))
            }
            "report_done" | "ask_parent" if call.caller.as_deref().is_some_and(|c| self.serves_workflow(c)) => self.workflow_report(&call, tool),
            "report_done" | "ask_parent" => Err(anyhow::Error::msg("Only a workflow's session reports back or asks: ask the user instead (end your turn with the question).")),
            _ => Err(anyhow::anyhow!("harlo has no tool {tool}.")),
        }
    }

    /// The open sessions, for an agent's `list_sessions` without a query.
    fn open_sessions_for(&self, call: &ToolCall<'_>) -> serde_json::Value {
        use serde_json::json;
        let list: Vec<serde_json::Value> = self
            .live
            .values()
            .map(|e| {
                let i = &e.info;
                json!({
                    "session": i.key, "title": tool_calls::session_title(&i.name, &i.first_message), "agent": i.harness, "status": tool_calls::status_word(i.needs_input, i.status.as_str()),
                    "folder": i.cwd.to_string_lossy(),
                    "you": call.caller.as_deref() == Some(i.key.as_str()),
                })
            })
            .collect();
        json!({ "sessions": list })
    }

    /// The open session an agent's call names (`session`).
    fn open_session(&self, call: &ToolCall<'_>) -> anyhow::Result<String> {
        let key = call.text("session");
        self.live.contains_key(&key).then_some(key.clone()).ok_or_else(|| anyhow::anyhow!("There's no open session {key}."))
    }

    /// A session's title, as the sidebar shows it.
    fn title_of(&self, key: &str) -> String {
        if let Some(row) = self.rows.iter().find(|r| r.key == key) {
            return row.title.clone();
        }
        self.live.get(key).map(|e| tool_calls::session_title(&e.info.name, &e.info.first_message)).unwrap_or_default()
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
    }

    /// A wiki run that's done: on the Wiki shelf (once you've left it).
    fn settle_wiki_run(mut self: Pin<&mut Self>, key: &str) {
        if key.is_empty() || self.setting_str("settle_sessions") == "0" {
            return;
        }
        if self.active_key.to_string() == key || self.file_for_key(key).is_none() {
            self.as_mut().rust_mut().settle_later.insert(key.to_owned());
            return;
        }
        self.as_mut().rust_mut().settle_later.remove(key);
        // (A little after now: its last writes may land a moment later.)
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX)) + 5;
        self.as_mut().update_meta(key, |m| {
            if !m.pinned && !m.archived {
                m.settled_why = "wiki".into();
                m.settled_at = at;
            }
        });
    }

    pub fn stop_session(&self, key: &QString) {
        if let Some(live) = registry::get(&key.to_string()) {
            live.handle.abort();
        }
    }
}

/// The setting with what was used last on `host` ("" this computer).
fn meter_setting(host: &str) -> String {
    if host.is_empty() { "meter_last".to_owned() } else { format!("meter_last@{host}") }
}

/// A diff tab's key, `diff:<0|1>:<path>`: (staged, path).
fn diff_key(key: &str) -> Option<(bool, &str)> {
    let rest = key.strip_prefix("diff:")?;
    let (staged, path) = rest.split_once(':')?;
    Some((staged == "1", path))
}

/// What a session is told of harlo's tools, and, when it reads its
/// project's wiki, the wiki's note (and map, as the project has it) to add,
/// worked out off the UI thread (another machine's is asked over SSH).
fn instructions_for(facts: &tool_calls::Caller) -> serde_json::Value {
    let text = tool_calls::instructions();
    let wiki_found = facts.project.as_ref().is_some_and(|p| harlo_core::remote::is_remote(&p.cwd) || harlo_core::wiki::has_wiki(&p.cwd));
    match tool_calls::reads_wiki(facts, wiki_found) {
        Some(cwd) => {
            let map = facts.project.as_ref().is_some_and(|p| p.map);
            tool_calls::deferred(json!({ "cwd": cwd.to_string_lossy(), "text": text, "map": map }))
        }
        None => json!(text),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn answered_approvals_reach_the_workspace() {
        // (It sends the answer to the agent; filtered out, the agent never heard it.)
        let answered = harlo_core::session::SessionUpdate::GateAnswered { id: "harlo-gate-1".into(), question: "Delete it?".into(), answer: serde_json::json!({ "value": "Reject" }) };
        assert!(super::workspace_wants(&answered));
    }

    use super::url_to_path;

    #[test]
    fn decodes_file_urls() {
        assert_eq!(url_to_path("file:///home/u/My%20Project"), "/home/u/My Project");
        assert_eq!(url_to_path("/plain/path"), "/plain/path");
        assert_eq!(url_to_path("file:///a%2"), "/a%2");
    }
}
