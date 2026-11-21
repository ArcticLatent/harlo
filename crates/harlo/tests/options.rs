//! Command-line options: `--server`, its password, and help.

// Rust guideline compliant 2026-10-09

use harlo::options::{ArgsError, Options};
use harlo_core::server::Location;

fn parse(args: &[&str], password: Option<&str>) -> Result<Options, ArgsError> {
    Options::parse(
        args.iter().map(|a| (*a).to_owned()),
        password.map(str::to_owned),
    )
}

#[test]
fn no_arguments_follow_the_shared_service() {
    let options = parse(&[], None).expect("valid");
    assert_eq!(options.locations(), [Location::Shared]);
}

#[test]
fn server_adds_a_location_with_the_password() {
    for args in [
        &["--server", "http://127.0.0.1:4096"][..],
        &["--server=http://127.0.0.1:4096"][..],
    ] {
        let options = parse(args, Some("pw")).expect("valid");
        assert_eq!(
            options.locations(),
            [
                Location::Shared,
                Location::Server {
                    url: "http://127.0.0.1:4096".into(),
                    password: Some("pw".into()),
                },
            ]
        );
        assert!(!format!("{options:?}").contains("pw\""), "{options:?}");
    }
    let options = parse(&["--server", "http://x"], Some("")).expect("valid");
    assert_eq!(options.password, None, "an empty password counts as none");
}

#[test]
fn bad_arguments_are_explained() {
    assert_eq!(parse(&["--help"], None), Err(ArgsError::Help));
    assert_eq!(
        parse(&["--server"], None),
        Err(ArgsError::Invalid("--server needs a URL".into()))
    );
    assert_eq!(
        parse(&["--server", "--help"], None),
        Err(ArgsError::Invalid("--server needs a URL".into()))
    );
    assert_eq!(
        parse(&["--verbose"], None),
        Err(ArgsError::Invalid("unknown argument: --verbose".into()))
    );
}
