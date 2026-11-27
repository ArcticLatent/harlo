//! What providers tell of an account's use, for the clients' usage meter.
//!
//! Each provider is asked the way its own API offers, with the key the
//! server already keeps for it: `OpenRouter` tells how much of a key's
//! spending limit and of its budget is gone, `DeepSeek` what is left to
//! spend. `OpenAI` tells an app nothing of a `ChatGPT` plan's use, so
//! clients get the page that shows it. Claude's plan is read from Claude
//! Code by the clients themselves.
//!
//! Beside what a provider tells, the server knows when a provider's last
//! request was refused for a limit, which is all there is to show for
//! most of them. A reading is kept for a short while, so several clients
//! asking cost a provider one request.

// Rust guideline compliant 2026-10-09

use std::collections::HashSet;
use std::time::{Duration, Instant};

use harlo_engine::ProviderSpec;
use serde_json::{Value, json};

use crate::server::{Server, lock};

/// How long a reading is handed out before the providers are asked again.
///
/// Clients ask when a turn ends; several of them, or several turns ending
/// together, then share one reading.
const FRESH_FOR: Duration = Duration::from_secs(30);

/// How long a provider is waited for before its reading counts as failed.
const WAIT: Duration = Duration::from_secs(10);

/// Where a `ChatGPT` plan's use is shown and managed, which `OpenAI` asks
/// apps to link to.
const CHATGPT_USAGE: &str = "https://chatgpt.com/settings/usage";

/// The status a provider refuses a request over a limit with.
const TOO_MANY: u16 = 429;

/// How providers word a spent limit in an error, in lower case.
const SPENT: &[&str] = &[
    "usage limit",
    "usage_limit",
    "quota",
    "insufficient balance",
    "insufficient credits",
];

/// What is known of the providers' use.
#[derive(Debug, Default)]
pub(crate) struct Usage {
    /// The last readings, with when they were made.
    read: Option<(Instant, Value)>,
    /// The providers whose last request was refused for a limit.
    limited: HashSet<String>,
}

/// Where `spec`'s use is read: its API, or the stand-in `HARLO_USAGE_BASE`
/// names (`<base>/<provider>`), for tests.
fn base(spec: &ProviderSpec) -> String {
    std::env::var("HARLO_USAGE_BASE")
        .ok()
        .filter(|b| !b.trim().is_empty())
        .map_or_else(
            || spec.base_url.trim_end_matches('/').to_owned(),
            |b| format!("{}/{}", b.trim_end_matches('/'), spec.id),
        )
}

/// Asks `url` with `key` as a bearer token, for the JSON it answers.
async fn ask(url: &str, key: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(WAIT)
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .bearer_auth(key)
        .send()
        .await
        .map_err(|e| format!("It couldn't be reached: {}.", e.without_url()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("It answered {}.", status.as_u16()));
    }
    response
        .json()
        .await
        .map_err(|e| format!("Its answer couldn't be read: {}.", e.without_url()))
}

/// A number a provider wrote as one or as text.
fn number(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// One limit of which `spent` of `limit` is gone.
fn limit(label: &str, spent: f64, limit: f64, resets_at: Option<i64>, period: &Value) -> Value {
    json!({
        "label": label,
        "percent": (spent / limit * 100.0).clamp(0.0, 100.0),
        "resetsAt": resets_at,
        "period": period.as_str(),
    })
}

/// What `OpenRouter` says of a key (`GET /key`): its budget and its own
/// spending limit, each when it has one, and what it spent this month.
fn openrouter(answer: &Value) -> Value {
    let data = &answer["data"];
    let mut limits = Vec::new();
    let budget = &data["effective_budget"];
    if let (Some(cap), Some(spent)) = (number(&budget["limit_usd"]), number(&budget["spend_usd"]))
        && cap > 0.0
    {
        let resets = budget["resets_at"]
            .as_str()
            .and_then(|at| at.parse::<jiff::Timestamp>().ok())
            .map(jiff::Timestamp::as_second);
        limits.push(limit(
            "Budget",
            spent,
            cap,
            resets,
            &budget["reset_interval"],
        ));
    }
    if let Some(cap) = number(&data["limit"])
        && cap > 0.0
    {
        let spent = number(&data["limit_remaining"])
            .map(|left| cap - left)
            .or_else(|| number(&data["usage"]))
            .unwrap_or(0.0);
        limits.push(limit("Key limit", spent, cap, None, &data["limit_reset"]));
    }
    let mut out = json!({ "limits": limits });
    if let Some(month) = number(&data["usage_monthly"]) {
        out["amount"] = json!({ "label": "Spent this month", "value": month, "currency": "USD" });
    }
    out
}

/// What `DeepSeek` says of an account (`GET /user/balance`): what is
/// left to spend, in dollars when it keeps a balance in them.
fn deepseek(answer: &Value) -> Value {
    let balances = answer["balance_infos"].as_array();
    let balance = balances.and_then(|all| {
        all.iter()
            .find(|b| b["currency"] == "USD")
            .or_else(|| all.first())
    });
    let mut out = json!({ "limits": [] });
    if let Some(b) = balance
        && let Some(left) = number(&b["total_balance"])
    {
        out["amount"] = json!({
            "label": "Balance",
            "value": left,
            "currency": b["currency"].as_str().unwrap_or("USD"),
        });
    }
    if answer["is_available"] == false {
        out["reached"] = json!(true);
    }
    out
}

/// Reads what the provider `spec` tells of its use, when it tells any.
async fn read(spec: &ProviderSpec) -> Option<Value> {
    let key = spec.api_key.as_deref()?;
    let base = base(spec);
    let read = match spec.id.as_str() {
        "openrouter" => ask(&format!("{base}/key"), key)
            .await
            .map(|a| openrouter(&a)),
        "deepseek" => {
            // The balance sits beside the versioned API, not under it.
            let root = base.trim_end_matches("/v1");
            ask(&format!("{root}/user/balance"), key)
                .await
                .map(|a| deepseek(&a))
        }
        _ => return None,
    };
    let mut out = read.unwrap_or_else(|why| {
        tracing::warn!(name: "usage.read.failed", provider = %spec.id, reason = %why, "couldn't read {{provider}}'s use: {{reason}}");
        json!({ "limits": [], "error": why })
    });
    out["providerID"] = json!(spec.id);
    Some(out)
}

impl Server {
    /// Forgets the last reading, as when a provider was signed in to or
    /// out of.
    pub(crate) fn usage_changed(&self) {
        lock(&self.live).usage.read = None;
    }

    /// Notes how a request to `provider` ended, as far as its limits go.
    ///
    /// A request refused over a limit marks the provider as limited until
    /// one of its requests succeeds again.
    pub(crate) fn note_limit(&self, provider: &str, failed: Option<&harlo_engine::Error>) {
        let limited = failed.is_some_and(|e| {
            let said = e.to_string().to_lowercase();
            e.status() == Some(TOO_MANY) || SPENT.iter().any(|word| said.contains(word))
        });
        let mut live = lock(&self.live);
        let changed = if limited {
            live.usage.limited.insert(provider.to_owned())
        } else if failed.is_none() {
            live.usage.limited.remove(provider)
        } else {
            false
        };
        if changed {
            live.usage.read = None;
        }
    }

    /// What the signed-in providers tell of their use, one entry for each
    /// that tells something or was refused over a limit.
    ///
    /// An entry names its `providerID` and carries its `limits` (each a
    /// `label`, the `percent` used, and when it has them `resetsAt` in
    /// Unix seconds and its `period`), an `amount` spent or left, a
    /// `manage` page, whether its last request was refused over a limit
    /// (`reached`), and an `error` when it couldn't be read.
    pub(crate) async fn usage(&self) -> Value {
        if let Some((at, read)) = &lock(&self.live).usage.read
            && at.elapsed() < FRESH_FOR
        {
            return read.clone();
        }
        let config = self.with_sign_ins();
        let mut entries: Vec<Value> = Vec::new();
        for spec in &config.providers {
            entries.extend(read(spec).await);
        }
        let plan = self
            .store
            .credentials()
            .unwrap_or_default()
            .iter()
            .any(|c| {
                c["integrationID"] == "openai"
                    && c["active"] == true
                    && c["value"]["methodID"] == crate::oauth::CHATGPT
            });
        if plan {
            entries.push(json!({
                "providerID": "openai",
                "plan": "ChatGPT plan",
                "limits": [],
                "manage": CHATGPT_USAGE,
            }));
        }
        let mut live = lock(&self.live);
        for provider in &live.usage.limited {
            match entries.iter_mut().find(|e| e["providerID"] == *provider) {
                Some(entry) => entry["reached"] = json!(true),
                None => entries.push(json!({
                    "providerID": provider, "limits": [], "reached": true,
                })),
            }
        }
        for entry in &mut entries {
            if entry["reached"].is_null() {
                entry["reached"] = json!(false);
            }
        }
        let read = Value::Array(entries);
        live.usage.read = Some((Instant::now(), read.clone()));
        read
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{deepseek, openrouter};

    #[test]
    fn a_keys_budget_and_limit_are_each_a_share_used() {
        let read = openrouter(&json!({ "data": {
            "effective_budget": {
                "limit_usd": 50, "spend_usd": 32.5, "remaining_usd": 17.5,
                "reset_interval": "weekly", "resets_at": "2026-10-12T00:00:00.000Z",
            },
            "limit": 100, "limit_remaining": 74.5, "limit_reset": "monthly",
            "usage": 250.0, "usage_monthly": 25.5,
        }}));
        assert_eq!(read["limits"][0]["label"], "Budget");
        assert_eq!(read["limits"][0]["percent"], 65.0);
        assert_eq!(read["limits"][0]["resetsAt"], 1_791_763_200);
        assert_eq!(read["limits"][0]["period"], "weekly");
        // What is left of the limit counts, not all the key ever spent.
        assert_eq!(read["limits"][1]["label"], "Key limit");
        assert_eq!(read["limits"][1]["percent"], 25.5);
        assert_eq!(read["amount"]["value"], 25.5);

        // A key with no limit has only what it spent.
        let free = openrouter(&json!({ "data": {
            "limit": null, "limit_remaining": null, "usage": 3.0, "usage_monthly": 1.25,
        }}));
        assert_eq!(free["limits"], json!([]));
        assert_eq!(free["amount"]["value"], 1.25);
    }

    #[test]
    fn a_balance_is_what_is_left_and_an_empty_one_is_reached() {
        let read = deepseek(&json!({ "is_available": true, "balance_infos": [
            { "currency": "CNY", "total_balance": "110.00" },
            { "currency": "USD", "total_balance": "4.20" },
        ]}));
        assert_eq!(read["amount"]["value"], 4.2);
        assert_eq!(read["amount"]["currency"], "USD");
        assert!(read["reached"].is_null());

        let empty = deepseek(&json!({ "is_available": false, "balance_infos": [
            { "currency": "CNY", "total_balance": "0.00" },
        ]}));
        assert_eq!(empty["amount"]["currency"], "CNY");
        assert_eq!(empty["reached"], true);
    }
}
