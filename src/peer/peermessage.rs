use anyhow::anyhow;
use std::convert::TryFrom;

pub enum PeerMessageId {
    Choke,         // messageID = 0
    Unchoke,       // messageID = 1
    Interested,    // messageID = 2
    NotInterested, // messageID = 3
    Have,          // messageID = 4
    Bitfield,      // messageID = 5
    Request,       // messageID = 6
    Piece,         // messageID = 7
    Cancel,        // messageId = 8
}

impl PeerMessageId {
    pub fn message_id(&self) -> u8 {
        match self {
            Self::Choke => 0,         // messageID = 0
            Self::Unchoke => 1,       // messageID = 1
            Self::Interested => 2,    // messageID = 2
            Self::NotInterested => 3, // messageID = 3
            Self::Have => 4,          // messageID = 4
            Self::Bitfield => 5,      // messageID = 5
            Self::Request => 6,       // messageID = 6
            Self::Piece => 7,         // messageID = 7
            Self::Cancel => 8,        // messageId = 8
        }
    }
}

impl TryFrom<u8> for PeerMessageId {
    type Error = anyhow::Error;

    fn try_from(i: u8) -> Result<Self, Self::Error> {
        let val = match i {
            0 => Self::Choke,
            1 => Self::Unchoke,
            2 => Self::Interested,
            3 => Self::NotInterested,
            4 => Self::Have,
            5 => Self::Bitfield,
            6 => Self::Request,
            7 => Self::Piece,
            8 => Self::Cancel,
            _ => return Err(anyhow!("Couldn't parse message")),
        };

        Ok(val)
    }
}

pub struct PeerMessage {
    id: PeerMessageId,
    payload: Vec<u8>, // Maybe should be &'a [u8]? Or Bytes?
}
