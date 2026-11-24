//! Harlo's server.
//!
//! - `harlo-server serve [--hostname H] [--port P] [--register]` runs it until it's
//!   stopped. The password, when set, comes from `HARLO_SERVER_PASSWORD`; clients
//!   send it as basic auth. `--register` records it as the
//!   background service.
//! - `harlo-server service start` starts the background service unless one answers, and
//!   prints where it is; `service status` only says.
//! - `harlo-server --version` prints its version.

// Rust guideline compliant 2026-10-09

use std::future::IntoFuture as _;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let mut hostname = "127.0.0.1".to_owned();
    let mut port = 4096_u16;
    let mut register = false;
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().map(String::as_str) == Some("service") {
        return service(args.nth(1).as_deref()).await;
    }
    if matches!(args.peek().map(String::as_str), Some("-V" | "--version")) {
        println!("harlo-server {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "serve" => {}
            "--register" => register = true,
            "--hostname" => hostname = args.next().unwrap_or(hostname),
            "--port" => match args.next().map(|p| p.parse()) {
                Some(Ok(p)) => port = p,
                _ => return fail("--port needs a number."),
            },
            other => {
                return fail(&format!(
                    "Unknown argument {other}. Usage: harlo-server serve [--hostname H] [--port P] [--register] | service start|status"
                ));
            }
        }
    }
    let password = std::env::var("HARLO_SERVER_PASSWORD")
        .ok()
        .filter(|v| !v.is_empty());
    let server = match harlo_server::open(&harlo_server::Places::from_env(), password) {
        Ok(s) => s,
        Err(e) => return fail(&e),
    };
    let listener = match tokio::net::TcpListener::bind((hostname.as_str(), port)).await {
        Ok(l) => l,
        Err(e) => return fail(&format!("Couldn't listen on {hostname}:{port}: {e}")),
    };
    let port = listener.local_addr().map_or(port, |a| a.port());
    println!("harlo-server listening on http://{hostname}:{port}");
    // Phone access and the registration reach this server through loopback.
    let me = if hostname == "0.0.0.0" || hostname == "::" {
        "127.0.0.1"
    } else {
        hostname.as_str()
    };
    if register {
        let registration = harlo_server::service::Registration {
            url: format!("http://{me}:{port}"),
            password: server.password().map(str::to_owned),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            pid: std::process::id(),
        };
        if let Err(e) = harlo_server::service::register(&registration) {
            return fail(&e);
        }
    }
    let phone = std::sync::Arc::clone(&server.phone);
    let (url, pw) = (
        format!("http://{me}:{port}"),
        server.password().map(str::to_owned),
    );
    tokio::spawn(async move { phone.attach(url, pw).await });
    server.start_automations();
    server.start_catalog();
    server.start_refreshing();
    server.refresh_claude();
    let stopped = async {
        let mut term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            () = async { match term.as_mut() { Some(t) => { t.recv().await; } None => std::future::pending().await } } => {}
        }
    };
    // Event streams never end by themselves, so a graceful stop alone would
    // wait for every client to leave. Requests get a moment to finish, then
    // the server exits and clients see their connections close.
    let stop = std::sync::Arc::new(tokio::sync::Notify::new());
    let signal = std::sync::Arc::clone(&stop);
    let serving = axum::serve(listener, harlo_server::router(server))
        .with_graceful_shutdown(async move { signal.notified().await })
        .into_future();
    tokio::pin!(serving);
    let code = tokio::select! {
        result = &mut serving => Some(finished(result)),
        () = stopped => None,
    };
    let code = if let Some(code) = code {
        code
    } else {
        stop.notify_one();
        match tokio::time::timeout(STOP_GRACE, &mut serving).await {
            Ok(result) => finished(result),
            Err(_elapsed) => ExitCode::SUCCESS,
        }
    };
    if register {
        harlo_server::service::unregister();
    }
    code
}

/// `service start` or `service status`.
async fn service(what: Option<&str>) -> ExitCode {
    let found = match what {
        Some("start") => harlo_server::service::start().await,
        Some("status") => harlo_server::service::running()
            .await
            .ok_or_else(|| "The service isn't running.".to_owned()),
        _ => return fail("Usage: harlo-server service start|status"),
    };
    match found {
        Ok(registration) => {
            println!(
                "harlo-server {} is running at {} (process {})",
                registration.version, registration.url, registration.pid
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

/// How long requests get to finish once the server is told to stop.
const STOP_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// The exit code for how serving ended.
fn finished(result: std::io::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&format!("The server stopped: {e}")),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("harlo-server: {message}");
    ExitCode::FAILURE
}
