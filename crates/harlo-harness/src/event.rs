//! Harness-agnostic event vocabulary and the per-harness wire translators.
//!
//! This module is the only place that knows the pi-protocol event shapes
//! harlo-agent speaks. Everything downstream (reducer, UI) consumes
//! [`HarnessEvent`].

use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HarnessKind {
    /// Claude Code (`claude`, stream-json; see `claude.rs`).
    Claude,
    /// harlo's own agent (`harlo-agent --mode rpc`: pi's protocol, with
    /// harlo's approvals and modes built in).
    Harlo,
    /// Synthetic event source for UI development and benchmarks.
    Demo,
}

impl HarnessKind {
    /// The providers, in Settings → Providers' order.
    pub const PROVIDERS: [HarnessKind; 2] = [Self::Harlo, Self::Claude];

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "claude" => Some(Self::Claude),
            "harlo" => Some(Self::Harlo),
            "demo" => Some(Self::Demo),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Harlo => "harlo",
            Self::Demo => "demo",
        }
    }

    /// Its name for people.
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Harlo => "Harlo",
            Self::Demo => "Demo",
        }
    }

    /// harlo can run sessions with it (a provider it only lists is shown in
    /// Settings → Providers but not offered).
    pub fn can_chat(self) -> bool {
        matches!(self, Self::Claude | Self::Harlo | Self::Demo)
    }

    /// The program's name (on PATH, on another machine).
    pub fn program_name(self) -> &'static str {
        match self {
            Self::Harlo => "harlo-agent",
            other => other.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Text,
    Thinking,
    ToolCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

impl NoticeLevel {
    fn parse(s: Option<&str>) -> Self {
        match s {
            Some("warning" | "warn") => Self::Warning,
            Some("error") => Self::Error,
            _ => Self::Info,
        }
    }
}

#[derive(Debug, Clone)]
pub enum HarnessEvent {
    /// The process is ready for commands.
    Ready,
    RunStarted,
    RunEnded,
    /// No automatic work remains (`agent_settled`).
    Settled,
    /// How many commands the agent left running in the background (Claude
    /// Code's): it picks the work up again when one finishes.
    BackgroundTasks(usize),

    /// `message` is the harness `AgentMessage` as sent on the wire.
    MessageStart { id: String, message: Value },
    BlockStart { id: String, index: usize, kind: BlockKind, tool: Option<(String, String)> },
    BlockDelta { id: String, index: usize, kind: BlockKind, delta: String },
    /// Authoritative block content: a string for text/thinking, the
    /// `ToolCall` object for tool calls.
    BlockEnd { id: String, index: usize, kind: BlockKind, content: Value },
    /// Authoritative final message.
    MessageEnd { id: String, message: Value },

    ToolStart { call_id: String, name: String, args: Value },
    ToolUpdate { call_id: String, partial: Value },
    ToolEnd { call_id: String, result: Value, is_error: bool },

    Compaction { started: bool, reason: Option<String>, error: Option<String> },
    Retry { started: bool, attempt: u64, max_attempts: u64, error: Option<String> },
    QueueChanged { steering: Vec<String>, follow_up: Vec<String> },
    /// A subagent started, progressed or ended (the frame, for its id).
    SubagentActivity(Value),
    /// harlo-agent calling one of harlo's tools on its pipe (answered with
    /// a `harlo_result` command of the same id).
    HarloCall { id: String, tool: String, args: Value },
    Renamed(Option<String>),
    ThinkingLevel(String),
    ModelChanged(Value),
    /// The slash-command list changed (Claude Code's).
    CommandsChanged(Value),
    /// Plan limits after a request. Claude: `rate_limit_info` (each
    /// window's `utilization`, `resetsAt` in seconds and `status`). Harlo:
    /// `{ provider, account, windows: [{ windowMinutes, usedPercent,
    /// resetsAt (s) }] }` (it has a `provider`).
    Limits(Value),
    /// The context's size or window became known outside a turn (ask
    /// `get_session_stats`).
    ContextChanged,
    /// A side question (`/btw`) and its answer as it comes: `{ question,
    /// text, done, error? }` (Harlo's `harlo_aside`).
    Aside(Value),

    /// `system` is true for the harness's own status notices (`notice`), as
    /// opposed to messages meant for the user.
    Notice { level: NoticeLevel, text: String, system: bool },
    /// Extension status-bar entry (`setStatus`); `None` clears it.
    Status { key: String, text: Option<String> },
    /// Extension widget lines (`setWidget`); `None` clears it.
    Widget { key: String, lines: Option<Vec<String>> },
    /// A blocking dialog (select/confirm/input/editor/ask) awaiting an
    /// `extension_ui_response`.
    UiRequest(Value),
    /// The harness withdrew a dialog (`cancel` on timeout/abort).
    UiCancel { target_id: String },

    /// A command response nobody was waiting for (e.g. a failed fire-and-forget send).
    Response { id: Option<String>, command: String, success: bool, data: Value, error: Option<String> },

    /// Protocol problem that did not kill the process.
    ProtocolError(String),
    Exited { code: Option<i32>, stderr_tail: String },
    /// Anything not yet modelled; kept so nothing is silently lost.
    Other(Value),
}

/// Stateful translator from one harness's wire frames to [`HarnessEvent`]s.
pub struct Translator {
    kind: HarnessKind,
    msg_seq: u64,
    current_msg: Option<String>,
}

impl Translator {
    pub fn new(kind: HarnessKind) -> Self {
        Self { kind, msg_seq: 0, current_msg: None }
    }

    fn message_id(&mut self, starting: bool) -> String {
        // Messages never interleave: a sequence number per message_start is
        // enough.
        if starting || self.current_msg.is_none() {
            self.msg_seq += 1;
            self.current_msg = Some(format!("{}-m{}", self.kind.as_str(), self.msg_seq));
        }
        self.current_msg.clone().expect("current message id")
    }

    pub fn translate(&mut self, frame: Value) -> Vec<HarnessEvent> {
        use HarnessEvent as E;
        let ty = frame["type"].as_str().unwrap_or_default().to_owned();
        let ev = match ty.as_str() {
            "agent_start" => E::RunStarted,
            "agent_end" => E::RunEnded,
            "agent_settled" => E::Settled,

            "message_start" => {
                let id = self.message_id(true);
                E::MessageStart { id, message: take(frame, "message") }
            }
            "message_end" => {
                let id = self.message_id(false);
                E::MessageEnd { id, message: take(frame, "message") }
            }
            "message_update" => {
                let id = self.message_id(false);
                match translate_block_event(id, &frame["assistantMessageEvent"]) {
                    Some(ev) => ev,
                    None => return Vec::new(),
                }
            }

            "tool_execution_start" => E::ToolStart {
                call_id: str_of(&frame, "toolCallId"),
                name: str_of(&frame, "toolName"),
                args: frame["args"].clone(),
            },
            "tool_execution_update" => E::ToolUpdate {
                call_id: str_of(&frame, "toolCallId"),
                partial: frame["partialResult"].clone(),
            },
            "tool_execution_end" => E::ToolEnd {
                call_id: str_of(&frame, "toolCallId"),
                is_error: frame["isError"].as_bool().unwrap_or(false),
                result: take(frame, "result"),
            },

            "compaction_start" | "auto_compaction_start" => E::Compaction {
                started: true,
                reason: opt_str(&frame, "reason"),
                error: None,
            },
            "compaction_end" | "auto_compaction_end" => E::Compaction {
                started: false,
                reason: opt_str(&frame, "reason"),
                // (Stopped: it didn't compact either.)
                error: opt_str(&frame, "errorMessage").or_else(|| (frame["aborted"] == true).then(|| "stopped".to_owned())),
            },
            "auto_retry_start" => E::Retry {
                started: true,
                attempt: frame["attempt"].as_u64().unwrap_or(0),
                max_attempts: frame["maxAttempts"].as_u64().unwrap_or(0),
                error: opt_str(&frame, "errorMessage"),
            },
            "auto_retry_end" => E::Retry {
                started: false,
                attempt: frame["attempt"].as_u64().unwrap_or(0),
                max_attempts: 0,
                error: opt_str(&frame, "finalError"),
            },
            "subagent_lifecycle" | "subagent_progress" => E::SubagentActivity(frame),
            "harlo_limits" => E::Limits(json!({ "provider": frame["provider"], "account": frame["account"], "windows": frame["windows"] })),
            "harlo_aside" => E::Aside(json!({ "question": frame["question"], "text": frame["text"], "done": frame["done"], "error": frame["error"] })),
            "harlo_call" => E::HarloCall { id: str_of(&frame, "id"), tool: str_of(&frame, "tool"), args: frame["args"].clone() },
            "queue_update" => E::QueueChanged {
                steering: string_list(&frame["steering"]),
                follow_up: string_list(&frame["followUp"]),
            },
            "session_info_changed" => E::Renamed(opt_str(&frame, "name")),
            "thinking_level_changed" => E::ThinkingLevel(
                opt_str(&frame, "level").or_else(|| opt_str(&frame, "thinkingLevel")).unwrap_or_default(),
            ),
            "model_changed" => E::ModelChanged(take(frame, "model")),
            "notice" => E::Notice {
                level: NoticeLevel::parse(frame["level"].as_str()),
                text: str_of(&frame, "message"),
                system: true,
            },
            "extension_ui_request" => translate_ui_request(frame),
            "response" => E::Response {
                id: opt_str(&frame, "id"),
                command: str_of(&frame, "command"),
                success: frame["success"].as_bool().unwrap_or(false),
                error: opt_str(&frame, "error"),
                data: take(frame, "data"),
            },
            _ => E::Other(frame),
        };
        vec![ev]
    }
}

fn translate_block_event(id: String, ev: &Value) -> Option<HarnessEvent> {
    use HarnessEvent as E;
    let ty = ev["type"].as_str()?;
    let index = ev["contentIndex"].as_u64().unwrap_or(0) as usize;
    let (kind, phase) = match ty {
        "text_start" => (BlockKind::Text, 0),
        "text_delta" => (BlockKind::Text, 1),
        "text_end" => (BlockKind::Text, 2),
        "thinking_start" => (BlockKind::Thinking, 0),
        "thinking_delta" => (BlockKind::Thinking, 1),
        "thinking_end" => (BlockKind::Thinking, 2),
        "toolcall_start" => (BlockKind::ToolCall, 0),
        "toolcall_delta" => (BlockKind::ToolCall, 1),
        "toolcall_end" => (BlockKind::ToolCall, 2),
        // start/done/error are folded into message_start/message_end.
        _ => return None,
    };
    Some(match phase {
        0 => E::BlockStart {
            id,
            index,
            kind,
            tool: (kind == BlockKind::ToolCall).then(|| (str_of(ev, "id"), str_of(ev, "toolName"))),
        },
        1 => E::BlockDelta { id, index, kind, delta: str_of(ev, "delta") },
        _ => E::BlockEnd {
            id,
            index,
            kind,
            content: if kind == BlockKind::ToolCall { ev["toolCall"].clone() } else { ev["content"].clone() },
        },
    })
}

fn translate_ui_request(frame: Value) -> HarnessEvent {
    use HarnessEvent as E;
    match frame["method"].as_str().unwrap_or_default() {
        "notify" => E::Notice {
            level: NoticeLevel::parse(frame["notifyType"].as_str()),
            text: str_of(&frame, "message"),
            system: false,
        },
        "setStatus" => E::Status { key: str_of(&frame, "statusKey"), text: opt_str(&frame, "statusText") },
        "setWidget" => E::Widget {
            key: str_of(&frame, "widgetKey"),
            lines: frame["widgetLines"].as_array().map(|a| a.iter().filter_map(|l| l.as_str().map(str::to_owned)).collect()),
        },
        "cancel" => E::UiCancel { target_id: str_of(&frame, "targetId") },
        "setTitle" | "set_editor_text" | "open_url" => E::Other(frame),
        _ => E::UiRequest(frame),
    }
}

fn take(mut frame: Value, key: &str) -> Value {
    frame.get_mut(key).map(Value::take).unwrap_or(Value::Null)
}

fn str_of(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_owned()
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v[key].as_str().map(str::to_owned)
}

fn string_list(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|item| item.as_str().map(str::to_owned).or_else(|| item["text"].as_str().map(str::to_owned)))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn synthesizes_message_ids() {
        let mut t = Translator::new(HarnessKind::Harlo);
        let start = t.translate(json!({"type":"message_start","message":{"role":"assistant","content":[]}}));
        let delta = t.translate(json!({"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"Hi"}}));
        let HarnessEvent::MessageStart { id: a, .. } = &start[0] else { panic!() };
        let HarnessEvent::BlockDelta { id: b, delta, .. } = &delta[0] else { panic!() };
        assert_eq!(a, b);
        assert_eq!(delta, "Hi");
        let next = t.translate(json!({"type":"message_start","message":{"role":"user","content":"x"}}));
        let HarnessEvent::MessageStart { id: c, .. } = &next[0] else { panic!() };
        assert_ne!(a, c);
    }

    #[test]
    fn harlo_agents_plan_limits() {
        let mut t = Translator::new(HarnessKind::Harlo);
        let w = json!([{ "windowMinutes": 300, "usedPercent": 12.5, "resetsAt": 1792300303 }]);
        let ev = t.translate(json!({ "type": "harlo_limits", "provider": "openai-codex", "account": "a@b", "windows": w }));
        let HarnessEvent::Limits(l) = &ev[0] else { panic!("{ev:?}") };
        assert_eq!(l, &json!({ "provider": "openai-codex", "account": "a@b", "windows": w }));
    }

    #[test]
    fn harlo_agent_calls_harlos_tools() {
        let mut t = Translator::new(HarnessKind::Harlo);
        let ev = t.translate(json!({"type":"harlo_aside","question":"why?","text":"Because.","done":true}));
        assert!(matches!(&ev[..], [HarnessEvent::Aside(a)] if a["question"] == "why?" && a["text"] == "Because." && a["done"] == true && a["error"].is_null()), "{ev:?}");
        let ev = t.translate(json!({"type":"harlo_call","id":"hc-3","tool":"list_sessions","args":{"all":true}}));
        let HarnessEvent::HarloCall { id, tool, args } = &ev[0] else { panic!("{ev:?}") };
        assert_eq!((id.as_str(), tool.as_str(), args), ("hc-3", "list_sessions", &json!({"all":true})));
    }
}
