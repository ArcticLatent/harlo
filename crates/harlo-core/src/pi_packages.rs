//! pi's packages: the extensions it loads.
//!
//! What `pi list` names, each one's `package.json`, and whether it's on are
//! read in one go by [`LIST_SCRIPT`] (here or on a host). Installing and
//! removing are pi's own `pi install` and `pi remove`; switching a package
//! off is pi's documented filter
//! `"extensions": []` on its entry in `settings.json` (it loads none of the
//! package's extensions, its skills and prompts stay), and on removes it.

// Rust guideline compliant 2026-10-09

use serde_json::{Value, json};

/// Prints `pi list`, then pi's settings, then each listed package's
/// `package.json`, each after a `@@` line (`$PI` is pi's program, else
/// `pi` on PATH; `$PI_CODING_AGENT_DIR` pi's folder if set).
pub const LIST_SCRIPT: &str = r#"t=$(mktemp)
"${PI:-pi}" list >"$t" 2>&1; st=$?
printf '@@list %s\n' "$st"; cat "$t"
printf '\n@@settings\n'; cat "${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}/settings.json" 2>/dev/null
sed -n 's/^    //p' "$t" | while IFS= read -r p; do printf '\n@@package %s\n' "$p"; cat "$p/package.json" 2>/dev/null; done
rm -f "$t""#;

/// One of pi's packages, as the extensions list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// As pi's settings have it (`npm:pi-lens`, a path, a git URL): what
    /// `pi remove` takes.
    pub source: String,
    /// Where it's installed.
    pub path: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
}

impl Package {
    /// As the list's JSON has it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({ "source": self.source, "path": self.path, "name": self.name, "version": self.version, "description": self.description, "enabled": self.enabled })
    }
}

/// What [`LIST_SCRIPT`] printed: the packages, and pi's settings as read
/// (for switching one on or off). An error when `pi list` failed.
///
/// # Errors
///
/// When `pi list` didn't work: its output's last line.
pub fn parse(output: &str) -> Result<(Vec<Package>, Value), String> {
    let mut list = String::new();
    let mut status = String::new();
    let mut settings = String::new();
    let mut manifests: Vec<(String, String)> = Vec::new();
    // (A package's lines go to the last one begun.)
    let mut part = "";
    for line in output.lines() {
        if let Some(st) = line.strip_prefix("@@list ") {
            (part, status) = ("list", st.trim().to_owned());
        } else if line == "@@settings" {
            part = "settings";
        } else if let Some(p) = line.strip_prefix("@@package ") {
            part = "package";
            manifests.push((p.to_owned(), String::new()));
        } else {
            let into = match part {
                "list" => &mut list,
                "settings" => &mut settings,
                "package" => match manifests.last_mut() {
                    Some((_, m)) => m,
                    None => continue,
                },
                _ => continue,
            };
            into.push_str(line);
            into.push('\n');
        }
    }
    if status != "0" {
        let why = list.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or("pi didn't answer");
        return Err(format!("pi list: {why}"));
    }
    let settings: Value = serde_json::from_str(&settings).unwrap_or_else(|_| json!({}));
    let packages = listed(&list)
        .into_iter()
        .map(|(source, path)| {
            let manifest: Value = manifests.iter().find(|(p, _)| *p == path).and_then(|(_, m)| serde_json::from_str(m).ok()).unwrap_or(Value::Null);
            let text = |k: &str| manifest[k].as_str().unwrap_or_default().to_owned();
            let name = Some(text("name")).filter(|n| !n.is_empty()).unwrap_or_else(|| short_name(&source));
            Package { enabled: enabled(&settings, &source), name, version: text("version"), description: text("description"), source, path }
        })
        .collect();
    Ok((packages, settings))
}

/// The user's packages in `pi list`'s output: (source, path), from its
/// "User packages:" section (a source, then its path indented under it).
fn listed(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut user = false;
    let mut source: Option<String> = None;
    for line in text.lines() {
        if !line.starts_with(' ') {
            user = line.trim_end() == "User packages:";
            continue;
        }
        if !user {
            continue;
        }
        if let Some(path) = line.strip_prefix("    ") {
            if let Some(s) = source.take() {
                out.push((s, path.trim().to_owned()));
            }
        } else if let Some(s) = line.strip_prefix("  ") {
            // (A package with a filter: "<source> (filtered)".)
            let s = s.trim();
            source = Some(s.strip_suffix(" (filtered)").unwrap_or(s).to_owned());
        }
    }
    out
}

/// A package's name from its source, when it has no `package.json`.
fn short_name(source: &str) -> String {
    let s = source.trim_start_matches("npm:").trim_start_matches("git:").trim_end_matches('/');
    let s = s.rsplit('/').next().unwrap_or(s);
    s.strip_suffix(".git").unwrap_or(s).to_owned()
}

/// The package's entry in pi's settings (`packages`), as its index.
fn entry(settings: &Value, source: &str) -> Option<usize> {
    settings["packages"].as_array()?.iter().position(|p| p.as_str().or_else(|| p["source"].as_str()) == Some(source))
}

/// Whether pi loads a package's extensions: off only with the filter
/// `"extensions": []`.
#[must_use]
pub fn enabled(settings: &Value, source: &str) -> bool {
    entry(settings, source).is_none_or(|i| settings["packages"][i]["extensions"].as_array().is_none_or(|a| !a.is_empty()))
}

/// pi's settings with a package switched on or off (None: it isn't in
/// them, or it already is). Off adds the filter `"extensions": []`; on
/// takes that filter away (any other filter of the user's stays).
#[must_use]
pub fn switched(settings: &Value, source: &str, on: bool) -> Option<Value> {
    let i = entry(settings, source)?;
    if enabled(settings, source) == on {
        return None;
    }
    let mut out = settings.clone();
    let slot = &mut out["packages"][i];
    let mut item = match slot.take() {
        Value::String(s) => json!({ "source": s }),
        v => v,
    };
    if on {
        if let Some(o) = item.as_object_mut() {
            o.remove("extensions");
        }
    } else {
        item["extensions"] = json!([]);
    }
    // (Only its source left: the plain form, as pi writes it.)
    *slot = match item.as_object() {
        Some(o) if o.len() == 1 => o["source"].clone(),
        _ => item,
    };
    Some(out)
}

/// The packages harlo suggests, each with what it's for: (source, title,
/// what it does).
pub const RECOMMENDED: [(&str, &str, &str); 2] = [
    ("npm:pi-web-access", "Web access", "Web search, pages, GitHub repositories, PDFs and videos."),
    ("npm:pi-lens", "Lens", "Language servers, linters, formatters and type checks as pi edits."),
];

/// Whether `source` is one of pi's sources: npm, git, a URL or a path
/// (what harlo hands `pi install`, quoted).
#[must_use]
pub fn valid_source(source: &str) -> bool {
    let s = source.trim();
    !s.is_empty() && !s.chars().any(char::is_control) && (s.starts_with("npm:") || s.starts_with("git:") || s.starts_with("https://") || s.starts_with('/') || s.starts_with("./") || s.starts_with("~/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: &str = r#"@@list 0
User packages:
  npm:pi-lens
    /h/.pi/agent/npm/node_modules/pi-lens
  ../../pkg (filtered)
    /h/pkg

Project packages:
  npm:other
    /p/.pi/npm/node_modules/other

@@settings
{ "packages": ["npm:pi-lens", { "source": "../../pkg", "extensions": [] }] }

@@package /h/.pi/agent/npm/node_modules/pi-lens
{ "name": "pi-lens", "version": "4.5.1", "description": "Real-time code feedback" }

@@package /h/pkg
"#;

    #[test]
    fn reads_the_users_packages() {
        let (list, settings) = parse(OUTPUT).unwrap();
        assert_eq!(list.len(), 2, "the project's are left out");
        assert_eq!((list[0].source.as_str(), list[0].name.as_str(), list[0].version.as_str(), list[0].enabled), ("npm:pi-lens", "pi-lens", "4.5.1", true));
        assert_eq!((list[1].path.as_str(), list[1].name.as_str(), list[1].enabled), ("/h/pkg", "pkg", false), "filtered; no package.json: named by its source; off");
        assert!(settings["packages"].is_array());
        assert_eq!(parse("@@list 1\nError: no such command\n").unwrap_err(), "pi list: Error: no such command");
        assert!(parse("@@list 0\nNo packages installed.\n@@settings\n").unwrap().0.is_empty());
    }

    #[test]
    fn switches_a_package_off_and_on_again() {
        let s = json!({ "theme": "dark", "packages": ["npm:a", { "source": "npm:b", "skills": [] }] });
        let off = switched(&s, "npm:a", false).unwrap();
        assert_eq!(off["packages"][0], json!({ "source": "npm:a", "extensions": [] }));
        assert!(!enabled(&off, "npm:a") && enabled(&off, "npm:b"));
        assert!(switched(&off, "npm:a", false).is_none(), "already off");
        assert_eq!(switched(&off, "npm:a", true).unwrap()["packages"][0], json!("npm:a"), "back to the plain form");
        // Another filter of the user's stays.
        let b = switched(&s, "npm:b", false).unwrap();
        assert_eq!(switched(&b, "npm:b", true).unwrap()["packages"][1], json!({ "source": "npm:b", "skills": [] }));
        assert_eq!(b["theme"], "dark");
        assert!(switched(&s, "npm:none", false).is_none());
    }

    #[test]
    fn only_pis_sources_are_installed() {
        assert!(valid_source("npm:pi-lens") && valid_source("git:github.com/a/b@v1") && valid_source("https://github.com/a/b") && valid_source("./x"));
        assert!(!valid_source("") && !valid_source("rm -rf /") && !valid_source("npm:a\nb"));
    }
}
