//! Paletted block storage for one 16×16×16 section.
//!
//! Entries are indexed in Bedrock's SubChunk order, `(x << 8) | (z << 4) | y`, so a Bedrock
//! block storage can be copied in without reordering ([`PalettedStorage::from_packed`]).
//! Internally the entry width is always a power of two, so an entry never straddles a word
//! and a lookup is a multiply, a shift and a mask.

use std::fmt;

/// Entries in one section.
pub const SECTION_VOLUME: usize = 4096;

/// Index of local coordinates (each 0-15) in a section.
#[inline]
pub fn index(x: usize, y: usize, z: usize) -> usize {
    debug_assert!(x < 16 && y < 16 && z < 16);
    (x << 8) | (z << 4) | y
}

/// Why packed storage was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageError {
    /// Bits per entry above 16.
    Bits(u8),
    /// Wrong word count for the bits per entry.
    Words { expected: usize, actual: usize },
    /// The palette is empty, or has more entries than the bits per entry can address.
    Palette(usize),
    /// An entry points past the end of the palette.
    Entry { index: usize, value: u32 },
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Bits(bits) => write!(f, "{bits} bits per block"),
            StorageError::Words { expected, actual } => {
                write!(f, "{actual} words of block storage, expected {expected}")
            }
            StorageError::Palette(len) => write!(f, "palette of {len} entries"),
            StorageError::Entry { index, value } => {
                write!(
                    f,
                    "block {index} has palette index {value} past the palette"
                )
            }
        }
    }
}

impl std::error::Error for StorageError {}

/// Host state ids of one section, as a palette and packed palette indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalettedStorage {
    /// 0 (single value, no words), 1, 2, 4, 8 or 16.
    bits: u8,
    /// Exactly `SECTION_VOLUME * bits` bits. Lookups rely on this.
    words: Box<[u64]>,
    /// Never empty, and every entry in `words` indexes it. Lookups rely on this.
    palette: Vec<u32>,
}

impl PalettedStorage {
    /// Every entry is `id`.
    pub fn single(id: u32) -> Self {
        Self {
            bits: 0,
            words: Box::new([]),
            palette: vec![id],
        }
    }

    /// From one id per entry, in [`index`] order.
    pub fn from_ids(ids: &[u32; SECTION_VOLUME]) -> Self {
        let mut storage = Self::single(ids[0]);
        for (i, &id) in ids.iter().enumerate() {
            storage.set(i, id);
        }
        storage
    }

    /// From a Bedrock block storage: `bits` per entry (0-16), `32 / bits` entries per word
    /// starting at the low bits, entries in [`index`] order, and a palette of host state ids.
    pub fn from_packed(bits: u8, words: &[u32], palette: Vec<u32>) -> Result<Self, StorageError> {
        if bits > 16 {
            return Err(StorageError::Bits(bits));
        }
        if palette.is_empty() || (bits < 16 && palette.len() > 1 << bits) {
            return Err(StorageError::Palette(palette.len()));
        }
        if bits == 0 {
            if !words.is_empty() {
                return Err(StorageError::Words {
                    expected: 0,
                    actual: words.len(),
                });
            }
            return Ok(Self::single(palette[0]));
        }
        let per_word = 32 / bits as usize;
        let expected = SECTION_VOLUME.div_ceil(per_word);
        if words.len() != expected {
            return Err(StorageError::Words {
                expected,
                actual: words.len(),
            });
        }
        let mask = (1u32 << bits) - 1;
        let mut storage = Self {
            bits: 0,
            words: Box::new([]),
            palette,
        };
        storage.resize(bits_for(storage.palette.len()));
        for i in 0..SECTION_VOLUME {
            let value = (words[i / per_word] >> ((i % per_word) * bits as usize)) & mask;
            if value as usize >= storage.palette.len() {
                return Err(StorageError::Entry { index: i, value });
            }
            if storage.bits != 0 {
                storage.write(i, value);
            }
        }
        Ok(storage)
    }

    /// The host state id at `index`.
    #[inline]
    pub fn get(&self, index: usize) -> u32 {
        let value = self.read(index) as usize;
        // SAFETY: every entry indexes the palette (see the field), and 0 does as well.
        unsafe { *self.palette.get_unchecked(value) }
    }

    /// Sets the entry at `index`, growing the palette and the entry width as needed. The
    /// palette never shrinks.
    pub fn set(&mut self, index: usize, id: u32) {
        let value = match self.palette.iter().position(|&p| p == id) {
            Some(value) => value,
            None => {
                self.palette.push(id);
                let bits = bits_for(self.palette.len());
                if bits > self.bits {
                    self.resize(bits);
                }
                self.palette.len() - 1
            }
        };
        if self.bits != 0 {
            self.write(index, value as u32);
        }
    }

    /// Host state ids that may occur (some may no longer be used).
    pub fn palette(&self) -> &[u32] {
        &self.palette
    }

    /// Every entry is the palette's only id, with no packed entries (Java's
    /// `SingleValuePalette`). Setting another id packs the storage for good.
    pub fn is_single_value(&self) -> bool {
        self.bits == 0
    }

    /// The index into [`Self::palette`] of the entry at `index`.
    #[inline]
    pub fn palette_index(&self, index: usize) -> u32 {
        self.read(index)
    }

    #[inline]
    fn read(&self, index: usize) -> u32 {
        if self.bits == 0 {
            return 0;
        }
        debug_assert!(index < SECTION_VOLUME);
        let bits = self.bits as usize;
        let bit = (index & (SECTION_VOLUME - 1)) * bits;
        // SAFETY: `bit` is below `SECTION_VOLUME * bits`, the bits `words` holds.
        let word = unsafe { *self.words.get_unchecked(bit >> 6) };
        ((word >> (bit & 63)) & ((1 << bits) - 1)) as u32
    }

    #[inline]
    fn write(&mut self, index: usize, value: u32) {
        let bits = self.bits as usize;
        let bit = index * bits;
        let shift = bit & 63;
        let word = &mut self.words[bit >> 6];
        *word = (*word & !(((1 << bits) - 1) << shift)) | (u64::from(value) << shift);
    }

    /// Repacks every entry with `bits` per entry.
    fn resize(&mut self, bits: u8) {
        let old = std::mem::replace(
            self,
            Self {
                bits,
                words: vec![0; SECTION_VOLUME * bits as usize / 64].into_boxed_slice(),
                palette: Vec::new(),
            },
        );
        if bits != 0 {
            for i in 0..SECTION_VOLUME {
                self.write(i, old.read(i));
            }
        }
        self.palette = old.palette;
    }
}

/// The smallest power-of-two entry width (0 for one entry) that addresses `palette_len`
/// entries.
fn bits_for(palette_len: usize) -> u8 {
    match palette_len {
        0..=1 => 0,
        2 => 1,
        3..=4 => 2,
        5..=16 => 4,
        17..=256 => 8,
        _ => 16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// xorshift, so the tests need no dependency
    fn rng(seed: &mut u64) -> u64 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    }

    #[test]
    fn index_order_is_xzy() {
        assert_eq!(index(0, 1, 0), 1);
        assert_eq!(index(0, 0, 1), 16);
        assert_eq!(index(1, 0, 0), 256);
        assert_eq!(index(15, 15, 15), 4095);
    }

    #[test]
    fn single_value() {
        let storage = PalettedStorage::single(7);
        assert!((0..SECTION_VOLUME).all(|i| storage.get(i) == 7));
        assert_eq!(storage.palette(), [7]);
    }

    #[test]
    fn set_grows_through_every_width() {
        let mut seed = 0x9e3779b97f4a7c15;
        let mut storage = PalettedStorage::single(0);
        let mut expected = [0u32; SECTION_VOLUME];
        for distinct in [2u64, 3, 5, 17, 300, 5000] {
            for _ in 0..3000 {
                let i = (rng(&mut seed) % SECTION_VOLUME as u64) as usize;
                let id = (rng(&mut seed) % distinct) as u32 * 1000;
                storage.set(i, id);
                expected[i] = id;
            }
            assert!((0..SECTION_VOLUME).all(|i| storage.get(i) == expected[i]));
        }
        assert_eq!(storage.bits, 16);
        let from_ids = PalettedStorage::from_ids(&expected);
        assert!((0..SECTION_VOLUME).all(|i| from_ids.get(i) == expected[i]));
    }

    /// Packs like Bedrock: `32 / bits` entries per word, low bits first.
    fn pack(bits: u8, values: &[u32]) -> Vec<u32> {
        let per_word = 32 / bits as usize;
        let mut words = vec![0u32; SECTION_VOLUME.div_ceil(per_word)];
        for (i, &v) in values.iter().enumerate() {
            words[i / per_word] |= v << ((i % per_word) * bits as usize);
        }
        words
    }

    #[test]
    fn from_packed_every_bedrock_width() {
        let mut seed = 0x243f6a8885a308d3;
        for bits in [1u8, 2, 3, 4, 5, 6, 8, 16] {
            let palette_len = if bits == 16 { 5000 } else { 1usize << bits };
            let palette: Vec<u32> = (0..palette_len as u32).map(|i| i * 7 + 3).collect();
            let values: Vec<u32> = (0..SECTION_VOLUME)
                .map(|_| (rng(&mut seed) % palette_len as u64) as u32)
                .collect();
            let storage = PalettedStorage::from_packed(bits, &pack(bits, &values), palette.clone())
                .unwrap_or_else(|e| panic!("{bits} bits: {e}"));
            for (i, &v) in values.iter().enumerate() {
                assert_eq!(
                    storage.get(i),
                    palette[v as usize],
                    "{bits} bits, entry {i}"
                );
            }
        }
    }

    #[test]
    fn from_packed_rejects_bad_input() {
        assert_eq!(
            PalettedStorage::from_packed(17, &[], vec![0]),
            Err(StorageError::Bits(17))
        );
        assert_eq!(
            PalettedStorage::from_packed(1, &pack(1, &[0; SECTION_VOLUME]), vec![]),
            Err(StorageError::Palette(0))
        );
        assert_eq!(
            PalettedStorage::from_packed(1, &pack(1, &[0; SECTION_VOLUME]), vec![1, 2, 3]),
            Err(StorageError::Palette(3))
        );
        assert_eq!(
            PalettedStorage::from_packed(3, &[0; 10], vec![0]),
            Err(StorageError::Words {
                expected: 410,
                actual: 10
            })
        );
        let mut values = [0u32; SECTION_VOLUME];
        values[100] = 5;
        assert_eq!(
            PalettedStorage::from_packed(3, &pack(3, &values), vec![1, 2]),
            Err(StorageError::Entry {
                index: 100,
                value: 5
            })
        );
        assert_eq!(
            PalettedStorage::from_packed(0, &[], vec![9]),
            Ok(PalettedStorage::single(9))
        );
        let one_entry = PalettedStorage::from_packed(4, &pack(4, &[0; SECTION_VOLUME]), vec![9]);
        assert_eq!(one_entry, Ok(PalettedStorage::single(9)));
    }
}
