//! Generates `opencode-client`'s serde types from OpenCode's pinned OpenAPI schema.
//!
//! OpenCode publishes its v2 server contract as an OpenAPI 3.1 document
//! (`packages/protocol/openapi.json` at each release tag). Its component schemas
//! are JSON Schema, so this tool moves them under `definitions`, points every
//! `$ref` there, and lets `typify` turn them into Rust types. The endpoints
//! themselves are written by hand in `opencode-client`.
//!
//! Usage:
//!
//! - `cargo run -p opencode-gen -- <openapi.json> <out.rs>` generates the types
//!   and prints the schema's path, operation, event and schema counts.
//! - `cargo run -p opencode-gen -- diff <old.json> <new.json>` prints how the
//!   contract moved between two versions: counts, then the operations, event
//!   types and schemas that were added, removed or changed. Descriptions and
//!   examples are ignored when deciding whether something changed.

// Rust guideline compliant 2026-10-09

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use typify::{TypeSpace, TypeSpaceSettings};

/// Where OpenAPI component references point.
const COMPONENT_REF: &str = "#/components/schemas/";

/// Where JSON Schema draft-07 (which `schemars` 0.8 reads) expects them.
const DEFINITION_REF: &str = "#/definitions/";

/// HTTP methods that count as operations in the summary.
const METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

/// Keys that document a schema or operation without changing the contract.
const PROSE_KEYS: [&str; 4] = ["description", "summary", "example", "examples"];

const USAGE: &str =
    "usage: opencode-gen <openapi.json> <out.rs> | opencode-gen diff <old.json> <new.json>";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, old, new] if cmd == "diff" => {
            let (old, new) = (read_spec(Path::new(old))?, read_spec(Path::new(new))?);
            print!("{}", diff(&old, &new)?);
            Ok(())
        }
        [input, output] => {
            let spec = read_spec(Path::new(input))?;
            println!("{}", Counts::of(&spec)?.line());
            let code = generate(&spec)?;
            let output = PathBuf::from(output);
            fs::write(&output, code).with_context(|| format!("writing {}", output.display()))
        }
        _ => bail!(USAGE),
    }
}

fn read_spec(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("parsing {} as an OpenAPI document", path.display()))
}

/// Path, operation, event type and schema counts of one document.
#[derive(Debug, Clone, Copy)]
struct Counts {
    paths: usize,
    operations: usize,
    events: usize,
    schemas: usize,
}

impl Counts {
    fn of(spec: &Value) -> Result<Self> {
        let paths = spec["paths"]
            .as_object()
            .context("the document has no paths")?
            .len();
        Ok(Self {
            paths,
            operations: operations(spec).len(),
            events: event_types(spec).len(),
            schemas: spec["components"]["schemas"]
                .as_object()
                .map_or(0, Map::len),
        })
    }

    fn line(self) -> String {
        format!(
            "schema: {} paths, {} operations, {} event types, {} schemas",
            self.paths, self.operations, self.events, self.schemas
        )
    }
}

/// Returns every operation as `METHOD /path`, with its definition.
fn operations(spec: &Value) -> BTreeMap<String, &Value> {
    let Some(paths) = spec["paths"].as_object() else {
        return BTreeMap::new();
    };
    paths
        .iter()
        .filter_map(|(path, ops)| Some((path, ops.as_object()?)))
        .flat_map(|(path, ops)| {
            ops.iter()
                .filter(|(method, _)| METHODS.contains(&method.as_str()))
                .map(move |(method, op)| (format!("{} {path}", method.to_uppercase()), op))
        })
        .collect()
}

/// Describes what changed between two versions of the document.
fn diff(old: &Value, new: &Value) -> Result<String> {
    let (before, after) = (Counts::of(old)?, Counts::of(new)?);
    let mut out = String::new();
    for (name, a, b) in [
        ("paths", before.paths, after.paths),
        ("operations", before.operations, after.operations),
        ("event types", before.events, after.events),
        ("schemas", before.schemas, after.schemas),
    ] {
        writeln!(
            out,
            "  {name:<12} {a:>4} -> {b:<4} ({:+})",
            i64::try_from(b).unwrap_or(i64::MAX) - i64::try_from(a).unwrap_or(i64::MAX)
        )?;
    }

    let empty = Map::new();
    let schemas = |spec: &Value| -> BTreeMap<String, Value> {
        let all = spec["components"]["schemas"].as_object().unwrap_or(&empty);
        all.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    };
    let ops = |spec: &Value| -> BTreeMap<String, Value> {
        operations(spec)
            .into_iter()
            .map(|(k, v)| (k, v.clone()))
            .collect()
    };
    let events = |spec: &Value| -> BTreeMap<String, Value> {
        event_types(spec)
            .into_iter()
            .map(|e| (e, Value::Null))
            .collect()
    };
    section(&mut out, "operations", &ops(old), &ops(new))?;
    section(&mut out, "event types", &events(old), &events(new))?;
    section(&mut out, "schemas", &schemas(old), &schemas(new))?;
    Ok(out)
}

/// Appends the added, removed and changed names of one kind of item.
fn section(
    out: &mut String,
    title: &str,
    old: &BTreeMap<String, Value>,
    new: &BTreeMap<String, Value>,
) -> std::fmt::Result {
    let added: BTreeSet<_> = new.keys().filter(|k| !old.contains_key(*k)).collect();
    let removed: BTreeSet<_> = old.keys().filter(|k| !new.contains_key(*k)).collect();
    let changed: BTreeSet<_> = new
        .iter()
        .filter(|(k, v)| {
            old.get(*k)
                .is_some_and(|o| without_prose(o) != without_prose(v))
        })
        .map(|(k, _)| k)
        .collect();
    if added.is_empty() && removed.is_empty() && changed.is_empty() {
        writeln!(out, "{title}: no changes")?;
        return Ok(());
    }
    writeln!(out, "{title}:")?;
    for (mark, names) in [("+", &added), ("-", &removed), ("~", &changed)] {
        for name in names {
            writeln!(out, "  {mark} {name}")?;
        }
    }
    Ok(())
}

/// Returns a copy with descriptions and examples removed at every level.
fn without_prose(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| !PROSE_KEYS.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), without_prose(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(without_prose).collect()),
        other => other.clone(),
    }
}

/// Returns the event type names `/api/event` accepts as filters.
fn event_types(spec: &Value) -> Vec<String> {
    let params = spec["paths"]["/api/event"]["get"]["parameters"].as_array();
    let Some(types) = params.and_then(|ps| ps.iter().find(|p| p["name"] == "types")) else {
        return Vec::new();
    };
    let schema = &types["schema"];
    let candidates = [
        &schema["anyOf"][0]["enum"],
        &schema["enum"],
        &schema["anyOf"][0]["items"]["enum"],
    ];
    candidates
        .into_iter()
        .find_map(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Turns the document's component schemas into formatted Rust source.
fn generate(spec: &Value) -> Result<String> {
    let mut definitions = spec["components"]["schemas"]
        .as_object()
        .cloned()
        .context("the document has no component schemas")?;
    for schema in definitions.values_mut() {
        rewrite(schema);
    }

    let mut root = Map::new();
    root.insert(
        "$schema".into(),
        "http://json-schema.org/draft-07/schema#".into(),
    );
    root.insert("definitions".into(), Value::Object(definitions));
    let root: schemars::schema::RootSchema = serde_json::from_value(Value::Object(root))
        .context("reading the schemas as JSON Schema")?;

    let settings = TypeSpaceSettings::default();
    let mut space = TypeSpace::new(&settings);
    space.add_root_schema(root).context("generating types")?;

    let file: syn::File = syn::parse2(space.to_stream().context("emitting the generated code")?)
        .context("parsing the generated code")?;
    let body = prettyplease::unparse(&file);
    Ok(format!(
        "//! Types generated from OpenCode's OpenAPI schema by `opencode-gen`. Do not edit.\n\n\
         #![allow(dead_code, clippy::all, clippy::pedantic, clippy::cargo, missing_docs, missing_debug_implementations, reason = \"generated code\")]\n\n{body}"
    ))
}

/// Rewrites OpenAPI 3.1 constructs into the draft-07 forms `typify` understands.
fn rewrite(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(r)) = map.get_mut("$ref")
                && let Some(name) = r.strip_prefix(COMPONENT_REF)
            {
                *r = format!("{DEFINITION_REF}{name}");
            }
            // `not` is used for "any non-null value" and "must be absent"; typify
            // handles neither, and an unconstrained value is a safe reading of both.
            if map.contains_key("not") {
                map.clear();
                return;
            }
            // `prefixItems` repeats `items` for the first element (a non-empty list).
            map.remove("prefixItems");
            // Closed objects become `deny_unknown_fields`, so a field added in a
            // minor OpenCode release would break decoding. Tolerate new fields.
            if map.get("additionalProperties") == Some(&Value::Bool(false)) {
                map.remove("additionalProperties");
            }
            // OpenCode's `anyOf` lists are alternatives a value matches one of
            // (often tagged by `type`). Read as `oneOf`, typify emits enums;
            // read as `anyOf`, it tries to merge them into flattened structs.
            if let Some(alternatives) = map.remove("anyOf") {
                map.insert("oneOf".into(), alternatives);
            }
            for child in map.values_mut() {
                rewrite(child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(rewrite),
        _ => {}
    }
}
