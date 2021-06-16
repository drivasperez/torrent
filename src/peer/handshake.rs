use bytes::{Buf, BufMut, BytesMut};
use std::convert::TryInto;
use tokio_util::codec::{Decoder, Encoder};

pub(crate) const PROTOCOL_NAME: [u8; 19] = *b"BitTorrent protocol";

#[derive(Debug, PartialEq, Clone)]
pub struct Handshake {
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
    pub protocol_name: [u8; 19],
    pub reserved: [u8; 8],
}

#[derive(Debug)]
pub struct HandshakeCodec;

impl Handshake {
    pub fn new(info_hash: &[u8; 20], peer_id: &[u8; 20]) -> Self {
        Self {
            info_hash: info_hash.to_owned(),
            peer_id: peer_id.to_owned(),
            protocol_name: PROTOCOL_NAME,
            reserved: [0_u8; 8],
        }
    }
}

impl Encoder<Handshake> for HandshakeCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: Handshake, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.put_u8(item.protocol_name.len().try_into().unwrap());
        dst.extend_from_slice(&item.protocol_name);
        dst.extend_from_slice(&[0u8; 8]);
        dst.extend_from_slice(&item.info_hash);
        dst.extend_from_slice(&item.peer_id);

        Ok(())
    }
}

impl Decoder for HandshakeCodec {
    type Item = Handshake;

    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        let prot_len = src[0] as usize;
        if prot_len != PROTOCOL_NAME.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Handshake must have the string \"BitTorrent protocol\"",
            ));
        }

        let payload_len = prot_len + 8 + 20 + 20;
        if src.remaining() > payload_len {
            // we have the full message in the buffer so advance the buffer
            // cursor past the message length header
            src.advance(1);
        } else {
            return Ok(None);
        }

        // protocol string
        let mut protocol_name = [0; 19];
        src.copy_to_slice(&mut protocol_name);
        // reserved field
        let mut reserved = [0; 8];
        src.copy_to_slice(&mut reserved);
        // info hash
        let mut info_hash = [0; 20];
        src.copy_to_slice(&mut info_hash);
        // peer id
        let mut peer_id = [0; 20];
        src.copy_to_slice(&mut peer_id);

        Ok(Some(Handshake {
            info_hash,
            peer_id,
            protocol_name,
            reserved,
        }))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn encode_decode_handshake() {
        let handshake = Handshake::new(&[1u8; 20], b"Daniel Rivas12345678");
        let original_handshake = handshake.clone();
        let mut codec = HandshakeCodec;

        let mut bytes = BytesMut::new();
        codec.encode(handshake, &mut bytes).unwrap();

        assert_eq!(bytes.len(), 68);

        let round_tripped_handshake = codec.decode(&mut bytes).unwrap().unwrap();

        assert_eq!(original_handshake, round_tripped_handshake);
    }
}
