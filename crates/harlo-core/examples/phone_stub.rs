//! A stand-in phone for the phone handoff check: it pairs with a pairing
//! link, opens a session, waits for an approval, and answers it twice when
//! told to, as a phone retrying would.
//!
//! `phone_stub <link> <session> <go-file>` prints one line per step:
//! `paired`, `asked <request>`, then, once `<go-file>` exists, both
//! answers (`answered <reply> <reply>`) and `cleared` when the approval
//! went away. It exits 0 when all went as described, else 1 with why.
//!
//! Tests and checks only: it never runs against a real setup by itself.

// Rust guideline compliant 2026-10-09

#[path = "../tests/test_phone/mod.rs"]
mod test_phone;

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use harlo_core::phone::{Identity, Offer};
use serde_json::json;

fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [link, session, go] = args.as_slice() else {
        eprintln!("usage: phone_stub <link> <session> <go-file>");
        return ExitCode::FAILURE;
    };
    match run(link, session, Path::new(go)).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            say(&format!("failed {why}"));
            ExitCode::FAILURE
        }
    }
}

async fn run(link: &str, session: &str, go: &Path) -> Result<(), String> {
    let offer = Offer::decode(link)?;
    // The offer lists this computer's network addresses; the stand-in
    // reaches the same port over loopback.
    let port = offer
        .endpoints
        .first()
        .and_then(|e| e.rsplit_once(':'))
        .and_then(|(_, rest)| rest.split('/').next())
        .ok_or("the offer names no address")?
        .to_owned();
    let endpoint = format!("ws://127.0.0.1:{port}/ws");
    let mut phone =
        test_phone::connect_to(&endpoint, &Identity::generate(), &offer.key, &offer.code)
            .await
            .map_err(|answer| format!("turned away: {answer}"))?;
    say("paired");

    // The session shows in the list once the bridge has caught up.
    let mut listed = false;
    for _ in 0..100 {
        let list = phone.call(json!({ "op": "sessions" })).await;
        if list["sessions"]
            .as_array()
            .is_some_and(|l| l.iter().any(|s| s["key"] == session))
        {
            listed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !listed {
        return Err(format!("the session {session} was never listed"));
    }
    phone.call(json!({ "op": "open", "key": session })).await;

    let request = phone
        .until("the approval", |_, m| {
            (m["ev"] == "request").then(|| m["request"].clone())
        })
        .await;
    say(&format!(
        "asked {}",
        request["id"].as_str().unwrap_or_default()
    ));

    while !go.exists() {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // The same answer twice, back to back, before either reply.
    let answer = json!({ "op": "answer", "key": session, "requestId": request["id"],
                         "answer": { "value": "Allow once" } });
    let first = phone.send(answer.clone()).await;
    let second = phone.send(answer).await;
    let (first, second) = (phone.reply(first).await, phone.reply(second).await);
    say(&format!("answered {} {}", first["ok"], second["ok"]));
    phone
        .until("cleared", |_, m| {
            (m["ev"] == "cleared" && m["id"] == request["id"]).then_some(())
        })
        .await;
    say("cleared");
    Ok(())
}
