//! A stand-in Claude Code for tests: signed in to a Max plan, and asked
//! for the plan's usage, it answers with one session limit. The real
//! Claude Code and its sign-in are never touched.

// Rust guideline compliant 2026-10-09

#![allow(dead_code, reason = "each user takes part of it")]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

/// The stand-in's program: `PERCENT` is replaced by how used the limit is.
const STAND_IN: &str = r#"#!/usr/bin/env python3
import json, sys
args = sys.argv[1:]
if args[:1] == ["--version"]:
    print("2.1.330 (Claude Code)")
elif args[:2] == ["auth", "status"]:
    print(json.dumps({"loggedIn": True, "authMethod": "claude.ai",
                      "email": "a@example.com", "subscriptionType": "max"}))
else:
    for line in sys.stdin:
        request = json.loads(line)
        if request.get("type") != "control_request":
            continue
        answer = {}
        if request["request"]["subtype"] == "get_usage":
            answer = {"rate_limits_available": True, "rate_limits": {"limits": [
                {"kind": "session", "group": "session", "percent": PERCENT,
                 "severity": "normal", "resets_at": "2033-05-18T03:33:20Z",
                 "is_active": True, "scope": None}]}}
        print(json.dumps({"type": "control_response", "response": {
            "subtype": "success", "request_id": request["request_id"],
            "response": answer}}), flush=True)
"#;

/// When the stand-in's limit resets, in milliseconds since the Unix epoch.
pub const RESETS_AT_MS: i64 = 2_000_000_000_000;

/// Writes the stand-in in `folder`, its session limit `percent` used;
/// returns its path.
pub fn write(folder: &Path, percent: u32) -> PathBuf {
    let path = folder.join("claude");
    std::fs::write(&path, STAND_IN.replace("PERCENT", &percent.to_string())).expect("stand-in");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("executable");
    path
}
