pub struct Bitfield<'a>(&'a mut [u8]);

impl<'a> Bitfield<'a> {
    pub fn new(slice: &'a mut [u8]) -> Self {
        Self(slice)
    }

    pub fn has_piece(&self, index: usize) -> bool {
        let byte_idx = index / 8;
        let offset = index % 8;
        self.0[byte_idx] >> (7 - offset) & 1 != 0
    }

    pub fn set_piece(&mut self, index: usize) {
        let byte_idx = index / 8;
        let offset = index % 8;

        self.0[byte_idx] |= 1 << (7 - offset);
    }

    pub fn unset_piece(&mut self, index: usize) {
        let byte_idx = index / 8;
        let offset = index % 8;

        self.0[byte_idx] &= 0 << (7 - offset);
    }
}
#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn set_piece_on_bitfield() {
        let slice = &mut [0u8; 8];

        let mut bitfield = Bitfield::new(slice);

        bitfield.set_piece(3);

        assert!(bitfield.has_piece(3));
    }

    #[test]
    fn unset_piece_on_bitfield() {
        let slice = &mut [0u8; 8];

        let mut bitfield = Bitfield::new(slice);

        bitfield.set_piece(3);

        assert!(bitfield.has_piece(3));

        bitfield.unset_piece(3);
        assert!(bitfield.has_piece(3) == false);
    }
}
