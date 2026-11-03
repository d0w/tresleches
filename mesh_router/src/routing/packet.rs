pub type NodeId = u32;

const HEADER_LENGTH: usize = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub src_node: NodeId,
    pub dst_node: NodeId,
    pub hop_limit: u8,
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn encode(&self) -> Vec<u8> {
        // udp packet
        // src_port, dst_port
        // length, checksum
        // --data--
        // src_node
        // dst_node
        // hop_limit, ...
        // payload[0,3]
        // payload[4,7]
        // ...

        let mut buf: Vec<u8> = Vec::with_capacity(HEADER_LENGTH + self.payload.len());
        buf.extend_from_slice(&self.src_node.to_be_bytes());
        buf.extend_from_slice(&self.dst_node.to_be_bytes());
        buf.push(self.hop_limit);
        buf.extend_from_slice(&self.payload);
        buf
    }

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < HEADER_LENGTH {
            return None;
        }

        let src_node = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let dst_node = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let hop_limit = buf[8];
        let data = buf[9..].to_vec();

        Some(Packet {
            src_node,
            dst_node,
            hop_limit,
            payload: data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let original = Packet {
            src_node: 1,
            dst_node: 2,
            hop_limit: 15,
            payload: b"data payload".to_vec(),
        };

        let encoded = original.encode();
        let decoded = Packet::decode(&encoded).expect("Failed to decode valid packet");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_empty_payload_boundary() {
        let packet = Packet {
            src_node: 1,
            dst_node: 2,
            hop_limit: 1,
            payload: vec![],
        };

        let encoded = packet.encode();

        assert_eq!(encoded.len(), 9);

        let decoded = Packet::decode(&encoded).expect("9 bytes must decode to empty Packet");
        assert_eq!(decoded.payload.len(), 0);
        assert_eq!(decoded, packet);
    }
}
