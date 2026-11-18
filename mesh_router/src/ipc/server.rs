use crate::routing::state::MeshState;

use super::{
    frame::{read_frame, write_frame},
    protocol::{Command, Request},
    socket_path::SocketPathGuard,
};
use serde_json::{Value, json};
use std::{fs, net::SocketAddr, os::unix::fs::PermissionsExt, sync::Arc, time::Duration};
use tokio::{
    io,
    io::AsyncWriteExt,
    net::{UdpSocket, UnixListener, UnixStream},
    task::JoinSet,
    time::timeout,
};
use tokio_util::sync::CancellationToken;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONNECTIONS: usize = 16;
const MAX_NEIGHBORS: usize = 16;

/// Starts the interprocess communication server
///
/// # Arguments
///
/// * `socket_path` - Reference to string referring to IPC socket file
/// * `udp` - UdpSocket referring to socket where data is routed
/// * `shutdown` - Tokio CancellationToken
///
pub async fn serve(
    socket_path: &str,
    udp: &UdpSocket,
    shutdown: CancellationToken,
) -> io::Result<()> {
    let state = Arc::new(MeshState::new(udp.local_addr()?));
    serve_with_state(socket_path, state, shutdown).await
}

/// Starts the interprocess communication server with state
///
/// # Arguments
///
/// * `socket_path` - Reference to string referring to IPC socket file
/// * `state` - The mesh network state holding data on neighbors
/// * `shutdown` - Tokio CancellationToken
///
pub async fn serve_with_state(
    socket_path: &str,
    state: Arc<MeshState>,
    shutdown: CancellationToken,
) -> io::Result<()> {
    // set a listener
    let listener = UnixListener::bind(socket_path)?;
    let _socket_guard = SocketPathGuard::new(socket_path);
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))?; // -rw-----

    let mut tasks: JoinSet<()> = JoinSet::new();

    let local_addr = &state.udp_address;
    println!("UDP bound at {local_addr}");
    println!("Control socket: {socket_path}");
    // on event, read and validate frame
    // deserialize to get command

    let outcome: io::Result<()> = loop {
        tokio::select! {
        // polling order dependent
        biased;

        _ = shutdown.cancelled() => break Ok(()),

        completed = tasks.join_next(), if !tasks.is_empty() => {
            if let Some(Err(error)) = completed {
                eprintln!("control task failed: {error}");
            }
        }

        accepted = listener.accept(), if tasks.len() < MAX_CONNECTIONS => {
            match accepted {
                Ok((stream, _address)) => {
                    if shutdown.is_cancelled() {
                        break Ok(())
                    }

                    let state = Arc::clone(&state);

                    // move value to async function and prevent outside use
                    tasks.spawn(async move {
                        match timeout(
                            REQUEST_TIMEOUT,
                            handle(stream, state),
                        ).await {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                eprintln!("control request failed: {error}");
                            }
                            Err(_) => {
                                eprintln!("control request timed out");
                            }
                        }
                    });
                }

                Err(error) if error.kind() == io::ErrorKind::Interrupted => { continue; }

                Err(error) => break Err(error),
            }
            }
        }
    };

    drop(listener);

    let drained = timeout(SHUTDOWN_TIMEOUT, async {
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result {
                eprintln!("control task failed during shutdown: {error}")
            }
        }
    })
    .await;

    // timeout reached
    if drained.is_err() {
        eprintln!("shutdown grace period expired; aborting remaining requests");
        tasks.abort_all();

        // wait for all tasks to be cancelled
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result {
                eprintln!("control task failed during abort: {error}")
            }
        }
    }

    outcome
}

fn invalid(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

async fn handle(mut stream: UnixStream, state: Arc<MeshState>) -> io::Result<()> {
    let body = read_frame(&mut stream).await?;
    let request: Request = serde_json::from_slice(&body).map_err(invalid)?;

    let response = match apply_request(request, &state).await {
        Ok(response) => response,
        Err(error) => json!({
            "ok": false,
            "error": error,
        }),
    };

    let body = serde_json::to_vec(&response).map_err(invalid)?;
    write_frame(&mut stream, &body).await?;
    stream.shutdown().await
}

async fn apply_request(request: Request, state: &MeshState) -> Result<Value, String> {
    if request.version != 1 {
        return Err("unsupported IPC version".into());
    }

    match request.command {
        Command::GetStatus => {
            let neighbors = state.neighbors.read().await.clone();

            Ok(json!({
                "ok": true,
                "udp_address": &state.udp_address,
                "neighbors": neighbors,
            }))
        }

        Command::UpsertNeighbor {
            peer_id,
            udp_address,
        } => {
            if peer_id.is_empty() || peer_id.len() > 256 {
                return Err("invalid peer ID length".into());
            }

            let endpoint: SocketAddr = udp_address
                .parse()
                .map_err(|err| format!("invalid UDP endpoint: {err}"))?;

            if endpoint.port() == 0
                || endpoint.ip().is_unspecified()
                || endpoint.ip().is_multicast()
            {
                return Err("unusable UDP endpoint: {endpoint}".into());
            }

            let mut neighbors = state.neighbors.write().await;

            if neighbors.len() >= MAX_NEIGHBORS && !neighbors.contains_key(&peer_id) {
                return Err("neighbor limit reached. unable to add peer: {peer_id}".into());
            }

            let address = endpoint.to_string();
            let changed = neighbors.get(&peer_id) != Some(&address);

            neighbors.insert(peer_id, address);
            drop(neighbors);

            if changed {
                state.neighbors_changed.notify_one();
            }

            Ok(json!({ "ok": true }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{io::AsyncReadExt, time::sleep};

    #[tokio::test]
    async fn serves_other_clients_and_cleans_up_on_shutdown() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ipc.sock");
        let path_string = path.to_str().unwrap().to_owned();

        let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let shutdown = CancellationToken::new();

        let task_path = path_string.clone();
        let task_shutdown = shutdown.clone();

        let server = tokio::spawn(async move { serve(&task_path, &udp, task_shutdown).await });

        // Wait for readiness and retain a client that sends no request.
        let mut stalled = timeout(Duration::from_secs(2), async {
            loop {
                match UnixStream::connect(&path_string).await {
                    Ok(stream) => break stream,
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                        ) =>
                    {
                        sleep(Duration::from_millis(10)).await;
                    }
                    Err(error) => panic!("connect failed: {error}"),
                }
            }
        })
        .await
        .expect("server did not become ready");

        let mut healthy = UnixStream::connect(&path_string).await.unwrap();

        write_frame(&mut healthy, br#"{"version":1,"type":"get_status"}"#)
            .await
            .unwrap();

        let body = timeout(Duration::from_secs(2), read_frame(&mut healthy))
            .await
            .expect("silent client blocked healthy request")
            .unwrap();

        let reply: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(reply["ok"], true);

        shutdown.cancel();

        timeout(SHUTDOWN_TIMEOUT + Duration::from_secs(1), server)
            .await
            .expect("server did not shut down")
            .expect("server task panicked")
            .expect("server returned an error");

        assert!(!path.exists(), "socket pathname was not removed");

        let mut byte = [0u8; 1];
        let count = timeout(Duration::from_secs(1), stalled.read(&mut byte))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(count, 0, "stalled connection remained open");
    }
}
