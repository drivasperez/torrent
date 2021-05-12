pub trait Bitfield {
    fn has_piece(&self, index: usize) -> bool;
}

pub trait BitfieldMut: Bitfield {
    fn set_piece(&mut self, index: usize);

    fn unset_piece(&mut self, index: usize);
}

impl Bitfield for &[u8] {
    fn has_piece(&self, index: usize) -> bool {
        let byte_idx = index / 8;
        let offset = index % 8;
        self[byte_idx] >> (7 - offset) & 1 != 0
    }
}

impl Bitfield for &mut [u8] {
    fn has_piece(&self, index: usize) -> bool {
        let byte_idx = index / 8;
        let offset = index % 8;
        self[byte_idx] >> (7 - offset) & 1 != 0
    }
}

impl BitfieldMut for &mut [u8] {
    fn set_piece(&mut self, index: usize) {
        let byte_idx = index / 8;
        let offset = index % 8;

        self[byte_idx] |= 1 << (7 - offset);
    }

    fn unset_piece(&mut self, index: usize) {
        let byte_idx = index / 8;
        let offset = index % 8;

        self[byte_idx] &= 0 << (7 - offset);
    }
}

impl<const N: usize> Bitfield for [u8; N] {
    fn has_piece(&self, index: usize) -> bool {
        let byte_idx = index / 8;
        let offset = index % 8;
        self[byte_idx] >> (7 - offset) & 1 != 0
    }
}

impl<const N: usize> BitfieldMut for [u8; N] {
    fn set_piece(&mut self, index: usize) {
        let byte_idx = index / 8;
        let offset = index % 8;

        self[byte_idx] |= 1 << (7 - offset);
    }

    fn unset_piece(&mut self, index: usize) {
        let byte_idx = index / 8;
        let offset = index % 8;

        self[byte_idx] &= 0 << (7 - offset);
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn set_piece_on_bitfield() {
        let bitfield = &mut [0u8; 8];

        bitfield.set_piece(3);

        assert!(bitfield.has_piece(3));
    }

    #[test]
    fn unset_piece_on_bitfield() {
        let bitfield = &mut [0u8; 8];

        bitfield.set_piece(3);

        assert!(bitfield.has_piece(3));

        bitfield.unset_piece(3);
        assert!(bitfield.has_piece(3) == false);
    }

    #[test]
    fn set_unset_on_slice() {
        let mut v = vec![0, 0, 0];

        let mut bitfield = &mut v[0..2];

        bitfield.set_piece(3);

        assert!(bitfield.has_piece(3));

        bitfield.unset_piece(3);
        assert!(bitfield.has_piece(3) == false);
    }
}
