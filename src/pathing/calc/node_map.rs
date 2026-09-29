//! Not in upstream: the search's `longHash` → node map, standing in for fastutil's
//! `Long2ObjectOpenHashMap`. Like fastutil it is open addressing with linear probing, sized
//! by `pathingMapDefaultSize` and `pathingMapLoadFactor`. Unlike fastutil, a key sits next to
//! its value, so a lookup usually touches one cache line. The map is never iterated, so its
//! order cannot reach a result.

/// Marks an empty slot. Keys can be any `i64`, node indices can't be this.
const EMPTY: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
struct Slot {
    key: i64,
    node: u32,
}

const EMPTY_SLOT: Slot = Slot {
    key: 0,
    node: EMPTY,
};

/// `longHash` → index in the search's node arena.
#[derive(Clone, Debug)]
pub struct NodeMap {
    /// A power of two long.
    slots: Box<[Slot]>,
    /// `64 - log2(slots.len())`: the slot of a key is the top bits of its Fibonacci hash.
    shift: u32,
    len: usize,
    load_factor: f32,
    /// Grows when `len` reaches this (fastutil's `maxFill`).
    max_fill: usize,
}

impl NodeMap {
    /// `new Long2ObjectOpenHashMap<>(expected, f)`: room for `expected` entries before it
    /// grows, at most `load_factor` full. A factor outside `(0, 1)`, which fastutil rejects,
    /// falls back to 0.75.
    pub fn new(expected: usize, load_factor: f32) -> Self {
        let load_factor = if load_factor > 0.0 && load_factor < 1.0 {
            load_factor
        } else {
            0.75
        };
        // fastutil's arraySize
        let len = ((expected as f64 / f64::from(load_factor)).ceil() as usize)
            .max(2)
            .next_power_of_two();
        Self::with_slots(len, load_factor)
    }

    fn with_slots(len: usize, load_factor: f32) -> Self {
        Self {
            slots: vec![EMPTY_SLOT; len].into_boxed_slice(),
            shift: 64 - len.trailing_zeros(),
            len: 0,
            load_factor,
            // fastutil's maxFill
            max_fill: ((len as f64 * f64::from(load_factor)).ceil() as usize).min(len - 1),
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    fn slot_of(&self, key: i64) -> usize {
        ((key as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> self.shift) as usize
    }

    /// Starts loading `key`'s slot into the cache, for a lookup of `key` soon after.
    #[inline]
    pub fn prefetch(&self, key: i64) {
        #[cfg(target_arch = "x86_64")]
        {
            use std::arch::x86_64::{_MM_HINT_T0, _mm_prefetch};
            let slot = self.slots[self.slot_of(key)..].as_ptr();
            // SAFETY: a prefetch has no effect but on the cache, and `slot` is in bounds anyway.
            unsafe { _mm_prefetch::<_MM_HINT_T0>(slot.cast()) };
        }
        #[cfg(not(target_arch = "x86_64"))]
        let _ = key;
    }

    /// The node at `key`, or `insert()`'s node, now at `key`.
    #[inline]
    pub fn get_or_insert_with(&mut self, key: i64, insert: impl FnOnce() -> u32) -> u32 {
        let mask = self.slots.len() - 1;
        let mut i = self.slot_of(key);
        loop {
            let slot = self.slots[i];
            if slot.node == EMPTY {
                break;
            }
            if slot.key == key {
                return slot.node;
            }
            i = (i + 1) & mask;
        }
        let node = insert();
        assert_ne!(node, EMPTY, "too many path nodes");
        self.slots[i] = Slot { key, node };
        self.len += 1;
        if self.len >= self.max_fill {
            self.grow();
        }
        node
    }

    #[cold]
    fn grow(&mut self) {
        let old = std::mem::replace(
            self,
            Self::with_slots(self.slots.len() * 2, self.load_factor),
        );
        let mask = self.slots.len() - 1;
        for slot in old.slots.iter().filter(|slot| slot.node != EMPTY) {
            let mut i = self.slot_of(slot.key);
            while self.slots[i].node != EMPTY {
                i = (i + 1) & mask;
            }
            self.slots[i] = *slot;
        }
        self.len = old.len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_once_and_grows() {
        let mut map = NodeMap::new(2, 0.75);
        let mut next = 0;
        // keys that share low and high bits, and i64::MIN / 0, which a sentinel key would hit
        let keys: Vec<i64> = (0..5000i64)
            .map(|i| i << 32)
            .chain((0..5000).map(|i| i * 0x1_0000_0001))
            .chain([i64::MIN, 0, -1, i64::MAX])
            .collect();
        let mut expected = std::collections::HashMap::new();
        for &key in &keys {
            let node = map.get_or_insert_with(key, || {
                next += 1;
                next - 1
            });
            assert_eq!(*expected.entry(key).or_insert(node), node, "{key}");
        }
        assert_eq!(map.len(), expected.len());
        for &key in &keys {
            assert_eq!(
                map.get_or_insert_with(key, || panic!("{key}")),
                expected[&key]
            );
        }
        assert!(map.len() < map.max_fill && map.max_fill < map.slots.len());
    }

    #[test]
    fn sizes_like_fastutil() {
        // new Long2ObjectOpenHashMap<>(1024, 0.75f): n = 2048, maxFill = 1536
        let map = NodeMap::new(1024, 0.75);
        assert_eq!((map.slots.len(), map.max_fill), (2048, 1536));
        let map = NodeMap::new(0, 0.5);
        assert_eq!((map.slots.len(), map.max_fill), (2, 1));
        assert_eq!(NodeMap::new(10, f32::NAN).load_factor, 0.75);
    }
}
