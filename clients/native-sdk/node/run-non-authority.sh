#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'ERA_REVIEW_NODE_STATUS=BLOCKED_%s\n' "$1" >&2
  exit 1
}

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
surface_root=$(CDPATH= cd -- "$script_dir/.." && pwd -P)

node_binary=${ERA_REVIEW_NODE_BINARY:-}
nodejs_binary=${ERA_REVIEW_NODEJS_BINARY:-node}
chain_spec=${ERA_REVIEW_CHAIN_SPEC:-}
base_path=${ERA_REVIEW_BASE_PATH:-}
node_name=${ERA_REVIEW_NODE_NAME:-era-v14-review-node}
rpc_port=${ERA_REVIEW_RPC_PORT:-9944}
p2p_port=${ERA_REVIEW_P2P_PORT:-30333}
dapp_origin=${ERA_REVIEW_DAPP_ORIGIN:-http://127.0.0.1:8080}

[ -n "$node_binary" ] || fail MISSING_NODE_BINARY
[ -n "$chain_spec" ] || fail MISSING_CHAIN_SPEC
[ -n "$base_path" ] || fail MISSING_BASE_PATH
[ -x "$node_binary" ] || fail NODE_BINARY_NOT_EXECUTABLE
[ -f "$chain_spec" ] || fail CHAIN_SPEC_NOT_FILE

case "$base_path" in
  / | . | ..) fail UNSAFE_BASE_PATH ;;
esac
if [ -e "$base_path" ]; then
  [ -d "$base_path" ] || fail BASE_PATH_NOT_DIRECTORY
  [ -z "$(find "$base_path" -mindepth 1 -maxdepth 1 -print -quit)" ] || fail BASE_PATH_NOT_EMPTY
fi

case "$node_name" in
  '' | *[!A-Za-z0-9._-]*) fail INVALID_NODE_NAME ;;
esac

validate_port() {
  case "$1" in
    '' | *[!0-9]*) fail INVALID_PORT ;;
  esac
  [ "$1" -ge 1 ] && [ "$1" -le 65535 ] || fail INVALID_PORT
}
validate_port "$rpc_port"
validate_port "$p2p_port"
[ "$rpc_port" -ne "$p2p_port" ] || fail PORT_COLLISION

case "$dapp_origin" in
  http://127.0.0.1:* | http://localhost:*) ;;
  *) fail DAPP_ORIGIN_NOT_LOOPBACK ;;
esac

command -v "$nodejs_binary" >/dev/null 2>&1 || fail NODEJS_NOT_AVAILABLE
"$nodejs_binary" "$surface_root/tools/validate-chain-spec.mjs" "$chain_spec"

umask 077
exec "$node_binary" \
  --chain "$chain_spec" \
  --base-path "$base_path" \
  --name "$node_name" \
  --listen-addr "/ip4/127.0.0.1/tcp/$p2p_port" \
  --rpc-port "$rpc_port" \
  --rpc-methods safe \
  --rpc-cors "$dapp_origin" \
  --no-mdns \
  --no-telemetry \
  --no-prometheus
