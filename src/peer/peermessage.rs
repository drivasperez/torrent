use bytes::{Buf, BufMut};
use tokio_util::codec::{Decoder, Encoder};

#[derive(Debug, Clone, PartialEq)]
pub enum PeerMessage {
    KeepAlive,
    Choke,                    // messageID = 0
    Unchoke,                  // messageID = 1
    Interested,               // messageID = 2
    NotInterested,            // messageID = 3
    Have(u32),                // messageID = 4
    Bitfield(Vec<u8>),        // messageID = 5
    Request(u32, u32, u32),   // messageID = 6
    Piece(u32, u32, Vec<u8>), // messageID = 7
    Cancel(u32, u32, u32),    // messageId = 8
}

impl std::fmt::Display for PeerMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::KeepAlive => String::from("Keepalive"),
            Self::Choke => String::from("Choke"),
            Self::Unchoke => String::from("Unchoke"),
            Self::Interested => String::from("Interested"),
            Self::NotInterested => String::from("NotInterested"),
            Self::Have(idx) => format!("Have {}", idx),
            Self::Bitfield(_) => String::from("Bitfield"),
            Self::Request(idx, begin, length) => format!(
                "Request (index {}, begin: {}, length: {})",
                idx, begin, length
            ),
            Self::Piece(idx, offset, data) => format!(
                "Piece (idx: {}, offset: {}, len: {})",
                idx,
                offset,
                data.len()
            ),
            Self::Cancel(idx, begin, length) => format!(
                "Cancel (index {}, begin: {}, length: {})",
                idx, begin, length
            ),
        };

        write!(f, "[PeerMessage]: {}", s)
    }
}

impl PeerMessage {
    pub fn payload_len(&self) -> usize {
        let u32_size = std::mem::size_of::<u32>();
        match self {
            Self::Choke
            | Self::Unchoke
            | Self::Interested
            | Self::NotInterested
            | Self::KeepAlive => 0,
            Self::Have(_) => u32_size,
            Self::Bitfield(p) => p.len(),
            Self::Request(_, _, _) => u32_size * 3,
            Self::Piece(_, _, p) => u32_size + u32_size + p.len(),
            Self::Cancel(_, _, _) => u32_size * 3,
        }
    }
    pub fn message_id(&self) -> Option<u8> {
        let id = match self {
            Self::KeepAlive => return None,
            Self::Choke => 0,            // messageID = 0
            Self::Unchoke => 1,          // messageID = 1
            Self::Interested => 2,       // messageID = 2
            Self::NotInterested => 3,    // messageID = 3
            Self::Have(_) => 4,          // messageID = 4
            Self::Bitfield(_) => 5,      // messageID = 5
            Self::Request(_, _, _) => 6, // messageID = 6
            Self::Piece(_, _, _) => 7,   // messageID = 7
            Self::Cancel(_, _, _) => 8,  // messageId = 8
        };

        Some(id)
    }
}

#[derive(Debug)]
pub(crate) struct PeerMessageCodec;

impl Encoder<PeerMessage> for PeerMessageCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: PeerMessage, dst: &mut bytes::BytesMut) -> Result<(), Self::Error> {
        use PeerMessage::*;
        let mut payload = Vec::new();
        let message_id = item.message_id();
        match item {
            KeepAlive | Choke | Unchoke | Interested | NotInterested => {}
            Have(p) => payload.extend(&p.to_be_bytes()),
            Bitfield(p) => payload = p,
            Piece(idx, offset, data) => {
                payload.extend(&idx.to_be_bytes());
                payload.extend(&offset.to_be_bytes());
                payload.extend(data);
            }
            Request(idx, begin, length) | Cancel(idx, begin, length) => {
                payload.extend(&idx.to_be_bytes());
                payload.extend(&begin.to_be_bytes());
                payload.extend(&length.to_be_bytes());
            }
        };

        if let Some(message_id) = message_id {
            let length = payload.len() as u32 + 1;
            dst.put_u32(length);
            dst.put_u8(message_id);
            dst.extend_from_slice(&payload);
        }

        Ok(())
    }
}

impl Decoder for PeerMessageCodec {
    type Item = PeerMessage;

    type Error = std::io::Error;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.remaining() < 4 {
            return Ok(None);
        }

        // Clone the bytes to peek ahead. TODO: Expensive, fix.
        let mut tmp_buf = src.clone();

        let message_length = tmp_buf.get_u32() as usize;
        let length_size = std::mem::size_of::<u32>();

        if src.remaining() > message_length + length_size {
            src.advance(length_size);
            if message_length == 0 {
                // Keep-alive
                return Ok(Some(PeerMessage::KeepAlive));
            }
        } else {
            return Ok(None);
        }

        let message_id = src.get_u8();

        let message = match message_id {
            0 => PeerMessage::Choke,
            1 => PeerMessage::Unchoke,
            2 => PeerMessage::Interested,
            3 => PeerMessage::NotInterested,
            4 => {
                let payload = src.get_u32();
                PeerMessage::Have(payload)
            }
            5 => {
                let mut payload = vec![0; message_length - 1];
                src.copy_to_slice(&mut payload);

                PeerMessage::Bitfield(payload)
            }
            6 => {
                let idx = src.get_u32();
                let begin = src.get_u32();
                let length = src.get_u32();
                PeerMessage::Request(idx, begin, length)
            }
            7 => {
                let idx = src.get_u32();
                let offset = src.get_u32();
                let mut payload = vec![0; message_length - 9];
                src.copy_to_slice(&mut payload);
                PeerMessage::Piece(idx, offset, payload)
            }
            8 => {
                let idx = src.get_u32();
                let begin = src.get_u32();
                let length = src.get_u32();
                PeerMessage::Cancel(idx, begin, length)
            }
            n => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Invalid message ID: {}", n),
                ))
            }
        };

        Ok(Some(message))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn encode_decode_message() {
        let msg = PeerMessage::Request(12, 333, 4);
        let original_handshake = msg.clone();
        let mut codec = PeerMessageCodec;

        let mut bytes = BytesMut::new();
        codec.encode(msg, &mut bytes).unwrap();

        assert_eq!(bytes.len(), 17);

        let round_tripped_handshake = codec.decode(&mut bytes).unwrap().unwrap();

        assert_eq!(original_handshake, round_tripped_handshake);
    }
}
