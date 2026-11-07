//! harlo-agent: harlo's own coding agent. Signs in with ChatGPT, and works
//! on a task in the current folder with its tools (`-p`).
//!
//! ```text
//! harlo-agent login            sign in with ChatGPT (in the browser)
//! harlo-agent logout           forget the sign-in here
//! harlo-agent status           who's signed in
//! harlo-agent models           the models your plan can use
//! harlo-agent -p "…" [-m model] [--thinking low|medium|high]
//!                              [--resume | --session <file>]
//! harlo-agent --version        and the commit it was built from
//! harlo-agent --mode rpc [--session <file> | --no-session] [-m model] [--thinking level]
//!                         [--harlo-tools]
//!                              pi's RPC protocol on stdin/stdout (for harlo;
//!                              with harlo's own tools on the same pipe)
//! ```
//!
//! Each run is saved as a session (in pi's format) under
//! `~/.local/share/harlo-agent/sessions`; `--resume` continues this
//! folder's newest one.

mod agent;
mod auth;
mod chat;
mod compact;
mod providers;
mod responses;
mod rpc;
mod session;
mod tools;

use std::io::Write as _;
use std::process::ExitCode;

use anyhow::{Result, bail};

#[tokio::main]
async fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("harlo-agent: {e:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: Vec<String>) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("login") => login().await,
        Some("logout") => {
            if auth::sign_out()? {
                println!("Signed out here. To disconnect harlo from your account too: ChatGPT → Settings → Connected apps.");
            } else {
                println!("Not signed in.");
            }
            Ok(())
        }
        Some("status") => {
            // For harlo: `{ "signedIn", "email" }`.
            if args.get(1).is_some_and(|a| a == "--json") {
                let c = auth::load();
                println!("{}", serde_json::json!({ "signedIn": c.is_some(), "email": c.map(|c| c.email).unwrap_or_default() }));
                return Ok(());
            }
            match auth::load() {
                Some(c) => println!("Signed in with ChatGPT{} (your ChatGPT plan pays for harlo-agent's requests).", if c.email.is_empty() { String::new() } else { format!(" as {}", c.email) }),
                None => println!("Not signed in: run `harlo-agent login`."),
            }
            Ok(())
        }
        Some("models") => {
            let (models, errors) = providers::every_model().await;
            for e in &errors {
                eprintln!("{e}");
            }
            if args.get(1).is_some_and(|a| a == "--json") {
                let list: Vec<_> = models.iter().map(|m| serde_json::json!({ "provider": m.provider, "id": m.id, "name": m.name, "contextWindow": m.context_window, "levels": m.levels })).collect();
                println!("{:#}", serde_json::Value::from(list));
                return Ok(());
            }
            for m in &models {
                println!("{}\t{}", m.key(), m.name);
            }
            if models.is_empty() {
                bail!("No models: sign in (`harlo-agent login`) or set up a provider in {}", auth::config_dir().join("providers.json").display());
            }
            Ok(())
        }
        Some("--version" | "-V") => {
            println!("harlo-agent {} ({})", env!("CARGO_PKG_VERSION"), env!("HARLO_AGENT_COMMIT"));
            Ok(())
        }
        Some("--mode") if args.get(1).is_some_and(|m| m == "rpc") => {
            let (mut keep, mut model, mut effort, mut harlo_tools) = (rpc::Keep::New, None, None, false);
            let mut it = args.into_iter().skip(2);
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--session" => keep = rpc::Keep::File(it.next().ok_or_else(|| anyhow::anyhow!("--session needs the file"))?.into()),
                    "--no-session" => keep = rpc::Keep::Nowhere,
                    "-m" | "--model" => model = it.next(),
                    "--thinking" => effort = it.next(),
                    "--harlo-tools" => harlo_tools = true,
                    other => bail!("unknown option {other}"),
                }
            }
            rpc::serve(keep, model, effort, harlo_tools).await
        }
        Some("-p") => {
            let mut model = None;
            let mut effort = None;
            let mut from = Resume::New;
            let mut it = args.into_iter().skip(1);
            let prompt = it.next();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "-m" | "--model" => model = it.next(),
                    "--thinking" => effort = it.next(),
                    "--resume" | "-c" => from = Resume::Latest,
                    "--session" => from = Resume::File(it.next().ok_or_else(|| anyhow::anyhow!("--session needs the file"))?.into()),
                    other => bail!("unknown option {other}"),
                }
            }
            let Some(prompt) = prompt else { bail!("-p needs the prompt") };
            ask(&prompt, model, effort, from).await
        }
        _ => {
            println!("harlo-agent login | logout | status | models | -p \"prompt\" [-m model] [--thinking low|medium|high] [--resume | --session file]");
            Ok(())
        }
    }
}

async fn login() -> Result<()> {
    let creds = auth::sign_in(|url| {
        println!("Continue with ChatGPT in your browser. If it doesn't open, visit:\n\n{url}\n");
        let _ = std::process::Command::new("xdg-open").arg(url).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    })
    .await?;
    println!(
        "Signed in{}.\n\nYou're using your ChatGPT plan: harlo-agent's requests use your plan's usage, which you can manage in ChatGPT → Settings → Usage.",
        if creds.email.is_empty() { String::new() } else { format!(" as {}", creds.email) }
    );
    Ok(())
}

/// Which session a run goes in.
enum Resume {
    New,
    /// This folder's newest.
    Latest,
    File(std::path::PathBuf),
}

async fn ask(prompt: &str, model: Option<String>, effort: Option<String>, from: Resume) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = session::root().ok_or_else(|| anyhow::anyhow!("No home folder to keep sessions in"))?;
    let loaded = match from {
        Resume::New => None,
        Resume::Latest => match session::latest(&root, &cwd) {
            Some(p) => Some(session::Log::open(&p)?),
            None => bail!("There's no session in this folder to resume"),
        },
        Resume::File(p) => Some(session::Log::open(&p)?),
    };
    let (log, past) = match loaded {
        Some(l) => {
            let (log, past, _) = l.into_parts();
            (log, past)
        }
        None => (session::Log::new(&root, &cwd), session::Past::default()),
    };
    // The model asked for, else the session's, else the plan's first.
    let effort = effort.or_else(|| past.effort.clone());
    let model = match model.or_else(|| past.model.clone()) {
        Some(m) => providers::full(&m),
        None => {
            let (models, errors) = providers::every_model().await;
            models.first().map(|m| m.key()).ok_or_else(|| anyhow::anyhow!("No models: {}", if errors.is_empty() { "set up a provider".to_owned() } else { errors.join("; ") }))?
        }
    };
    let mut agent = agent::Agent::new(cwd, log, past, model, effort);
    let mut out = std::io::stdout();
    let mut thinking = false;
    // Whether the last thing printed ended a line.
    let mut fresh = true;
    let started = std::time::Instant::now();
    let mut show = |s: agent::Shown| {
        let line = |fresh: &mut bool| {
            if !*fresh {
                println!();
                *fresh = true;
            }
        };
        let end_thinking = |thinking: &mut bool| {
            if *thinking {
                eprintln!("\x1b[0m");
                *thinking = false;
            }
        };
        match s {
            agent::Shown::Stream(e) => match e["type"].as_str() {
                Some("thinking_delta") => {
                    if !thinking {
                        eprint!("\x1b[2m");
                        thinking = true;
                    }
                    eprint!("{}", e["delta"].as_str().unwrap_or_default());
                }
                Some("text_delta") => {
                    end_thinking(&mut thinking);
                    let t = e["delta"].as_str().unwrap_or_default();
                    print!("{t}");
                    fresh = t.ends_with('\n');
                    let _ = out.flush();
                }
                _ => {}
            },
            agent::Shown::ToolStart { name, args, .. } => {
                end_thinking(&mut thinking);
                line(&mut fresh);
                eprintln!("\x1b[36m→ {name}\x1b[0m \x1b[2m{}\x1b[0m", summary(name, args));
            }
            agent::Shown::ToolEnd { text, failed, .. } => {
                let first = text.lines().next().unwrap_or_default();
                let more = text.lines().count().saturating_sub(1);
                let first: String = first.chars().take(120).collect();
                eprintln!("  \x1b[{}m{first}{}\x1b[0m", if failed { "31" } else { "2" }, if more > 0 { format!(" (+{more} lines)") } else { String::new() });
            }
            agent::Shown::Usage { model, usage } => {
                end_thinking(&mut thinking);
                line(&mut fresh);
                eprintln!("\x1b[2m[{model} · {} in ({} cached), {} out]\x1b[0m", usage["input_tokens"], usage["input_tokens_details"]["cached_tokens"], usage["output_tokens"]);
            }
            agent::Shown::CompactionStart { .. } => {
                end_thinking(&mut thinking);
                line(&mut fresh);
                eprintln!("\x1b[2mThe context is nearly full: summarizing the older part…\x1b[0m");
            }
            agent::Shown::CompactionEnd { result, error, .. } => match (result, error) {
                (Some(r), _) => eprintln!("\x1b[2mSummarized: {} tokens → about {}\x1b[0m", r["tokensBefore"], r["estimatedTokensAfter"]),
                (None, Some(e)) => eprintln!("\x1b[31mCouldn't summarize: {e}\x1b[0m"),
                (None, None) => {}
            },
            agent::Shown::MessageStart(_) | agent::Shown::MessageEnd(_) | agent::Shown::Queue { .. } => {}
        }
    };
    // Ctrl+C stops the turn; a second one quits at once.
    let stop = tokio_util::sync::CancellationToken::new();
    let s = stop.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            // Over the terminal's "^C".
            eprint!("\r");
            s.cancel();
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("\x1b[0m");
                std::process::exit(130);
            }
        }
    });
    let result = agent.run(prompt, &stop, &mut show).await;
    if matches!(result, Ok(agent::Ended::Stopped)) {
        eprintln!("\x1b[0m\x1b[33mStopped.\x1b[0m");
    }
    let t = &agent.totals;
    eprintln!(
        "\x1b[2m[{} request{} · {} in ({} cached, {}%) · {} out · {:.0}s]\x1b[0m",
        t.requests,
        if t.requests == 1 { "" } else { "s" },
        t.input,
        t.cached,
        if t.input > 0 { t.cached * 100 / t.input } else { 0 },
        t.output,
        started.elapsed().as_secs_f64()
    );
    if agent.session_file().exists() {
        eprintln!("\x1b[2mSession: {} (continue it with --resume)\x1b[0m", agent.session_file().display());
    }
    if result? == agent::Ended::Stopped {
        std::process::exit(130);
    }
    Ok(())
}

/// A call's arguments in one line.
fn summary(name: &str, args: &serde_json::Value) -> String {
    let s = |k: &str| args[k].as_str().unwrap_or_default().to_owned();
    let text = match name {
        "bash" => s("command"),
        "read" | "write" | "ls" => s("path"),
        "edit" => format!("{} ({} edit{})", s("path"), args["edits"].as_array().map_or(0, Vec::len), if args["edits"].as_array().map_or(0, Vec::len) == 1 { "" } else { "s" }),
        "grep" => format!("{} {}", s("pattern"), s("path")),
        "find" => format!("{} {}", s("pattern"), s("path")),
        _ => args.to_string(),
    };
    let one: String = text.replace('\n', " ⏎ ").chars().take(160).collect();
    one
}
