//! The hosts Settings → Server offers from an SSH config, read from a
//! scratch file, never the user's `~/.ssh`.

// Rust guideline compliant 2026-10-09

use harlo_core::ssh::config_hosts;

#[test]
fn named_hosts_are_offered_once_with_includes_and_without_patterns() {
    let home = tempfile::tempdir().expect("a scratch home");
    let ssh = home.path().join(".ssh");
    std::fs::create_dir_all(ssh.join("config.d")).expect("the folder");
    std::fs::write(
        ssh.join("config"),
        "Include config.d/*\n\
         \n\
         Host buildbox gpu-box\n    HostName 10.0.0.5\n    User me\n\
         Host *.internal !secret\n    ForwardAgent yes\n\
         Host buildbox\n    Port 2222\n\
         Host *\n    ServerAliveInterval 30\n    SomeOptionNobodyKnows yes\n",
    )
    .expect("the config");
    std::fs::write(
        ssh.join("config.d").join("work"),
        "Host nano\n    User pipeline\n",
    )
    .expect("the included file");

    let hosts = config_hosts(&ssh.join("config"));
    assert_eq!(hosts, ["nano", "buildbox", "gpu-box"]);
}

#[test]
fn a_missing_config_offers_none() {
    let home = tempfile::tempdir().expect("a scratch home");
    assert!(config_hosts(&home.path().join(".ssh").join("config")).is_empty());
}
