//! harlo: native desktop front end for coding agents (its own, Harlo, and
//! Claude Code).

mod accounts;
mod audio;
mod chat_session;
mod ci;
mod diagnostics;
mod diagrams;
mod dictation;
mod terminal_session;
mod test_input;
mod text_file;
mod themes;
mod usage_stats;
mod file_browser;
mod file_icons;
mod fonts;
mod git_panel;
mod instance;
mod notify;
mod mcp;
mod phone;
mod phone_activity;
mod phone_bridge;
mod phone_funnel;
mod phone_push;
mod phone_relay;
mod phone_server;
mod platform;
mod provider_logos;
mod qt_base;
mod registry;
mod remotes;
mod server_setup;
mod setup;
mod keep_above;
mod workspace;
mod orchestration;
mod plans;
mod graphics;
mod jobs;
mod updates;

use std::sync::OnceLock;

use cxx_qt_lib::{QQmlApplicationEngine, QString, QStringList, QUrl};

/// Shared tokio runtime for harness processes and session drivers.
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("harlo-rt")
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("harlo {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    // An agent's MCP server for harlo's tools (orchestration): relays to the
    // running harlo and exits with the agent.
    if std::env::args().nth(1).as_deref() == Some("--mcp-bridge") {
        // (Named apart: in a process list it isn't a second harlo.)
        // SAFETY: plain syscall with a NUL-terminated name of at most 16 bytes.
        unsafe {
            libc::prctl(libc::PR_SET_NAME, c"harlo-bridge".as_ptr());
        }
        harlo_core::orchestration::run_bridge();
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("--check-diagram") {
        std::process::exit(diagrams::check());
    }
    // Already running? Hand over (show the window / toggle the drop-down).
    // Checked first, so a launch that only hands over keeps the running
    // instance's log intact.
    // (`--check-start`: a package's own start, never handed over.)
    if !std::env::args().any(|a| a == "--check-start") && instance::forward_to_running() {
        return;
    }
    // harlo was called piqt: its data and settings move over (once), before
    // anything opens them, and not while piqt itself still has them open.
    if harlo_core::migrate::needed() && harlo_core::migrate::piqt_running() {
        let message = "piqt (harlo's old name) is still running. Quit it (its tray icon → Quit), then start harlo: your projects and settings move over.";
        eprintln!("{message}");
        let _ = std::process::Command::new("notify-send").args(["--app-name=harlo", "harlo", message]).status();
        std::process::exit(1);
    }
    // Another relay for this run (a test's stand-in on this machine).
    if let Some(url) = std::env::args().find_map(|a| a.strip_prefix("--relay=").map(str::to_owned)) {
        harlo_core::phone::use_relay_for_run(&url);
    }
    let migrated = harlo_core::migrate::from_piqt();
    // The bridge the agents get (orchestration): this very harlo.
    if let Ok(exe) = std::env::current_exe() {
        harlo_harness::set_mcp_bridge(exe, harlo_core::orchestration::socket_path());
    }
    harlo_harness::set_remote_bridge(|host| match harlo_core::orchestration::remote_bridge(host) {
        Ok(command) => Some(command),
        Err(e) => {
            tracing::warn!("orchestration on {host}: {e:#}");
            None
        }
    });
    harlo_harness::set_remote_start(harlo_core::model_servers::forward_to);
    // (Harlo's agent on other machines: the copy harlo puts there.)
    harlo_harness::set_remote_program(harlo_core::agent_host::remote_program);
    // (Other machines' servers this computer's agents reach through SSH.)
    std::thread::spawn(harlo_core::model_servers::forward_uses);
    // (Node for the MCP servers harlo installed and turned on, if theirs is gone.)
    std::thread::spawn(harlo_core::mcp_catalog::on_start);
    harlo_core::servers::set_after_start(accounts::sync_context);
    diagnostics::init_logging();
    diagnostics::install_panic_hook();
    for note in &migrated {
        tracing::info!("piqt → harlo: {note}");
    }
    // Remote hosts, before any `ssh://` project or session is opened.
    remotes::load_saved();
    // Checks the crash report end to end.
    if std::env::args().any(|a| a == "--crash-test") {
        panic!("crash test requested with --crash-test");
    }
    instance::listen();
    // OpenGL, Vulkan or software (Settings → General → Graphics).
    graphics::apply();

    // Our own look comes from QML; keep the platform style out of the way.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        // SAFETY: still single-threaded; no other thread reads the environment yet.
        unsafe { std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic") };
    }

    // Parsing the syntax definitions takes a moment; do it off the UI thread.
    runtime().spawn_blocking(harlo_core::markdown::warm_up);

    // A widgets-capable application: the desktop's tray (KDE's needs widgets)
    // and native file dialogs work. Also names the QtCore `Settings` file
    // (~/.config/harlo/harlo.conf).
    let mut args = QStringList::default();
    for a in std::env::args() {
        args.append(QString::from(&a));
    }
    platform::create_application(&args);
    // Asked to end (logging out, shutting down, Ctrl+C): quit as from the
    // menu, so what harlo started (remote sessions too) ends with it.
    runtime().spawn(async {
        use tokio::signal::unix::{SignalKind, signal};
        let (Ok(mut term), Ok(mut int), Ok(mut hup)) = (signal(SignalKind::terminate()), signal(SignalKind::interrupt()), signal(SignalKind::hangup())) else { return };
        tokio::select! { _ = term.recv() => {}, _ = int.recv() => {}, _ = hup.recv() => {} }
        platform::quit_application();
    });
    // The interface font, bundled.
    fonts::load_bundled();
    fonts::set_application_font_family(&QString::from(fonts::DEFAULT_UI_FONT));
    diagnostics::capture_qt_messages();
    // Named only when harlo.desktop is installed (a package): Qt registers
    // that name with the desktop portal, which refuses names it can't find
    // ("App info not found"), as under `cargo run`. Without it the Wayland
    // app id is still "harlo" (the program's name).
    if desktop_entry_installed() {
        platform::set_desktop_file_name(&QString::from("harlo"));
    }
    platform::set_window_icon(&QString::from(":/qt/qml/harlo/icons/app/harlo.svg"));
    accounts::offer_levels();
    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        // No window to draw (a QML error): says nothing about the graphics.
        engine.as_mut().on_object_creation_failed(|_, _| graphics::inconclusive()).release();
        engine.load(&QUrl::from("qrc:/qt/qml/harlo/qml/Main.qml"));
    }
    platform::exec_application();
    // Quit normally, drawn or not (it started in the tray, say): the
    // graphics didn't stop it starting.
    graphics::inconclusive();
    // Remote sessions end with harlo: what they're running there stops, and
    // a session ending so isn't said as a failure.
    for h in harlo_core::remote::hosts() {
        harlo_core::remote::set_disconnected(&h.name, true);
    }
    // Model servers harlo started (Settings → Model servers).
    harlo_core::servers::stop_started_servers();
    harlo_core::remote::disconnect_all();
    instance::cleanup();
    // Orchestration's socket.
    let _ = std::fs::remove_file(harlo_core::orchestration::socket_path());
    // The QML engine, then what's left to delete, then the application, in
    // that order (Qt's own exit order crashed on quit).
    drop(engine);
    platform::shutdown_application();
}

/// Whether harlo.desktop is installed where desktops look for it: the
/// user's and the system's `applications` folders (XDG data folders).
fn desktop_entry_installed() -> bool {
    let env_dir = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(std::path::PathBuf::from);
    let home = env_dir("XDG_DATA_HOME").or_else(|| env_dir("HOME").map(|h| h.join(".local/share")));
    let system = std::env::var("XDG_DATA_DIRS").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    home.into_iter()
        .chain(system.split(':').map(std::path::PathBuf::from))
        .any(|dir| dir.join("applications/harlo.desktop").is_file())
}
