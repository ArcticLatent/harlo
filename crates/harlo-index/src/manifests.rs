//! Package manifests: each package's name and dependencies, read as text.
//!
//! Each format gets a small reader for the parts that name dependencies,
//! not a full parser: TOML sections and keys (`Cargo.toml`,
//! `pyproject.toml`), JSON objects (`package.json`, `composer.json`), XML
//! elements (`pom.xml`), YAML keys (`pubspec.yaml`) and the calls that
//! declare them in code (`build.gradle`, `Gemfile`, `Package.swift`,
//! `mix.exs`). What a reader doesn't understand it leaves out.

// Rust guideline compliant 2026-10-09

#[cfg(test)]
use crate::import_names;
use crate::{Dependency, Ecosystem, Manifest};

/// The manifest in the file at `path` (from the repository's root), whose
/// text is `text`; `None` for a file that isn't one.
#[must_use]
pub fn manifest(path: &str, text: &str) -> Option<Manifest> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let folder = path.rsplit_once('/').map_or("", |(d, _)| d);
    let (ecosystem, read): (Ecosystem, fn(&str) -> Read) = match name {
        "Cargo.toml" => (Ecosystem::Cargo, cargo),
        "package.json" => (Ecosystem::Npm, npm),
        "pyproject.toml" => (Ecosystem::Python, pyproject),
        "go.mod" => (Ecosystem::Go, go_mod),
        "pom.xml" => (Ecosystem::Maven, pom),
        "build.gradle" | "build.gradle.kts" => (Ecosystem::Gradle, gradle),
        "Gemfile" => (Ecosystem::Ruby, gemfile),
        "composer.json" => (Ecosystem::Composer, composer),
        "Package.swift" => (Ecosystem::Swift, swift),
        "pubspec.yaml" => (Ecosystem::Dart, pubspec),
        "mix.exs" => (Ecosystem::Mix, mix),
        _ if name.ends_with(".gemspec") => (Ecosystem::Ruby, gemfile),
        _ if name.starts_with("requirements") && name.ends_with(".txt") => (Ecosystem::Python, requirements),
        _ => return None,
    };
    let mut read = read(text);
    if read.name.is_none() && ecosystem == Ecosystem::Gradle {
        // (A Gradle project is named by its folder unless its settings say otherwise.)
        read.name = folder.rsplit('/').next().filter(|f| !f.is_empty()).map(str::to_owned);
    }
    if name.starts_with("requirements") && (name.contains("dev") || name.contains("test")) {
        for d in &mut read.deps {
            d.dev = true;
        }
    }
    for d in &mut read.deps {
        d.local = d.local.take().and_then(|l| join(folder, &l));
    }
    Some(Manifest { path: path.to_owned(), ecosystem, name: read.name, dependencies: read.deps })
}

/// What a reader found.
#[derive(Default)]
struct Read {
    name: Option<String>,
    deps: Vec<Dependency>,
}

impl Read {
    fn dep(&mut self, name: &str, version: Option<String>, dev: bool, local: Option<String>, line: usize) {
        let name = name.trim();
        if !name.is_empty() && !self.deps.iter().any(|d| d.name == name && d.dev == dev) {
            self.deps.push(Dependency { name: name.to_owned(), version: version.filter(|v| !v.is_empty()), dev, local, line });
        }
    }
}

/// `folder/rel`, with `.` and `..` worked out; `None` when it leaves the repository.
fn join(folder: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = folder.split('/').filter(|p| !p.is_empty()).collect();
    for p in rel.trim_start_matches("file:").split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

/// A quoted string's contents (`"x"` or `'x'`), else the trimmed text.
fn unquote(s: &str) -> &str {
    let s = s.trim().trim_end_matches(',');
    s.strip_prefix('"').and_then(|s| s.strip_suffix('"')).or_else(|| s.strip_prefix('\'').and_then(|s| s.strip_suffix('\''))).unwrap_or(s)
}

/// A key in an inline table or call (`path = "../x"`, `path: "../x"`), unquoted.
fn field(text: &str, key: &str) -> Option<String> {
    for sep in [" = ", "=", ": ", ":"] {
        let pat = format!("{key}{sep}");
        let mut from = 0;
        while let Some(i) = text[from..].find(&pat).map(|i| i + from) {
            let before = text[..i].chars().next_back();
            from = i + pat.len();
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-') {
                continue;
            }
            let rest = text[from..].trim_start();
            let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
            let end = rest[1..].find(quote)?;
            return Some(rest[1..=end].to_owned());
        }
    }
    None
}

/// A TOML file's lines with their sections: `(line, section, text)`.
fn toml_lines(text: &str) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    let mut section = String::new();
    for (i, raw) in text.lines().enumerate() {
        let line = strip_hash_comment(raw).trim();
        if let Some(s) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            s.trim_matches(['[', ']']).trim().clone_into(&mut section);
            continue;
        }
        if !line.is_empty() {
            out.push((i + 1, section.clone(), line.to_owned()));
        }
    }
    out
}

/// A line without its `#` comment (outside quotes).
fn strip_hash_comment(line: &str) -> &str {
    let mut quote = None;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, '#') => return &line[..i],
            _ => {}
        }
    }
    line
}

/// A line without its `//` comment (outside quotes: a URL keeps its `//`).
fn strip_slash_comment(line: &str) -> &str {
    let mut quote = None;
    let bytes = line.as_bytes();
    for (i, &c) in bytes.iter().enumerate() {
        match (quote, c) {
            (None, b'"' | b'\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, b'/') if bytes.get(i + 1) == Some(&b'/') => return &line[..i],
            _ => {}
        }
    }
    line
}

/// `key = value` split, the key unquoted.
fn key_value(line: &str) -> Option<(&str, &str)> {
    let (k, v) = line.split_once('=')?;
    Some((unquote(k.trim()), v.trim()))
}

fn cargo(text: &str) -> Read {
    let mut r = Read::default();
    for (line, section, l) in toml_lines(text) {
        let Some((key, value)) = key_value(&l) else { continue };
        if section == "package" && key == "name" {
            r.name = Some(unquote(value).to_owned());
            continue;
        }
        // `[dependencies]`, `[dev-dependencies]`, `[workspace.dependencies]`,
        // `[target.'cfg(unix)'.dependencies]`, and `[dependencies.serde]`.
        let (deps_section, name) = if section.ends_with("dependencies") {
            (section.as_str(), key.split('.').next().unwrap_or(key))
        } else if let Some((s, n)) = section.rsplit_once('.').filter(|(s, _)| s.ends_with("dependencies")) {
            (s, n)
        } else {
            continue;
        };
        let dev = deps_section.ends_with("dev-dependencies") || deps_section.ends_with("build-dependencies");
        let (version, local) = if value.starts_with('{') {
            (field(value, "version"), field(value, "path"))
        } else if section.ends_with("dependencies") {
            (value.starts_with(['"', '\'']).then(|| unquote(value).to_owned()), None)
        } else {
            // `[dependencies.serde]` then `version = "1"`, `path = "…"`.
            match key {
                "version" => (Some(unquote(value).to_owned()), None),
                "path" => (None, Some(unquote(value).to_owned())),
                _ => (None, None),
            }
        };
        merge(&mut r, name, version, dev, local, line);
    }
    r
}

/// Adds a dependency, or fills in one already listed (a table's later keys).
fn merge(r: &mut Read, name: &str, version: Option<String>, dev: bool, local: Option<String>, line: usize) {
    if let Some(d) = r.deps.iter_mut().find(|d| d.name == name && d.dev == dev) {
        d.version = d.version.take().or(version);
        d.local = d.local.take().or(local);
    } else {
        r.dep(name, version, dev, local, line);
    }
}

fn pyproject(text: &str) -> Read {
    let mut r = Read::default();
    let lines = toml_lines(text);
    // Arrays of requirement strings may span lines: `dependencies = [ … ]`.
    let mut array: Option<bool> = None;
    for (line, section, l) in &lines {
        if let Some(dev) = array {
            for item in l.split(',') {
                let item = item.trim().trim_end_matches(']');
                if item.starts_with(['"', '\'']) {
                    requirement(&mut r, unquote(item), dev, *line);
                }
            }
            if l.contains(']') {
                array = None;
            }
            continue;
        }
        let Some((key, value)) = key_value(l) else { continue };
        if (section == "project" || section == "tool.poetry") && key == "name" {
            r.name = Some(unquote(value).to_owned());
        }
        let list = (section == "project" && key == "dependencies") || section == "project.optional-dependencies" || section == "dependency-groups";
        if list && value.starts_with('[') {
            let dev = section != "project" && is_dev_group(key);
            for item in value.trim_start_matches('[').split(',') {
                let item = item.trim().trim_end_matches(']');
                if item.starts_with(['"', '\'']) {
                    requirement(&mut r, unquote(item), dev, *line);
                }
            }
            if !value.contains(']') {
                array = Some(dev);
            }
            continue;
        }
        // Poetry: `[tool.poetry.dependencies]`, `[tool.poetry.group.dev.dependencies]`.
        if section.starts_with("tool.poetry") && section.ends_with("dependencies") && key != "python" {
            let dev = section.contains("dev") || section.contains("group.") && !section.contains("group.main");
            let local = value.starts_with('{').then(|| field(value, "path")).flatten();
            let version = if value.starts_with('{') { field(value, "version") } else { Some(unquote(value).to_owned()) };
            r.dep(key, version, dev, local, *line);
        }
    }
    r
}

fn is_dev_group(name: &str) -> bool {
    ["dev", "test", "lint", "doc", "typing", "check"].iter().any(|g| name.contains(g))
}

/// One PEP 508 requirement (`requests>=2; python_version>"3"`).
fn requirement(r: &mut Read, req: &str, dev: bool, line: usize) {
    let req = req.trim();
    let end = req.find(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))).unwrap_or(req.len());
    let (name, rest) = req.split_at(end);
    let rest = rest.trim_start();
    let rest = if rest.starts_with('[') { rest.find(']').map_or(rest, |i| &rest[i + 1..]) } else { rest };
    let version = rest.split(';').next().map(str::trim).filter(|v| !v.is_empty() && !v.starts_with('@')).map(str::to_owned);
    r.dep(name, version, dev, None, line);
}

fn requirements(text: &str) -> Read {
    let mut r = Read::default();
    for (i, raw) in text.lines().enumerate() {
        let l = strip_hash_comment(raw).trim();
        if let Some(path) = l.strip_prefix("-e ").map(str::trim).filter(|p| p.starts_with('.')) {
            let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or(path);
            r.dep(name, None, false, Some(path.to_owned()), i + 1);
        } else if !l.is_empty() && !l.starts_with('-') && !l.contains("://") {
            requirement(&mut r, l, false, i + 1);
        }
    }
    r
}

/// The keys of a JSON object (as a `serde_json` value) with their lines in `text`.
fn json_deps(r: &mut Read, text: &str, v: &serde_json::Value, key: &str, dev: bool, skip: fn(&str) -> bool) {
    let Some(obj) = v.get(key).and_then(|d| d.as_object()) else { return };
    let section = text.find(&format!("\"{key}\"")).unwrap_or(0);
    for (name, version) in obj {
        if skip(name) {
            continue;
        }
        let at = text[section..].find(&format!("\"{name}\"")).map_or(section, |i| section + i);
        let line = text[..at].matches('\n').count() + 1;
        let version = version.as_str().map(str::to_owned);
        let local = version.as_deref().and_then(|v| v.strip_prefix("file:").or_else(|| v.strip_prefix("link:"))).map(str::to_owned);
        r.dep(name, version, dev, local, line);
    }
}

fn npm(text: &str) -> Read {
    let mut r = Read::default();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return r };
    r.name = v.get("name").and_then(|n| n.as_str()).map(str::to_owned);
    for (key, dev) in [("dependencies", false), ("peerDependencies", false), ("optionalDependencies", false), ("devDependencies", true)] {
        json_deps(&mut r, text, &v, key, dev, |_| false);
    }
    r
}

fn composer(text: &str) -> Read {
    let mut r = Read::default();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return r };
    r.name = v.get("name").and_then(|n| n.as_str()).map(str::to_owned);
    let platform = |n: &str| n == "php" || n.starts_with("ext-") || n.starts_with("lib-");
    json_deps(&mut r, text, &v, "require", false, platform);
    json_deps(&mut r, text, &v, "require-dev", true, platform);
    r
}

fn go_mod(text: &str) -> Read {
    let mut r = Read::default();
    let mut block: Option<&str> = None;
    let mut replaced: Vec<(String, String)> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let l = strip_slash_comment(raw).trim();
        if l == ")" {
            block = None;
            continue;
        }
        let (word, rest) = match block {
            Some(b) => (b, l),
            None => l.split_once(char::is_whitespace).map_or((l, ""), |(w, rest)| (w, rest.trim())),
        };
        if rest == "(" {
            block = Some(word);
            continue;
        }
        match word {
            "module" => r.name = Some(unquote(rest).to_owned()),
            "require" => {
                let mut parts = rest.split_whitespace();
                if let Some(name) = parts.next() {
                    r.dep(name, parts.next().map(str::to_owned), false, None, i + 1);
                }
            }
            "replace" => {
                if let Some((from, to)) = rest.split_once("=>") {
                    let to = to.trim();
                    if to.starts_with('.') {
                        replaced.push((from.split_whitespace().next().unwrap_or("").to_owned(), to.to_owned()));
                    }
                }
            }
            _ => {}
        }
    }
    for (name, path) in replaced {
        if let Some(d) = r.deps.iter_mut().find(|d| d.name == name) {
            d.local = Some(path);
        }
    }
    r
}

/// The text of each `<tag>…</tag>` in `text`, with its byte offset.
fn elements<'t>(text: &'t str, tag: &str) -> Vec<(usize, &'t str)> {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(a) = text[from..].find(&open).map(|i| from + i) {
        let Some(b) = text[a..].find(&close).map(|i| a + i) else { break };
        out.push((a, &text[a + open.len()..b]));
        from = b + close.len();
    }
    out
}

fn pom(text: &str) -> Read {
    let mut r = Read::default();
    // The project's own id: outside `<parent>`, `<dependencies>` and `<build>`.
    let mut own = text.to_owned();
    for tag in ["parent", "dependencies", "dependencyManagement", "build", "plugins", "profiles"] {
        for (_, inner) in elements(text, tag) {
            own = own.replace(inner, "");
        }
    }
    let first = |t: &str, tag: &str| elements(t, tag).first().map(|(_, v)| v.trim().to_owned());
    r.name = match (first(&own, "groupId"), first(&own, "artifactId")) {
        (Some(g), Some(a)) => Some(format!("{g}:{a}")),
        (None, a) => a,
        (Some(_), None) => None,
    };
    for (at, dep) in elements(text, "dependency") {
        let (Some(group), Some(artifact)) = (first(dep, "groupId"), first(dep, "artifactId")) else { continue };
        let dev = first(dep, "scope").is_some_and(|s| s == "test" || s == "provided");
        let line = text[..at].matches('\n').count() + 1;
        r.dep(&format!("{group}:{artifact}"), first(dep, "version"), dev, None, line);
    }
    r
}

fn gradle(text: &str) -> Read {
    const CONFIGS: [&str; 12] = [
        "implementation",
        "api",
        "compileOnly",
        "runtimeOnly",
        "compile",
        "kapt",
        "ksp",
        "annotationProcessor",
        "testImplementation",
        "testRuntimeOnly",
        "androidTestImplementation",
        "testCompileOnly",
    ];
    let mut r = Read::default();
    for (i, raw) in text.lines().enumerate() {
        let l = strip_slash_comment(raw).trim();
        let Some(config) = CONFIGS.iter().find(|c| l.starts_with(**c) && l[c.len()..].starts_with([' ', '(', '\t'])) else { continue };
        let rest = l[config.len()..].trim_start().trim_start_matches('(').trim();
        let dev = config.starts_with("test") || config.starts_with("android");
        if let Some(project) = rest.strip_prefix("project(") {
            // `project(":core")`: a module in this build.
            let name = unquote(project.trim_end_matches(')').trim_end_matches(')')).trim_start_matches(':');
            r.dep(name, None, dev, Some(format!("../{}", name.replace(':', "/"))), i + 1);
            continue;
        }
        let coord = unquote(rest.trim_end_matches(')'));
        let parts: Vec<&str> = coord.split(':').collect();
        if parts.len() >= 2 && !coord.contains(' ') {
            r.dep(&format!("{}:{}", parts[0], parts[1]), parts.get(2).map(|v| (*v).to_owned()), dev, None, i + 1);
        }
    }
    r
}

fn gemfile(text: &str) -> Read {
    let mut r = Read::default();
    let mut dev_block = false;
    for (i, raw) in text.lines().enumerate() {
        let l = strip_hash_comment(raw).trim();
        if l.starts_with("group ") {
            dev_block = l.contains(":development") || l.contains(":test");
            continue;
        }
        if l == "end" {
            dev_block = false;
            continue;
        }
        if let Some(name) = l.split_once(".name").and_then(|(_, v)| v.trim().strip_prefix('=')) {
            r.name = Some(unquote(name.trim()).to_owned());
            continue;
        }
        let (word, dev) = if let Some(rest) = l.strip_prefix("gem ") {
            (rest, dev_block)
        } else if let Some((_, rest)) = l.split_once("add_development_dependency") {
            (rest, true)
        } else if let Some((_, rest)) = l.split_once("add_runtime_dependency").or_else(|| l.split_once("add_dependency")) {
            (rest, false)
        } else {
            continue;
        };
        let mut args = word.trim().trim_start_matches('(').split(',');
        let Some(name) = args.next().map(unquote) else { continue };
        let version = args.next().map(|v| unquote(v).to_owned()).filter(|v| !v.contains(':'));
        let local = field(word, "path");
        r.dep(name, version, dev || l.contains(":development") || l.contains(":test"), local, i + 1);
    }
    r
}

fn swift(text: &str) -> Read {
    let mut r = Read { name: text.find("Package(").and_then(|at| field(&text[at..], "name")), deps: Vec::new() };
    for (i, raw) in text.lines().enumerate() {
        let l = strip_slash_comment(raw).trim();
        let Some(at) = l.find(".package(") else { continue };
        let args = &l[at..];
        if let Some(path) = field(args, "path") {
            let name = field(args, "name").unwrap_or_else(|| path.trim_end_matches('/').rsplit('/').next().unwrap_or(&path).to_owned());
            r.dep(&name, None, false, Some(path), i + 1);
        } else if let Some(url) = field(args, "url") {
            let base = url.trim_end_matches('/').trim_end_matches(".git");
            let name = field(args, "name").unwrap_or_else(|| base.rsplit('/').next().unwrap_or(base).to_owned());
            r.dep(&name, field(args, "from").or_else(|| field(args, "exact")), false, None, i + 1);
        }
    }
    r
}

fn pubspec(text: &str) -> Read {
    let mut r = Read::default();
    let mut section: Option<bool> = None;
    let mut current: Option<usize> = None;
    for (i, raw) in text.lines().enumerate() {
        let l = strip_hash_comment(raw);
        if l.trim().is_empty() {
            continue;
        }
        let indent = l.len() - l.trim_start().len();
        let t = l.trim();
        if indent == 0 {
            current = None;
            section = match t {
                "dependencies:" => Some(false),
                "dev_dependencies:" => Some(true),
                _ => None,
            };
            if let Some(name) = t.strip_prefix("name:") {
                r.name = Some(unquote(name).to_owned());
            }
            continue;
        }
        let Some(dev) = section else { continue };
        let Some((key, value)) = t.split_once(':') else { continue };
        if indent <= 2 {
            r.dep(key, Some(unquote(value).to_owned()).filter(|v| !v.is_empty()), dev, None, i + 1);
            current = Some(r.deps.len() - 1);
        } else if key == "path"
            && let Some(c) = current
        {
            r.deps[c].local = Some(unquote(value).to_owned());
        }
    }
    r
}

fn mix(text: &str) -> Read {
    let mut r = Read::default();
    if let Some(app) = text.split_once("app:").map(|(_, rest)| rest.trim_start()) {
        r.name = app.strip_prefix(':').map(|a| a.split(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or(a).to_owned());
    }
    for (i, raw) in text.lines().enumerate() {
        let l = strip_hash_comment(raw).trim();
        let Some(rest) = l.strip_prefix("{:") else { continue };
        let name = rest.split(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or("");
        let version = rest.split_once(',').and_then(|(_, v)| v.trim().strip_prefix('"')).and_then(|v| v.split('"').next()).map(str::to_owned);
        let dev = l.contains("only: :test") || l.contains("only: :dev") || (l.contains("only: [") && !l.contains(":prod"));
        r.dep(name, version, dev, field(l, "path"), i + 1);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deps(m: &Manifest) -> Vec<String> {
        m.dependencies
            .iter()
            .map(|d| {
                let mut s = d.name.clone();
                if let Some(v) = &d.version {
                    s.push(' ');
                    s.push_str(v);
                }
                if d.dev {
                    s += " (dev)";
                }
                if let Some(l) = &d.local {
                    s.push_str(" @");
                    s.push_str(l);
                }
                s
            })
            .collect()
    }

    #[test]
    fn cargo_toml() {
        let m = manifest(
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\ncore = { path = \"../core\" }\nlog = \"0.4\" # logging\nharlo-index.workspace = true\n\n[dependencies.rand]\nversion = \"0.8\"\n\n[dev-dependencies]\ntempfile = \"3\"\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n",
        )
        .unwrap();
        assert_eq!(m.name.as_deref(), Some("app"));
        assert_eq!(deps(&m), ["serde 1", "core @crates/core", "log 0.4", "harlo-index", "rand 0.8", "tempfile 3 (dev)", "libc 0.2"]);
        assert_eq!(m.dependencies[2].line, 7);
    }

    #[test]
    fn python() {
        let m = manifest(
            "pyproject.toml",
            "[project]\nname = \"shop\"\ndependencies = [\n  \"requests>=2.31\",\n  \"PyYAML\",  # config\n  \"rich[jupyter]~=13; python_version>'3.8'\",\n]\n\n[project.optional-dependencies]\ntest = [\"pytest>=8\"]\n",
        )
        .unwrap();
        assert_eq!(deps(&m), ["requests >=2.31", "PyYAML", "rich ~=13", "pytest >=8 (dev)"]);
        let m = manifest("requirements-dev.txt", "# tools\nruff==0.6\n-e ./libs/core\n-r base.txt\n").unwrap();
        assert_eq!(deps(&m), ["ruff ==0.6 (dev)", "core (dev) @libs/core"]);
        assert_eq!(import_names(Ecosystem::Python, "PyYAML"), ["yaml", "pyyaml"]);
    }

    #[test]
    fn npm_and_composer() {
        let m = manifest(
            "web/package.json",
            "{\n  \"name\": \"web\",\n  \"dependencies\": {\n    \"react\": \"^18\",\n    \"ui\": \"file:../ui\"\n  },\n  \"devDependencies\": { \"vitest\": \"^1\" }\n}\n",
        )
        .unwrap();
        assert_eq!(deps(&m), ["react ^18", "ui file:../ui @ui", "vitest ^1 (dev)"]);
        assert_eq!(m.dependencies[0].line, 4);
        let m = manifest("composer.json", "{\"name\": \"a/b\", \"require\": {\"php\": \">=8\", \"monolog/monolog\": \"^3\"}}").unwrap();
        assert_eq!(deps(&m), ["monolog/monolog ^3"]);
        assert_eq!(import_names(Ecosystem::Composer, "monolog/monolog"), ["Monolog\\Monolog"]);
    }

    #[test]
    fn go_and_jvm() {
        let m = manifest(
            "go.mod",
            "module example.com/shop\n\ngo 1.22\n\nrequire (\n\tgolang.org/x/sync v0.7.0\n\texample.com/core v0.0.0 // indirect\n)\nrequire github.com/a/b v1.2.0\nreplace example.com/core => ./core\n",
        )
        .unwrap();
        assert_eq!(m.name.as_deref(), Some("example.com/shop"));
        assert_eq!(deps(&m), ["golang.org/x/sync v0.7.0", "example.com/core v0.0.0 @core", "github.com/a/b v1.2.0"]);
        let m = manifest(
            "pom.xml",
            "<project><parent><groupId>p</groupId><artifactId>parent</artifactId></parent><groupId>org.shop</groupId><artifactId>cart</artifactId>\n<dependencies>\n<dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId><version>2.0</version></dependency>\n<dependency><groupId>junit</groupId><artifactId>junit</artifactId><scope>test</scope></dependency>\n</dependencies></project>",
        )
        .unwrap();
        assert_eq!(m.name.as_deref(), Some("org.shop:cart"));
        assert_eq!(deps(&m), ["org.slf4j:slf4j-api 2.0", "junit:junit (dev)"]);
        assert_eq!(m.dependencies[0].line, 3);
        let m = manifest(
            "app/build.gradle.kts",
            "dependencies {\n    implementation(\"com.squareup.okhttp3:okhttp:4.12.0\")\n    implementation(project(\":core\"))\n    testImplementation 'junit:junit:4.13'\n}\n",
        )
        .unwrap();
        assert_eq!(m.name.as_deref(), Some("app"));
        assert_eq!(deps(&m), ["com.squareup.okhttp3:okhttp 4.12.0", "core @core", "junit:junit 4.13 (dev)"]);
    }

    #[test]
    fn ruby_swift_dart_mix() {
        let m = manifest("Gemfile", "source \"https://rubygems.org\"\ngem \"rails\", \"~> 7.1\"\ngem 'core', path: 'engines/core'\ngroup :development, :test do\n  gem \"rspec\"\nend\n").unwrap();
        assert_eq!(deps(&m), ["rails ~> 7.1", "core @engines/core", "rspec (dev)"]);
        let m = manifest(
            "Package.swift",
            "let package = Package(\n    name: \"Shop\",\n    dependencies: [\n        .package(url: \"https://github.com/apple/swift-log.git\", from: \"1.5.0\"),\n        .package(path: \"../HarloKit\"),\n    ]\n)\n",
        )
        .unwrap();
        assert_eq!(m.name.as_deref(), Some("Shop"));
        assert_eq!(deps(&m), ["swift-log 1.5.0", "HarloKit"]);
        let m =
            manifest("app/pubspec.yaml", "name: app\ndependencies:\n  http: ^1.2.0\n  core:\n    path: ../core\ndev_dependencies:\n  test: ^1.24.0\n").unwrap();
        assert_eq!(deps(&m), ["http ^1.2.0", "core @core", "test ^1.24.0 (dev)"]);
        let m = manifest("mix.exs", "  def project, do: [app: :shop]\n  defp deps do\n    [\n      {:phoenix, \"~> 1.7\"},\n      {:credo, \"~> 1.7\", only: [:dev, :test]},\n    ]\n  end\n").unwrap();
        assert_eq!(m.name.as_deref(), Some("shop"));
        assert_eq!(deps(&m), ["phoenix ~> 1.7", "credo ~> 1.7 (dev)"]);
        assert!(manifest("src/main.rs", "").is_none());
    }
}
