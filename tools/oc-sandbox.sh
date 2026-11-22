#!/usr/bin/env bash
# Starts a private OpenCode server and the scripted model in a scratch folder.
#
#   tools/oc-sandbox.sh [dir]     prints KEY=VALUE lines (URL, PASSWORD, DIR, PROJECT, …)
#   tools/oc-sandbox.sh stop dir  stops both and removes the folder
#
# Never touches the real ~/.config/opencode, ~/.local/state/opencode or the shared
# service: HOME and every XDG folder point inside the scratch folder.
set -euo pipefail

VERSION=$(cat "$(dirname "$0")/../crates/opencode-client/OPENCODE_VERSION" 2>/dev/null || echo 2.3.2)
BIN=${HARLO_OPENCODE_BIN:-$HOME/.local/share/harlo-dev/opencode/$VERSION/opencode}
TOOLS=$(cd "$(dirname "$0")" && pwd)

if [[ ${1:-} == stop ]]; then
  dir=$2
  for f in "$dir"/opencode.pid "$dir"/model.pid; do
    [[ -f $f ]] && kill "$(cat "$f")" 2>/dev/null || true
  done
  sleep 0.3
  [[ $dir == /tmp/harlo-oc.* ]] && rm -rf "$dir"
  exit 0
fi

dir=${1:-$(mktemp -d /tmp/harlo-oc.XXXXXX)}
mkdir -p "$dir"/{home,config/opencode,data,state,cache,project}
free_port() { python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }
model_port=$(free_port)
port=$(free_port)
password=$(python3 -I -c 'import secrets; print(secrets.token_hex(16))')

cat > "$dir/config/opencode/opencode.json" <<EOF
{
  "\$schema": "https://opencode.ai/config.json",
  "model": "fake/fake-coder",
  "providers": {
    "fake": {
      "name": "Fake",
      "package": "@opencode/ai/providers/openai-compatible",
      "settings": { "baseURL": "http://127.0.0.1:$model_port/v1", "apiKey": "fake-key" },
      "models": { "fake-coder": { "name": "Fake Coder", "limit": { "context": 128000, "output": 8192 } } }
    }
  },
  "permissions": [
    { "action": "shell", "resource": "*", "effect": "ask" }
  ]
}
EOF

# A git project for sessions to work in.
(cd "$dir/project" && git init -q && printf 'alpha\n' > a.txt && printf 'beta\n' > b.txt \
  && git add . && git -c user.name=t -c user.email=t@t commit -qm init)

setsid python3 -I "$TOOLS/fake-model.py" "$model_port" "$dir/model.jsonl" > "$dir/model.log" 2>&1 < /dev/null &
echo $! > "$dir/model.pid"

cd "$dir/project"
env HOME="$dir/home" XDG_CONFIG_HOME="$dir/config" XDG_DATA_HOME="$dir/data" \
  XDG_STATE_HOME="$dir/state" XDG_CACHE_HOME="$dir/cache" OPENCODE_PASSWORD="$password" \
  setsid "$BIN" serve --hostname 127.0.0.1 --port "$port" > "$dir/opencode.log" 2>&1 < /dev/null &
echo $! > "$dir/opencode.pid"
cd - > /dev/null

for _ in $(seq 1 100); do
  if curl -s -o /dev/null -u "opencode:$password" "http://127.0.0.1:$port/api/info"; then break; fi
  sleep 0.1
done

echo "DIR=$dir"
echo "URL=http://127.0.0.1:$port"
echo "PASSWORD=$password"
echo "PROJECT=$dir/project"
echo "MODEL_LOG=$dir/model.jsonl"
