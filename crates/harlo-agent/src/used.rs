//! What requests used: tokens in (the cache's share of them apart) and
//! out, and what they cost. Every request a session makes counts through
//! this, whatever made it: a turn, a compaction, a side question (`/btw`),
//! a web search, a subagent.
//!
//! A cost is only ever what's known: a ChatGPT plan's requests are in the
//! plan, a server on this computer or network charges nothing, a model
//! with a price (`providers.json`, or its server's list) is priced, and
//! any other is unknown: never a made-up 0.

// Rust guideline compliant 2026-10-09

use serde_json::{Value, json};

/// Dollars per million tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    /// Input the cache served.
    pub cached: f64,
    /// Input written to the cache (servers that charge for that).
    pub write: f64,
    /// The rates for a request with a long input, when they differ.
    pub long: Option<Long>,
}

/// The rates a request pays once its input reaches `from` tokens (all of
/// it, not just the part past `from`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Long {
    pub from: u64,
    pub input: f64,
    pub output: f64,
    pub cached: f64,
    pub write: f64,
}

impl Price {
    /// One set of rates, whatever the input's length; writing the cache
    /// costs what input does.
    pub const fn flat(input: f64, output: f64, cached: f64) -> Self {
        Self { input, output, cached, write: input, long: None }
    }
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

    /// Its dollars (None: unknown).
    fn charge(&self, u: &Used) -> Option<Charge> {
        match self {
            Self::Plan | Self::Local => Some(Charge::default()),
            Self::Priced(p) => {
                let m = |tokens: u64, rate: f64| tokens as f64 * rate / 1e6;
                let (input, cached, write, output) = match p.long {
                    Some(l) if u.requests <= 1 && u.input >= l.from => (l.input, l.cached, l.write, l.output),
                    _ => (p.input, p.cached, p.write, p.output),
                };
                Some(Charge { input: m(u.input.saturating_sub(u.cached + u.written), input), read: m(u.cached, cached), write: m(u.written, write), output: m(u.output, output) })
            }
            Self::Unknown => None,
        }
    }
}

/// A request's dollars, by what they paid for.
#[derive(Clone, Copy, Debug, Default)]
struct Charge {
    /// Input neither read from nor written to the cache.
    input: f64,
    read: f64,
    write: f64,
    output: f64,
}

impl Charge {
    fn total(self) -> f64 {
        self.input + self.read + self.write + self.output
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
    /// Of `input`, those written to the cache (servers that say it apart).
    pub written: u64,
    pub output: u64,
    /// Dollars, of the requests whose cost is known.
    pub cost: f64,
    /// The kinds of cost among them (`PLAN | PRICED`, say; 0: not said).
    pub kinds: u8,
    /// Of `requests`, those whose server didn't say (all of) what they
    /// used: their missing tokens aren't in these, and a priced one's cost
    /// is unknown.
    pub unreported: u64,
    /// What those left unsaid, estimated (a request's size, its reply's
    /// length): counted against a token budget only, so one it can't measure
    /// still uses it up, never shown as tokens or priced.
    pub estimated: u64,
}

impl Used {
    /// A reply's usage as the API says it (`input_tokens`,
    /// `input_tokens_details.cached_tokens`, `output_tokens`; the cache's
    /// writes as `input_tokens_details.cache_write_tokens`, harlo's).
    pub fn from_api(usage: &Value) -> Self {
        Self {
            requests: 1,
            input: usage["input_tokens"].as_u64().unwrap_or(0),
            cached: usage["input_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0),
            written: usage["input_tokens_details"]["cache_write_tokens"].as_u64().unwrap_or(0),
            output: usage["output_tokens"].as_u64().unwrap_or(0),
            unreported: u64::from(usage["input_tokens"].as_u64().is_none() || usage["output_tokens"].as_u64().is_none()),
            ..Self::default()
        }
    }

    /// With what `usage` left unsaid estimated: `input`, the request's
    /// size; `output`, its reply's.
    pub fn estimating(mut self, usage: &Value, input: u64, output: u64) -> Self {
        if usage["input_tokens"].as_u64().is_none() {
            self.estimated += input;
        }
        if usage["output_tokens"].as_u64().is_none() {
            self.estimated += output;
        }
        self
    }

    /// A usage as the session log has it (`input` without the cache's,
    /// `cacheRead`, `cacheWrite`, `output`, `cost.total` with its
    /// `costKind`): one request. (One logged before costs were known says
    /// no kind.)
    pub fn from_log(usage: &Value) -> Self {
        let cached = usage["cacheRead"].as_u64().unwrap_or(0);
        let written = usage["cacheWrite"].as_u64().unwrap_or(0);
        let kinds = kind_of(usage["costKind"].as_str().unwrap_or_default());
        Self {
            requests: 1,
            input: usage["input"].as_u64().unwrap_or(0) + cached + written,
            cached,
            written,
            output: usage["output"].as_u64().unwrap_or(0),
            cost: if kinds == UNKNOWN { 0.0 } else { usage["cost"]["total"].as_f64().unwrap_or(0.0) },
            kinds,
            unreported: u64::from(usage["tokensUnknown"] == true),
            estimated: usage["tokensEstimated"].as_u64().unwrap_or(0),
        }
    }

    /// A tool result's `details.usage` (a web search's, a subagent's).
    pub fn from_details(usage: &Value) -> Self {
        Self {
            requests: usage["requests"].as_u64().unwrap_or(0),
            input: usage["input"].as_u64().unwrap_or(0),
            cached: usage["cached"].as_u64().unwrap_or(0),
            written: usage["written"].as_u64().unwrap_or(0),
            output: usage["output"].as_u64().unwrap_or(0),
            cost: usage["cost"].as_f64().unwrap_or(0.0),
            kinds: usage["costKinds"].as_array().into_iter().flatten().filter_map(Value::as_str).fold(0, |k, n| k | kind_of(n)),
            unreported: usage["unreported"].as_u64().unwrap_or(0),
            estimated: usage["estimated"].as_u64().unwrap_or(0),
        }
    }

    /// As a tool result's `details.usage`.
    pub fn details(&self) -> Value {
        let mut v = json!({ "requests": self.requests, "input": self.input, "cached": self.cached, "output": self.output, "cost": self.cost, "costKinds": self.kind_names() });
        if self.written > 0 {
            v["written"] = json!(self.written);
        }
        if self.unreported > 0 {
            v["unreported"] = json!(self.unreported);
        }
        if self.estimated > 0 {
            v["estimated"] = json!(self.estimated);
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
        self.cost = pricing.charge(&self).map_or(0.0, Charge::total);
        self.kinds = pricing.kind();
        self
    }

    /// As the session log has it (one request), beside what isn't counted
    /// here (`reasoning`, from the API's usage). Its cost only when it's
    /// known.
    pub fn log(&self, api: &Value, pricing: &Pricing) -> Value {
        let pricing = self.pricing(pricing);
        let mut v = json!({
            "input": self.input.saturating_sub(self.cached + self.written), "output": self.output, "cacheRead": self.cached, "cacheWrite": self.written,
            "reasoning": api["output_tokens_details"]["reasoning_tokens"].as_u64().unwrap_or(0),
            "totalTokens": self.input + self.output,
            "costKind": kind_name(pricing.kind()),
        });
        if let Some(c) = pricing.charge(self) {
            v["cost"] = json!({ "input": c.input, "output": c.output, "cacheRead": c.read, "cacheWrite": c.write, "total": c.total() });
        }
        if self.unreported > 0 {
            v["tokensUnknown"] = json!(true);
        }
        if self.estimated > 0 {
            v["tokensEstimated"] = json!(self.estimated);
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
            () if k & UNKNOWN != 0 && k & PRICED != 0 => "partly",
            () if k & UNKNOWN != 0 => "unknown",
            () if k & PRICED != 0 => "priced",
            () if k & PLAN != 0 => "plan",
            () if k & LOCAL != 0 => "local",
            () => "",
        }
    }

    /// What a token budget counts: tokens sent and received (those written
    /// to the cache too), not those the cache served, and what servers
    /// didn't say, estimated.
    pub fn billed(&self) -> u64 {
        self.input.saturating_sub(self.cached) + self.output + self.estimated
    }

    pub fn add(&mut self, other: &Used) {
        self.requests += other.requests;
        self.input += other.input;
        self.cached += other.cached;
        self.written += other.written;
        self.output += other.output;
        self.cost += other.cost;
        self.kinds |= other.kinds;
        self.unreported += other.unreported;
        self.estimated += other.estimated;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a server leaves unsaid is estimated for the budget, apart from
    /// what it said, and kept when logged.
    #[test]
    fn unsaid_usage_is_estimated() {
        let none = json!({});
        let u = Used::from_api(&none).estimating(&none, 5_000, 300);
        assert_eq!((u.input, u.output, u.unreported, u.estimated, u.billed()), (0, 0, 1, 5_300, 5_300));
        let half = json!({ "input_tokens": 4_000 });
        let h = Used::from_api(&half).estimating(&half, 5_000, 300);
        assert_eq!((h.input, h.output, h.unreported, h.estimated, h.billed()), (4_000, 0, 1, 300, 4_300));
        let price = Pricing::Priced(Price::flat(2.0, 10.0, 0.5));
        let logged = h.priced(&price).log(&half, &price);
        assert_eq!((logged["costKind"].as_str(), logged["tokensEstimated"].as_u64()), (Some("unknown"), Some(300)));
        assert_eq!(Used::from_log(&logged).billed(), 4_300);
        assert_eq!(Used::from_details(&h.details()).billed(), 4_300);
        // Said in full: nothing estimated.
        let all = json!({ "input_tokens": 10, "output_tokens": 2 });
        assert_eq!(Used::from_api(&all).estimating(&all, 5_000, 300).estimated, 0);
    }

    #[test]
    fn the_shapes_agree() {
        let api = json!({ "input_tokens": 3000, "input_tokens_details": { "cached_tokens": 2048 }, "output_tokens": 120, "output_tokens_details": { "reasoning_tokens": 40 } });
        let u = Used::from_api(&api);
        assert_eq!(u, Used { requests: 1, input: 3000, cached: 2048, output: 120, ..Used::default() });
        assert_eq!(u.billed(), 952 + 120);
        // Logged and read back, or passed in a tool result: the same.
        let price = Pricing::Priced(Price::flat(2.0, 10.0, 0.5));
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

    /// Cache writes (Anthropic's) are input of their own, at their own rate.
    #[test]
    fn cache_writes_are_priced_apart() {
        let api = json!({ "input_tokens": 10_000, "input_tokens_details": { "cached_tokens": 6_000, "cache_write_tokens": 3_000 }, "output_tokens": 100 });
        let u = Used::from_api(&api);
        assert_eq!((u.input, u.cached, u.written, u.billed()), (10_000, 6_000, 3_000, 4_100));
        let price = Pricing::Priced(Price { write: 2.5, ..Price::flat(2.0, 10.0, 0.2) });
        let logged = u.log(&api, &price);
        assert_eq!((logged["input"].as_u64(), logged["cacheRead"].as_u64(), logged["cacheWrite"].as_u64()), (Some(1_000), Some(6_000), Some(3_000)));
        let u = u.priced(&price);
        assert!((u.cost - (1_000.0 * 2.0 + 6_000.0 * 0.2 + 3_000.0 * 2.5 + 100.0 * 10.0) / 1e6).abs() < 1e-12, "{}", u.cost);
        assert!((logged["cost"]["cacheWrite"].as_f64().unwrap() - 0.0075).abs() < 1e-12);
        assert_eq!(Used::from_log(&logged), u);
        assert_eq!(Used::from_details(&u.details()), u);
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
        let mut sum = u.priced(&Pricing::Priced(Price::flat(1.0, 1.0, 1.0)));
        sum.add(&u.priced(&Pricing::Unknown));
        sum.add(&u.priced(&Pricing::Plan));
        assert_eq!(sum.cost_kind(), "partly");
        assert!((sum.cost - 1100.0 / 1e6).abs() < 1e-12);
        // A log from before costs: no kind said.
        assert_eq!(Used::from_log(&json!({ "input": 5, "output": 1, "cost": { "total": 0 } })).cost_kind(), "");
    }
}
