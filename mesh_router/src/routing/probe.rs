use super::state::MeshState;
use std::{collections::BTreeMap, io, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    net::UdpSocket,
    time::{Instant, MissedTickBehavior, interval},
};
use tokio_util::sync::CancellationToken;

const MAGIC: &[u8; 4] = b"TLPB";
const VERSION: u8 = 1;
const PING: u8 = 1;
const PONG: u8 = 2;

// magic(4) | version(1) | type(1) | nonce(16)
const FRAME_SIZE: usize = 22;
const MAX_PENDING: usize = 16;
const PROBE_INTERVAL: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

struct Pending {
    endpoint: SocketAddr,
    nonce: [u8; 16],
    sent_at: Instant,
}

enum Event {
    Probe,
    Expire,
    Received(usize, SocketAddr),
}

/// Exchanges probes with configured neighbors
pub async fn run(
    udp: &UdpSocket,
    state: Arc<MeshState>,
    shutdown: CancellationToken,
) -> io::Result<()> {
    let mut pending: BTreeMap<String, Pending> = BTreeMap::new();

    let mut probe_tick = interval(PROBE_INTERVAL);
    probe_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut expiry_tick = interval(Duration::from_secs(3));
    expiry_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut buffer = [0u8; FRAME_SIZE + 1];

    loop {
        // listen for poll, received data, or updates from control plane
        let event = tokio::select! {
            biased;

            _ = shutdown.cancelled() => return Ok(()),
            _ = expiry_tick.tick() => Event::Expire,
            _ = probe_tick.tick() => Event::Probe,
            _ = state.neighbors_changed.notified() => Event::Probe,

            received = udp.recv_from(&mut buffer) => {
                let (size, source) = received?;
                Event::Received(size, source)
            }

        };

        match event {
            Event::Probe => {
                println!("PROBING");
                let known = configured_neighbors(&state).await;

                // only keep neighbors with defined enpoints
                pending.retain(|peer, request| known.get(peer) == Some(&request.endpoint));

                // send a ping to each known peer
                for (peer_id, endpoint) in known {
                    println!("PROBING {peer_id} at {endpoint}");
                    if pending.contains_key(&peer_id) || pending.len() >= MAX_PENDING {
                        continue;
                    }
                    let mut nonce = [0u8; 16];
                    getrandom::fill(&mut nonce).map_err(|error| {
                        io::Error::other(format!("generate probe nonce: {error}"))
                    })?;

                    let sent_at = Instant::now();

                    if try_send(udp, &encode(PING, nonce), endpoint) {
                        pending.insert(
                            peer_id,
                            Pending {
                                endpoint,
                                nonce,
                                sent_at,
                            },
                        );
                    }
                }
            }

            // Remove expired probe requests
            Event::Expire => {
                pending.retain(|peer_id, request| {
                    if request.sent_at.elapsed() >= PROBE_TIMEOUT {
                        println!(
                            "UDP probe timed out peer={peer_id} endpoint={}",
                            request.endpoint
                        );
                        false
                    } else {
                        true
                    }
                });
            }

            Event::Received(size, source) => {
                println!("RECEIVED {size} {source}");
                let Some((kind, nonce)) = decode(&buffer[..size]) else {
                    continue;
                };

                let known = configured_neighbors(&state).await;

                if !known.values().any(|endpoint| *endpoint == source) {
                    continue;
                }

                match kind {
                    // send response to sender
                    PING => {
                        try_send(udp, &encode(PONG, nonce), source);
                    }

                    // confirmed found peer
                    PONG => {
                        let peer = pending.iter().find_map(|(peer, request)| {
                            (request.endpoint == source && request.nonce == nonce)
                                .then(|| peer.clone())
                        });

                        let Some(peer_id) = peer else {
                            continue;
                        };

                        let request = pending.remove(&peer_id).unwrap();

                        // endpoint updates can invalidate old replies
                        if known.get(&peer_id) != Some(&source) {
                            continue;
                        }

                        let elapsed = request.sent_at.elapsed();

                        if elapsed >= PROBE_TIMEOUT {
                            println!("Late UDP pong ignored peer={peer_id} endpoint={source}");
                            continue;
                        }

                        println!(
                            "UDP round trip peer={peer_id} endpoint{source} rtt_us={}",
                            elapsed.as_micros()
                        );
                    }

                    _ => unreachable!(),
                }
            }
        }
    }
}

async fn configured_neighbors(state: &MeshState) -> BTreeMap<String, SocketAddr> {
    let neighbors = state.neighbors.read().await;

    neighbors
        .iter()
        .filter_map(|(peer, address)| {
            address
                .parse::<SocketAddr>()
                .ok()
                .map(|endpoint| (peer.clone(), endpoint))
        })
        .collect()
}

fn encode(kind: u8, nonce: [u8; 16]) -> [u8; FRAME_SIZE] {
    let mut frame = [0u8; FRAME_SIZE];

    frame[..4].copy_from_slice(MAGIC);
    frame[4] = VERSION;
    frame[5] = kind;
    frame[6..].copy_from_slice(&nonce);

    frame
}

fn decode(frame: &[u8]) -> Option<(u8, [u8; 16])> {
    if frame.len() != FRAME_SIZE
        || &frame[..4] != MAGIC
        || frame[4] != VERSION
        || !matches!(frame[5], PING | PONG)
    {
        return None;
    }

    let nonce: [u8; 16] = frame[6..].try_into().ok()?;
    Some((frame[5], nonce))
}

fn try_send(udp: &UdpSocket, frame: &[u8], endpoint: SocketAddr) -> bool {
    match udp.try_send_to(frame, endpoint) {
        Ok(_) => true,

        Err(error) if error.kind() == io::ErrorKind::WouldBlock => false,

        Err(error) => {
            eprintln!("UDP probe send failed endpoint={endpoint}: {error}");
            false
        }
    }
}
