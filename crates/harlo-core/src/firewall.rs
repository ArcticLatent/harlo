//! Is this computer's firewall letting phones reach harlo's port? Linux
//! desktops often run ufw or firewalld, which drop incoming connections
//! unless told otherwise; then a phone on the same network can't open the
//! pairing link. This finds the firewall, says whether the port is open to
//! the phone's network (when that can be read without root), and makes the
//! command that opens it, for harlo to run through pkexec (the system's own
//! password dialog).

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ufw,
    Firewalld,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Ufw => "ufw",
            Kind::Firewalld => "firewalld",
        }
    }
}

/// Where phones would come from: an address of this computer and its
/// network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    pub address: Ipv4Addr,
    pub prefix: u8,
    pub interface: String,
}

impl Network {
    /// The network in CIDR form (`192.168.8.0/24`). Tailscale addresses
    /// (a /32 on its own interface) stand for all of Tailscale's range.
    pub fn cidr(&self) -> String {
        let bits = self.bits();
        let mask = if bits == 0 { 0 } else { u32::MAX << (32 - bits) };
        format!("{}/{bits}", Ipv4Addr::from(u32::from(self.address) & mask))
    }

    fn bits(&self) -> u32 {
        if self.prefix >= 31 && is_tailscale(self.address) { 10 } else { u32::from(self.prefix.min(32)) }
    }
}

pub fn is_tailscale(a: Ipv4Addr) -> bool {
    let [x, y, ..] = a.octets();
    x == 100 && (64..128).contains(&y)
}

/// Where the firewall's files are: `/etc`, or a stand-in for tests
/// (`HARLO_TEST_FIREWALL_ETC`).
fn etc() -> PathBuf {
    std::env::var_os("HARLO_TEST_FIREWALL_ETC").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/etc"))
}

fn testing() -> bool {
    std::env::var_os("HARLO_TEST_FIREWALL_ETC").is_some()
}

/// The firewall that's on, if any.
pub fn detect() -> Option<Kind> {
    let ufw = std::fs::read_to_string(etc().join("ufw/ufw.conf")).unwrap_or_default();
    if ufw.lines().any(|l| l.trim().replace('"', "").eq_ignore_ascii_case("ENABLED=yes")) {
        return Some(Kind::Ufw);
    }
    if !testing() && Command::new("systemctl").args(["is-active", "--quiet", "firewalld"]).status().is_ok_and(|s| s.success()) {
        return Some(Kind::Firewalld);
    }
    None
}

/// Whether `kind` lets `port` in from `network`: None when it can't be
/// told without root.
pub fn allows(kind: Kind, network: &Network, port: u16) -> Option<bool> {
    match kind {
        Kind::Ufw => {
            let rules = std::fs::read_to_string(etc().join("ufw/user.rules")).ok()?;
            let defaults = std::fs::read_to_string(etc().join("default/ufw")).unwrap_or_default();
            Some(ufw_allows(&rules, &defaults, network, port))
        }
        Kind::Firewalld => {
            let out = Command::new("firewall-cmd").arg(format!("--query-port={port}/tcp")).output().ok()?;
            match String::from_utf8_lossy(&out.stdout).trim() {
                "yes" => Some(true),
                "no" => Some(false),
                _ => None,
            }
        }
    }
}

/// ufw's saved rules (`user.rules`, one `### tuple ###` comment per rule:
/// action, protocol, port, destination, source port, source, direction
/// [_interface]) and its defaults: is `port` open to `network`?
pub fn ufw_allows(rules: &str, defaults: &str, network: &Network, port: u16) -> bool {
    let policy = defaults.lines().find_map(|l| l.trim().strip_prefix("DEFAULT_INPUT_POLICY=")).map(|p| p.trim_matches('"').to_ascii_uppercase());
    if policy.as_deref() == Some("ACCEPT") {
        return true;
    }
    rules.lines().filter_map(|l| l.trim().strip_prefix("### tuple ###")).any(|tuple| {
        let f: Vec<&str> = tuple.split_whitespace().collect();
        let [action, proto, dport, _dst, _sport, src, direction, ..] = f[..] else { return false };
        let action_ok = action == "allow" || action == "limit";
        let proto_ok = proto == "tcp" || proto == "any";
        let port_ok = dport == "any" || dport.split(',').any(|p| match p.split_once(':') {
            Some((lo, hi)) => lo.parse::<u16>().is_ok_and(|lo| lo <= port) && hi.parse::<u16>().is_ok_and(|hi| port <= hi),
            None => p.parse::<u16>() == Ok(port),
        });
        let direction_ok = match direction.strip_prefix("in") {
            Some("") => true,
            Some(iface) => iface.strip_prefix('_') == Some(network.interface.as_str()),
            None => false,
        };
        action_ok && proto_ok && port_ok && direction_ok && source_covers(src, network)
    })
}

/// A rule's source (`0.0.0.0/0`, `192.168.8.0/24`, `10.0.0.5`) covers the
/// phone's network (taken as containing this computer's address).
fn source_covers(src: &str, network: &Network) -> bool {
    if src == "0.0.0.0/0" || src == "any" {
        return true;
    }
    let (base, bits) = src.split_once('/').unwrap_or((src, "32"));
    let (Ok(base), Ok(bits)) = (base.parse::<Ipv4Addr>(), bits.parse::<u32>()) else { return false };
    if bits > 32 {
        return false;
    }
    let mask = if bits == 0 { 0 } else { u32::MAX << (32 - bits) };
    // The rule must take in the whole network, not only this computer.
    bits <= network.bits() && u32::from(network.address) & mask == u32::from(base) & mask
}

fn program(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file())
}

/// What opens `port` to `network`, to run as root: the program and its
/// arguments.
pub fn allow_argv(kind: Kind, network: &Network, port: u16) -> Vec<String> {
    match kind {
        Kind::Ufw => {
            let ufw = program("ufw").unwrap_or_else(|| PathBuf::from("/usr/bin/ufw"));
            [ufw.to_string_lossy().as_ref(), "allow", "from", &network.cidr(), "to", "any", "port", &port.to_string(), "proto", "tcp"].map(str::to_owned).to_vec()
        }
        // Now and after a restart. (The port alone, in the default zone:
        // firewall-cmd can then say it's open.)
        Kind::Firewalld => {
            let cmd = format!("firewall-cmd --add-port={port}/tcp && firewall-cmd --permanent --add-port={port}/tcp");
            ["/bin/sh".to_owned(), "-c".to_owned(), cmd]
        }
        .to_vec(),
    }
}

/// The same, to type in a terminal.
pub fn manual_command(kind: Kind, network: &Network, port: u16) -> String {
    match kind {
        Kind::Ufw => format!("sudo ufw allow from {} to any port {port} proto tcp", network.cidr()),
        Kind::Firewalld => format!("sudo firewall-cmd --add-port={port}/tcp && sudo firewall-cmd --permanent --add-port={port}/tcp"),
    }
}

/// pkexec, when there is one.
pub fn pkexec() -> Option<PathBuf> {
    program("pkexec")
}

/// Runs `allow_argv` through pkexec (the system asks for the password).
/// Blocks until it's done; Err says why it didn't.
pub fn allow(kind: Kind, network: &Network, port: u16) -> Result<(), String> {
    let pkexec = pkexec().ok_or("pkexec isn't installed")?;
    let out = Command::new(pkexec).args(allow_argv(kind, network, port)).output().map_err(|e| format!("couldn't run pkexec: {e}"))?;
    match out.status.code() {
        Some(0) => Ok(()),
        // pkexec: the password dialog was dismissed, or not allowed.
        Some(126) | Some(127) => Err("Not allowed: the password wasn't given.".to_owned()),
        _ => {
            let text = String::from_utf8_lossy(&out.stderr);
            let text = text.trim();
            Err(if text.is_empty() { format!("{} failed.", kind.name()) } else { text.lines().last().unwrap_or(text).to_owned() })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lan() -> Network {
        Network { address: "192.168.8.226".parse().unwrap(), prefix: 24, interface: "enp10s0".into() }
    }

    #[test]
    fn networks() {
        assert_eq!(lan().cidr(), "192.168.8.0/24");
        let ts = Network { address: "100.101.5.9".parse().unwrap(), prefix: 32, interface: "tailscale0".into() };
        assert_eq!(ts.cidr(), "100.64.0.0/10");
    }

    #[test]
    fn reads_ufw_rules() {
        let drop = "DEFAULT_INPUT_POLICY=\"DROP\"\n";
        let ours = "### tuple ### allow tcp 7676 0.0.0.0/0 any 192.168.8.0/24 in\n-A ufw-user-input -p tcp --dport 7676 -s 192.168.8.0/24 -j ACCEPT\n";
        assert!(ufw_allows(ours, drop, &lan(), 7676));
        assert!(!ufw_allows(ours, drop, &lan(), 7677), "another port");
        assert!(!ufw_allows("", drop, &lan(), 7676), "no rules");
        assert!(ufw_allows("", "DEFAULT_INPUT_POLICY=\"ACCEPT\"", &lan(), 7676), "lets everything in");
        assert!(ufw_allows("### tuple ### allow any 7000:8000 0.0.0.0/0 any 0.0.0.0/0 in\n", drop, &lan(), 7676), "a range from anywhere");
        assert!(ufw_allows("### tuple ### allow tcp 22,7676 0.0.0.0/0 any 0.0.0.0/0 in_enp10s0\n", drop, &lan(), 7676), "a list, on our interface");
        assert!(!ufw_allows("### tuple ### allow tcp 7676 0.0.0.0/0 any 0.0.0.0/0 in_wlan0\n", drop, &lan(), 7676), "another interface");
        assert!(!ufw_allows("### tuple ### deny tcp 7676 0.0.0.0/0 any 0.0.0.0/0 in\n", drop, &lan(), 7676), "deny");
        assert!(!ufw_allows("### tuple ### allow tcp 7676 0.0.0.0/0 any 10.0.0.0/8 in\n", drop, &lan(), 7676), "another network");
        assert!(!ufw_allows("### tuple ### allow tcp 7676 0.0.0.0/0 any 192.168.8.226 in\n", drop, &lan(), 7676), "only this computer");
        assert!(ufw_allows("### tuple ### allow tcp 7676 0.0.0.0/0 any 192.168.0.0/16 in\n", drop, &lan(), 7676), "a wider network");
    }

    #[test]
    fn commands() {
        assert_eq!(manual_command(Kind::Ufw, &lan(), 7676), "sudo ufw allow from 192.168.8.0/24 to any port 7676 proto tcp");
        let argv = allow_argv(Kind::Ufw, &lan(), 7676);
        assert!(argv[0].ends_with("ufw"));
        assert_eq!(argv[1..].join(" "), "allow from 192.168.8.0/24 to any port 7676 proto tcp");
    }
}

