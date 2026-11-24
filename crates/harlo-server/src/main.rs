//! `harlo-server serve [--hostname H] [--port P]`: runs the server until
//! it's stopped. The password, when set, comes from `OPENCODE_PASSWORD`
//! (or `HARLO_SERVER_PASSWORD`); clients send it as basic auth.

// Rust guideline compliant 2026-10-09

use std::future::IntoFuture as _;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let mut hostname = "127.0.0.1".to_owned();
    let mut port = 4096_u16;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "serve" => {}
            "--hostname" => hostname = args.next().unwrap_or(hostname),
            "--port" => match args.next().map(|p| p.parse()) {
                Some(Ok(p)) => port = p,
                _ => return fail("--port needs a number."),
            },
            other => {
                return fail(&format!(
                    "Unknown argument {other}. Usage: harlo-server serve [--hostname H] [--port P]"
                ));
            }
        }
    }
    let password = ["HARLO_SERVER_PASSWORD", "OPENCODE_PASSWORD"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()));
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
    // Phone access reaches this server through loopback.
    let me = if hostname == "0.0.0.0" || hostname == "::" {
        "127.0.0.1"
    } else {
        hostname.as_str()
    };
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
    tokio::select! {
        result = &mut serving => return finished(result),
        () = stopped => {}
    }
    stop.notify_one();
    match tokio::time::timeout(STOP_GRACE, &mut serving).await {
        Ok(result) => finished(result),
        Err(_elapsed) => ExitCode::SUCCESS,
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
