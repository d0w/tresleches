use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::thread;

const HOP_LIMIT: u8 = 16;

use super::{packet::NodeId, packet::Packet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Direct(SocketAddr),
    Relay(SocketAddr),
}

pub struct Router {
    pub node_id: NodeId,
    pub routing_table: HashMap<NodeId, Route>,
    socket: Arc<UdpSocket>,
}

impl Router {
    pub fn new(node_id: NodeId, bind_addr: &str) -> std::io::Result<Self> {
        let socket = Arc::new(UdpSocket::bind(bind_addr)?);
        Ok(Self {
            node_id,
            routing_table: HashMap::new(),
            socket,
        })
    }

    pub fn add_route(&mut self, target: NodeId, route: Route) {
        self.routing_table.insert(target, route);
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub fn process_packet(&self, mut packet: Packet) {
        // packet for this node
        if packet.dst_node == self.node_id {
            let message = String::from_utf8_lossy(&packet.payload);
            println!(
                "[Node {}] Received payload from Node {}: \"{}\"",
                self.node_id, packet.dst_node, message
            );
            return;
        }

        // packet exceeds ttl
        if packet.hop_limit <= 1 {
            println!(
                "[Node {}] Packet's (Node {} -> Node {}) TTL has exceeded.",
                self.node_id, packet.src_node, packet.dst_node
            );
            return;
        }

        // packet needs to be forwarded
        packet.hop_limit -= 1;
        match self.routing_table.get(&packet.dst_node) {
            Some(Route::Direct(addr)) | Some(Route::Relay(addr)) => {
                println!(
                    "[Node {}] Relaying packet (Dst: Node {}) to physical addr {}",
                    self.node_id, packet.dst_node, addr
                );
                let bytes = packet.encode();
                let _ = self.socket.send_to(&bytes, addr);
            }
            None => {
                println!(
                    "[Node {}] No route to host Node {}. Dropping packet.",
                    self.node_id, packet.dst_node
                );
            }
        }
    }

    pub fn send_message(&self, dst_node: NodeId, message: &[u8]) {
        let packet = Packet {
            src_node: self.node_id,
            dst_node,
            hop_limit: HOP_LIMIT,
            payload: message.to_vec(),
        };
        self.process_packet(packet);
    }

    pub fn run_listener(self: Arc<Self>) {
        thread::spawn(move || {
            let mut buf = [0u8; 65535];
            loop {
                match self.socket.recv_from(&mut buf) {
                    Ok((amt, _src_addr)) => {
                        if let Some(packet) = Packet::decode(&buf[..amt]) {
                            self.process_packet(packet);
                        }
                    }
                    Err(e) => {
                        eprintln!("[Node {}] Socket error: {}", self.node_id, e);
                        break;
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_add_route() {
        let mut router = Router::new(1, "127.0.0.1:0").unwrap();
        let target_addr: SocketAddr = "127.0.0.1:54321".parse().unwrap();

        router.add_route(2, Route::Direct(target_addr));
        router.add_route(3, Route::Relay(target_addr));

        assert_eq!(
            router.routing_table.get(&2),
            Some(&Route::Direct(target_addr))
        );
        assert_eq!(
            router.routing_table.get(&3),
            Some(&Route::Relay(target_addr))
        );
        assert_eq!(router.routing_table.get(&99), None);
    }

    #[test]
    fn test_drop_when_hop_limit_expired() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let receiver_addr = receiver.local_addr().unwrap();

        let mut router = Router::new(1, "127.0.0.1:0").unwrap();
        router.add_route(2, Route::Direct(receiver_addr));

        // Packet with hop_limit 1 must be dropped before transmission
        let packet = Packet {
            src_node: 10,
            dst_node: 2,
            hop_limit: 1,
            payload: b"should not arrive".to_vec(),
        };

        router.process_packet(packet);

        let mut buf = [0u8; 64];
        let result = receiver.recv_from(&mut buf);
        assert!(
            result.is_err(),
            "Packet with hop_limit=1 must not be forwarded"
        );
    }

    #[test]
    fn test_forwarding_decrements_hop_limit() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let receiver_addr = receiver.local_addr().unwrap();

        let mut router = Router::new(1, "127.0.0.1:0").unwrap();
        router.add_route(2, Route::Direct(receiver_addr));

        let packet = Packet {
            src_node: 10,
            dst_node: 2,
            hop_limit: 5,
            payload: b"forward me".to_vec(),
        };

        router.process_packet(packet);

        let mut buf = [0u8; 64];
        let (len, _) = receiver
            .recv_from(&mut buf)
            .expect("Receiver did not get forwarded packet");
        let received_packet = Packet::decode(&buf[..len]).expect("Invalid packet received");

        assert_eq!(received_packet.src_node, 10);
        assert_eq!(received_packet.dst_node, 2);
        assert_eq!(received_packet.hop_limit, 4);
        assert_eq!(received_packet.payload, b"forward me");
    }

    #[test]
    fn test_unroutable_packet_dropped() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();

        let router = Router::new(1, "127.0.0.1:0").unwrap();
        // No routes configured

        let packet = Packet {
            src_node: 1,
            dst_node: 99,
            hop_limit: 10,
            payload: b"nowhere".to_vec(),
        };

        // Should drop silently without panic
        router.process_packet(packet);
    }
}
