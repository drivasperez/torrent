use tokio::net::TcpStream;
use tokio_util::codec::{Framed, FramedParts};

use super::{handshake::HandshakeCodec, message::PeerMessageCodec};

#[derive(Debug)]
pub(crate) enum PeerStream {
    Handshake(Option<Framed<TcpStream, HandshakeCodec>>),
    Message(Option<Framed<TcpStream, PeerMessageCodec>>),
}

impl PeerStream {
    pub fn get_handshake_framed(&mut self) -> &mut Framed<TcpStream, HandshakeCodec> {
        match self {
            Self::Handshake(stream) => stream.as_mut().unwrap(),
            Self::Message(stream) => {
                let stream = stream.take().unwrap();
                *self = Self::make_handshake(stream);
                if let Self::Handshake(Some(stream)) = self {
                    stream
                } else {
                    unreachable!();
                }
            }
        }
    }

    pub fn get_message_framed(&mut self) -> &mut Framed<TcpStream, PeerMessageCodec> {
        match self {
            Self::Message(stream) => stream.as_mut().unwrap(),
            Self::Handshake(stream) => {
                let stream = stream.take().unwrap();
                *self = Self::make_message(stream);
                if let Self::Message(Some(stream)) = self {
                    stream
                } else {
                    unreachable!();
                }
            }
        }
    }

    fn make_handshake<T>(stream: Framed<TcpStream, T>) -> Self {
        let old_parts = stream.into_parts();
        let mut new_parts = FramedParts::new(old_parts.io, HandshakeCodec);
        // reuse buffers of previous codec
        new_parts.read_buf = old_parts.read_buf;
        new_parts.write_buf = old_parts.write_buf;
        let stream = Framed::from_parts(new_parts);

        Self::Handshake(Some(stream))
    }

    fn make_message<T>(stream: Framed<TcpStream, T>) -> Self {
        let old_parts = stream.into_parts();
        let mut new_parts = FramedParts::new(old_parts.io, PeerMessageCodec);
        // reuse buffers of previous codec
        new_parts.read_buf = old_parts.read_buf;
        new_parts.write_buf = old_parts.write_buf;
        let stream = Framed::from_parts(new_parts);

        Self::Message(Some(stream))
    }
}
