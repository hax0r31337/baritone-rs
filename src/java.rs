//! Java numeric semantics that Rust does not provide out of the box.
//!
//! Rust's `f64::min`/`f64::max` drop NaN and leave the sign of zero unspecified, while Java's
//! `Math.min`/`Math.max` propagate NaN and order `-0.0` below `0.0`. Heuristics and costs are
//! compared bit-exactly against upstream, so ported code uses these helpers wherever Java calls
//! `Math.min`/`Math.max` on floating point values.

use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::{Condvar, LazyLock, Mutex, PoisonError};
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

    #[test]
    fn ordinary() {
        assert_eq!(min_f64(1.0, 2.0), 1.0);
        assert_eq!(max_f64(1.0, 2.0), 2.0);
        assert_eq!(min_f64(f64::NEG_INFINITY, 2.0), f64::NEG_INFINITY);
        assert_eq!(max_f32(-90.0, -100.0), -90.0);
    }
}
