use bytes::{Buf, BufMut, Bytes};
use std::io::Read;
use tokio_util::codec::{Decoder, Encoder};

#[derive(Debug, Clone, PartialEq)]
pub enum PeerMessage {
    Choke,                  // messageID = 0
    Unchoke,                // messageID = 1
    Interested,             // messageID = 2
    NotInterested,          // messageID = 3
    Have([u8; 4]),          // messageID = 4
    Bitfield(Bytes),        // messageID = 5
    Request(u32, u32, u32), // messageID = 6
    Piece(Bytes),           // messageID = 7
    Cancel(u32, u32, u32),  // messageId = 8
}

impl PeerMessage {
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
            Self::Have(p) => Some(Bytes::copy_from_slice(p)),
            Self::Bitfield(p) | Self::Piece(p) => Some(p.clone()),
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

        if src.remaining() > message_length + 4 {
            src.advance(4);
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
                let mut payload = [0; 4];
                src.copy_to_slice(&mut payload);
                PeerMessage::Have(payload)
            }
            5 => {
                let payload = src.to_vec();
                PeerMessage::Bitfield(Bytes::from(payload))
            }
            6 => {
                let idx = src.get_u32();
                let begin = src.get_u32();
                let length = src.get_u32();
                PeerMessage::Request(idx, begin, length)
            }
            7 => {
                let payload = src.to_vec();
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
