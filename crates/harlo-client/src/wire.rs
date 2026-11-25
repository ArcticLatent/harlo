//! The server API's wire types, as they travel over HTTP and in events.
//!
//! First generated from an OpenAPI schema, and kept by hand since; private
//! to this crate.

#![allow(
    dead_code,
    clippy::all,
    clippy::pedantic,
    clippy::cargo,
    missing_docs,
    missing_debug_implementations,
    reason = "wire types, generated in bulk"
)]

///`AbsolutePath`
#[derive(
    ::serde::Deserialize, ::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd,
)]
#[serde(transparent)]
pub struct AbsolutePath(pub ::std::string::String);
impl ::std::ops::Deref for AbsolutePath {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<AbsolutePath> for ::std::string::String {
    fn from(value: AbsolutePath) -> Self {
        value.0
    }
}
impl ::std::convert::From<::std::string::String> for AbsolutePath {
    fn from(value: ::std::string::String) -> Self {
        Self(value)
    }
}
impl ::std::fmt::Display for AbsolutePath {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        self.0.fmt(f)
    }
}
impl ::std::str::FromStr for AbsolutePath {
    type Err = ::std::convert::Infallible;
    fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
        Ok(Self(value.to_string()))
    }
}
///`AgentId`
#[derive(
    ::serde::Deserialize, ::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd,
)]
#[serde(transparent)]
pub struct AgentId(pub ::std::string::String);
impl ::std::ops::Deref for AgentId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<AgentId> for ::std::string::String {
    fn from(value: AgentId) -> Self {
        value.0
    }
}
impl ::std::convert::From<::std::string::String> for AgentId {
    fn from(value: ::std::string::String) -> Self {
        Self(value)
    }
}
impl ::std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        self.0.fmt(f)
    }
}
impl ::std::str::FromStr for AgentId {
    type Err = ::std::convert::Infallible;
    fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
        Ok(Self(value.to_string()))
    }
}
///`AutomationAction`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum AutomationAction {
    Prompt(AutomationActionPrompt),
    Code(AutomationActionCode),
    Shell(AutomationActionShell),
}
impl ::std::convert::From<AutomationActionPrompt> for AutomationAction {
    fn from(value: AutomationActionPrompt) -> Self {
        Self::Prompt(value)
    }
}
impl ::std::convert::From<AutomationActionCode> for AutomationAction {
    fn from(value: AutomationActionCode) -> Self {
        Self::Code(value)
    }
}
impl ::std::convert::From<AutomationActionShell> for AutomationAction {
    fn from(value: AutomationActionShell) -> Self {
        Self::Shell(value)
    }
}
///`AutomationActionCode`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct AutomationActionCode {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub args: ::std::option::Option<::serde_json::Value>,
    pub code: ::std::string::String,
    #[serde(rename = "type")]
    pub type_: AutomationActionCodeType,
}
///`AutomationActionCodeType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum AutomationActionCodeType {
    #[serde(rename = "code")]
    Code,
}
impl ::std::fmt::Display for AutomationActionCodeType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Code => f.write_str("code"),
        }
    }
}
impl ::std::str::FromStr for AutomationActionCodeType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "code" => Ok(Self::Code),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for AutomationActionCodeType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AutomationActionCodeType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`AutomationActionPrompt`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct AutomationActionPrompt {
    pub prompt: ::std::string::String,
    pub to: AutomationTarget,
    #[serde(rename = "type")]
    pub type_: AutomationActionPromptType,
}
///`AutomationActionPromptType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum AutomationActionPromptType {
    #[serde(rename = "prompt")]
    Prompt,
}
impl ::std::fmt::Display for AutomationActionPromptType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Prompt => f.write_str("prompt"),
        }
    }
}
impl ::std::str::FromStr for AutomationActionPromptType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "prompt" => Ok(Self::Prompt),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for AutomationActionPromptType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AutomationActionPromptType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`AutomationActionShell`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct AutomationActionShell {
    pub command: AutomationActionShellCommand,
    #[serde(rename = "type")]
    pub type_: AutomationActionShellType,
}
///`AutomationActionShellCommand`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct AutomationActionShellCommand(::std::string::String);
impl ::std::ops::Deref for AutomationActionShellCommand {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<AutomationActionShellCommand> for ::std::string::String {
    fn from(value: AutomationActionShellCommand) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for AutomationActionShellCommand {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value)
    }
}
impl ::std::convert::TryFrom<&str> for AutomationActionShellCommand {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() < 1usize {
            return Err("shorter than 1 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AutomationActionShellCommand {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value.as_str())
    }
}
impl<'de> ::serde::Deserialize<'de> for AutomationActionShellCommand {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::convert::TryFrom::try_from(::std::string::String::deserialize(deserializer)?)
            .map_err(|e: self::error::ConversionError| {
                <D::Error as ::serde::de::Error>::custom(e.to_string())
            })
    }
}
///`AutomationActionShellType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum AutomationActionShellType {
    #[serde(rename = "shell")]
    Shell,
}
impl ::std::fmt::Display for AutomationActionShellType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Shell => f.write_str("shell"),
        }
    }
}
impl ::std::str::FromStr for AutomationActionShellType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "shell" => Ok(Self::Shell),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for AutomationActionShellType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AutomationActionShellType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///A duration such as "90 seconds", "5 minutes" or "2 hours".
#[derive(
    ::serde::Deserialize, ::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd,
)]
#[serde(transparent)]
pub struct AutomationDuration(pub ::std::string::String);
impl ::std::ops::Deref for AutomationDuration {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<AutomationDuration> for ::std::string::String {
    fn from(value: AutomationDuration) -> Self {
        value.0
    }
}
impl ::std::convert::From<::std::string::String> for AutomationDuration {
    fn from(value: ::std::string::String) -> Self {
        Self(value)
    }
}
impl ::std::fmt::Display for AutomationDuration {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        self.0.fmt(f)
    }
}
impl ::std::str::FromStr for AutomationDuration {
    type Err = ::std::convert::Infallible;
    fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
        Ok(Self(value.to_string()))
    }
}
///`AutomationId`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct AutomationId(::std::string::String);
impl ::std::ops::Deref for AutomationId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<AutomationId> for ::std::string::String {
    fn from(value: AutomationId) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for AutomationId {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value)
    }
}
impl ::std::convert::TryFrom<&str> for AutomationId {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| ::regress::Regex::with_flags("^aut_", "u").unwrap());
        if PATTERN.find(value).is_none() {
            return Err("doesn't match pattern \"^aut_\"".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AutomationId {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value.as_str())
    }
}
impl<'de> ::serde::Deserialize<'de> for AutomationId {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::convert::TryFrom::try_from(::std::string::String::deserialize(deserializer)?)
            .map_err(|e: self::error::ConversionError| {
                <D::Error as ::serde::de::Error>::custom(e.to_string())
            })
    }
}
///`AutomationInfo`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct AutomationInfo {
    pub action: AutomationAction,
    pub directory: ::std::string::String,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub expires: ::std::option::Option<f64>,
    pub id: AutomationId,
    #[serde(
        rename = "lastOutcome",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub last_outcome: ::std::option::Option<AutomationOutcome>,
    #[serde(
        rename = "lastOutput",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub last_output: ::std::option::Option<::std::string::String>,
    #[serde(
        rename = "lastRunAt",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub last_run_at: ::std::option::Option<f64>,
    pub name: ::std::string::String,
    #[serde(
        rename = "nextDueAt",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub next_due_at: ::std::option::Option<f64>,
    #[serde(rename = "projectID")]
    pub project_id: ProjectId,
    #[serde(rename = "runCount")]
    pub run_count: i64,
    #[serde(
        rename = "sessionID",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub session_id: ::std::option::Option<SessionId>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub state: ::std::option::Option<::serde_json::Value>,
    pub status: AutomationStatus,
    pub time: AutomationInfoTime,
    pub trigger: AutomationTrigger1,
    pub tz: ::std::string::String,
}
///`AutomationInfoTime`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct AutomationInfoTime {
    pub created: f64,
    pub updated: f64,
}
///`AutomationOutcome`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum AutomationOutcome {
    #[serde(rename = "fired")]
    Fired,
    #[serde(rename = "late")]
    Late,
    #[serde(rename = "skipped")]
    Skipped,
    #[serde(rename = "disabled")]
    Disabled,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "expired")]
    Expired,
}
impl ::std::fmt::Display for AutomationOutcome {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Fired => f.write_str("fired"),
            Self::Late => f.write_str("late"),
            Self::Skipped => f.write_str("skipped"),
            Self::Disabled => f.write_str("disabled"),
            Self::Failed => f.write_str("failed"),
            Self::Expired => f.write_str("expired"),
        }
    }
}
impl ::std::str::FromStr for AutomationOutcome {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "fired" => Ok(Self::Fired),
            "late" => Ok(Self::Late),
            "skipped" => Ok(Self::Skipped),
            "disabled" => Ok(Self::Disabled),
            "failed" => Ok(Self::Failed),
            "expired" => Ok(Self::Expired),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for AutomationOutcome {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AutomationOutcome {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`AutomationStatus`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum AutomationStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "paused")]
    Paused,
    #[serde(rename = "done")]
    Done,
}
impl ::std::fmt::Display for AutomationStatus {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Active => f.write_str("active"),
            Self::Paused => f.write_str("paused"),
            Self::Done => f.write_str("done"),
        }
    }
}
impl ::std::str::FromStr for AutomationStatus {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "done" => Ok(Self::Done),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for AutomationStatus {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AutomationStatus {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`AutomationTarget`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum AutomationTarget {
    Variant0 {
        session: SessionId,
        #[serde(skip_serializing_if = "::std::option::Option::is_none")]
        subagent: ::std::option::Option<::serde_json::Value>,
    },
    Variant1 {
        #[serde(skip_serializing_if = "::std::option::Option::is_none")]
        session: ::std::option::Option<::serde_json::Value>,
        subagent: AgentId,
    },
}
///`AutomationTrigger1`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum AutomationTrigger1 {
    At(AutomationTriggerAt),
    Every(AutomationTriggerEvery),
    Cron(AutomationTriggerCron),
    Event(AutomationTriggerEvent),
}
impl ::std::convert::From<AutomationTriggerAt> for AutomationTrigger1 {
    fn from(value: AutomationTriggerAt) -> Self {
        Self::At(value)
    }
}
impl ::std::convert::From<AutomationTriggerEvery> for AutomationTrigger1 {
    fn from(value: AutomationTriggerEvery) -> Self {
        Self::Every(value)
    }
}
impl ::std::convert::From<AutomationTriggerCron> for AutomationTrigger1 {
    fn from(value: AutomationTriggerCron) -> Self {
        Self::Cron(value)
    }
}
impl ::std::convert::From<AutomationTriggerEvent> for AutomationTrigger1 {
    fn from(value: AutomationTriggerEvent) -> Self {
        Self::Event(value)
    }
}
///`AutomationTriggerAt`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct AutomationTriggerAt {
    ///An instant with an offset, a wall-clock time in tz such as "2026-10-27T09:00", or "in 10 minutes".
    pub at: ::std::string::String,
    #[serde(rename = "type")]
    pub type_: AutomationTriggerAtType,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub tz: ::std::option::Option<::std::string::String>,
}
///`AutomationTriggerAtType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum AutomationTriggerAtType {
    #[serde(rename = "at")]
    At,
}
impl ::std::fmt::Display for AutomationTriggerAtType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::At => f.write_str("at"),
        }
    }
}
impl ::std::str::FromStr for AutomationTriggerAtType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "at" => Ok(Self::At),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for AutomationTriggerAtType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AutomationTriggerAtType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`AutomationTriggerCron`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct AutomationTriggerCron {
    pub cron: ::std::string::String,
    #[serde(rename = "type")]
    pub type_: AutomationTriggerCronType,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub tz: ::std::option::Option<::std::string::String>,
}
///`AutomationTriggerCronType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum AutomationTriggerCronType {
    #[serde(rename = "cron")]
    Cron,
}
impl ::std::fmt::Display for AutomationTriggerCronType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Cron => f.write_str("cron"),
        }
    }
}
impl ::std::str::FromStr for AutomationTriggerCronType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "cron" => Ok(Self::Cron),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for AutomationTriggerCronType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AutomationTriggerCronType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`AutomationTriggerEvent`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct AutomationTriggerEvent {
    #[serde(default, skip_serializing_if = "::serde_json::Map::is_empty")]
    pub args: ::serde_json::Map<::std::string::String, ::serde_json::Value>,
    pub event: AutomationTriggerEventEvent,
    pub server: AutomationTriggerEventServer,
    #[serde(rename = "type")]
    pub type_: AutomationTriggerEventType,
}
///`AutomationTriggerEventEvent`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct AutomationTriggerEventEvent(::std::string::String);
impl ::std::ops::Deref for AutomationTriggerEventEvent {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<AutomationTriggerEventEvent> for ::std::string::String {
    fn from(value: AutomationTriggerEventEvent) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for AutomationTriggerEventEvent {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value)
    }
}
impl ::std::convert::TryFrom<&str> for AutomationTriggerEventEvent {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() < 1usize {
            return Err("shorter than 1 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AutomationTriggerEventEvent {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value.as_str())
    }
}
impl<'de> ::serde::Deserialize<'de> for AutomationTriggerEventEvent {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::convert::TryFrom::try_from(::std::string::String::deserialize(deserializer)?)
            .map_err(|e: self::error::ConversionError| {
                <D::Error as ::serde::de::Error>::custom(e.to_string())
            })
    }
}
///`AutomationTriggerEventServer`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct AutomationTriggerEventServer(::std::string::String);
impl ::std::ops::Deref for AutomationTriggerEventServer {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<AutomationTriggerEventServer> for ::std::string::String {
    fn from(value: AutomationTriggerEventServer) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for AutomationTriggerEventServer {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value)
    }
}
impl ::std::convert::TryFrom<&str> for AutomationTriggerEventServer {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        if value.chars().count() < 1usize {
            return Err("shorter than 1 characters".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AutomationTriggerEventServer {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value.as_str())
    }
}
impl<'de> ::serde::Deserialize<'de> for AutomationTriggerEventServer {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::convert::TryFrom::try_from(::std::string::String::deserialize(deserializer)?)
            .map_err(|e: self::error::ConversionError| {
                <D::Error as ::serde::de::Error>::custom(e.to_string())
            })
    }
}
///`AutomationTriggerEventType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum AutomationTriggerEventType {
    #[serde(rename = "event")]
    Event,
}
impl ::std::fmt::Display for AutomationTriggerEventType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Event => f.write_str("event"),
        }
    }
}
impl ::std::str::FromStr for AutomationTriggerEventType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "event" => Ok(Self::Event),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for AutomationTriggerEventType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AutomationTriggerEventType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`AutomationTriggerEvery`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct AutomationTriggerEvery {
    pub every: AutomationDuration,
    #[serde(rename = "type")]
    pub type_: AutomationTriggerEveryType,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub tz: ::std::option::Option<::std::string::String>,
}
///`AutomationTriggerEveryType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum AutomationTriggerEveryType {
    #[serde(rename = "every")]
    Every,
}
impl ::std::fmt::Display for AutomationTriggerEveryType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Every => f.write_str("every"),
        }
    }
}
impl ::std::str::FromStr for AutomationTriggerEveryType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "every" => Ok(Self::Every),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for AutomationTriggerEveryType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for AutomationTriggerEveryType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`FileDiffInfo`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct FileDiffInfo {
    pub additions: i64,
    pub deletions: i64,
    pub file: ::std::string::String,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub more: ::std::option::Option<bool>,
    pub patch: ::std::string::String,
    pub status: FileDiffInfoStatus,
}
///`FileDiffInfoStatus`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum FileDiffInfoStatus {
    #[serde(rename = "added")]
    Added,
    #[serde(rename = "deleted")]
    Deleted,
    #[serde(rename = "modified")]
    Modified,
}
impl ::std::fmt::Display for FileDiffInfoStatus {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Added => f.write_str("added"),
            Self::Deleted => f.write_str("deleted"),
            Self::Modified => f.write_str("modified"),
        }
    }
}
impl ::std::str::FromStr for FileDiffInfoStatus {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "added" => Ok(Self::Added),
            "deleted" => Ok(Self::Deleted),
            "modified" => Ok(Self::Modified),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for FileDiffInfoStatus {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for FileDiffInfoStatus {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`LocationPublicRef`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct LocationPublicRef {
    pub directory: AbsolutePath,
}
///`LocationRef`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct LocationRef {
    pub directory: AbsolutePath,
    #[serde(
        rename = "workspaceID",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub workspace_id: ::std::option::Option<WorkspaceId>,
}
///`ModelRef`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct ModelRef {
    pub id: ::std::string::String,
    #[serde(rename = "providerID")]
    pub provider_id: ProviderId,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub variant: ::std::option::Option<::std::string::String>,
}
///`MoneyUsd`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(transparent)]
pub struct MoneyUsd(pub f64);
impl ::std::ops::Deref for MoneyUsd {
    type Target = f64;
    fn deref(&self) -> &f64 {
        &self.0
    }
}
impl ::std::convert::From<MoneyUsd> for f64 {
    fn from(value: MoneyUsd) -> Self {
        value.0
    }
}
impl ::std::convert::From<f64> for MoneyUsd {
    fn from(value: f64) -> Self {
        Self(value)
    }
}
impl ::std::fmt::Display for MoneyUsd {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        self.0.fmt(f)
    }
}
impl ::std::str::FromStr for MoneyUsd {
    type Err = <f64 as ::std::str::FromStr>::Err;
    fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
        Ok(Self(value.parse()?))
    }
}
///`PermissionAskedData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct PermissionAskedData {
    pub action: ::std::string::String,
    pub id: PermissionId,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub message: ::std::option::Option<::std::string::String>,
    #[serde(default, skip_serializing_if = "::serde_json::Map::is_empty")]
    pub metadata: ::serde_json::Map<::std::string::String, ::serde_json::Value>,
    pub resources: ::std::vec::Vec<::std::string::String>,
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub save: ::std::vec::Vec<::std::string::String>,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub source: ::std::option::Option<PermissionSource>,
}
///`PermissionEffect`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum PermissionEffect {
    #[serde(rename = "allow")]
    Allow,
    #[serde(rename = "deny")]
    Deny,
    #[serde(rename = "ask")]
    Ask,
}
impl ::std::fmt::Display for PermissionEffect {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Allow => f.write_str("allow"),
            Self::Deny => f.write_str("deny"),
            Self::Ask => f.write_str("ask"),
        }
    }
}
impl ::std::str::FromStr for PermissionEffect {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "allow" => Ok(Self::Allow),
            "deny" => Ok(Self::Deny),
            "ask" => Ok(Self::Ask),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for PermissionEffect {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for PermissionEffect {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`PermissionId`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct PermissionId(::std::string::String);
impl ::std::ops::Deref for PermissionId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<PermissionId> for ::std::string::String {
    fn from(value: PermissionId) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for PermissionId {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value)
    }
}
impl ::std::convert::TryFrom<&str> for PermissionId {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| ::regress::Regex::with_flags("^per", "u").unwrap());
        if PATTERN.find(value).is_none() {
            return Err("doesn't match pattern \"^per\"".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<::std::string::String> for PermissionId {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value.as_str())
    }
}
impl<'de> ::serde::Deserialize<'de> for PermissionId {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::convert::TryFrom::try_from(::std::string::String::deserialize(deserializer)?)
            .map_err(|e: self::error::ConversionError| {
                <D::Error as ::serde::de::Error>::custom(e.to_string())
            })
    }
}
///`PermissionRepliedData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct PermissionRepliedData {
    pub reply: PermissionReply,
    #[serde(rename = "requestID")]
    pub request_id: PermissionId,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
}
///`PermissionReply`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum PermissionReply {
    #[serde(rename = "once")]
    Once,
    #[serde(rename = "always")]
    Always,
    #[serde(rename = "reject")]
    Reject,
}
impl ::std::fmt::Display for PermissionReply {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Once => f.write_str("once"),
            Self::Always => f.write_str("always"),
            Self::Reject => f.write_str("reject"),
        }
    }
}
impl ::std::str::FromStr for PermissionReply {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "once" => Ok(Self::Once),
            "always" => Ok(Self::Always),
            "reject" => Ok(Self::Reject),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for PermissionReply {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for PermissionReply {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`PermissionRequest`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct PermissionRequest {
    pub action: ::std::string::String,
    pub id: PermissionId,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub message: ::std::option::Option<::std::string::String>,
    #[serde(default, skip_serializing_if = "::serde_json::Map::is_empty")]
    pub metadata: ::serde_json::Map<::std::string::String, ::serde_json::Value>,
    pub resources: ::std::vec::Vec<::std::string::String>,
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub save: ::std::vec::Vec<::std::string::String>,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub source: ::std::option::Option<PermissionSource>,
}
///`PermissionRule`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct PermissionRule {
    pub action: ::std::string::String,
    pub effect: PermissionEffect,
    pub resource: ::std::string::String,
}
///`PermissionRuleset`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(transparent)]
pub struct PermissionRuleset(pub ::std::vec::Vec<PermissionRule>);
impl ::std::ops::Deref for PermissionRuleset {
    type Target = ::std::vec::Vec<PermissionRule>;
    fn deref(&self) -> &::std::vec::Vec<PermissionRule> {
        &self.0
    }
}
impl ::std::convert::From<PermissionRuleset> for ::std::vec::Vec<PermissionRule> {
    fn from(value: PermissionRuleset) -> Self {
        value.0
    }
}
impl ::std::convert::From<::std::vec::Vec<PermissionRule>> for PermissionRuleset {
    fn from(value: ::std::vec::Vec<PermissionRule>) -> Self {
        Self(value)
    }
}
///`PermissionSource`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum PermissionSource {
    #[serde(rename = "tool")]
    Tool {
        id: ::std::string::String,
        #[serde(rename = "messageID")]
        message_id: ::std::string::String,
    },
}
///`Project`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct Project {
    pub canonical: AbsolutePath,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub commands: ::std::option::Option<ProjectCommands>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub icon: ::std::option::Option<ProjectIcon>,
    pub id: ProjectId,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub name: ::std::option::Option<::std::string::String>,
    pub sandboxes: ::std::vec::Vec<::std::string::String>,
    pub time: ProjectTime,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub vcs: ::std::option::Option<ProjectVcs>,
}
///`ProjectCommands`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, Default)]
pub struct ProjectCommands {
    ///Startup script to run when creating a new workspace (worktree)
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub start: ::std::option::Option<::std::string::String>,
}
///`ProjectIcon`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, Default)]
pub struct ProjectIcon {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub color: ::std::option::Option<::std::string::String>,
    #[serde(
        rename = "override",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub override_: ::std::option::Option<::std::string::String>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub url: ::std::option::Option<::std::string::String>,
}
///`ProjectId`
#[derive(
    ::serde::Deserialize, ::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd,
)]
#[serde(transparent)]
pub struct ProjectId(pub ::std::string::String);
impl ::std::ops::Deref for ProjectId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<ProjectId> for ::std::string::String {
    fn from(value: ProjectId) -> Self {
        value.0
    }
}
impl ::std::convert::From<::std::string::String> for ProjectId {
    fn from(value: ::std::string::String) -> Self {
        Self(value)
    }
}
impl ::std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        self.0.fmt(f)
    }
}
impl ::std::str::FromStr for ProjectId {
    type Err = ::std::convert::Infallible;
    fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
        Ok(Self(value.to_string()))
    }
}
///`ProjectTime`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct ProjectTime {
    pub active: i64,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub archived: ::std::option::Option<i64>,
    pub created: i64,
    pub updated: i64,
}
///`ProjectVcs`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct ProjectVcs(::std::string::String);
impl ::std::ops::Deref for ProjectVcs {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<ProjectVcs> for ::std::string::String {
    fn from(value: ProjectVcs) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for ProjectVcs {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value)
    }
}
impl ::std::convert::TryFrom<&str> for ProjectVcs {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| {
                ::regress::Regex::with_flags("^[a-z][a-z0-9._-]*$", "u").unwrap()
            });
        if PATTERN.find(value).is_none() {
            return Err("doesn't match pattern \"^[a-z][a-z0-9._-]*$\"".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<::std::string::String> for ProjectVcs {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value.as_str())
    }
}
impl<'de> ::serde::Deserialize<'de> for ProjectVcs {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::convert::TryFrom::try_from(::std::string::String::deserialize(deserializer)?)
            .map_err(|e: self::error::ConversionError| {
                <D::Error as ::serde::de::Error>::custom(e.to_string())
            })
    }
}
///`PromptAgentAttachment`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct PromptAgentAttachment {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub mention: ::std::option::Option<PromptMention>,
    pub name: ::std::string::String,
}
///`PromptBase64`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct PromptBase64(::std::string::String);
impl ::std::ops::Deref for PromptBase64 {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<PromptBase64> for ::std::string::String {
    fn from(value: PromptBase64) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for PromptBase64 {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value)
    }
}
impl ::std::convert::TryFrom<&str> for PromptBase64 {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| {
                ::regress::Regex::with_flags(
                    "^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$",
                    "u",
                )
                .unwrap()
            });
        if PATTERN.find(value).is_none() {
            return Err(
                "doesn't match pattern \"^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$\""
                    .into(),
            );
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<::std::string::String> for PromptBase64 {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value.as_str())
    }
}
impl<'de> ::serde::Deserialize<'de> for PromptBase64 {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::convert::TryFrom::try_from(::std::string::String::deserialize(deserializer)?)
            .map_err(|e: self::error::ConversionError| {
                <D::Error as ::serde::de::Error>::custom(e.to_string())
            })
    }
}
///`PromptFileAttachment`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct PromptFileAttachment {
    pub data: PromptBase64,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub description: ::std::option::Option<::std::string::String>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub mention: ::std::option::Option<PromptMention>,
    pub mime: ::std::string::String,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub name: ::std::option::Option<::std::string::String>,
    pub source: PromptFileSource,
}
///`PromptFileSource`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(tag = "type", content = "uri")]
pub enum PromptFileSource {
    #[serde(rename = "inline")]
    Inline,
    #[serde(rename = "uri")]
    Uri(::std::string::String),
}
///`PromptMention`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct PromptMention {
    pub end: f64,
    pub start: f64,
    pub text: ::std::string::String,
}
///`PromptSkillAttachment`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct PromptSkillAttachment {
    pub id: SkillId,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub mention: ::std::option::Option<PromptMention>,
    pub name: SkillName,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub text: ::std::option::Option<::std::string::String>,
}
///`ProviderId`
#[derive(
    ::serde::Deserialize, ::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd,
)]
#[serde(transparent)]
pub struct ProviderId(pub ::std::string::String);
impl ::std::ops::Deref for ProviderId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<ProviderId> for ::std::string::String {
    fn from(value: ProviderId) -> Self {
        value.0
    }
}
impl ::std::convert::From<::std::string::String> for ProviderId {
    fn from(value: ::std::string::String) -> Self {
        Self(value)
    }
}
impl ::std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        self.0.fmt(f)
    }
}
impl ::std::str::FromStr for ProviderId {
    type Err = ::std::convert::Infallible;
    fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
        Ok(Self(value.to_string()))
    }
}
///`Pty`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct Pty {
    pub args: ::std::vec::Vec<::std::string::String>,
    pub command: ::std::string::String,
    pub cwd: ::std::string::String,
    #[serde(
        rename = "exitCode",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub exit_code: ::std::option::Option<i64>,
    pub id: PtyId,
    pub pid: i64,
    pub status: PtyStatus,
    pub title: ::std::string::String,
}
///`PtyId`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct PtyId(::std::string::String);
impl ::std::ops::Deref for PtyId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<PtyId> for ::std::string::String {
    fn from(value: PtyId) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for PtyId {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value)
    }
}
impl ::std::convert::TryFrom<&str> for PtyId {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| ::regress::Regex::with_flags("^pty", "u").unwrap());
        if PATTERN.find(value).is_none() {
            return Err("doesn't match pattern \"^pty\"".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<::std::string::String> for PtyId {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value.as_str())
    }
}
impl<'de> ::serde::Deserialize<'de> for PtyId {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::convert::TryFrom::try_from(::std::string::String::deserialize(deserializer)?)
            .map_err(|e: self::error::ConversionError| {
                <D::Error as ::serde::de::Error>::custom(e.to_string())
            })
    }
}
///`PtyStatus`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum PtyStatus {
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "exited")]
    Exited,
}
impl ::std::fmt::Display for PtyStatus {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Running => f.write_str("running"),
            Self::Exited => f.write_str("exited"),
        }
    }
}
impl ::std::str::FromStr for PtyStatus {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "running" => Ok(Self::Running),
            "exited" => Ok(Self::Exited),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for PtyStatus {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for PtyStatus {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`RelativePath`
#[derive(
    ::serde::Deserialize, ::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd,
)]
#[serde(transparent)]
pub struct RelativePath(pub ::std::string::String);
impl ::std::ops::Deref for RelativePath {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<RelativePath> for ::std::string::String {
    fn from(value: RelativePath) -> Self {
        value.0
    }
}
impl ::std::convert::From<::std::string::String> for RelativePath {
    fn from(value: ::std::string::String) -> Self {
        Self(value)
    }
}
impl ::std::fmt::Display for RelativePath {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        self.0.fmt(f)
    }
}
impl ::std::str::FromStr for RelativePath {
    type Err = ::std::convert::Infallible;
    fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
        Ok(Self(value.to_string()))
    }
}
///`ServerInfo`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct ServerInfo {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub capabilities: ::std::option::Option<ServerInfoCapabilities>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub hostname: ::std::option::Option<::std::string::String>,
    pub paths: ServerInfoPaths,
    pub pid: i64,
    pub urls: ::std::vec::Vec<::std::string::String>,
    pub version: ::std::string::String,
}
///`ServerInfoCapabilities`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, Default)]
pub struct ServerInfoCapabilities {
    #[serde(
        rename = "persistentPty",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub persistent_pty: ::std::option::Option<bool>,
}
///`ServerInfoPaths`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct ServerInfoPaths {
    pub tmp: ::std::string::String,
}
///`SessionCreatedData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionCreatedData {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub agent: ::std::option::Option<AgentId>,
    pub location: LocationRef,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub metadata: ::std::option::Option<SessionMetadata>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub model: ::std::option::Option<ModelRef>,
    #[serde(
        rename = "parentID",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub parent_id: ::std::option::Option<SessionId>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub permissions: ::std::option::Option<PermissionRuleset>,
    #[serde(rename = "projectID")]
    pub project_id: ProjectId,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    pub slug: ::std::string::String,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub subpath: ::std::option::Option<RelativePath>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub title: ::std::option::Option<::std::string::String>,
    pub version: ::std::string::String,
}
///`SessionDeletedData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionDeletedData {
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
}
///`SessionExecutionStartedData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionExecutionStartedData {
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
}
///`SessionForkBoundary`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(tag = "type", content = "messageID")]
pub enum SessionForkBoundary {
    #[serde(rename = "before")]
    Before(SessionMessageId),
    #[serde(rename = "through")]
    Through(SessionMessageId),
}
///`SessionId`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct SessionId(::std::string::String);
impl ::std::ops::Deref for SessionId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<SessionId> for ::std::string::String {
    fn from(value: SessionId) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for SessionId {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value)
    }
}
impl ::std::convert::TryFrom<&str> for SessionId {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| ::regress::Regex::with_flags("^ses", "u").unwrap());
        if PATTERN.find(value).is_none() {
            return Err("doesn't match pattern \"^ses\"".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionId {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value.as_str())
    }
}
impl<'de> ::serde::Deserialize<'de> for SessionId {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::convert::TryFrom::try_from(::std::string::String::deserialize(deserializer)?)
            .map_err(|e: self::error::ConversionError| {
                <D::Error as ::serde::de::Error>::custom(e.to_string())
            })
    }
}
///`SessionInfo`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionInfo {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub agent: ::std::option::Option<AgentId>,
    pub cost: MoneyUsd,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub fork: ::std::option::Option<SessionInfoFork>,
    pub id: SessionId,
    pub location: LocationPublicRef,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub metadata: ::std::option::Option<SessionMetadata>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub model: ::std::option::Option<ModelRef>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub outcome: ::std::option::Option<SessionInfoOutcome>,
    #[serde(
        rename = "parentID",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub parent_id: ::std::option::Option<SessionId>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub permissions: ::std::option::Option<PermissionRuleset>,
    #[serde(rename = "projectID")]
    pub project_id: ProjectId,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub revert: ::std::option::Option<SessionRevert>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub subpath: ::std::option::Option<RelativePath>,
    pub time: SessionInfoTime,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub title: ::std::option::Option<::std::string::String>,
    pub tokens: TokenUsageInfo,
}
///`SessionInfoFork`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionInfoFork {
    pub boundary: SessionForkBoundary,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
}
///`SessionInfoOutcome`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionInfoOutcome {
    #[serde(rename = "succeeded")]
    Succeeded,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "interrupted")]
    Interrupted,
}
impl ::std::fmt::Display for SessionInfoOutcome {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Succeeded => f.write_str("succeeded"),
            Self::Failed => f.write_str("failed"),
            Self::Interrupted => f.write_str("interrupted"),
        }
    }
}
impl ::std::str::FromStr for SessionInfoOutcome {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionInfoOutcome {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionInfoOutcome {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionInfoTime`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionInfoTime {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub archived: ::std::option::Option<f64>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub busy: ::std::option::Option<f64>,
    pub created: f64,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub idle: ::std::option::Option<f64>,
    pub updated: f64,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub viewed: ::std::option::Option<f64>,
}
///`SessionMessageAssistant`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageAssistant {
    pub agent: AgentId,
    pub content: ::std::vec::Vec<SessionMessageAssistantContentItem>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub cost: ::std::option::Option<MoneyUsd>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub error: ::std::option::Option<SessionStructuredError>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub finish: ::std::option::Option<SessionMessageAssistantFinish>,
    pub id: SessionMessageId,
    #[serde(default, skip_serializing_if = "::serde_json::Map::is_empty")]
    pub metadata: ::serde_json::Map<::std::string::String, ::serde_json::Value>,
    pub model: ModelRef,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub native: ::std::option::Option<SessionMessageNative>,
    #[serde(
        rename = "rawFinish",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub raw_finish: ::std::option::Option<::std::string::String>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub retry: ::std::option::Option<SessionMessageAssistantRetry>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub snapshot: ::std::option::Option<SessionMessageAssistantSnapshot>,
    pub time: SessionMessageAssistantTime,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub tokens: ::std::option::Option<TokenUsageInfo>,
    #[serde(rename = "type")]
    pub type_: SessionMessageAssistantType,
}
///`SessionMessageAssistantContentItem`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum SessionMessageAssistantContentItem {
    Text(SessionMessageAssistantText),
    Reasoning(SessionMessageAssistantReasoning),
    Tool(SessionMessageAssistantTool),
}
impl ::std::convert::From<SessionMessageAssistantText> for SessionMessageAssistantContentItem {
    fn from(value: SessionMessageAssistantText) -> Self {
        Self::Text(value)
    }
}
impl ::std::convert::From<SessionMessageAssistantReasoning> for SessionMessageAssistantContentItem {
    fn from(value: SessionMessageAssistantReasoning) -> Self {
        Self::Reasoning(value)
    }
}
impl ::std::convert::From<SessionMessageAssistantTool> for SessionMessageAssistantContentItem {
    fn from(value: SessionMessageAssistantTool) -> Self {
        Self::Tool(value)
    }
}
///`SessionMessageAssistantFinish`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionMessageAssistantFinish {
    #[serde(rename = "stop")]
    Stop,
    #[serde(rename = "length")]
    Length,
    #[serde(rename = "tool-calls")]
    ToolCalls,
    #[serde(rename = "content-filter")]
    ContentFilter,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "unknown")]
    Unknown,
}
impl ::std::fmt::Display for SessionMessageAssistantFinish {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Stop => f.write_str("stop"),
            Self::Length => f.write_str("length"),
            Self::ToolCalls => f.write_str("tool-calls"),
            Self::ContentFilter => f.write_str("content-filter"),
            Self::Error => f.write_str("error"),
            Self::Unknown => f.write_str("unknown"),
        }
    }
}
impl ::std::str::FromStr for SessionMessageAssistantFinish {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "stop" => Ok(Self::Stop),
            "length" => Ok(Self::Length),
            "tool-calls" => Ok(Self::ToolCalls),
            "content-filter" => Ok(Self::ContentFilter),
            "error" => Ok(Self::Error),
            "unknown" => Ok(Self::Unknown),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionMessageAssistantFinish {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionMessageAssistantFinish {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionMessageAssistantReasoning`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageAssistantReasoning {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub native: ::std::option::Option<SessionMessageNative>,
    pub text: ::std::string::String,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub time: ::std::option::Option<SessionMessageAssistantReasoningTime>,
    #[serde(rename = "type")]
    pub type_: SessionMessageAssistantReasoningType,
}
///`SessionMessageAssistantReasoningTime`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageAssistantReasoningTime {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub completed: ::std::option::Option<f64>,
    pub created: f64,
}
///`SessionMessageAssistantReasoningType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionMessageAssistantReasoningType {
    #[serde(rename = "reasoning")]
    Reasoning,
}
impl ::std::fmt::Display for SessionMessageAssistantReasoningType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Reasoning => f.write_str("reasoning"),
        }
    }
}
impl ::std::str::FromStr for SessionMessageAssistantReasoningType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "reasoning" => Ok(Self::Reasoning),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionMessageAssistantReasoningType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionMessageAssistantReasoningType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionMessageAssistantRetry`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageAssistantRetry {
    pub at: f64,
    pub attempt: ::std::num::NonZeroU64,
    pub error: SessionStructuredError,
}
///`SessionMessageAssistantSnapshot`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, Default)]
pub struct SessionMessageAssistantSnapshot {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub end: ::std::option::Option<SnapshotId>,
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub files: ::std::vec::Vec<RelativePath>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub start: ::std::option::Option<SnapshotId>,
}
///`SessionMessageAssistantText`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageAssistantText {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub native: ::std::option::Option<SessionMessageNative>,
    pub text: ::std::string::String,
    #[serde(rename = "type")]
    pub type_: SessionMessageAssistantTextType,
}
///`SessionMessageAssistantTextType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionMessageAssistantTextType {
    #[serde(rename = "text")]
    Text,
}
impl ::std::fmt::Display for SessionMessageAssistantTextType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Text => f.write_str("text"),
        }
    }
}
impl ::std::str::FromStr for SessionMessageAssistantTextType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "text" => Ok(Self::Text),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionMessageAssistantTextType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionMessageAssistantTextType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionMessageAssistantTime`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageAssistantTime {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub completed: ::std::option::Option<f64>,
    pub created: f64,
    #[serde(
        rename = "firstToken",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub first_token: ::std::option::Option<f64>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub streamed: ::std::option::Option<f64>,
}
///`SessionMessageAssistantTool`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageAssistantTool {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub executed: ::std::option::Option<bool>,
    pub id: ::std::string::String,
    pub name: ::std::string::String,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub native: ::std::option::Option<SessionMessageNative>,
    pub state: SessionMessageAssistantToolState,
    pub time: SessionMessageAssistantToolTime,
    #[serde(rename = "type")]
    pub type_: SessionMessageAssistantToolType,
}
///`SessionMessageAssistantToolState`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum SessionMessageAssistantToolState {
    Streaming(SessionMessageToolStateStreaming),
    Running(SessionMessageToolStateRunning),
    Completed(SessionMessageToolStateCompleted),
    Error(SessionMessageToolStateError),
}
impl ::std::convert::From<SessionMessageToolStateStreaming> for SessionMessageAssistantToolState {
    fn from(value: SessionMessageToolStateStreaming) -> Self {
        Self::Streaming(value)
    }
}
impl ::std::convert::From<SessionMessageToolStateRunning> for SessionMessageAssistantToolState {
    fn from(value: SessionMessageToolStateRunning) -> Self {
        Self::Running(value)
    }
}
impl ::std::convert::From<SessionMessageToolStateCompleted> for SessionMessageAssistantToolState {
    fn from(value: SessionMessageToolStateCompleted) -> Self {
        Self::Completed(value)
    }
}
impl ::std::convert::From<SessionMessageToolStateError> for SessionMessageAssistantToolState {
    fn from(value: SessionMessageToolStateError) -> Self {
        Self::Error(value)
    }
}
///`SessionMessageAssistantToolTime`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageAssistantToolTime {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub completed: ::std::option::Option<f64>,
    pub created: f64,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub ran: ::std::option::Option<f64>,
}
///`SessionMessageAssistantToolType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionMessageAssistantToolType {
    #[serde(rename = "tool")]
    Tool,
}
impl ::std::fmt::Display for SessionMessageAssistantToolType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Tool => f.write_str("tool"),
        }
    }
}
impl ::std::str::FromStr for SessionMessageAssistantToolType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "tool" => Ok(Self::Tool),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionMessageAssistantToolType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionMessageAssistantToolType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionMessageAssistantType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionMessageAssistantType {
    #[serde(rename = "assistant")]
    Assistant,
}
impl ::std::fmt::Display for SessionMessageAssistantType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Assistant => f.write_str("assistant"),
        }
    }
}
impl ::std::str::FromStr for SessionMessageAssistantType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "assistant" => Ok(Self::Assistant),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionMessageAssistantType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionMessageAssistantType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionMessageId`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct SessionMessageId(::std::string::String);
impl ::std::ops::Deref for SessionMessageId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<SessionMessageId> for ::std::string::String {
    fn from(value: SessionMessageId) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for SessionMessageId {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value)
    }
}
impl ::std::convert::TryFrom<&str> for SessionMessageId {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| ::regress::Regex::with_flags("^msg_", "u").unwrap());
        if PATTERN.find(value).is_none() {
            return Err("doesn't match pattern \"^msg_\"".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionMessageId {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value.as_str())
    }
}
impl<'de> ::serde::Deserialize<'de> for SessionMessageId {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::convert::TryFrom::try_from(::std::string::String::deserialize(deserializer)?)
            .map_err(|e: self::error::ConversionError| {
                <D::Error as ::serde::de::Error>::custom(e.to_string())
            })
    }
}
///`SessionMessageIdle`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageIdle {
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub error: ::std::option::Option<SessionStructuredError>,
    pub id: SessionMessageId,
    #[serde(default, skip_serializing_if = "::serde_json::Map::is_empty")]
    pub metadata: ::serde_json::Map<::std::string::String, ::serde_json::Value>,
    pub outcome: SessionMessageIdleOutcome,
    pub time: SessionMessageIdleTime,
    #[serde(rename = "type")]
    pub type_: SessionMessageIdleType,
}
///`SessionMessageIdleOutcome`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionMessageIdleOutcome {
    #[serde(rename = "succeeded")]
    Succeeded,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "interrupted")]
    Interrupted,
}
impl ::std::fmt::Display for SessionMessageIdleOutcome {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Succeeded => f.write_str("succeeded"),
            Self::Failed => f.write_str("failed"),
            Self::Interrupted => f.write_str("interrupted"),
        }
    }
}
impl ::std::str::FromStr for SessionMessageIdleOutcome {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionMessageIdleOutcome {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionMessageIdleOutcome {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionMessageIdleTime`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageIdleTime {
    pub created: f64,
}
///`SessionMessageIdleType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionMessageIdleType {
    #[serde(rename = "idle")]
    Idle,
}
impl ::std::fmt::Display for SessionMessageIdleType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Idle => f.write_str("idle"),
        }
    }
}
impl ::std::str::FromStr for SessionMessageIdleType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "idle" => Ok(Self::Idle),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionMessageIdleType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionMessageIdleType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionMessageNative`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(transparent)]
pub struct SessionMessageNative(pub ::serde_json::Map<::std::string::String, ::serde_json::Value>);
impl ::std::ops::Deref for SessionMessageNative {
    type Target = ::serde_json::Map<::std::string::String, ::serde_json::Value>;
    fn deref(&self) -> &::serde_json::Map<::std::string::String, ::serde_json::Value> {
        &self.0
    }
}
impl ::std::convert::From<SessionMessageNative>
    for ::serde_json::Map<::std::string::String, ::serde_json::Value>
{
    fn from(value: SessionMessageNative) -> Self {
        value.0
    }
}
impl ::std::convert::From<::serde_json::Map<::std::string::String, ::serde_json::Value>>
    for SessionMessageNative
{
    fn from(value: ::serde_json::Map<::std::string::String, ::serde_json::Value>) -> Self {
        Self(value)
    }
}
///`SessionMessageToolStateCompleted`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageToolStateCompleted {
    pub content: ::std::vec::Vec<ToolContent>,
    pub input: ::serde_json::Map<::std::string::String, ::serde_json::Value>,
    #[serde(default, skip_serializing_if = "::serde_json::Map::is_empty")]
    pub metadata: ::serde_json::Map<::std::string::String, ::serde_json::Value>,
    pub status: SessionMessageToolStateCompletedStatus,
}
///`SessionMessageToolStateCompletedStatus`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionMessageToolStateCompletedStatus {
    #[serde(rename = "completed")]
    Completed,
}
impl ::std::fmt::Display for SessionMessageToolStateCompletedStatus {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Completed => f.write_str("completed"),
        }
    }
}
impl ::std::str::FromStr for SessionMessageToolStateCompletedStatus {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "completed" => Ok(Self::Completed),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionMessageToolStateCompletedStatus {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionMessageToolStateCompletedStatus {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionMessageToolStateError`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageToolStateError {
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub content: ::std::vec::Vec<ToolContent>,
    pub error: SessionStructuredError,
    pub input: ::serde_json::Map<::std::string::String, ::serde_json::Value>,
    #[serde(default, skip_serializing_if = "::serde_json::Map::is_empty")]
    pub metadata: ::serde_json::Map<::std::string::String, ::serde_json::Value>,
    pub status: SessionMessageToolStateErrorStatus,
}
///`SessionMessageToolStateErrorStatus`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionMessageToolStateErrorStatus {
    #[serde(rename = "error")]
    Error,
}
impl ::std::fmt::Display for SessionMessageToolStateErrorStatus {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Error => f.write_str("error"),
        }
    }
}
impl ::std::str::FromStr for SessionMessageToolStateErrorStatus {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "error" => Ok(Self::Error),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionMessageToolStateErrorStatus {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionMessageToolStateErrorStatus {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionMessageToolStateRunning`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageToolStateRunning {
    pub input: ::serde_json::Map<::std::string::String, ::serde_json::Value>,
    pub metadata: ::serde_json::Map<::std::string::String, ::serde_json::Value>,
    pub status: SessionMessageToolStateRunningStatus,
}
///`SessionMessageToolStateRunningStatus`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionMessageToolStateRunningStatus {
    #[serde(rename = "running")]
    Running,
}
impl ::std::fmt::Display for SessionMessageToolStateRunningStatus {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Running => f.write_str("running"),
        }
    }
}
impl ::std::str::FromStr for SessionMessageToolStateRunningStatus {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "running" => Ok(Self::Running),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionMessageToolStateRunningStatus {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionMessageToolStateRunningStatus {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionMessageToolStateStreaming`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageToolStateStreaming {
    pub input: ::std::string::String,
    pub status: SessionMessageToolStateStreamingStatus,
}
///`SessionMessageToolStateStreamingStatus`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionMessageToolStateStreamingStatus {
    #[serde(rename = "streaming")]
    Streaming,
}
impl ::std::fmt::Display for SessionMessageToolStateStreamingStatus {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Streaming => f.write_str("streaming"),
        }
    }
}
impl ::std::str::FromStr for SessionMessageToolStateStreamingStatus {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "streaming" => Ok(Self::Streaming),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionMessageToolStateStreamingStatus {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionMessageToolStateStreamingStatus {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionMessageUser`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageUser {
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub agents: ::std::vec::Vec<PromptAgentAttachment>,
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub files: ::std::vec::Vec<PromptFileAttachment>,
    pub id: SessionMessageId,
    #[serde(default, skip_serializing_if = "::serde_json::Map::is_empty")]
    pub metadata: ::serde_json::Map<::std::string::String, ::serde_json::Value>,
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub skills: ::std::vec::Vec<PromptSkillAttachment>,
    pub text: ::std::string::String,
    pub time: SessionMessageUserTime,
    #[serde(rename = "type")]
    pub type_: SessionMessageUserType,
}
///`SessionMessageUserTime`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMessageUserTime {
    pub created: f64,
}
///`SessionMessageUserType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionMessageUserType {
    #[serde(rename = "user")]
    User,
}
impl ::std::fmt::Display for SessionMessageUserType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::User => f.write_str("user"),
        }
    }
}
impl ::std::str::FromStr for SessionMessageUserType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "user" => Ok(Self::User),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionMessageUserType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionMessageUserType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionMetadata`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(transparent)]
pub struct SessionMetadata(pub ::serde_json::Map<::std::string::String, ::serde_json::Value>);
impl ::std::ops::Deref for SessionMetadata {
    type Target = ::serde_json::Map<::std::string::String, ::serde_json::Value>;
    fn deref(&self) -> &::serde_json::Map<::std::string::String, ::serde_json::Value> {
        &self.0
    }
}
impl ::std::convert::From<SessionMetadata>
    for ::serde_json::Map<::std::string::String, ::serde_json::Value>
{
    fn from(value: SessionMetadata) -> Self {
        value.0
    }
}
impl ::std::convert::From<::serde_json::Map<::std::string::String, ::serde_json::Value>>
    for SessionMetadata
{
    fn from(value: ::serde_json::Map<::std::string::String, ::serde_json::Value>) -> Self {
        Self(value)
    }
}
///`SessionModelSelectedData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionModelSelectedData {
    pub model: ModelRef,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub previous: ::std::option::Option<ModelRef>,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
}
///`SessionMovedData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionMovedData {
    pub location: LocationRef,
    #[serde(rename = "projectID")]
    pub project_id: ProjectId,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub subpath: ::std::option::Option<RelativePath>,
}
///`SessionReasoningDeltaData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionReasoningDeltaData {
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: SessionMessageId,
    pub delta: ::std::string::String,
    pub ordinal: i64,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
}
///`SessionRenamedData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionRenamedData {
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    pub title: ::std::string::String,
}
///`SessionRevert`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionRevert {
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub files: ::std::vec::Vec<FileDiffInfo>,
    #[serde(rename = "messageID")]
    pub message_id: SessionMessageId,
    #[serde(
        rename = "partID",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub part_id: ::std::option::Option<::std::string::String>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub snapshot: ::std::option::Option<SnapshotId>,
}
///`SessionRevertStagedData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionRevertStagedData {
    pub revert: SessionRevert,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
}
///`SessionStepEndedData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionStepEndedData {
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: SessionMessageId,
    pub cost: MoneyUsd,
    #[serde(default, skip_serializing_if = "::std::vec::Vec::is_empty")]
    pub files: ::std::vec::Vec<RelativePath>,
    pub finish: SessionStepEndedDataFinish,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub native: ::std::option::Option<SessionMessageNative>,
    #[serde(
        rename = "rawFinish",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub raw_finish: ::std::option::Option<::std::string::String>,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub snapshot: ::std::option::Option<SnapshotId>,
    pub tokens: TokenUsageInfo,
}
///`SessionStepEndedDataFinish`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum SessionStepEndedDataFinish {
    #[serde(rename = "stop")]
    Stop,
    #[serde(rename = "length")]
    Length,
    #[serde(rename = "tool-calls")]
    ToolCalls,
    #[serde(rename = "content-filter")]
    ContentFilter,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "unknown")]
    Unknown,
}
impl ::std::fmt::Display for SessionStepEndedDataFinish {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Stop => f.write_str("stop"),
            Self::Length => f.write_str("length"),
            Self::ToolCalls => f.write_str("tool-calls"),
            Self::ContentFilter => f.write_str("content-filter"),
            Self::Error => f.write_str("error"),
            Self::Unknown => f.write_str("unknown"),
        }
    }
}
impl ::std::str::FromStr for SessionStepEndedDataFinish {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "stop" => Ok(Self::Stop),
            "length" => Ok(Self::Length),
            "tool-calls" => Ok(Self::ToolCalls),
            "content-filter" => Ok(Self::ContentFilter),
            "error" => Ok(Self::Error),
            "unknown" => Ok(Self::Unknown),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for SessionStepEndedDataFinish {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for SessionStepEndedDataFinish {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`SessionStructuredError`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionStructuredError {
    pub message: ::std::string::String,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub response: ::std::option::Option<SessionStructuredErrorResponse>,
    #[serde(
        rename = "retryAfterMs",
        skip_serializing_if = "::std::option::Option::is_none"
    )]
    pub retry_after_ms: ::std::option::Option<i64>,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub status: ::std::option::Option<i64>,
    #[serde(rename = "type")]
    pub type_: ::std::string::String,
}
///`SessionStructuredErrorResponse`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionStructuredErrorResponse {
    pub body: ::std::string::String,
}
///`SessionTextDeltaData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionTextDeltaData {
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: SessionMessageId,
    pub delta: ::std::string::String,
    pub ordinal: i64,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
}
///`SessionToolInputStartedData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionToolInputStartedData {
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: SessionMessageId,
    pub id: ::std::string::String,
    pub name: ::std::string::String,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
}
///`SessionUsageUpdatedData`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct SessionUsageUpdatedData {
    pub cost: MoneyUsd,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    pub tokens: TokenUsageInfo,
}
///`SkillId`
#[derive(
    ::serde::Deserialize, ::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd,
)]
#[serde(transparent)]
pub struct SkillId(pub ::std::string::String);
impl ::std::ops::Deref for SkillId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<SkillId> for ::std::string::String {
    fn from(value: SkillId) -> Self {
        value.0
    }
}
impl ::std::convert::From<::std::string::String> for SkillId {
    fn from(value: ::std::string::String) -> Self {
        Self(value)
    }
}
impl ::std::fmt::Display for SkillId {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        self.0.fmt(f)
    }
}
impl ::std::str::FromStr for SkillId {
    type Err = ::std::convert::Infallible;
    fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
        Ok(Self(value.to_string()))
    }
}
///`SkillName`
#[derive(
    ::serde::Deserialize, ::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd,
)]
#[serde(transparent)]
pub struct SkillName(pub ::std::string::String);
impl ::std::ops::Deref for SkillName {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<SkillName> for ::std::string::String {
    fn from(value: SkillName) -> Self {
        value.0
    }
}
impl ::std::convert::From<::std::string::String> for SkillName {
    fn from(value: ::std::string::String) -> Self {
        Self(value)
    }
}
impl ::std::fmt::Display for SkillName {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        self.0.fmt(f)
    }
}
impl ::std::str::FromStr for SkillName {
    type Err = ::std::convert::Infallible;
    fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
        Ok(Self(value.to_string()))
    }
}
///`SnapshotId`
#[derive(
    ::serde::Deserialize, ::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd,
)]
#[serde(transparent)]
pub struct SnapshotId(pub ::std::string::String);
impl ::std::ops::Deref for SnapshotId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<SnapshotId> for ::std::string::String {
    fn from(value: SnapshotId) -> Self {
        value.0
    }
}
impl ::std::convert::From<::std::string::String> for SnapshotId {
    fn from(value: ::std::string::String) -> Self {
        Self(value)
    }
}
impl ::std::fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        self.0.fmt(f)
    }
}
impl ::std::str::FromStr for SnapshotId {
    type Err = ::std::convert::Infallible;
    fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
        Ok(Self(value.to_string()))
    }
}
///`TokenUsageInfo`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct TokenUsageInfo {
    pub cache: TokenUsageInfoCache,
    pub input: f64,
    pub output: f64,
    pub reasoning: f64,
}
///`TokenUsageInfoCache`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct TokenUsageInfoCache {
    pub read: f64,
    pub write: f64,
}
///`ToolContent`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum ToolContent {
    TextContent(ToolTextContent),
    FileContent(ToolFileContent),
}
impl ::std::convert::From<ToolTextContent> for ToolContent {
    fn from(value: ToolTextContent) -> Self {
        Self::TextContent(value)
    }
}
impl ::std::convert::From<ToolFileContent> for ToolContent {
    fn from(value: ToolFileContent) -> Self {
        Self::FileContent(value)
    }
}
///`ToolFileContent`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct ToolFileContent {
    pub mime: ::std::string::String,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub name: ::std::option::Option<::std::string::String>,
    #[serde(rename = "type")]
    pub type_: ToolFileContentType,
    pub uri: ::std::string::String,
}
///`ToolFileContentType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum ToolFileContentType {
    #[serde(rename = "file")]
    File,
}
impl ::std::fmt::Display for ToolFileContentType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::File => f.write_str("file"),
        }
    }
}
impl ::std::str::FromStr for ToolFileContentType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "file" => Ok(Self::File),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for ToolFileContentType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for ToolFileContentType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`ToolTextContent`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct ToolTextContent {
    pub text: ::std::string::String,
    #[serde(rename = "type")]
    pub type_: ToolTextContentType,
}
///`ToolTextContentType`
#[derive(
    ::serde::Deserialize,
    ::serde::Serialize,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum ToolTextContentType {
    #[serde(rename = "text")]
    Text,
}
impl ::std::fmt::Display for ToolTextContentType {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Text => f.write_str("text"),
        }
    }
}
impl ::std::str::FromStr for ToolTextContentType {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        match value {
            "text" => Ok(Self::Text),
            _ => Err("invalid value".into()),
        }
    }
}
impl ::std::convert::TryFrom<&str> for ToolTextContentType {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
impl ::std::convert::TryFrom<::std::string::String> for ToolTextContentType {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        value.parse()
    }
}
///`WorkspaceId`
#[derive(::serde::Serialize, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct WorkspaceId(::std::string::String);
impl ::std::ops::Deref for WorkspaceId {
    type Target = ::std::string::String;
    fn deref(&self) -> &::std::string::String {
        &self.0
    }
}
impl ::std::convert::From<WorkspaceId> for ::std::string::String {
    fn from(value: WorkspaceId) -> Self {
        value.0
    }
}
impl ::std::str::FromStr for WorkspaceId {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value)
    }
}
impl ::std::convert::TryFrom<&str> for WorkspaceId {
    type Error = self::error::ConversionError;
    fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| ::regress::Regex::with_flags("^wrk", "u").unwrap());
        if PATTERN.find(value).is_none() {
            return Err("doesn't match pattern \"^wrk\"".into());
        }
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::TryFrom<::std::string::String> for WorkspaceId {
    type Error = self::error::ConversionError;
    fn try_from(
        value: ::std::string::String,
    ) -> ::std::result::Result<Self, self::error::ConversionError> {
        ::std::convert::TryFrom::try_from(value.as_str())
    }
}
impl<'de> ::serde::Deserialize<'de> for WorkspaceId {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::convert::TryFrom::try_from(::std::string::String::deserialize(deserializer)?)
            .map_err(|e: self::error::ConversionError| {
                <D::Error as ::serde::de::Error>::custom(e.to_string())
            })
    }
}
///`WorktreeDirectory`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct WorktreeDirectory {
    pub directory: AbsolutePath,
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub strategy: ::std::option::Option<::std::string::String>,
}
///`WorktreeInfo`
#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
pub struct WorktreeInfo {
    pub directory: AbsolutePath,
}
/// Error types.
pub mod error {
    /// Error from a `TryFrom` or `FromStr` implementation.
    pub struct ConversionError(::std::borrow::Cow<'static, str>);
    impl ::std::error::Error for ConversionError {}
    impl ::std::fmt::Display for ConversionError {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
            ::std::fmt::Display::fmt(&self.0, f)
        }
    }
    impl ::std::fmt::Debug for ConversionError {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
            ::std::fmt::Debug::fmt(&self.0, f)
        }
    }
    impl From<&'static str> for ConversionError {
        fn from(value: &'static str) -> Self {
            Self(value.into())
        }
    }
    impl From<String> for ConversionError {
        fn from(value: String) -> Self {
            Self(value.into())
        }
    }
}
