//! Well-known MCP servers Settings offers to add with one click. Each entry
//! is how its maker's own documentation says to connect; adding one writes
//! it to OpenCode's global config, as any other server.

// Rust guideline compliant 2026-10-09

use crate::config::{McpEntry, McpRun};

/// How an offered server is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// A remote server at this address.
    Remote(&'static str),
    /// A program run on the server's computer, with its arguments.
    Local(&'static [&'static str]),
}

/// An MCP server offered in the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offered {
    /// The name it's added under, which OpenCode puts before its tools.
    pub name: &'static str,
    /// Its name for people.
    pub title: &'static str,
    /// What it gives agents, in a line.
    pub about: &'static str,
    /// How it's reached.
    pub reach: Reach,
    /// Whether it asks to sign in in the browser the first time.
    pub signs_in: bool,
}

impl Offered {
    /// The config entry that adds it.
    #[must_use]
    pub fn entry(&self) -> McpEntry {
        let run = match self.reach {
            Reach::Remote(url) => McpRun::Remote {
                url: url.to_owned(),
                headers: Vec::new(),
            },
            Reach::Local(command) => McpRun::Local {
                command: command.iter().map(|&part| part.to_owned()).collect(),
                environment: Vec::new(),
            },
        };
        McpEntry {
            name: self.name.to_owned(),
            run,
            disabled: false,
            codemode: true,
        }
    }
}

/// Every server the catalog offers, in the order shown.
pub const OFFERED: [Offered; 10] = [
    Offered {
        name: "context7",
        title: "Context7",
        about: "Up-to-date documentation and code examples for libraries.",
        reach: Reach::Remote("https://mcp.context7.com/mcp"),
        signs_in: true,
    },
    Offered {
        name: "deepwiki",
        title: "DeepWiki",
        about: "Answers questions about public GitHub repositories.",
        reach: Reach::Remote("https://mcp.deepwiki.com/mcp"),
        signs_in: false,
    },
    Offered {
        name: "github",
        title: "GitHub",
        about: "Issues, pull requests, code and workflows on GitHub.",
        reach: Reach::Remote("https://api.githubcopilot.com/mcp/"),
        signs_in: true,
    },
    Offered {
        name: "linear",
        title: "Linear",
        about: "Linear's issues, projects and comments.",
        reach: Reach::Remote("https://mcp.linear.app/mcp"),
        signs_in: true,
    },
    Offered {
        name: "notion",
        title: "Notion",
        about: "Search, read and edit Notion pages.",
        reach: Reach::Remote("https://mcp.notion.com/mcp"),
        signs_in: true,
    },
    Offered {
        name: "sentry",
        title: "Sentry",
        about: "Sentry's errors and issues, to look into what broke.",
        reach: Reach::Remote("https://mcp.sentry.dev/mcp"),
        signs_in: true,
    },
    Offered {
        name: "cloudflare-docs",
        title: "Cloudflare docs",
        about: "Searches Cloudflare's documentation.",
        reach: Reach::Remote("https://docs.mcp.cloudflare.com/mcp"),
        signs_in: false,
    },
    Offered {
        name: "huggingface",
        title: "Hugging Face",
        about: "Models, datasets, papers and Spaces on Hugging Face.",
        reach: Reach::Remote("https://huggingface.co/mcp"),
        signs_in: false,
    },
    Offered {
        name: "playwright",
        title: "Playwright",
        about: "Drives a browser: opens pages, clicks, types and reads them. Needs Node.",
        reach: Reach::Local(&["npx", "@playwright/mcp@latest"]),
        signs_in: false,
    },
    Offered {
        name: "chrome-devtools",
        title: "Chrome DevTools",
        about: "Debugs pages in Chrome: console, network and performance. Needs Node and Chrome.",
        reach: Reach::Local(&[
            "npx",
            "-y",
            "chrome-devtools-mcp@latest",
            "--no-usage-statistics",
        ]),
        signs_in: false,
    },
];
