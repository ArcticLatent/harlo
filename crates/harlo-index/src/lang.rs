//! The languages the index reads, by file name.

// Rust guideline compliant 2026-10-09

use serde::{Deserialize, Serialize};
use std::fmt;

/// A language the index reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Rust,
    Python,
    Go,
    JavaScript,
    TypeScript,
    /// TypeScript with JSX.
    Tsx,
    Java,
    C,
    /// C++, and C headers (`.h`), which C++'s grammar reads too.
    Cpp,
    CSharp,
    Ruby,
    Php,
    Swift,
    Kotlin,
    Scala,
    Julia,
    /// Bash and other POSIX shell scripts.
    Bash,
    Lua,
    Qml,
    /// YAML: its top two levels of keys.
    Yaml,
    /// SQL: its `CREATE` statements.
    Sql,
}

impl Lang {
    /// Every language, in the order above.
    pub const ALL: [Lang; 21] = [
        Lang::Rust,
        Lang::Python,
        Lang::Go,
        Lang::JavaScript,
        Lang::TypeScript,
        Lang::Tsx,
        Lang::Java,
        Lang::C,
        Lang::Cpp,
        Lang::CSharp,
        Lang::Ruby,
        Lang::Php,
        Lang::Swift,
        Lang::Kotlin,
        Lang::Scala,
        Lang::Julia,
        Lang::Bash,
        Lang::Lua,
        Lang::Qml,
        Lang::Yaml,
        Lang::Sql,
    ];

    /// Its name, as in the JSON output (`rust`, `csharp`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::Python => "python",
            Lang::Go => "go",
            Lang::JavaScript => "javascript",
            Lang::TypeScript => "typescript",
            Lang::Tsx => "tsx",
            Lang::Java => "java",
            Lang::C => "c",
            Lang::Cpp => "cpp",
            Lang::CSharp => "csharp",
            Lang::Ruby => "ruby",
            Lang::Php => "php",
            Lang::Swift => "swift",
            Lang::Kotlin => "kotlin",
            Lang::Scala => "scala",
            Lang::Julia => "julia",
            Lang::Bash => "bash",
            Lang::Lua => "lua",
            Lang::Qml => "qml",
            Lang::Yaml => "yaml",
            Lang::Sql => "sql",
        }
    }

    /// What separates a container's name from a member's (`Ledger::post`, `Cart.total`).
    #[must_use]
    pub fn separator(self) -> &'static str {
        match self {
            Lang::Rust | Lang::Cpp | Lang::Php => "::",
            _ => ".",
        }
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The language of the file at `path`, if the index reads it.
#[must_use]
pub fn lang(path: &str) -> Option<Lang> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => Lang::Rust,
        "py" | "pyi" => Lang::Python,
        "go" => Lang::Go,
        "js" | "jsx" | "mjs" | "cjs" => Lang::JavaScript,
        "ts" | "mts" | "cts" => Lang::TypeScript,
        "tsx" => Lang::Tsx,
        "java" => Lang::Java,
        "c" => Lang::C,
        "h" | "cc" | "cpp" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" | "ipp" | "tpp" => Lang::Cpp,
        "cs" => Lang::CSharp,
        "rb" | "rake" | "gemspec" => Lang::Ruby,
        "php" => Lang::Php,
        "swift" => Lang::Swift,
        "kt" | "kts" => Lang::Kotlin,
        "scala" | "sc" => Lang::Scala,
        "jl" => Lang::Julia,
        "sh" | "bash" => Lang::Bash,
        "lua" => Lang::Lua,
        "qml" => Lang::Qml,
        "yml" | "yaml" => Lang::Yaml,
        "sql" => Lang::Sql,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_extension() {
        assert_eq!(lang("src/main.rs"), Some(Lang::Rust));
        assert_eq!(lang("a/View.TSX"), Some(Lang::Tsx));
        assert_eq!(lang("x.d.ts"), Some(Lang::TypeScript));
        assert_eq!(lang("inc/list.h"), Some(Lang::Cpp));
        assert_eq!(lang(".github/workflows/ci.yml"), Some(Lang::Yaml));
        assert_eq!(lang("a.b/README"), None);
        assert_eq!(lang("data.csv"), None);
        assert_eq!(Lang::Rust.separator(), "::");
        assert_eq!(Lang::CSharp.to_string(), "csharp");
    }

    #[test]
    fn names_match_the_json() {
        for l in Lang::ALL {
            assert_eq!(serde_json::to_string(&l).unwrap(), format!("\"{}\"", l.name()));
        }
    }
}
