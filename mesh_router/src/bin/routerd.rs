//! Mesh networking router daemon

use mesh_router::{
    ipc,
    routing::{probe, state::MeshState},
};
use std::{env, io, net::IpAddr, sync::Arc};
use tokio::{
    net::UdpSocket,
    signal::unix::{SignalKind, signal},
};
use tokio_util::sync::CancellationToken;

/// Mesh router daemon
#[tokio::main]
async fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if !(args.len() == 3 || (args.len() == 4 && args[3] == "--allow-private-udp")) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: routerd <udp-bind> <control-socket> [--allow-private-udp]",
        ));
    }
    let allow_private_udp = args.len() == 4;

    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;

    let udp = UdpSocket::bind(&args[1]).await?;
    let local_addr = udp.local_addr()?;

    if !allowed_udp_address(local_addr.ip(), allow_private_udp) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "use a loopback UDP address, or --allow-private-udp for isolated private-network tests",
        ));
    }

    let shutdown = CancellationToken::new();
    let state = Arc::new(MeshState::new(local_addr));

    let control = async {
        let result = ipc::serve_with_state(&args[2], Arc::clone(&state), shutdown.clone()).await;

        shutdown.cancel();
        result
    };

    let probes = async {
        let result = probe::run(&udp, Arc::clone(&state), shutdown.clone()).await;

        shutdown.cancel();
        result
    };

    let services = async {
        let (control_result, probe_result) = tokio::join!(control, probes);

        control_result.and(probe_result)
    };

    tokio::pin!(services);

    tokio::select! {
        result = &mut services => return result,
        _ = interrupt.recv() => {}
        _ = terminate.recv() => {}
    }

    println!("Shutting down...");
    shutdown.cancel();

    services.await
}

fn allowed_udp_address(ip: IpAddr, allow_private: bool) -> bool {
    ip.is_loopback()
        || (allow_private
            && match ip {
                IpAddr::V4(ip) => ip.is_private(),
                IpAddr::V6(ip) => ip.is_unique_local(),
            })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_udp_requires_explicit_opt_in() {
        for address in ["127.0.0.1", "::1"] {
            let ip = address.parse().unwrap();
            assert!(allowed_udp_address(ip, false));
            assert!(allowed_udp_address(ip, true));
        }
        for address in ["10.0.0.1", "172.20.0.2", "192.168.1.2", "fd00::1"] {
            let ip = address.parse().unwrap();
            assert!(!allowed_udp_address(ip, false));
            assert!(allowed_udp_address(ip, true));
        }
        for address in [
            "0.0.0.0",
            "::",
            "224.0.0.1",
            "ff02::1",
            "8.8.8.8",
            "2001:db8::1",
        ] {
            let ip = address.parse().unwrap();
            assert!(!allowed_udp_address(ip, false));
            assert!(!allowed_udp_address(ip, true));
        }
    }
}
