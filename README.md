# Tres Leches

Implementation of mesh routers, load-balancers, and firewalls because why not. The goal is decentralized layer-4 networking: forwarding traffic through peers, balancing flows, and applying traffic policy. This does not currently distribute a single TCP connection across multiple peers.

This is primarily an experimentation and learning project, not a production networking stack.

## Architecture (evolving)

- **One Go node daemon (planned consolidation):** retain libp2p discovery and authenticated metadata exchange; own route/path selection, configuration, and userspace packet processing where useful, including experimental buffering/reassembly.
- **Rust eBPF programs (planned):** perform selected kernel-side classification, scheduling, encapsulation/redirection, and observation. Go loads programs and updates BPF maps; a second Rust userspace daemon is not required.
- **Selection boundary:** Go keeps the known-peer directory but installs only the forwarding state needed for selected paths/backends. Discovery alone does not select a next hop. eBPF is a tool for specific operations, not a requirement to implement the entire transport in kernel.

This is the current direction but its subject to change. Both implementations, packet formats, identity representation, and responsibility boundaries are open to change.

## Current status

The existing two-process prototype has Go discovery under `tresleches/mesh/v1`, authenticated peer-info exchange, and neighbor synchronization over Unix IPC. Rust's Tokio-based `routerd` has configuration/status and UDP probe code, but is not connected to the separate static-route payload-forwarding demo. Probe responses were reported missing; working reachability is not established. The single-Go-daemon/Rust-eBPF architecture below is planned, not implemented. Preserve the Rust userspace prototype as reference; `one_shot_test/` is another separate content-transfer experiment.

## Roadmap

Research goal: explore multipath delivery of a single TCP/UDP flow, then splitting individual packets into custom fragments. This is an experimental overlay carried over existing IP networks, not an already implemented replacement for the Internet.

1. **Consolidate userspace into Go.** Separate discovery, selected forwarding configuration, and packet processing into packages within one daemon. Reconcile only selected state, including removal when no longer needed; do not copy the entire peer directory into the forwarding plane.
2. **Build a working Linux overlay.** Learn namespaces, veth pairs, routing, and TUN interfaces. Carry application IP packets over UDP between Go nodes, first directly and then through a relay. Keep the existing kernel TCP/UDP stack rather than reimplementing it.
3. **Introduce Rust eBPF incrementally.** Start with counters at a suitable hook such as TC, then add bounded classification, path scheduling, and forwarding operations. Keep complex timers, queues, and experimental slow paths in Go; userspace payload processing is allowed.
4. **Stripe intact packets from one flow across paths.** Explore `A → B → D` and `A → C → D`, with B/C acting as relays and D remaining the transport endpoint. Compare scheduling policies, loss, latency, reordering, and throughput; multiple paths do not guarantee greater bandwidth.
5. **Explore custom packet fragmentation/reassembly.** Add packet IDs, fragment offsets/lengths, duplicate handling, bounded buffers, and expiry/missing-fragment policy. Initially reconstruct packets in Go before delivering them to the kernel; preserve UDP datagram boundaries. Replacing TCP's reliability or congestion control is a separate transport-research milestone, not something forwarding alone provides.
6. **Add service/backend load balancing and policy.** Preserve flow affinity when selecting independent application backends; distinguish this from per-packet striping across relay paths to the same endpoint. Address state expiry, return paths, filtering, and rate limits. Deep application-payload inspection is not an initial goal.
7. **Secure before untrusted deployment.** Keep unauthenticated experiments in an isolated local/private lab. Use an established authenticated-encryption protocol/implementation with replay protection before wider exposure. Libp2p control security does not protect separate UDP traffic; hooks before decryption cannot inspect encrypted inner headers.

Account for MTU, checksums, and Linux GSO/GRO/TSO offloads: a hook-visible buffer is not necessarily one wire packet. Keep discovery, authenticated metadata, selected/installed state, and observed reachability distinct. Use [MPTCP / RFC 8684](https://www.rfc-editor.org/rfc/rfc8684.html) as a design reference, not a requirement to adopt it; see also [Linux segmentation offloads](https://docs.kernel.org/networking/segmentation-offloads.html).

## Development

Run from the repository root, with Go and Rust toolchains matching their manifests:

```sh
go -C controller test ./...
cargo test --workspace
```

Start three Go controllers with automatic seed-address discovery:

```sh
make dev-mesh
```

Uses TCP ports 4001–4003 and advertises UDP endpoints 5001–5003; Rust routers are not started. Ctrl+C stops all three controllers, with a three-second shutdown grace period before force-stopping any stuck child. Per-node logs are kept in the printed temporary directory. Override ports with `TCP_BASE_PORT=6001 UDP_BASE_PORT=7001 make dev-mesh`.

### Three-node TUN mesh in Docker

```sh
make dev-mesh-tun

# In another terminal, inspect the route and generate packets on node-a:
docker compose -f docker/compose.tun.yaml exec node-a ip route get 10.203.0.3
docker compose -f docker/compose.tun.yaml exec node-a ping -c 3 -W 1 10.203.0.3

# After stopping the attached launcher with Ctrl+C:
make dev-mesh-tun-down
```

Starts three **Go-only** nodes with automatic bootstrap discovery and a `tl0` TUN interface in each container. The Docker build targets Go only; Rust `routerd` and IPC are not used. The existing `make dev-mesh` remains the local discovery-only launcher.

| Service | TUN address |
| --- | --- |
| `seed` | `10.203.0.1/32` |
| `node-a` | `10.203.0.2/32` |
| `node-b` | `10.203.0.3/32` |

Each node installs one route for the shared overlay subnet (`TUN_SUBNET=10.203.0.0/24`) into its own TUN, with MTU 1400. This steers overlay packets into Go without enumerating remote peers or selecting forwarding paths. **Ping timeouts and a nonzero ping exit status are expected:** the current Go observer logs and discards IP packets. Look for `TUN Packet` messages in the sender's logs, not replies or packets at the remote node. Advertised UDP endpoints are still metadata; this mode does not bind UDP, implement an overlay, or load eBPF programs yet.

Requires Docker Engine/Desktop running with `/dev/net/tun` available in its Linux host/VM. Containers run as root with `NET_ADMIN` for TUN/routes and `NET_RAW` for diagnostic ping; all other capabilities are dropped. No privileged mode, host networking, or published ports are used. TUN addresses/routes stay inside each container's network namespace. The internal bridge carries libp2p traffic, separately from the experimental TUN routes. Restart the whole mesh if the ephemeral seed is restarted.

Compose configurations live in `docker/`: `compose.tun.yaml` for this experiment and `compose.legacy.yaml` for the older two-process prototype. They use separate project names, networks, and bootstrap volumes.

### Three-node Docker mesh (existing two-process prototype)

```sh
docker compose -f docker/compose.legacy.yaml up --build
# In another terminal, inspect any node's Rust neighbor registry:
docker compose -f docker/compose.legacy.yaml exec node-a routerctl -socket /run/tresleches/router.sock get-status
docker compose -f docker/compose.legacy.yaml down
```

Each service (`seed`, `node-a`, `node-b`) runs one Go controller and one Rust router. A shared volume supplies the seed's current libp2p address; other nodes wait for seed health before starting. Containers have separate network namespaces, use their own private IPv4 address for TCP 4001/UDP 5001, and publish no host ports. The internal Docker network has no external connectivity. Start Docker Desktop/Engine first; images are built with Go 1.27.3 and Rust 1.98.1.

This launcher explicitly enables `routerd --allow-private-udp` for the isolated development network. Default router startup remains loopback-only; wildcard/public UDP binds are still rejected. This is not a security boundary or an Internet-safe deployment: UDP authentication/encryption and payload forwarding remain unfinished. Expect peer metadata to appear in each router's registry after discovery, not UDP delivery. Restart the whole mesh after restarting the seed, whose identity is ephemeral; the seed address is regenerated on startup.

For architecture context, discovery smoke-test commands, and implementation checkpoints, see [AGENTS.md](AGENTS.md). Agents should teach as well as implement; the project-local [teach-networking skill](.opencode/skills/teach-networking/SKILL.md) describes that workflow.
