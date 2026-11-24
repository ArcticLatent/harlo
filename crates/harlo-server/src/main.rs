//! `harlo-server serve [--hostname H] [--port P]`: runs the server until
//! it's stopped. The password, when set, comes from `OPENCODE_PASSWORD`
//! (or `HARLO_SERVER_PASSWORD`); clients send it as basic auth.

// Rust guideline compliant 2026-10-09

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
    println!("harlo-server listening on http://{hostname}:{port}");
    let stopped = async {
        let mut term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            () = async { match term.as_mut() { Some(t) => { t.recv().await; } None => std::future::pending().await } } => {}
        }
    };
    if let Err(e) = axum::serve(listener, harlo_server::router(server))
        .with_graceful_shutdown(stopped)
        .await
    {
        return fail(&format!("The server stopped: {e}"));
    }
    ExitCode::SUCCESS
}

fn fail(message: &str) -> ExitCode {
    eprintln!("harlo-server: {message}");
    ExitCode::FAILURE
}
