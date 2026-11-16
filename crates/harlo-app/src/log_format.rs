//! How a log line reads: the event's message template with its named
//! properties filled in, then any properties it doesn't name.
//!
//! The events are message templates (`"{{harness.kind}} session opened"`
//! with `harness.kind = "claude"`), formatted only when a line is written.
//! The plain `tracing_subscriber` format prints the template as is; this one
//! writes `claude session opened`.

// Rust guideline compliant 2026-10-09

use std::fmt::{self, Write as _};

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::{FormatTime, SystemTime};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

/// Writes `<time> <LEVEL> <target>: <filled message> <other properties>`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Templates;

impl<S, N> FormatEvent<S, N> for Templates
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(&self, _ctx: &FmtContext<'_, S, N>, mut writer: Writer<'_>, event: &Event<'_>) -> fmt::Result {
        let ansi = writer.has_ansi_escapes();
        let mut fields = Fields::default();
        event.record(&mut fields);
        let meta = event.metadata();
        // (A `log` crate record carries its own target as a property.)
        let target = fields.log_target.as_deref().unwrap_or_else(|| meta.target());

        if ansi {
            writer.write_str(DIM)?;
        }
        SystemTime.format_time(&mut writer)?;
        if ansi {
            write!(writer, "{RESET} {}{:>5}{RESET} {DIM}{target}:{RESET} ", level_color(*meta.level()), meta.level())?;
        } else {
            write!(writer, " {:>5} {target}: ", meta.level())?;
        }
        writeln!(writer, "{}", render(&fields.message, &fields.rest))
    }
}

/// Faint, for the time and the target.
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

fn level_color(level: Level) -> &'static str {
    match level {
        Level::ERROR => "\x1b[31m",
        Level::WARN => "\x1b[33m",
        Level::INFO => "\x1b[32m",
        Level::DEBUG => "\x1b[34m",
        Level::TRACE => "\x1b[35m",
    }
}

/// An event's message and properties, as text.
#[derive(Debug, Default)]
struct Fields {
    message: String,
    rest: Vec<(&'static str, String)>,
    log_target: Option<String>,
}

impl Fields {
    fn put(&mut self, field: &Field, value: String) {
        match field.name() {
            "message" => self.message = value,
            "log.target" => self.log_target = Some(value),
            // (The `log` crate's source location: not shown, as for events.)
            "log.module_path" | "log.file" | "log.line" => {}
            name => self.rest.push((name, value)),
        }
    }
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.put(field, format!("{value:?}"));
    }
}

/// The message with each `{name}` that names a property replaced by its
/// value, then ` name=value` for the properties it doesn't name.
///
/// Braces that don't name a property (JSON, say) stay as they are.
fn render(template: &str, fields: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len() + 32);
    let mut used = vec![false; fields.len()];
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let found = after.find('}').and_then(|close| {
            let name = &after[..close];
            let plain = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.');
            let at = fields.iter().position(|(n, _)| *n == name).filter(|_| plain)?;
            Some((at, close))
        });
        if let Some((at, close)) = found {
            out.push_str(&fields[at].1);
            used[at] = true;
            rest = &after[close + 1..];
        } else {
            out.push('{');
            rest = after;
        }
    }
    out.push_str(rest);
    for ((name, value), used) in fields.iter().zip(used) {
        if !used {
            let _ = write!(out, " {name}={value}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::render;

    fn fields(pairs: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
        pairs.iter().map(|(n, v)| (*n, (*v).to_owned())).collect()
    }

    #[test]
    fn fills_named_properties() {
        let f = fields(&[("harness.kind", "claude"), ("process.working_directory", "/home/a/p"), ("session.resumed", "true")]);
        assert_eq!(render("{harness.kind} session in {process.working_directory} opened", &f), "claude session in /home/a/p opened session.resumed=true");
    }

    #[test]
    fn leaves_other_braces() {
        let f = fields(&[("n", "3")]);
        assert_eq!(render(r#"got {"a":1} and {n} and {missing} and {"#, &f), r#"got {"a":1} and 3 and {missing} and {"#);
    }

    #[test]
    fn a_property_used_twice() {
        let f = fields(&[("phone.name", "iPhone")]);
        assert_eq!(render("{phone.name}: {phone.name}", &f), "iPhone: iPhone");
    }

    #[test]
    fn plain_message_lists_properties() {
        assert_eq!(render("started", &fields(&[("process.pid", "42")])), "started process.pid=42");
        assert_eq!(render("", &[]), "");
    }
}
