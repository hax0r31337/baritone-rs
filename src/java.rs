//! Java semantics that Rust does not provide out of the box: numerics, monitors, the executor
//! and `HashMap` iteration order.
//!
//! Rust's `f64::min`/`f64::max` drop NaN and leave the sign of zero unspecified, while Java's
//! `Math.min`/`Math.max` propagate NaN and order `-0.0` below `0.0`. Heuristics and costs are
//! compared bit-exactly against upstream, so ported code uses these helpers wherever Java calls
//! `Math.min`/`Math.max` on floating point values.

use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::{Arc, Condvar, LazyLock, Mutex, PoisonError};
use std::thread::{self, ThreadId};
use std::time::Instant;

/// A Java monitor (`synchronized`): a lock the thread holding it can take again. The data is
/// shared (`&T`) with whoever holds it; put a `RefCell` inside for mutable data.
pub struct ReentrantMutex<T> {
    /// The owner and how many times it took the lock.
    state: Mutex<(Option<ThreadId>, usize)>,
    released: Condvar,
    data: T,
}

// SAFETY: `data` is only reachable through a guard, and guards exist on one thread at a time
// (the owner), so `T` is never shared between threads; it only moves between them.
unsafe impl<T: Send> Sync for ReentrantMutex<T> {}

impl<T> ReentrantMutex<T> {
    pub fn new(data: T) -> Self {
        Self {
            state: Mutex::new((None, 0)),
            released: Condvar::new(),
            data,
        }
    }

    /// Enters the monitor, waiting while another thread holds it.
    pub fn lock(&self) -> ReentrantMutexGuard<'_, T> {
        let me = thread::current().id();
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            match state.0 {
                None => {
                    *state = (Some(me), 1);
                    break;
                }
                Some(owner) if owner == me => {
                    state.1 += 1;
                    break;
                }
                Some(_) => {
                    state = self
                        .released
                        .wait(state)
                        .unwrap_or_else(PoisonError::into_inner);
                }
            }
        }
        ReentrantMutexGuard {
            mutex: self,
            _not_send: PhantomData,
        }
    }

    /// `Thread.holdsLock(lock)`
    pub fn is_held_by_current_thread(&self) -> bool {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.0 == Some(thread::current().id())
    }
}

/// Held while inside the monitor; leaving the scope exits it once.
pub struct ReentrantMutexGuard<'a, T> {
    mutex: &'a ReentrantMutex<T>,
    /// Exits must happen on the thread that entered.
    _not_send: PhantomData<*const ()>,
}

impl<T> Deref for ReentrantMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.mutex.data
    }
}

impl<T> Drop for ReentrantMutexGuard<'_, T> {
    fn drop(&mut self) {
        let mut state = self
            .mutex
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.1 -= 1;
        if state.1 == 0 {
            state.0 = None;
            self.mutex.released.notify_one();
        }
    }
}

/// `java.lang.Math.min(double, double)`
#[inline]
pub fn min_f64(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && b.to_bits() == (-0.0f64).to_bits() {
        return b;
    }
    if a <= b { a } else { b }
}

/// `java.lang.Math.max(double, double)`
#[inline]
pub fn max_f64(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && a.to_bits() == (-0.0f64).to_bits() {
        return b;
    }
    if a >= b { a } else { b }
}

/// `java.lang.Math.min(float, float)`
#[inline]
pub fn min_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && b.to_bits() == (-0.0f32).to_bits() {
        return b;
    }
    if a <= b { a } else { b }
}

/// `java.lang.Math.max(float, float)`
#[inline]
pub fn max_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && a.to_bits() == (-0.0f32).to_bits() {
        return b;
    }
    if a >= b { a } else { b }
}

/// `Double.compare(double, double)`: `-0.0 < 0.0`, and NaN (any NaN) is above everything
/// and equal to itself.
pub fn double_compare(a: f64, b: f64) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => a.total_cmp(&b),
    }
}

/// `java.lang.Math.round(float)`: the closest `int`, ties toward positive infinity, NaN to 0,
/// saturating.
pub fn round_f32(a: f32) -> i32 {
    const SIGNIFICAND_WIDTH: i32 = 24;
    const EXP_BIAS: i32 = 127;
    const EXP_BIT_MASK: i32 = 0x7F80_0000;
    const SIGNIF_BIT_MASK: i32 = 0x007F_FFFF;
    let int_bits = a.to_bits() as i32;
    let biased_exp = (int_bits & EXP_BIT_MASK) >> (SIGNIFICAND_WIDTH - 1);
    let shift = (SIGNIFICAND_WIDTH - 2 + EXP_BIAS) - biased_exp;
    if shift & -32 == 0 {
        // shift >= 0 && shift < 32
        let mut r = (int_bits & SIGNIF_BIT_MASK) | (SIGNIF_BIT_MASK + 1);
        if int_bits < 0 {
            r = -r;
        }
        ((r >> shift) + 1) >> 1
    } else {
        a as i32
    }
}

/// `DoubleStream.average()`: the compensated (Kahan) sum over the count, as the JDK computes
/// it. `None` for no values.
pub fn average(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    // [high-order sum, compensation, count, simple sum]
    let mut ll = [0.0f64; 4];
    for d in values {
        ll[2] += 1.0;
        // Collectors.sumWithCompensation
        let tmp = d - ll[1];
        let sum = ll[0];
        let velvel = sum + tmp; // Little wolf of rounding error
        ll[1] = (velvel - sum) - tmp;
        ll[0] = velvel;
        ll[3] += d;
    }
    if ll[2] > 0.0 {
        // Collectors.computeFinalSum
        let tmp = ll[0] - ll[1];
        let simple_sum = ll[3];
        let sum = if tmp.is_nan() && simple_sum.is_infinite() {
            simple_sum
        } else {
            tmp
        };
        Some(sum / ll[2])
    } else {
        None
    }
}

/// `System.currentTimeMillis()` where upstream uses it to measure time: milliseconds on a
/// monotonic clock with an arbitrary origin, so only differences mean anything (upstream's
/// wall clock can jump).
pub fn current_time_millis() -> i64 {
    static ORIGIN: LazyLock<Instant> = LazyLock::new(Instant::now);
    ORIGIN.elapsed().as_millis() as i64
}

/// `java.lang.IllegalArgumentException`, where upstream rejects input that comes from the
/// user (block selectors, block names): the port returns it as an error, with upstream's
/// message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IllegalArgumentException(pub String);

impl fmt::Display for IllegalArgumentException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for IllegalArgumentException {}

/// `String.split(String)` with a one-character delimiter that is not a regex metacharacter:
/// the pieces between the delimiters without the trailing empty ones. A string without the
/// delimiter is one piece, even an empty one.
pub fn split(s: &str, delimiter: char) -> Vec<&str> {
    let mut parts: Vec<&str> = s.split(delimiter).collect();
    if parts.len() == 1 {
        return parts;
    }
    while parts.last() == Some(&"") {
        parts.pop();
    }
    parts
}

/// The `java.util.concurrent.Executor` background work runs on (upstream:
/// `Baritone.getExecutor()`, a thread pool that starts a thread whenever none is idle). Every
/// task runs on a thread of its own. Clones share the count of running tasks.
#[derive(Clone, Debug, Default)]
pub struct Executor {
    running: Arc<(Mutex<usize>, Condvar)>,
}

impl Executor {
    pub fn new() -> Self {
        Self::default()
    }

    /// `execute(Runnable)`. A task that panics ends its thread, like an exception thrown in a
    /// pool thread.
    pub fn execute(&self, task: impl FnOnce() + Send + 'static) {
        /// Counts the task as finished when its thread ends, however it ends.
        struct Running(Arc<(Mutex<usize>, Condvar)>);

        impl Drop for Running {
            fn drop(&mut self) {
                let (count, finished) = &*self.0;
                *count.lock().unwrap_or_else(PoisonError::into_inner) -= 1;
                finished.notify_all();
            }
        }

        *self
            .running
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;
        let running = Running(Arc::clone(&self.running));
        // if the thread cannot start, the task and `running` are dropped here
        let _ = thread::Builder::new()
            .name("Baritone Executor".to_owned())
            .spawn(move || {
                let _running = running;
                task();
            });
    }

    /// Blocks until no task is running: for hosts and tests that need a tick's background
    /// work done before going on.
    pub fn wait_until_idle(&self) {
        let (count, finished) = &*self.running;
        let mut count = count.lock().unwrap_or_else(PoisonError::into_inner);
        while *count > 0 {
            count = finished.wait(count).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// A `java.util.HashMap`, where upstream iterates one: iteration follows Java's order, which
/// is the order of the table's buckets and, within a bucket, insertion order. The bucket
/// depends on the key's `hashCode()` (the `hash` function the map is made with) and on the
/// table's capacity, which grows like Java's and never shrinks: past the load factor, and when
/// a bucket of a table smaller than 64 gets its 9th entry. Buckets that Java turns into trees
/// (a 9th entry in one bucket of a table of 64 or more) keep insertion order here, which
/// Java's does not always. Lookups are linear: the maps this is used for stay small.
#[derive(Clone)]
pub struct JavaHashMap<K, V> {
    hash: fn(&K) -> i32,
    /// In insertion order.
    entries: Vec<(K, V)>,
    /// The table's length, 0 before it is allocated.
    capacity: usize,
    /// The size above which the table grows; the initial capacity while there is no table.
    threshold: usize,
}

impl<K: PartialEq + fmt::Debug, V: fmt::Debug> fmt::Debug for JavaHashMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K: PartialEq, V> JavaHashMap<K, V> {
    const DEFAULT_INITIAL_CAPACITY: usize = 16;
    const LOAD_FACTOR: f32 = 0.75;
    const TREEIFY_THRESHOLD: usize = 8;
    const MIN_TREEIFY_CAPACITY: usize = 64;

    /// `new HashMap<>()`, for keys whose `hashCode()` is `hash`.
    pub fn new(hash: fn(&K) -> i32) -> Self {
        Self {
            hash,
            entries: Vec::new(),
            capacity: 0,
            threshold: 0,
        }
    }

    /// `new HashMap<>(other)`: sized for `other`, filled in `other`'s order.
    pub fn copy_of(other: &Self) -> Self
    where
        K: Clone,
        V: Clone,
    {
        let mut map = Self::new(other.hash);
        let s = other.len();
        if s > 0 {
            // pre-size
            let t = (s as f64 / Self::LOAD_FACTOR as f64).ceil() as usize;
            if t > map.threshold {
                map.threshold = t.next_power_of_two();
            }
            for (k, v) in other.iter() {
                map.insert(k.clone(), v.clone());
            }
        }
        map
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    /// `put(K, V)`: a new key goes last in its bucket, an existing key keeps its place.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.capacity == 0 {
            self.resize();
        }
        if let Some((_, v)) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            return Some(std::mem::replace(v, value));
        }
        let bucket = self.bucket(&key);
        let bin_count = self
            .entries
            .iter()
            .filter(|(k, _)| self.bucket(k) == bucket)
            .count();
        self.entries.push((key, value));
        // treeifyBin: a small table grows instead of making the bucket a tree
        if bin_count >= Self::TREEIFY_THRESHOLD && self.capacity < Self::MIN_TREEIFY_CAPACITY {
            self.resize();
        }
        if self.entries.len() > self.threshold {
            self.resize();
        }
        None
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let index = self.entries.iter().position(|(k, _)| k == key)?;
        Some(self.entries.remove(index).1)
    }

    /// `clear()`: the table keeps its capacity.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// The entries in Java's iteration order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        // stable: insertion order within a bucket
        order.sort_by_key(|&i| self.bucket(&self.entries[i].0));
        order.into_iter().map(|i| {
            let (k, v) = &self.entries[i];
            (k, v)
        })
    }

    /// `keySet()`, in Java's iteration order.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }

    /// The key's bucket in the current table: `(h = key.hashCode()) ^ (h >>> 16)`, masked.
    fn bucket(&self, key: &K) -> usize {
        let h = (self.hash)(key) as u32;
        (h ^ (h >> 16)) as usize & self.capacity.wrapping_sub(1)
    }

    fn resize(&mut self) {
        let old_cap = self.capacity;
        let old_thr = self.threshold;
        let mut new_thr = 0;
        let new_cap;
        if old_cap > 0 {
            new_cap = old_cap << 1;
            if old_cap >= Self::DEFAULT_INITIAL_CAPACITY {
                new_thr = old_thr << 1; // double threshold
            }
        } else if old_thr > 0 {
            // initial capacity was placed in threshold
            new_cap = old_thr;
        } else {
            // zero initial threshold signifies using defaults
            new_cap = Self::DEFAULT_INITIAL_CAPACITY;
            new_thr = (Self::DEFAULT_INITIAL_CAPACITY as f32 * Self::LOAD_FACTOR) as usize;
        }
        if new_thr == 0 {
            new_thr = (new_cap as f32 * Self::LOAD_FACTOR) as usize;
        }
        self.capacity = new_cap;
        self.threshold = new_thr;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_propagates() {
        assert!(min_f64(f64::NAN, 1.0).is_nan());
        assert!(min_f64(1.0, f64::NAN).is_nan());
        assert!(max_f64(f64::NAN, 1.0).is_nan());
        assert!(max_f64(1.0, f64::NAN).is_nan());
        assert!(min_f32(f32::NAN, 1.0).is_nan());
        assert!(max_f32(1.0, f32::NAN).is_nan());
    }

    #[test]
    fn signed_zero() {
        assert!(min_f64(0.0, -0.0).is_sign_negative());
        assert!(min_f64(-0.0, 0.0).is_sign_negative());
        assert!(max_f64(0.0, -0.0).is_sign_positive());
        assert!(max_f64(-0.0, 0.0).is_sign_positive());
        assert!(min_f32(0.0, -0.0).is_sign_negative());
        assert!(max_f32(-0.0, 0.0).is_sign_positive());
    }

    #[test]
    fn reentrant_mutex() {
        use std::cell::Cell;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let m = Arc::new(ReentrantMutex::new(Cell::new(0)));
        assert!(!m.is_held_by_current_thread());
        let outer = m.lock();
        let inner = m.lock();
        inner.set(1);
        assert!(m.is_held_by_current_thread());
        drop(inner);
        assert!(m.is_held_by_current_thread());

        let entered = Arc::new(AtomicBool::new(false));
        let other = {
            let m = Arc::clone(&m);
            let entered = Arc::clone(&entered);
            std::thread::spawn(move || {
                let guard = m.lock();
                entered.store(true, Ordering::SeqCst);
                guard.get()
            })
        };
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(!entered.load(Ordering::SeqCst), "the other thread waits");
        outer.set(2);
        drop(outer);
        assert_eq!(other.join().unwrap(), 2);
        assert!(!m.is_held_by_current_thread());
    }

    #[test]
    fn round() {
        assert_eq!(round_f32(0.5), 1);
        assert_eq!(round_f32(-0.5), 0);
        assert_eq!(round_f32(-1.5), -1);
        assert_eq!(round_f32(2.4999998), 2);
        assert_eq!(round_f32(-2.5), -2);
        assert_eq!(round_f32(f32::NAN), 0);
        assert_eq!(round_f32(1e20), i32::MAX);
        assert_eq!(round_f32(-1e20), i32::MIN);
        assert_eq!(round_f32(0.49999997), 0);
        assert_eq!(round_f32(8388609.0), 8388609);
    }

    #[test]
    fn compensated_average() {
        assert_eq!(average([]), None);
        assert_eq!(average([1.0, 2.0]), Some(1.5));
        // the compensation recovers what a plain sum loses
        assert_eq!(average([1e16, 1.0, 1.0, -1e16]), Some(0.5));
        assert_eq!(average([f64::INFINITY, f64::INFINITY]), Some(f64::INFINITY));
    }

    /// Orders printed by a JVM (OpenJDK 25) for the same operations on `HashMap<BlockPos, _>`.
    #[test]
    fn java_hash_map_order() {
        use crate::api::utils::BetterBlockPos;

        fn parse(s: &str) -> Vec<BetterBlockPos> {
            s.split(' ')
                .map(|p| {
                    let c: Vec<i32> = p.split(',').map(|c| c.parse().unwrap()).collect();
                    BetterBlockPos::new(c[0], c[1], c[2])
                })
                .collect()
        }
        fn keys(m: &JavaHashMap<BetterBlockPos, i32>) -> Vec<BetterBlockPos> {
            m.keys().copied().collect()
        }
        let hash: fn(&BetterBlockPos) -> i32 = BetterBlockPos::block_pos_hash_code;

        let added = parse(
            "-10,3,-12 -16,0,-15 5,8,19 -7,2,2 16,2,16 12,6,-10 3,9,-20 3,6,13 -17,1,10 \
             -2,7,-14 -10,6,10 -15,7,-13 -8,3,13 -16,9,5 3,7,-5 -6,7,-5 17,3,13 -20,8,16 \
             -20,0,13 -1,1,-17 -3,9,12 -19,4,14 18,9,-5 -1,0,1 -7,9,-7 -7,6,0 5,3,-20 14,3,2 \
             -13,3,9 4,8,19 7,6,-17 -5,9,3 9,6,16 -12,0,0 -8,9,3 11,9,-14 15,2,-14 4,7,-9 \
             11,0,-14 15,6,19",
        );
        let mut m = JavaHashMap::new(hash);
        for (i, &p) in added.iter().enumerate() {
            m.insert(p, i as i32);
        }
        for i in (0..40).step_by(3) {
            m.remove(&added[i]);
        }
        m.insert(added[0], 99);
        let expected = parse(
            "-1,0,1 -15,7,-13 11,0,-14 12,6,-10 3,6,13 -16,9,5 4,8,19 5,8,19 -8,9,3 9,6,16 \
             -5,9,3 -17,1,10 -13,3,9 18,9,-5 -16,0,-15 16,2,16 -3,9,12 3,7,-5 11,9,-14 \
             4,7,-9 5,3,-20 -1,1,-17 -7,6,0 -20,8,16 -10,3,-12 -10,6,10 17,3,13",
        );
        assert_eq!(keys(&m), expected);
        assert_eq!(keys(&JavaHashMap::copy_of(&m)), expected);
        assert_eq!(m.get(&added[0]), Some(&99));

        let mut small = JavaHashMap::new(hash);
        small.insert(BetterBlockPos::new(1, 2, 3), 0);
        small.insert(BetterBlockPos::new(-5, 70, 2), 1);
        let mut copy = JavaHashMap::copy_of(&small);
        copy.insert(BetterBlockPos::new(0, 0, 0), 2);
        copy.insert(BetterBlockPos::new(17, 0, 0), 3);
        copy.insert(BetterBlockPos::new(1, 0, 0), 4);
        assert_eq!(keys(&copy), parse("0,0,0 17,0,0 1,0,0 1,2,3 -5,70,2"));

        let mut empty = JavaHashMap::copy_of(&JavaHashMap::new(hash));
        empty.insert(BetterBlockPos::new(16, 0, 0), 0);
        empty.insert(BetterBlockPos::new(0, 0, 0), 1);
        assert_eq!(keys(&empty), parse("16,0,0 0,0,0"));

        // (k, k, 0) all hash to 32 * k: one bucket until the table has 64 slots
        let staircase = |k: i32| BetterBlockPos::new(k, k, 0);
        let mut m = JavaHashMap::new(hash);
        for k in 0..9 {
            m.insert(staircase(k), k);
        }
        assert_eq!(
            keys(&m),
            parse("0,0,0 1,1,0 2,2,0 3,3,0 4,4,0 5,5,0 6,6,0 7,7,0 8,8,0")
        );
        for k in 9..14 {
            m.insert(staircase(k), k);
        }
        assert_eq!(
            keys(&m),
            parse(
                "0,0,0 2,2,0 4,4,0 6,6,0 8,8,0 10,10,0 12,12,0 \
                 1,1,0 3,3,0 5,5,0 7,7,0 9,9,0 11,11,0 13,13,0"
            )
        );

        let mut m = JavaHashMap::new(hash);
        for k in 0..8 {
            m.insert(staircase(k), k);
        }
        m.insert(BetterBlockPos::new(1, 0, 0), 100);
        m.insert(BetterBlockPos::new(3, 0, 5), 101);
        m.insert(BetterBlockPos::new(16, 0, 0), 102);
        m.remove(&staircase(2));
        m.insert(staircase(8), 8);
        assert_eq!(
            keys(&m),
            parse("0,0,0 1,1,0 3,3,0 4,4,0 5,5,0 6,6,0 7,7,0 8,8,0 1,0,0 3,0,5 16,0,0")
        );
        m.insert(staircase(9), 9);
        assert_eq!(
            keys(&m),
            parse("0,0,0 4,4,0 6,6,0 8,8,0 1,0,0 3,0,5 16,0,0 1,1,0 3,3,0 5,5,0 7,7,0 9,9,0")
        );

        let mut m = JavaHashMap::new(hash);
        for k in 0..10 {
            m.insert(staircase(k), k);
        }
        m.insert(BetterBlockPos::new(16, 0, 0), 16);
        m.remove(&staircase(1));
        let expected = parse("0,0,0 2,2,0 4,4,0 6,6,0 8,8,0 16,0,0 3,3,0 5,5,0 7,7,0 9,9,0");
        assert_eq!(keys(&m), expected);
        assert_eq!(keys(&JavaHashMap::copy_of(&m)), expected);
    }

    #[test]
    fn java_split() {
        assert_eq!(split("a,b", ','), ["a", "b"]);
        assert_eq!(split("a,,b,,", ','), ["a", "", "b"]);
        assert_eq!(split(",a", ','), ["", "a"]);
        assert_eq!(split("", ','), [""]);
        assert_eq!(split(",,", ','), [""; 0]);
        assert_eq!(split("a", ','), ["a"]);
    }

    #[test]
    fn executor_waits_for_its_tasks() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let executor = Executor::new();
        let done = Arc::new(AtomicUsize::new(0));
        for _ in 0..4 {
            let done = Arc::clone(&done);
            executor.execute(move || {
                std::thread::sleep(std::time::Duration::from_millis(10));
                done.fetch_add(1, Ordering::SeqCst);
            });
        }
        executor.execute(|| panic!("a task that fails"));
        executor.wait_until_idle();
        assert_eq!(done.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn ordinary() {
        assert_eq!(min_f64(1.0, 2.0), 1.0);
        assert_eq!(max_f64(1.0, 2.0), 2.0);
        assert_eq!(min_f64(f64::NEG_INFINITY, 2.0), f64::NEG_INFINITY);
        assert_eq!(max_f32(-90.0, -100.0), -90.0);
    }
}
