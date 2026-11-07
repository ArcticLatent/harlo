//! `harlo-engines`: harlo's model servers on another machine. harlo puts it
//! there (it has no window, so it runs where harlo itself can't) and runs it
//! over SSH; it does there what harlo does here (`harlo_core::engine_setup`,
//! `harlo_core::servers`), with harlo's folders there, and answers in JSON.
//!
//! | command | what it does | prints |
//! |---|---|---|
//! | `catalog` | this machine and its engines | the catalog |
//! | `updates` | newer versions of the engines harlo installed | `{ <engine>: { latest, available } }` |
//! | `servers` | the servers harlo set up here | `[{ baseUrl, name, engine, models, running, log }]` |
//! | `overview` | those and the engines installed | `{ servers, engines: [{ id, name, version, updatable }] }` |
//! | `needs-start <url>` | whether a server has to start before a message | `true` or `false` |
//! | `free-port <from>` | a free port, from `from` up | the port |
//! | `setup <job>` | a setup or update, its request on stdin | `{"job": …}` and `{"output": "…"}` lines as it goes |
//! | `cancel <job>` | stops that setup | |
//! | `start <url>` / `stop <url>` | a server, until it answers / stops | `{ "error": … }` or `{}` |
//! | `log <url>` | the end of a server's log | its text |
//! | `remove <url> <name> <0\|1>` | stops and forgets it (and deletes its model files) | `{ errors, summary }` |
//!
//! A server it starts goes on when it exits (a session of its own).

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use harlo_core::engine_setup::{Event, Report};
use harlo_core::{engines, servers};
use serde_json::{Value, json};

/// Where a setup's cancel mark goes.
fn cancel_mark(job: &str) -> PathBuf {
    let job: String = job.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    std::env::temp_dir().join(format!("harlo-engines-cancel-{job}"))
}

/// A line to harlo; false once harlo isn't there to read it.
fn say(v: &Value) -> bool {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{v}").and_then(|()| out.flush()).is_ok()
}

fn main() {
    servers::set_detached(true);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    let code = match arg(0).as_str() {
        "catalog" => print(&harlo_core::engine_setup::catalog()),
        "updates" => print(&harlo_core::engine_setup::updates()),
        "servers" => print(&json!(servers::list())),
        "overview" => print(&json!({ "servers": servers::list(), "engines": harlo_core::engine_setup::installed_engines() })),
        "needs-start" => print(&json!(servers::needs_start(&arg(1)))),
        "free-port" => {
            let taken: Vec<u16> = servers::ports_in_use().into_iter().map(|(p, _)| p).collect();
            let mut port = arg(1).parse::<u16>().unwrap_or(8080).clamp(1024, 65000);
            loop {
                port = engines::free_port(port);
                if !taken.contains(&port) || port >= 65000 {
                    break;
                }
                port += 1;
            }
            print(&json!(port))
        }
        "setup" => setup(&arg(1)),
        "cancel" => {
            let _ = std::fs::write(cancel_mark(&arg(1)), "");
            0
        }
        "start" => print(&match servers::ensure_running(&arg(1)) {
            Ok(()) => json!({}),
            Err(e) => json!({ "error": e }),
        }),
        "stop" => {
            servers::stop_any(&arg(1));
            print(&json!({}))
        }
        "log" => {
            let text = std::fs::read_to_string(servers::log_path(&arg(1))).unwrap_or_default();
            let lines: Vec<&str> = text.lines().flat_map(|l| l.rsplit('\r').next()).collect();
            println!("{}", lines[lines.len().saturating_sub(400)..].join("\n"));
            0
        }
        "remove" => {
            let (errors, summary) = servers::remove(&arg(2), &arg(1), arg(3) == "1", &|_, _, _| {}, &|_| {});
            print(&json!({ "errors": errors, "summary": summary }))
        }
        "version" => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            0
        }
        other => {
            eprintln!("harlo-engines: unknown command {other:?}");
            2
        }
    };
    std::process::exit(code);
}

fn print(v: &Value) -> i32 {
    if say(v) { 0 } else { 1 }
}

/// A setup or update (its request on stdin), told as it goes. It stops when
/// cancelled, or when harlo stops listening (a download picks up where it
/// was, next time).
fn setup(job: &str) -> i32 {
    let mut text = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut text);
    let req: Value = serde_json::from_str(&text).unwrap_or_default();
    let mark = cancel_mark(job);
    let _ = std::fs::remove_file(&mark);
    let stop = Arc::new(AtomicBool::new(false));
    {
        let (stop, mark) = (stop.clone(), mark.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if mark.exists() {
                    stop.store(true, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }
    let first = json!({ "state": "running", "step": "Starting", "progress": -1, "detail": "", "result": null, "update": req["update"].is_string() });
    let gone = stop.clone();
    // Over SSH: the output a second apart at most, how far it is four times a
    // second (a new step or the end at once; the latest always goes).
    let held: Arc<Mutex<Held>> = Arc::default();
    let report = {
        let held = held.clone();
        Report::new(first, Duration::from_secs(1), move |event| {
            let sent = match event {
                // (Said with the lock held: nothing held back goes after it.)
                Event::Job(text) => {
                    let job = serde_json::from_str::<Value>(&text).unwrap_or_default();
                    let mut h = held.lock().unwrap();
                    let new_step = job["state"] != "running" || job["step"] != h.step;
                    if !new_step && h.sent.is_some_and(|t| t.elapsed() < JOB_EVERY) {
                        h.pending = Some(job);
                        return;
                    }
                    h.step = job["step"].clone();
                    h.sent = Some(Instant::now());
                    h.pending = None;
                    say(&json!({ "job": job }))
                }
                Event::Output(text) => say(&json!({ "output": text })),
            };
            if !sent {
                gone.store(true, Ordering::Relaxed);
            }
        })
    };
    let flush = {
        let (held, stop) = (held.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(JOB_EVERY);
                let mut h = held.lock().unwrap();
                if let Some(job) = h.pending.take() {
                    h.sent = Some(Instant::now());
                    if !say(&json!({ "job": job })) {
                        stop.store(true, Ordering::Relaxed);
                    }
                }
            }
        })
    };
    harlo_core::engine_setup::run(&req, &report, &stop);
    let _ = std::fs::remove_file(&mark);
    // (Ends the watches.)
    stop.store(true, Ordering::Relaxed);
    let _ = flush.join();
    0
}

/// How often how far a setup is goes to harlo, at most.
const JOB_EVERY: Duration = Duration::from_millis(250);

/// A setup's state held back to go a moment later.
#[derive(Default)]
struct Held {
    sent: Option<Instant>,
    step: Value,
    pending: Option<Value>,
}
