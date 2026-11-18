# Tres Leches

Implementation of mesh routers, load-balancers, and firewalls because why not. The goal is decentralized layer-4 networking: forwarding traffic through peers, balancing flows, and applying traffic policy. This does not currently distribute a single TCP connection across multiple peers.

This is primarily an experimentation and learning project, not a production networking stack.

## Architecture (evolving)

- **Go control plane (`controller/`):** libp2p/DHT peer discovery, followed by authenticated peer-info exchange and coordination of neighbors/routes.
- **Rust data plane (`mesh_router/`):** UDP payload delivery and forwarding, eventually using live configuration from the local Go controller.
- **Local IPC:** Unix-socket configuration/status API exists; wiring it into the live controller remains unfinished. Payloads are intended to travel between Rust routers, directly or through Rust relays, rather than being proxied through Go.

This is the current direction but its subject to change. Both implementations, packet formats, identity representation, and responsibility boundaries are open to change.

## Current status

Go has namespace-based discovery under `tresleches/mesh/v1`, authenticated peer-info exchange, and a standalone IPC client. Rust has a Tokio-based `routerd` with status/neighbor configuration, plus a separate localhost forwarding demo with manually configured routes. The daemon binds UDP but does not yet receive or forward payloads; its IPC neighbor registry is not connected to the forwarding engine. `one_shot_test/` contains a separate content-transfer experiment.

## Roadmap

Current direction: keep Go responsible for discovery, topology, backend membership, and policy configuration; use Rust for userspace packet processing. Rust needs installed forwarding state, not a duplicate of Go's complete peer directory. This is a design/learning choice, not a demonstrated performance requirement.

1. **Finish controller/router IPC integration.** Check local router readiness, install selected neighbor/route configuration, and verify acknowledgements, retries, and restoration after router restart.
2. **Deliver real payloads through `routerd`.** Define how test applications submit traffic, connect live routes to the forwarding engine, add UDP reachability probes, and test direct delivery followed by relay forwarding. Validate packet framing and hop limits.
3. **Add flow-aware load balancing.** Select eligible backends or relay paths for new flows, preserve flow affinity, expire state, and verify the return path. Backend balancing and path balancing are distinct; do not randomly split one TCP connection across independent servers.
4. **Add traffic policy and visibility.** Start with header-based allow/drop rules, bounded queues, rate limits, and packet/byte/drop counters. Deep application-payload inspection is not an initial goal.
5. **Secure the data plane before untrusted deployment.** Use an established authenticated-encryption protocol/implementation with replay protection; do not invent cryptography. Keep earlier unauthenticated experiments on localhost. Libp2p control-stream security does not protect separate UDP packets.
6. **Evaluate eBPF for specific jobs.** Begin with Linux-based observability experiments, then profile before moving filtering, route lookup/redirection, or L4 balancing into kernel hooks. eBPF is optional targeted acceleration, not a commitment to move everything into the kernel. Encrypted inner headers are unavailable to hooks before decryption.

Keep discovery, authenticated metadata exchange, installed configuration, and proven UDP reachability as separate states. Each milestone needs its own tests; learning an advertised endpoint does not establish payload delivery.

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

### Three-node Docker mesh

```sh
docker compose up --build
# In another terminal, inspect any node's Rust neighbor registry:
docker compose exec node-a routerctl -socket /run/tresleches/router.sock get-status
docker compose down
```

Each service (`seed`, `node-a`, `node-b`) runs one Go controller and one Rust router. A shared volume supplies the seed's current libp2p address; other nodes wait for seed health before starting. Containers have separate network namespaces, use their own private IPv4 address for TCP 4001/UDP 5001, and publish no host ports. The internal Docker network has no external connectivity. Start Docker Desktop/Engine first; images are built with Go 1.27.3 and Rust 1.98.1.

This launcher explicitly enables `routerd --allow-private-udp` for the isolated development network. Default router startup remains loopback-only; wildcard/public UDP binds are still rejected. This is not a security boundary or an Internet-safe deployment: UDP authentication/encryption and payload forwarding remain unfinished. Expect peer metadata to appear in each router's registry after discovery, not UDP delivery. Restart the whole mesh after restarting the seed, whose identity is ephemeral; the seed address is regenerated on startup.

For architecture context, discovery smoke-test commands, and implementation checkpoints, see [AGENTS.md](AGENTS.md). Agents should teach as well as implement; the project-local [teach-networking skill](.opencode/skills/teach-networking/SKILL.md) describes that workflow.
