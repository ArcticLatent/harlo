#!/usr/bin/env bash
# Installs or updates harlo-relay on a server (Ubuntu or Debian, systemd)
# over SSH: a static build of the relay, a systemd service (its own
# throwaway user, read-only system, restarts by itself), and, given one,
# the app's APNs key for notifications (readable by the service alone).
# The relay listens on 127.0.0.1:8080 only: HTTPS comes from a Cloudflare
# Tunnel on the server (cloudflared, set up once from Cloudflare's
# dashboard), so the firewall lets in SSH alone.
#
#   tools/deploy-relay.sh root@203.0.113.5                       update the relay
#   tools/deploy-relay.sh root@203.0.113.5 AuthKey_ABC.p8 TEAMID  with the APNs key
#
# SSH_KEY=~/.ssh/id_ed25519_harlo_relay picks the SSH key. Needs clang and
# Rust's x86_64-unknown-linux-musl target (rustup target add …).
set -euo pipefail
cd "$(dirname "$0")/.."
host=${1:?usage: tools/deploy-relay.sh user@server [AuthKey_ID.p8 TEAM_ID]}
key=${2:-}
team=${3:-}
ssh_opts=(-o StrictHostKeyChecking=accept-new)
[ -n "${SSH_KEY:-}" ] && ssh_opts+=(-i "$SSH_KEY" -o IdentitiesOnly=yes)
if [ -n "$key" ]; then
    [ -n "$team" ] || { echo "the Team ID goes after the key"; exit 2; }
    name=$(basename "$key")
    [[ "$name" =~ ^AuthKey_[A-Z0-9]{10}\.p8$ ]] || { echo "the key's file should be named as Apple names it: AuthKey_<Key ID>.p8"; exit 2; }
    [[ "$team" =~ ^[A-Z0-9]{10}$ ]] || { echo "a Team ID is 10 letters and digits"; exit 2; }
fi

# Static (musl): runs on any x86-64 Linux, whatever its C library.
CC_x86_64_unknown_linux_musl=clang cargo build --release -p harlo-relay --target x86_64-unknown-linux-musl
bin=target/x86_64-unknown-linux-musl/release/harlo-relay

scp "${ssh_opts[@]}" "$bin" "$host:/tmp/harlo-relay.new"
[ -n "$key" ] && scp "${ssh_opts[@]}" "$key" "$host:/tmp/$name"

ssh "${ssh_opts[@]}" "$host" "KEY_NAME='${name:-}' TEAM='$team' bash -s" <<'REMOTE'
set -euo pipefail
sudo=""; [ "$(id -u)" = 0 ] || sudo=sudo
$sudo install -m 755 /tmp/harlo-relay.new /usr/local/bin/harlo-relay
rm -f /tmp/harlo-relay.new
$sudo mkdir -p /etc/harlo-relay /etc/systemd/system/harlo-relay.service.d
$sudo chmod 700 /etc/harlo-relay

# The APNs key: root's file, handed to the service alone (systemd's
# credentials), never readable by its user otherwise.
if [ -n "$KEY_NAME" ]; then
    $sudo rm -f /etc/harlo-relay/AuthKey_*.p8
    $sudo install -m 600 "/tmp/$KEY_NAME" "/etc/harlo-relay/$KEY_NAME"
    rm -f "/tmp/$KEY_NAME"
    $sudo tee /etc/systemd/system/harlo-relay.service.d/apns.conf > /dev/null <<EOF
[Service]
LoadCredential=$KEY_NAME:/etc/harlo-relay/$KEY_NAME
Environment=HARLO_RELAY_APNS_KEY=/run/credentials/harlo-relay.service/$KEY_NAME
Environment=HARLO_RELAY_APNS_TEAM=$TEAM
EOF
fi

$sudo tee /etc/systemd/system/harlo-relay.service > /dev/null <<'EOF'
[Unit]
Description=harlo relay (phones reach harlo from anywhere; the iOS app's notifications)
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/harlo-relay
# Behind the Cloudflare Tunnel only: it says who's connecting.
Environment=HARLO_RELAY_LISTEN=127.0.0.1:8080
Environment=HARLO_RELAY_TRUST_FORWARDED=1
Restart=always
RestartSec=2
DynamicUser=yes
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
PrivateDevices=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
RestrictAddressFamilies=AF_INET AF_INET6
MemoryMax=300M
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
EOF
$sudo systemctl daemon-reload
$sudo systemctl enable --quiet harlo-relay
$sudo systemctl restart harlo-relay

# The firewall: SSH only (the tunnel goes out, nothing comes in for it).
if command -v ufw > /dev/null; then
    $sudo ufw allow OpenSSH > /dev/null
    $sudo ufw --force enable > /dev/null
fi

sleep 1
systemctl is-active --quiet harlo-relay && echo "harlo-relay: running"
curl -fsS http://127.0.0.1:8080/health > /dev/null && echo "harlo-relay: answers on 127.0.0.1:8080"
echo "harlo-relay: notifications $(curl -fsS http://127.0.0.1:8080/push)"
journalctl -u harlo-relay -n 3 --no-pager -o cat
REMOTE
