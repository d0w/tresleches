# Tres Leches: agent context

## Purpose and working style

Tres Leches is an experimental decentralized layer-4 networking project and a
learning exercise: mesh routers, forwarding, and eventually load balancing and
firewalls. Do not describe the current prototype as a complete TCP/UDP stack or
as already distributing a single connection across multiple peers.

- Teach as well as help build. Load the project skill `teach-networking` from
  `.opencode/skills/teach-networking/SKILL.md` for implementation guidance,
  architectural discussions, networking explanations, and debugging.
- Explain mechanisms and tradeoffs with small concrete examples. Give ordered
  steps, exact file paths, runnable commands, and observable success criteria.
- A request for steps or an explanation is not permission to implement. When
  implementation is requested, make focused changes and explain the result.
- Read current files before advising: the user edits code between messages.
  Preserve unfinished scaffolding and unrelated changes.
- Distinguish observed behavior, hypotheses, proposed work, and tested results.
  Do not repeat restart advice or insist on a cause after contrary evidence.

## Architecture: current direction, not a fixed contract

The working design is one Go controller and one Rust router per node:

```text
Node A                                  Node B
Go controller <--- libp2p control ----> Go controller
      ^                                       ^
      | local IPC                             | local IPC
      v                                       v
Rust router   <------ UDP payload ------> Rust router
```

- **Go control plane:** discover application peers, exchange authenticated
  control messages, maintain peer/neighbor metadata, coordinate routes, and
  configure the local Rust router. It is more than an IP/port lookup service.
- **Rust data plane:** own payload sockets, test data-plane reachability, maintain
  live neighbor/route state, and send, receive, or relay payload packets.
- **Payloads bypass Go.** The default plan is direct Rust-to-Rust UDP, or
  forwarding through another Rust router when a relay is needed. Go coordinates
  this; it is not the application-payload proxy.
- **Local IPC:** a Unix-domain socket with small versioned messages is the
  initial candidate, not an implemented or mandatory choice. Eventually Rust
  reports its bound endpoint/status and Go sends neighbor/route updates.
- **Identity:** use authenticated libp2p `peer.ID` as the canonical control-plane
  identity. Rust can initially treat it as opaque data. Its eventual packet
  representation and any compact-ID mapping remain design decisions; do not
  truncate peer IDs to `u32` and assume uniqueness.
- **Flexibility:** either language's APIs, packet format, identity representation,
  process layout, transport, and responsibility boundaries may change when
  justified. Existing Rust static routes and `u32` node IDs are prototype details,
  not constraints on future design. Explain a proposed change and its cost.

Keep these states separate:

```text
namespace peer discovered -> control handshake complete -> data plane reachable
```

An existing connection is not a completed application handshake. An advertised
UDP endpoint is not proof that a UDP path works or that the peer owns that endpoint.
libp2p stream authentication/encryption does not automatically authenticate or
encrypt Rust UDP packets. Keep initial unauthenticated UDP experiments local;
design data-plane security before exposing them to untrusted networks.
NAT traversal, relays, route expiry, and production hardening are later work.

## Repository and observed implementation

This snapshot reflects the context handoff. Re-read files before relying on it.

- `controller/`: Go control plane. Its module path is still
  `github.com/d0w/tresleches/discovery`; the directory was renamed, not the module.
  Use `controller/go.mod` for current toolchain/dependency versions.
- `controller/cmd/controller/main.go`: CLI, signal context, libp2p host, connection
  notifications, explicit seed dialing, server-mode DHT, and discovery startup.
  Current flags are `-listen`, `-bootstrap`, and legacy `-find`. The last is parsed
  but not needed for namespace discovery. Proposed flags are not available yet.
- `controller/internal/discovery/discovery.go`: namespace advertisement and
  periodic searches; skips self and already-connected candidates, then dials.
- `controller/constants/const.go`: namespace `tresleches/mesh/v1`; search interval
  30 seconds despite the name `AdvertiseInterval`. Searches have a 20-second
  context budget and explicit candidate dials have a five-second budget.
- `controller/internal/control/peerinfo.go`: `PeerInfo{Version, UDPAddress}` and
  protocol constant exist. Current literal is `/tresleches/control/1.0.0/`, with a
  trailing slash; earlier design examples used `/tresleches/control/1.0.0`.
  Protocol IDs must match exactly. Agree on one literal when implementing.
- `controller/internal/control/control.go`: unfinished `NetworkingController`,
  mutex, and peer-info map. Constructor currently does not retain local info;
  `Register` and `Exchange` are stubs. Nothing is wired into `main` yet.
  Control metadata should be exchanged over a stream, not read from the DHT as
  the existing `Exchange` comment suggests.
- `controller/internal/proxy/`: unfinished scaffolding. Its names do not establish
  a decision to proxy application payloads through Go. Preserve user work.
- `mesh_router/src/routing/engine.rs`: Rust UDP forwarding, hop limit, and
  manually configured `HashMap<NodeId, Route>` with direct/relay next hops.
- `mesh_router/src/routing/packet.rs`: prototype `NodeId = u32` packet format;
  no packet authentication/encryption. May be redesigned.
- `mesh_router/src/main.rs`: a three-router localhost demo in one process, not
  yet one router process per Go controller. No Go/Rust IPC integration exists.
- `one_shot_test/`: separate Rust content-transfer experiment with simulated
  provider-directory discovery, not the live libp2p discovery implementation.

Do not claim endpoint exchange, `-udp-advertise`, `-identity`, configurable
namespaces, dynamic Go-to-Rust routes, or data-plane probing already work.

## Discovery mechanics and lessons learned

- Bootstrap seeds provide entry into the DHT. They are not inherently a central
  application registry or payload relay.
- `routingdiscovery` hashes the namespace into a content key. Advertising stores
  provider records with DHT nodes responsible for that key; it is not a broadcast
  to every peer. Searching returns peers claiming that namespace.
- DHT routing discovers and dials peers independently of application membership.
  All current nodes run `dht.ModeServer`. Bootstrap, advertising, and searches can
  quickly connect all nodes in a small test before the discovery consumer dials.
- Therefore, "already connected" is normal even after a fresh restart.
  "connected to peer" in the discovery loop only observes that loop's explicit
  successful dial. The `network.NotifyBundle` hook in `main` observes connection
  openings regardless of the caller, including incoming connections.
- A connection alone does not prove namespace discovery. Check that `FindPeers`
  returns another participant's **current ID** for the requested namespace.
  Later use different namespaces as a negative test: peers may still have DHT
  connections without appearing in each other's namespace results.
- Provider records are expiring claims, not live-presence guarantees. Disconnecting
  does not immediately delete announcements held by other nodes. Ephemeral
  identity plus a reused port can leave old IDs pointing at a new process.
- Earlier logs returned several identities from previous runs at ports 4002/4003.
  A handshake ID mismatch authenticates a different peer at that endpoint;
  "dial to self attempted" rejects a destination matching a local listener even
  if the candidate ID differs. Neither error means authentication should be bypassed.
- "dial backoff" suppresses another attempt after a prior failure; find the
  original error instead of disabling backoff or spinning on retries.
- Persistent identity was proposed as optional key-file loading/creation, not
  implemented. A private key determines peer ID; each node needs its own file.
  Protect files, keep them out of Git, and fail on corrupt/unreadable existing
  keys rather than silently replacing them. Persistence prevents identity churn;
  it does not erase existing provider records.
- `routingdiscovery.Advertise` performs one announcement. Plain DHT `Provide`
  does not automatically republish. `discoveryutil.Advertise` starts a background
  renewal/retry worker; calling it does not prove publication succeeded. Check
  the installed library for timing and log actual publication failures if needed.
- `FindPeers` returns a channel of `peer.AddrInfo`, not goroutines. One call can
  start concurrent internal queries. An unbuffered send blocks its sender until
  receipt, not until the consumer finishes processing; other queries may continue.

## Immediate milestone: peer-info exchange

Complete the existing control scaffolding before adding Rust integration:

1. Add a proposed `-udp-advertise` flag and pass local `PeerInfo` into the control
   service. For localhost tests, endpoints are configured manually; this flag
   advertises a claim and does not bind a UDP socket.
2. Validate version `1` and a concrete IP/nonzero-port endpoint, initially with
   `netip.ParseAddrPort`; reject unspecified/multicast addresses and port zero.
   A binding address such as `0.0.0.0` is not an advertised destination.
3. Register the agreed control protocol handler before bootstrap/discovery.
4. Use one request/response stream: initiator sends its info, receiver validates
   and replies with its own, and both save validated remote info. Identify remote
   peers with `stream.Conn().RemotePeer()`, not a peer-ID field supplied in JSON.
5. Proposed framing: four-byte big-endian JSON length plus JSON body, maximum
   4096-byte body checked before allocation. Handle partial reads/writes; use
   `io.ReadFull` for reads, deadlines (initially five seconds), close on success,
   and reset on malformed/failed exchanges. Honor cancellation.
6. Store `map[peer.ID]PeerInfo` behind synchronization. Incoming handlers and
   outgoing exchanges can run concurrently. Retain local info and make repeated
   exchanges idempotent updates rather than duplicate registrations.
7. Keep discovery generic with a candidate callback, or a similarly small
   interface. Ensure connectivity, then invoke exchange even for already-connected
   peers; remove the unconditional connectedness `continue` from that path.
   Failed candidates/exchanges must not stop the whole search.
8. First test two hosts without DHT, already connected before exchange. Verify
   both registries, repeated exchanges, invalid version/address/framing, limits,
   and timeout handling. Then test the same exchange through DHT discovery.

Acceptance: A and B know only the seed address initially; A learns B's current
peer ID and advertised endpoint, and B learns A's. Existing DHT connections do
not invalidate the test. No UDP reachability or payload-transfer claim yet.

Afterward: local IPC -> Rust dynamic neighbor/routes and identity representation
-> data-plane probes -> payload delivery without manual routes -> expiry/relay
selection/multi-hop -> NAT traversal and further hardening. Revise this sequence
when evidence or learning goals warrant it.

## Diagnostics and verification

- Prefer one root `slog.Logger`, injected into components and enriched with
  component/local-peer/namespace/round/remote-peer fields. Debug: repeated rounds,
  skips, expected candidate dial failures. Info: lifecycle, connections, completed
  handshakes. Warn: operational problems. Error: fatal failures.
  Current root handler is configured for Debug; a `LOG_LEVEL` switch is not wired.
- Application `slog` configuration does not configure dependency logging.
  Context-aware logging does not automatically add correlation IDs; cross-node
  correlation requires explicitly propagating IDs in protocol messages.
- Shutdown hangs were reported, but no root cause was established. Signal context
  registration cancels context; it does not force exit. Trace discovery return,
  `idht.Close()`, and `h.Close()` with before/after logs, then obtain stacks if
  needed. Ctrl+\\ normally dumps Go stacks and terminates, skipping cleanup.
- Current discovery runs in a goroutine that `main` does not join. Cancellation
  should eventually stop workers before their dependencies close. The loop also
  defers search cancellation repeatedly; prefer per-round scope when refactoring.

Run from repository root, only as appropriate to the change:

```sh
go -C controller test ./...
cargo test --workspace
```

Current discovery-only smoke test (three terminals):

```sh
# Seed: copy its newly printed peer ID.
go -C controller run ./cmd/controller -listen /ip4/127.0.0.1/tcp/4001

# Replace SEED_ID in both commands with that seed's current ID.
go -C controller run ./cmd/controller -listen /ip4/127.0.0.1/tcp/4002 -bootstrap /ip4/127.0.0.1/tcp/4001/p2p/SEED_ID
go -C controller run ./cmd/controller -listen /ip4/127.0.0.1/tcp/4003 -bootstrap /ip4/127.0.0.1/tcp/4001/p2p/SEED_ID
```

For independent tests, ensure all actual node processes have exited before
expecting an empty in-memory network; closing a shell alone is not proof. Fresh
namespace names can isolate provider records once namespace configuration exists.
Docker is optional for interface/failure tests, not a fix for early DHT connections.
Container `127.0.0.1` is local to that container; cross-container tests need
reachable advertised addresses, separately from wildcard listen addresses.

Report what was actually run, observed, or blocked. Do not claim unrun tests pass.
