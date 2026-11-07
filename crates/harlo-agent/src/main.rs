//! harlo-agent: harlo's own coding agent. This first step signs in with
//! ChatGPT and streams one reply.
//!
//! ```text
//! harlo-agent login            sign in with ChatGPT (in the browser)
//! harlo-agent logout           forget the sign-in here
//! harlo-agent status           who's signed in
//! harlo-agent models           the models your plan can use
//! harlo-agent -p "…" [-m model] [--thinking low|medium|high]
//! ```

mod auth;
mod responses;

use std::io::Write as _;
use std::process::ExitCode;

use anyhow::{Result, bail};
use serde_json::json;

const INSTRUCTIONS: &str = "You are harlo-agent, a coding assistant. Be concise.";

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
    let mut token = auth::access_token(false).await?;
    let model = match model {
        Some(m) => m,
        None => responses::models(&token).await?.into_iter().next().map(|(id, _)| id).ok_or_else(|| anyhow::anyhow!("Your plan lists no models"))?,
    };
    let input = json!([{ "role": "user", "content": [{ "type": "input_text", "text": prompt }] }]);
    let mut out = std::io::stdout();
    let mut thinking = false;
    let mut on = |ev: responses::Event| match ev {
        responses::Event::Reasoning(t) => {
            if !thinking {
                eprint!("\x1b[2m");
                thinking = true;
            }
            eprint!("{t}");
        }
        responses::Event::Text(t) => {
            if thinking {
                eprintln!("\x1b[0m");
                thinking = false;
            }
            print!("{t}");
            let _ = out.flush();
        }
        responses::Event::Completed { usage } => {
            println!();
            eprintln!("\x1b[2m[{model} · {} in, {} out]\x1b[0m", usage["input_tokens"], usage["output_tokens"]);
        }
    };
    // A token the API refuses: refreshed once, then tried again.
    let first = responses::stream(&token, &model, INSTRUCTIONS, input.clone(), effort.as_deref(), &mut on).await;
    match first {
        Err(e) if e.downcast_ref::<responses::ApiError>().is_some_and(|a| a.status == 401) => {
            token = auth::access_token(true).await?;
            responses::stream(&token, &model, INSTRUCTIONS, input, effort.as_deref(), &mut on).await
        }
        other => other,
    }
}
