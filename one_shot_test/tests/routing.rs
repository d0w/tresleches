//! Public-API integration test using a recording transport.

use std::io;

use tresleches::{PeerId, Router, transport::Transport};

// A test double: exercises the public API without opening network sockets.
#[derive(Default)]
struct RecordingTransport {
    sent: Vec<(PeerId, Vec<u8>)>,
}

impl Transport for RecordingTransport {
    fn send(&mut self, peer: PeerId, payload: &[u8]) -> io::Result<()> {
        self.sent.push((peer, payload.to_vec()));
        Ok(())
    }
}

#[test]
fn routes_payloads_in_round_robin_order() -> Result<(), Box<dyn std::error::Error>> {
    let mut router = Router::new(vec![PeerId(10), PeerId(20)])?;
    let mut transport = RecordingTransport::default();

    for _ in 0..3 {
        transport.send(router.next_peer(), b"hello")?;
    }

    assert_eq!(
        transport.sent,
        vec![
            (PeerId(10), b"hello".to_vec()),
            (PeerId(20), b"hello".to_vec()),
            (PeerId(10), b"hello".to_vec()),
        ]
    );
    Ok(())
}
