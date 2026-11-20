#!/usr/bin/env bash
set -euo pipefail
umask 077

run_dir=${RUN_DIR:-/run/tresleches}
bootstrap_dir=${BOOTSTRAP_DIR:-/bootstrap}
node_ip=${NODE_IP:-$(getent ahostsv4 "$HOSTNAME" | awk 'NR == 1 {print $1}')}
tcp_port=${TCP_PORT:-4001}
udp_port=${UDP_PORT:-5001}
mode=${NODE_MODE:-legacy}
socket="$run_dir/router.sock"
bootstrap_file="$bootstrap_dir/seed.addr"
controller_log="$run_dir/controller.log"
children=()
components=()

cleanup() {
  trap - EXIT INT TERM
  for pid in "${children[@]}"; do
    kill -TERM "$pid" 2>/dev/null || true
  done
  for pid in "${children[@]}"; do
    wait "$pid" 2>/dev/null || true
  done
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -p "$run_dir"
if [[ ${SEED_NODE:-0} == 1 ]]; then
  mkdir -p "$bootstrap_dir"
  rm -f "$bootstrap_file"
fi

args=(
  -listen "/ip4/$node_ip/tcp/$tcp_port"
  -udp-advertise "$node_ip:$udp_port"
)

case "$mode" in
legacy)
  # Private-address UDP is an explicit opt-in for this isolated dev mesh.
  routerd "$node_ip:$udp_port" "$socket" --allow-private-udp &
  router_pid=$!
  children+=("$router_pid")
  components+=("$router_pid")

  ready=false
  for ((attempt = 0; attempt < 100; attempt++)); do
    if ! kill -0 "$router_pid" 2>/dev/null; then
      echo "Router exited during startup." >&2
      exit 1
    fi
    if routerctl -socket "$socket" get-status >/dev/null 2>&1; then
      ready=true
      break
    fi
    sleep 0.1
  done
  if [[ $ready != true ]]; then
    echo "Timed out waiting for router IPC." >&2
    exit 1
  fi
  args+=(-router-socket "$socket")
  ;;
tun)
  : "${TUN_ADDRESS:?Set the local TUN address, including prefix length}"
  : "${TUN_SUBNET:?Set the shared overlay subnet, including prefix length}"
  tun_name=${TUN_NAME:-tl0}
  args+=(-tun-name "$tun_name")
  ;;
*)
  echo "Unknown NODE_MODE: $mode" >&2
  exit 1
  ;;
esac

if [[ ${SEED_NODE:-0} != 1 ]]; then
  if [[ ! -s $bootstrap_file ]]; then
    echo "Seed address missing; start the healthy seed first." >&2
    exit 1
  fi
  args+=(-bootstrap "$(cat "$bootstrap_file")")
fi

controller "${args[@]}" >"$controller_log" 2>&1 &
controller_pid=$!
children+=("$controller_pid")
components+=("$controller_pid")
tail -n +1 -f "$controller_log" &
children+=("$!")

if [[ $mode == tun ]]; then
  ready=false
  for ((attempt = 0; attempt < 100; attempt++)); do
    if ! kill -0 "$controller_pid" 2>/dev/null; then
      cat "$controller_log" >&2
      echo "Controller exited before creating TUN." >&2
      exit 1
    fi
    if ip link show dev "$tun_name" >/dev/null 2>&1; then
      ready=true
      break
    fi
    sleep 0.1
  done
  if [[ $ready != true ]]; then
    echo "Timed out waiting for TUN $tun_name." >&2
    exit 1
  fi

  ip address add "$TUN_ADDRESS" dev "$tun_name"
  ip link set dev "$tun_name" mtu "${TUN_MTU:-1400}" up
  # Steer overlay traffic into Go without enumerating remote peers.
  ip route add "$TUN_SUBNET" dev "$tun_name" src "${TUN_ADDRESS%/*}"
  echo "TUN $tun_name configured at $TUN_ADDRESS (packet observation only)."
fi

if [[ ${SEED_NODE:-0} == 1 ]]; then
  for ((attempt = 0; attempt < 100; attempt++)); do
    if ! kill -0 "$controller_pid" 2>/dev/null; then
      cat "$controller_log" >&2
      echo "Seed controller exited during startup." >&2
      exit 1
    fi
    address=$(awk -v prefix="/ip4/$node_ip/tcp/$tcp_port/p2p/" '
            /listen: / && index($0, prefix) {
                address = substr($0, index($0, prefix))
                sub(/[[:space:]"].*$/, "", address)
                print address
                exit
            }
        ' "$controller_log")
    if [[ -n $address ]]; then
      printf '%s\n' "$address" >"$bootstrap_file.tmp"
      mv "$bootstrap_file.tmp" "$bootstrap_file"
      break
    fi
    sleep 0.1
  done
  if [[ ! -s $bootstrap_file ]]; then
    echo "Timed out waiting for seed address." >&2
    exit 1
  fi
fi

# If a node component exits, clean up the remaining children.
status=0
wait -n "${components[@]}" || status=$?
echo "Node component exited (status $status); stopping node." >&2
if ((status == 0)); then
  status=1
fi
exit "$status"
