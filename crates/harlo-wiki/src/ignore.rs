//! A repository's `.openwikiignore`: gitignore-like patterns for paths the
//! wiki must not read or cite, matched as OpenWiki matches them (the last
//! matching rule wins, `!` re-includes, `/` anchors, a trailing `/` is a
//! folder).

use crate::jsre::Re;
use std::path::Path;

pub const FILE: &str = ".openwikiignore";

#[derive(Clone)]
struct Rule {
    directory_only: bool,
    matcher: Re,
    negated: bool,
}

impl Rule {
    fn compile(pattern: &str) -> Option<Rule> {
        let mut p = pattern.replace('\\', "/");
        let negated = p.starts_with('!');
        if negated {
            p.remove(0);
        }
        let collapsed = strip_dot_slashes(&p);
        let anchored = collapsed.starts_with('/');
        let directory_only = collapsed.ends_with('/');
        let core = collapsed.trim_start_matches('/').trim_end_matches('/');
        if core.is_empty() {
            return None;
        }
        let source = glob_source(core);
        let matcher = if anchored || core.contains('/') { format!("^{source}(?:/.*)?$") } else { format!("(^|/){source}(/.*)?$") };
        Some(Rule { directory_only, matcher: Re::new(&matcher, "iu"), negated })
    }

    fn matches(&self, path: &str, is_directory: bool) -> bool {
        if !self.matcher.test(path) {
            return false;
        }
        !self.directory_only || is_directory || path.contains('/')
    }
}

/// The pattern as OpenWiki normalizes it: a leading "./" (and the slashes
/// after it) dropped, then runs of slashes made one.
fn strip_dot_slashes(p: &str) -> String {
    let rest = match p.strip_prefix("./") {
        Some(r) => r.trim_start_matches('/'),
        None => p,
    };
    let mut out = String::with_capacity(rest.len());
    for c in rest.chars() {
        if c == '/' && out.ends_with('/') {
            continue;
        }
        out.push(c);
    }
    out
}

fn glob_source(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            if chars.get(i + 2) == Some(&'/') {
                out.push_str("(?:.*/)?");
                i += 3;
            } else {
                out.push_str(".*");
                i += 2;
            }
            continue;
        }
        match c {
            '*' => out.push_str("[^/]*"),
            '?' => out.push_str("[^/]"),
            '|' | '\\' | '{' | '}' | '(' | ')' | '[' | ']' | '^' | '$' | '+' | '.' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

#[derive(Clone)]
pub struct Ignore {
    pub patterns: Vec<String>,
    rules: Vec<Rule>,
}

impl Ignore {
    pub fn parse(contents: &str) -> Ignore {
        let patterns: Vec<String> = contents
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l))
            .map(crate::jsre::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
            .collect();
        let rules = patterns.iter().filter_map(|p| Rule::compile(p)).collect();
        Ignore { patterns, rules }
    }

    /// The repository's, or none (an unreadable one is an error).
    pub fn load(root: &Path) -> std::io::Result<Ignore> {
        match std::fs::read(root.join(FILE)) {
            Ok(b) => Ok(Ignore::parse(&String::from_utf8_lossy(&b))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Ignore::parse("")),
            Err(e) => Err(e),
        }
    }

    pub fn is_active(&self) -> bool {
        !self.rules.is_empty()
    }

    pub fn ignores(&self, path: &str, is_directory: bool) -> bool {
        let p = normalize(path);
        if p.is_empty() {
            return false;
        }
        let mut ignored = false;
        for r in &self.rules {
            if r.matches(&p, is_directory) {
                ignored = !r.negated;
            }
        }
        ignored
    }
}

fn normalize(path: &str) -> String {
    let slashed = path.replace('\\', "/");
    let n = crate::store::posix_normalize(&format!("/{}", slashed.trim_start_matches('/')));
    n.trim_start_matches('/').trim_end_matches('/').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        let i = Ignore::parse("# secrets\nsecrets/\n*.key\n!public.key\n/build\ndocs/**/draft.md\n./a//b\n");
        assert!(i.ignores("secrets/x.txt", false));
        assert!(i.ignores("nested/secrets/x", false));
        assert!(!i.ignores("secrets", false), "a folder-only rule, asked of a file");
        assert!(i.ignores("secrets", true));
        assert!(i.ignores("a/b.KEY", false));
        assert!(!i.ignores("public.key", false));
        assert!(i.ignores("build/out.o", false));
        assert!(!i.ignores("src/build/out.o", false));
        assert!(i.ignores("docs/draft.md", false));
        assert!(i.ignores("docs/x/y/draft.md", false));
        assert!(i.ignores("a/b", false));
        assert!(i.ignores("./x/../secrets/k", false));
        assert!(!i.ignores("", false));
        assert!(!Ignore::parse("\n# c\n").is_active());
    }
}
