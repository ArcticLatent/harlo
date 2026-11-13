//! What requests used: tokens in (the cache's share of them apart) and
//! out, and what they cost. Every request a session makes counts through
//! this, whatever made it: a turn, a compaction, a side question (`/btw`),
//! a web search, a subagent.
//!
//! A cost is only ever what's known: a ChatGPT plan's requests are in the
//! plan, a server on this computer or network charges nothing, a model
//! with a price (`providers.json`, or its server's list) is priced, and
//! any other is unknown: never a made-up 0.

use serde_json::{Value, json};

/// Dollars per million tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    /// Input the cache served.
    pub cached: f64,
}

/// What a model's requests cost.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Pricing {
    /// In a ChatGPT plan.
    Plan,
    /// A server on this computer or its network.
    Local,
    Priced(Price),
    #[default]
    Unknown,
}

/// The kinds of cost in a total (several, added up).
pub const PLAN: u8 = 1;
pub const LOCAL: u8 = 2;
pub const PRICED: u8 = 4;
pub const UNKNOWN: u8 = 8;

impl Pricing {
    pub fn kind(&self) -> u8 {
        match self {
            Self::Plan => PLAN,
            Self::Local => LOCAL,
            Self::Priced(_) => PRICED,
            Self::Unknown => UNKNOWN,
        }
    }

    /// Its dollars: input, cache reads, output (None: unknown).
    fn charge(&self, u: &Used) -> Option<(f64, f64, f64)> {
        match self {
            Self::Plan | Self::Local => Some((0.0, 0.0, 0.0)),
            Self::Priced(p) => {
                let m = |tokens: u64, rate: f64| tokens as f64 * rate / 1e6;
                Some((m(u.input.saturating_sub(u.cached), p.input), m(u.cached, p.cached), m(u.output, p.output)))
            }
            Self::Unknown => None,
        }
    }
}

fn kind_name(kind: u8) -> &'static str {
    match kind {
        PLAN => "plan",
        LOCAL => "local",
        PRICED => "priced",
        UNKNOWN => "unknown",
        _ => "",
    }
}

fn kind_of(name: &str) -> u8 {
    match name {
        "plan" => PLAN,
        "local" => LOCAL,
        "priced" => PRICED,
        "unknown" => UNKNOWN,
        _ => 0,
    }
}

/// One request's tokens and cost, or several added up.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Used {
    pub requests: u64,
    /// Tokens sent, the cache's included.
    pub input: u64,
    /// Of `input`, those the cache served.
    pub cached: u64,
    pub output: u64,
    /// Dollars, of the requests whose cost is known.
    pub cost: f64,
    /// The kinds of cost among them (`PLAN | PRICED`, say; 0: not said).
    pub kinds: u8,
    /// Of `requests`, those whose server didn't say what they used: their
    /// tokens aren't in these, and a priced one's cost is unknown.
    pub unreported: u64,
}

impl Used {
    /// A reply's usage as the API says it (`input_tokens`,
    /// `input_tokens_details.cached_tokens`, `output_tokens`).
    pub fn from_api(usage: &Value) -> Self {
        Self {
            requests: 1,
            input: usage["input_tokens"].as_u64().unwrap_or(0),
            cached: usage["input_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0),
            output: usage["output_tokens"].as_u64().unwrap_or(0),
            unreported: u64::from(usage["input_tokens"].as_u64().is_none()),
            ..Self::default()
        }
    }

    /// A usage as the session log has it (`input` without the cache's,
    /// `cacheRead`, `output`, `cost.total` with its `costKind`): one
    /// request. (One logged before costs were known says no kind.)
    pub fn from_log(usage: &Value) -> Self {
        let cached = usage["cacheRead"].as_u64().unwrap_or(0);
        let kinds = kind_of(usage["costKind"].as_str().unwrap_or_default());
        Self {
            requests: 1,
            input: usage["input"].as_u64().unwrap_or(0) + cached,
            cached,
            output: usage["output"].as_u64().unwrap_or(0),
            cost: if kinds == UNKNOWN { 0.0 } else { usage["cost"]["total"].as_f64().unwrap_or(0.0) },
            kinds,
            unreported: u64::from(usage["tokensUnknown"] == true),
        }
    }

    /// A tool result's `details.usage` (a web search's, a subagent's).
    pub fn from_details(usage: &Value) -> Self {
        Self {
            requests: usage["requests"].as_u64().unwrap_or(0),
            input: usage["input"].as_u64().unwrap_or(0),
            cached: usage["cached"].as_u64().unwrap_or(0),
            output: usage["output"].as_u64().unwrap_or(0),
            cost: usage["cost"].as_f64().unwrap_or(0.0),
            kinds: usage["costKinds"].as_array().into_iter().flatten().filter_map(Value::as_str).fold(0, |k, n| k | kind_of(n)),
            unreported: usage["unreported"].as_u64().unwrap_or(0),
        }
    }

    /// As a tool result's `details.usage`.
    pub fn details(&self) -> Value {
        let mut v = json!({ "requests": self.requests, "input": self.input, "cached": self.cached, "output": self.output, "cost": self.cost, "costKinds": self.kind_names() });
        if self.unreported > 0 {
            v["unreported"] = json!(self.unreported);
        }
        v
    }

    /// The pricing that holds for it: a price can't be put on tokens the
    /// server didn't say, so then it's unknown.
    fn pricing<'a>(&self, pricing: &'a Pricing) -> &'a Pricing {
        match pricing {
            Pricing::Priced(_) if self.unreported > 0 => &Pricing::Unknown,
            p => p,
        }
    }

    /// With its cost, as `pricing` has it.
    pub fn priced(mut self, pricing: &Pricing) -> Self {
        let pricing = self.pricing(pricing);
        self.cost = pricing.charge(&self).map_or(0.0, |(i, c, o)| i + c + o);
        self.kinds = pricing.kind();
        self
    }

    /// As the session log has it (one request), beside what isn't counted
    /// here (`reasoning`, from the API's usage). Its cost only when it's
    /// known.
    pub fn log(&self, api: &Value, pricing: &Pricing) -> Value {
        let pricing = self.pricing(pricing);
        let mut v = json!({
            "input": self.input.saturating_sub(self.cached), "output": self.output, "cacheRead": self.cached, "cacheWrite": 0,
            "reasoning": api["output_tokens_details"]["reasoning_tokens"].as_u64().unwrap_or(0),
            "totalTokens": self.input + self.output,
            "costKind": kind_name(pricing.kind()),
        });
        if let Some((i, c, o)) = pricing.charge(self) {
            v["cost"] = json!({ "input": i, "output": o, "cacheRead": c, "cacheWrite": 0, "total": i + c + o });
        }
        if self.unreported > 0 {
            v["tokensUnknown"] = json!(true);
        }
        v
    }

    fn kind_names(&self) -> Vec<&'static str> {
        [PLAN, LOCAL, PRICED, UNKNOWN].into_iter().filter(|k| self.kinds & k != 0).map(kind_name).collect()
    }

    /// What its cost is, in a word: "plan" or "local" (nothing extra),
    /// "priced", "partly" (priced, but some requests' costs unknown, so
    /// it's at least that), "unknown", or "" (nothing said yet).
    pub fn cost_kind(&self) -> &'static str {
        let k = self.kinds;
        match () {
            _ if k & UNKNOWN != 0 && k & PRICED != 0 => "partly",
            _ if k & UNKNOWN != 0 => "unknown",
            _ if k & PRICED != 0 => "priced",
            _ if k & PLAN != 0 => "plan",
            _ if k & LOCAL != 0 => "local",
            _ => "",
        }
    }

    /// What a token budget counts: tokens sent and received, not those the
    /// cache served.
    pub fn billed(&self) -> u64 {
        self.input.saturating_sub(self.cached) + self.output
    }

    pub fn add(&mut self, other: &Used) {
        self.requests += other.requests;
        self.input += other.input;
        self.cached += other.cached;
        self.output += other.output;
        self.cost += other.cost;
        self.kinds |= other.kinds;
        self.unreported += other.unreported;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shapes_agree() {
        let api = json!({ "input_tokens": 3000, "input_tokens_details": { "cached_tokens": 2048 }, "output_tokens": 120, "output_tokens_details": { "reasoning_tokens": 40 } });
        let u = Used::from_api(&api);
        assert_eq!(u, Used { requests: 1, input: 3000, cached: 2048, output: 120, ..Used::default() });
        assert_eq!(u.billed(), 952 + 120);
        // Logged and read back, or passed in a tool result: the same.
        let price = Pricing::Priced(Price { input: 2.0, output: 10.0, cached: 0.5 });
        let logged = u.log(&api, &price);
        assert_eq!((logged["input"].as_u64(), logged["cacheRead"].as_u64(), logged["reasoning"].as_u64()), (Some(952), Some(2048), Some(40)));
        let u = u.priced(&price);
        assert!((u.cost - (952.0 * 2.0 + 2048.0 * 0.5 + 120.0 * 10.0) / 1e6).abs() < 1e-12, "{}", u.cost);
        assert!((logged["cost"]["total"].as_f64().unwrap() - u.cost).abs() < 1e-12);
        assert_eq!(Used::from_log(&logged), u);
        assert_eq!(Used::from_details(&u.details()), u);
        let mut sum = u;
        sum.add(&u);
        assert_eq!((sum.requests, sum.billed(), sum.cost_kind()), (2, 2 * 1072, "priced"));
    }

    #[test]
    fn a_cost_is_never_a_made_up_zero() {
        let u = Used { requests: 1, input: 1000, output: 100, ..Used::default() };
        // Unknown: no cost in the log, and said so.
        let logged = u.log(&json!({}), &Pricing::Unknown);
        assert!(logged["cost"].is_null() && logged["costKind"] == "unknown", "{logged}");
        assert_eq!(Used::from_log(&logged).cost_kind(), "unknown");
        // In a plan or local: nothing extra, and said which.
        assert_eq!(u.log(&json!({}), &Pricing::Plan)["cost"]["total"], 0.0);
        assert_eq!(u.priced(&Pricing::Local).cost_kind(), "local");
        // Priced and unknown together: at least the priced part.
        let mut sum = u.priced(&Pricing::Priced(Price { input: 1.0, output: 1.0, cached: 1.0 }));
        sum.add(&u.priced(&Pricing::Unknown));
        sum.add(&u.priced(&Pricing::Plan));
        assert_eq!(sum.cost_kind(), "partly");
        assert!((sum.cost - 1100.0 / 1e6).abs() < 1e-12);
        // A log from before costs: no kind said.
        assert_eq!(Used::from_log(&json!({ "input": 5, "output": 1, "cost": { "total": 0 } })).cost_kind(), "");
    }
}
