#!/usr/bin/env bash
# Starts a private Harlo server (target/debug/harlo-server, or HARLO_SERVER_BIN)
# and the scripted model in a scratch folder.
#
#   tools/sandbox.sh [dir]        prints KEY=VALUE lines (URL, PASSWORD, DIR, PROJECT, …)
#   tools/sandbox.sh stop dir     stops both and removes the folder
#   tools/sandbox.sh restart dir  restarts the server only, on the same port
#                                 and password (the sandbox's own server, never
#                                 the background service)
#
# Never touches the real config, state or background service: HOME and every
# XDG folder point inside the scratch folder.
# The server also gets a session bus that leads nowhere, so it never reaches the
# desktop keyring (it keeps credentials there when it can).
set -euo pipefail

TOOLS=$(cd "$(dirname "$0")" && pwd)
BIN=${HARLO_SERVER_BIN:-$TOOLS/../target/debug/harlo-server}
if [[ ${1:-} != stop && ! -x $BIN ]]; then
  echo "no harlo-server at $BIN: cargo build -p harlo-server" >&2
  exit 1
fi

if [[ ${1:-} == restart ]]; then
  dir=$2
  pid=$(cat "$dir/server.pid")
  kill "$pid" 2>/dev/null || true
  for _ in $(seq 1 100); do kill -0 "$pid" 2>/dev/null || break; sleep 0.05; done
  kill -9 "$pid" 2>/dev/null || true
  bash "$dir/server.sh"
  # shellcheck disable=SC1091
  source "$dir/server.env"
  for _ in $(seq 1 100); do
    if curl -s -o /dev/null -u "harlo:$password" "http://127.0.0.1:$port/api/info"; then break; fi
    sleep 0.1
  done
  echo "restarted"
  exit 0
fi

if [[ ${1:-} == stop ]]; then
  dir=$2
  for f in "$dir"/server.pid "$dir"/model.pid; do
    [[ -f $f ]] && kill "$(cat "$f")" 2>/dev/null || true
  done
  sleep 0.3
  [[ $dir == /tmp/harlo-sandbox.* ]] && rm -rf "$dir"
  exit 0
fi

dir=${1:-$(mktemp -d /tmp/harlo-sandbox.XXXXXX)}
mkdir -p "$dir"/{home,config/harlo,data,state,cache,project}
free_port() { python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }
model_port=$(free_port)
port=$(free_port)
password=$(python3 -I -c 'import secrets; print(secrets.token_hex(16))')

# HARLO_SANDBOX_TODO=1 turns on the todo tool (experimental.todo).
experimental=""
if [ "${HARLO_SANDBOX_TODO:-}" = 1 ]; then
  experimental='"experimental": { "todo": true },'
fi

cat > "$dir/config/harlo/harlo.json" <<EOF
{
  "model": "fake/fake-coder",
  $experimental
  "providers": {
    "fake": {
      "name": "Fake",
      "package": "@ai-sdk/openai-compatible",
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

# The server's start, kept so `restart` can run it again the same way.
# A Claude Code session's own variables stay out of the sandbox.
unset_claude=""
while IFS='=' read -r name _; do
  [[ $name == CLAUDE* ]] && unset_claude+=" -u $name"
done < <(env)
printf 'port=%q\npassword=%q\n' "$port" "$password" > "$dir/server.env"
cat > "$dir/server.sh" <<EOF
cd $(printf %q "$dir/project")
env $unset_claude HOME=$(printf %q "$dir/home") XDG_CONFIG_HOME=$(printf %q "$dir/config") \\
  XDG_DATA_HOME=$(printf %q "$dir/data") XDG_STATE_HOME=$(printf %q "$dir/state") \\
  XDG_CACHE_HOME=$(printf %q "$dir/cache") HARLO_SERVER_PASSWORD=$(printf %q "$password") \\
  HARLO_CLAUDE_PATH=$(printf %q "$dir/bin/claude") HARLO_KEY_CHECK_BASE=http://127.0.0.1:9 HARLO_RELAY=http://127.0.0.1:9 HARLO_UPDATE_MANIFEST_URL=http://127.0.0.1:9 HARLO_UPDATE_CHANGELOG_URL=http://127.0.0.1:9 \\
  DBUS_SESSION_BUS_ADDRESS=$(printf %q "unix:path=$dir/no-bus") \\
  setsid $(printf %q "$BIN") serve --hostname 127.0.0.1 --port $port >> $(printf %q "$dir/server.log") 2>&1 < /dev/null &
echo \$! > $(printf %q "$dir/server.pid")
EOF
bash "$dir/server.sh"

for _ in $(seq 1 100); do
  if curl -s -o /dev/null -u "harlo:$password" "http://127.0.0.1:$port/api/info"; then break; fi
  sleep 0.1
done

echo "DIR=$dir"
echo "URL=http://127.0.0.1:$port"
echo "PASSWORD=$password"
echo "PROJECT=$dir/project"
echo "MODEL_LOG=$dir/model.jsonl"
