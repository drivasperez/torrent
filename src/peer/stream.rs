use tokio::net::TcpStream;
use tokio_util::codec::{Framed, FramedParts};

use super::{handshake::HandshakeCodec, message::PeerMessageCodec};

pub(crate) type HandshakeStream = Framed<TcpStream, HandshakeCodec>;
pub(crate) type MessageStream = Framed<TcpStream, PeerMessageCodec>;

pub(crate) fn make_message_stream(stream: HandshakeStream) -> MessageStream {
    let old_parts = stream.into_parts();
    let mut new_parts = FramedParts::new(old_parts.io, PeerMessageCodec);
    // reuse buffers of previous codec
    new_parts.read_buf = old_parts.read_buf;
    new_parts.write_buf = old_parts.write_buf;
    let stream = Framed::from_parts(new_parts);

    stream
}
