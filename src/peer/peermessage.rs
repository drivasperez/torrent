use bytes::{Buf, BufMut, Bytes};
use tokio_util::codec::{Decoder, Encoder};

#[derive(Debug, Clone, PartialEq)]
pub enum PeerMessage {
    Choke,                  // messageID = 0
    Unchoke,                // messageID = 1
    Interested,             // messageID = 2
    NotInterested,          // messageID = 3
    Have(u32),              // messageID = 4
    Bitfield(Vec<u8>),      // messageID = 5
    Request(u32, u32, u32), // messageID = 6
    Piece(Bytes),           // messageID = 7
    Cancel(u32, u32, u32),  // messageId = 8
}

impl std::fmt::Display for PeerMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
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
            Self::Piece(b) => format!("Piece (len: {})", b.len()),
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
        match self {
            Self::Choke | Self::Unchoke | Self::Interested | Self::NotInterested => 0,
            Self::Have(_) => std::mem::size_of::<u32>(),
            Self::Bitfield(p) => p.len(),
            Self::Request(_, _, _) => std::mem::size_of::<u32>() * 3,
            Self::Piece(p) => p.len(),
            Self::Cancel(_, _, _) => std::mem::size_of::<u32>() * 3,
        }
    }
    pub fn message_id(&self) -> u8 {
        match self {
            Self::Choke => 0,            // messageID = 0
            Self::Unchoke => 1,          // messageID = 1
            Self::Interested => 2,       // messageID = 2
            Self::NotInterested => 3,    // messageID = 3
            Self::Have(_) => 4,          // messageID = 4
            Self::Bitfield(_) => 5,      // messageID = 5
            Self::Request(_, _, _) => 6, // messageID = 6
            Self::Piece(_) => 7,         // messageID = 7
            Self::Cancel(_, _, _) => 8,  // messageId = 8
        }
    }

    pub fn payload_bytes(&self) -> Option<Bytes> {
        match self {
            Self::Choke | Self::Unchoke | Self::Interested | Self::NotInterested => None,
            Self::Have(p) => Some(Bytes::copy_from_slice(&p.to_be_bytes())),
            Self::Bitfield(p) => Some(p.clone().into()),
            Self::Piece(p) => Some(p.clone()),
            Self::Request(idx, begin, length) | Self::Cancel(idx, begin, length) => {
                let mut bytes = Vec::new();

                bytes.extend_from_slice(&idx.to_be_bytes());
                bytes.extend_from_slice(&begin.to_be_bytes());
                bytes.extend_from_slice(&length.to_be_bytes());

                Some(Bytes::from(bytes))
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct PeerMessageCodec;

impl Encoder<PeerMessage> for PeerMessageCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: PeerMessage, dst: &mut bytes::BytesMut) -> Result<(), Self::Error> {
        let payload = item.payload_bytes();

        if let Some(bytes) = payload {
            let length = bytes.len() as u32 + 1;
            dst.put_u32(length);
            dst.put_u8(item.message_id());
            dst.extend_from_slice(&bytes);
        } else {
            dst.put_u32(1);
            dst.put_u8(item.message_id());
        };

        Ok(())
    }
}

impl Decoder for PeerMessageCodec {
    type Item = PeerMessage;

    type Error = std::io::Error;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        let mut tmp_buf = src.clone();
        let message_length = tmp_buf.get_u32() as usize;

        let length_size = std::mem::size_of::<u32>();
        if src.remaining() > message_length + length_size {
            src.advance(length_size);
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
                let mut payload = vec![0; message_length - 1];
                src.copy_to_slice(&mut payload);
                PeerMessage::Piece(Bytes::from(payload))
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
