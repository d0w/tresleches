#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
tcp_base=${TCP_BASE_PORT:-4001}
udp_base=${UDP_BASE_PORT:-5001}

for port in "$tcp_base" "$udp_base"; do
    if ! [[ $port =~ ^[1-9][0-9]{0,4}$ ]] || (( port > 65533 )); then
        echo "Base ports must be integers from 1 to 65533." >&2
        exit 1
    fi
done

run_dir=$(mktemp -d "${TMPDIR:-/tmp}/tresleches-mesh.XXXXXX")
node_pids=""
log_pid=""

cleanup() {
    trap - EXIT INT TERM
    # Only stop processes started by this invocation, never other controllers.
    for pid in $node_pids $log_pid; do
        kill -TERM "$pid" 2>/dev/null || true
    done

    # Give controllers time to close; a stuck application must not orphan nodes.
    for ((attempt = 0; attempt < 30; attempt++)); do
        running=false
        for pid in $node_pids; do
            if kill -0 "$pid" 2>/dev/null; then
                running=true
            fi
        done
        if [[ $running == false ]]; then
            break
        fi
        sleep 0.1
    done

    for pid in $node_pids $log_pid; do
        if kill -0 "$pid" 2>/dev/null; then
            kill -KILL "$pid" 2>/dev/null || true
        fi
        wait "$pid" 2>/dev/null || true
    done
    rm -f "$run_dir/controller"
    echo "Stopped. Logs kept in $run_dir"
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

echo "Building Go controller..."
go -C "$repo_dir/controller" build -o "$run_dir/controller" ./cmd/controller

touch "$run_dir/seed.log" "$run_dir/node-a.log" "$run_dir/node-b.log"
echo "Starting three Go controllers. Rust routers are not started."
echo "Logs: $run_dir (tail headers identify each node). Ctrl+C stops all three."
tail -n +1 -f "$run_dir/seed.log" "$run_dir/node-a.log" "$run_dir/node-b.log" &
log_pid=$!

start_node() {
    local name=$1 tcp_port=$2 udp_port=$3
    shift 3
    "$run_dir/controller" \
        -listen "/ip4/127.0.0.1/tcp/$tcp_port" \
        -udp-advertise "127.0.0.1:$udp_port" \
        "$@" >"$run_dir/$name.log" 2>&1 &
    last_pid=$!
    node_pids="$node_pids $last_pid"
}

start_node seed "$tcp_base" "$udp_base"
seed_pid=$last_pid
seed_address=""

# Capture the full freshly printed address instead of assuming a stable peer ID.
for ((attempt = 0; attempt < 100; attempt++)); do
    if ! kill -0 "$seed_pid" 2>/dev/null; then
        cat "$run_dir/seed.log" >&2
        echo "Seed exited before announcing its address." >&2
        exit 1
    fi
    seed_address=$(awk -v prefix="/ip4/127.0.0.1/tcp/$tcp_base/p2p/" '
        /listen: / && index($0, prefix) {
            address = substr($0, index($0, prefix))
            sub(/[[:space:]"].*$/, "", address)
            print address
            exit
        }
    ' "$run_dir/seed.log")
    if [[ -n $seed_address ]]; then
        break
    fi
    sleep 0.1
done

if [[ -z $seed_address ]]; then
    cat "$run_dir/seed.log" >&2
    echo "Timed out waiting for seed address after 10 seconds." >&2
    exit 1
fi

echo "Bootstrap: $seed_address"
start_node node-a "$((tcp_base + 1))" "$((udp_base + 1))" -bootstrap "$seed_address"
start_node node-b "$((tcp_base + 2))" "$((udp_base + 2))" -bootstrap "$seed_address"

while true; do
    for pid in $node_pids; do
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "Controller $pid exited; stopping remaining nodes. See $run_dir" >&2
            exit 1
        fi
    done
    sleep 1
done
