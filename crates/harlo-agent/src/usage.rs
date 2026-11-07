//! The ChatGPT plan's usage, as the Codex backend says it (the Codex
//! sign-in only): `GET chatgpt.com/backend-api/wham/usage`, each window's
//! percent used, length and reset.

use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

use crate::auth::{self, Sign};

/// The usage now, as harlo takes a session's limits: `{ provider, account,
/// plan, windows: [{ windowMinutes, usedPercent, resetsAt (unix s) }] }`.
pub async fn fetch() -> Result<Value> {
    let creds = auth::credentials_now(Sign::Codex, false).await?;
    let resp = crate::responses::client()
        .get(format!("{}/wham/usage", crate::providers::chatgpt_backend()))
        .bearer_auth(&creds.access_token)
        .header("chatgpt-account-id", &creds.account_id)
        .header("originator", "harlo")
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| anyhow!("Couldn't reach ChatGPT: {e}"))?;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        bail!("ChatGPT didn't give the plan's usage ({status}): {}", body["error"]["message"].as_str().or_else(|| body["detail"].as_str()).unwrap_or("no reason given"));
    }
    Ok(parse(&body, &creds.email, now()))
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// The usage endpoint's answer in harlo's shape. A window's reset is its
/// `reset_at`, else `now` + `reset_after_seconds`.
pub fn parse(body: &Value, email: &str, now: i64) -> Value {
    let windows: Vec<Value> = ["primary_window", "secondary_window"]
        .iter()
        .filter_map(|k| {
            let w = &body["rate_limit"][*k];
            let used = w["used_percent"].as_f64()?;
            let minutes = w["limit_window_seconds"].as_i64().map(|s| (s + 30) / 60).filter(|m| *m > 0);
            let reset = w["reset_at"].as_i64().filter(|r| *r > 0).or_else(|| w["reset_after_seconds"].as_i64().filter(|s| *s > 0).map(|s| now + s));
            // (A window with nothing in it: a plan without that window.)
            (used != 0.0 || minutes.is_some() || reset.is_some()).then(|| json!({ "windowMinutes": minutes, "usedPercent": used, "resetsAt": reset }))
        })
        .collect();
    json!({ "provider": "openai-codex", "account": email, "plan": body["plan_type"].as_str().unwrap_or_default(), "windows": windows })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_usage() {
        let body = json!({ "plan_type": "plus", "rate_limit": {
            "allowed": true, "limit_reached": false,
            "primary_window": { "used_percent": 12, "limit_window_seconds": 18000, "reset_after_seconds": 600, "reset_at": 1792300303 },
            "secondary_window": { "used_percent": 40, "limit_window_seconds": 604800, "reset_after_seconds": 3600 } } });
        assert_eq!(parse(&body, "a@b", 1000), json!({ "provider": "openai-codex", "account": "a@b", "plan": "plus", "windows": [
            { "windowMinutes": 300, "usedPercent": 12.0, "resetsAt": 1792300303 },
            { "windowMinutes": 10080, "usedPercent": 40.0, "resetsAt": 4600 }] }));
        assert_eq!(parse(&json!({}), "", 0)["windows"], json!([]));
        let one = json!({ "rate_limit": { "primary_window": { "used_percent": 1, "limit_window_seconds": 604800, "reset_at": 1794537064 },
                                           "secondary_window": { "used_percent": 0, "limit_window_seconds": 0, "reset_after_seconds": 0, "reset_at": 0 } } });
        assert_eq!(parse(&one, "", 0)["windows"], json!([{ "windowMinutes": 10080, "usedPercent": 1.0, "resetsAt": 1794537064 }]));
    }
}
