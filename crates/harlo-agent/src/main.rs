//! harlo-agent: harlo's own coding agent. Signs in with ChatGPT, and works
//! on a task in the current folder with its tools (`-p`).
//!
//! ```text
//! harlo-agent login            sign in with ChatGPT (in the browser)
//! harlo-agent logout           forget the sign-in here
//! harlo-agent status           who's signed in
//! harlo-agent models           the models your plan can use
//! harlo-agent -p "…" [-m model] [--thinking low|medium|high]
//! ```

mod agent;
mod auth;
mod responses;
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
            match auth::load() {
                Some(c) => println!("Signed in with ChatGPT{} (your ChatGPT plan pays for harlo-agent's requests).", if c.email.is_empty() { String::new() } else { format!(" as {}", c.email) }),
                None => println!("Not signed in: run `harlo-agent login`."),
            }
            Ok(())
        }
        Some("models") => {
            let token = auth::access_token(false).await?;
            for (id, name) in responses::models(&token).await? {
                println!("{id}\t{name}");
            }
            Ok(())
        }
        Some("-p") => {
            let mut model = None;
            let mut effort = None;
            let mut it = args.into_iter().skip(1);
            let prompt = it.next();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "-m" | "--model" => model = it.next(),
                    "--thinking" => effort = it.next(),
                    other => bail!("unknown option {other}"),
                }
            }
            let Some(prompt) = prompt else { bail!("-p needs the prompt") };
            ask(&prompt, model, effort).await
        }
        _ => {
            println!("harlo-agent login | logout | status | models | -p \"prompt\" [-m model] [--thinking low|medium|high]");
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

async fn ask(prompt: &str, model: Option<String>, effort: Option<String>) -> Result<()> {
    let model = match model {
        Some(m) => m,
        None => {
            let token = auth::access_token(false).await?;
            responses::models(&token).await?.into_iter().next().map(|(id, _)| id).ok_or_else(|| anyhow::anyhow!("Your plan lists no models"))?
        }
    };
    let cwd = std::env::current_dir()?;
    let mut agent = agent::Agent::new(cwd, model, effort);
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
            agent::Shown::Reasoning(t) => {
                if !thinking {
                    eprint!("\x1b[2m");
                    thinking = true;
                }
                eprint!("{t}");
            }
            agent::Shown::Text(t) => {
                end_thinking(&mut thinking);
                print!("{t}");
                fresh = t.ends_with('\n');
                let _ = out.flush();
            }
            agent::Shown::ToolStart { name, args } => {
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
        }
    };
    let result = agent.run(prompt, &mut show).await;
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
    result
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
