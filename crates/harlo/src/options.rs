//! Command-line options.

// Rust guideline compliant 2026-10-09

use harlo_core::server::Location;

/// The environment variable holding the password for `--server`.
pub const PASSWORD_VAR: &str = "HARLO_OPENCODE_PASSWORD";

/// What Harlo was started with.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Options {
    /// A server to follow instead of the shared service, as `--server <url>`.
    pub server: Option<String>,
    /// The password for `server`, from [`PASSWORD_VAR`].
    pub password: Option<String>,
    /// Open the setup guide, as `--setup`.
    pub setup: bool,
}

impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Options")
            .field("server", &self.server)
            .field("password", &self.password.as_ref().map(|_| "…"))
            .field("setup", &self.setup)
            .finish()
    }
}

/// Usage text for `--help` and argument errors.
pub const USAGE: &str = "\
Usage: harlo [--server <url>] [--setup]

  --server <url>  Follow the OpenCode server at <url> instead of the shared
                  background service. Its password is read from
                  HARLO_OPENCODE_PASSWORD.
  --setup         Open the setup guide: OpenCode, models, git and the theme.
                  A Harlo already running opens it in its window.
  -h, --help      Show this help.";

/// Why the arguments couldn't be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgsError {
    /// `--help` was asked for.
    Help,
    /// The arguments are wrong; the message says how.
    Invalid(String),
}

impl Options {
    /// Reads the options from arguments (without the program name) and a password.
    ///
    /// # Errors
    ///
    /// Returns [`ArgsError::Help`] for `--help`, and [`ArgsError::Invalid`]
    /// for unknown arguments or a `--server` without a URL.
    pub fn parse(
        args: impl IntoIterator<Item = String>,
        password: Option<String>,
    ) -> Result<Self, ArgsError> {
        let mut options = Self {
            server: None,
            password: password.filter(|p| !p.is_empty()),
            setup: false,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => return Err(ArgsError::Help),
                "--setup" => options.setup = true,
                "--server" => {
                    let url = args
                        .next()
                        .filter(|u| !u.starts_with('-'))
                        .ok_or_else(|| ArgsError::Invalid("--server needs a URL".to_owned()))?;
                    options.server = Some(url);
                }
                other => {
                    if let Some(url) = other.strip_prefix("--server=") {
                        options.server = Some(url.to_owned());
                    } else {
                        return Err(ArgsError::Invalid(format!("unknown argument: {other}")));
                    }
                }
            }
        }
        Ok(options)
    }

    /// The servers the location picker offers; the last one is followed at start.
    #[must_use]
    pub fn locations(&self) -> Vec<Location> {
        let mut locations = vec![Location::Shared];
        if let Some(url) = &self.server {
            locations.push(Location::Server {
                url: url.clone(),
                password: self.password.clone(),
            });
        }
        locations
    }
}
