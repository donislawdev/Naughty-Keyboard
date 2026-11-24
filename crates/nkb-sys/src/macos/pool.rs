//! An autorelease pool around a piece of work, drained however the work ends.
//!
//! What AppKit hands back "for a moment" is released into the innermost pool.
//! A pool pushed and then popped by hand is left on the thread's stack when the
//! work between the two panics - the pop never runs, and the objects released
//! into it stay alive until an outer pool happens to drain them, on a thread
//! that may have none. The guard pops in its `Drop`, which runs on the way out
//! of a panic too, before any `catch_unwind` further up sees it.

use super::ffi::{self, MutRef};

/// The pool this guard pushed, popped when the guard goes.
struct Pool(MutRef);

impl Drop for Pool {
    fn drop(&mut self) {
        unsafe { ffi::objc_autoreleasePoolPop(self.0) };
    }
}

/// Runs `work` inside an autorelease pool of its own, so what AppKit hands back
/// for a moment is released when it is done - and when it panics. What `work`
/// returns must not point into the pool: it is copied out first.
pub(crate) fn in_a_pool<T>(work: impl FnOnce() -> T) -> T {
    let _pool = Pool(unsafe { ffi::objc_autoreleasePoolPush() });
    work()
}

#[cfg(test)]
#[allow(clippy::panic, reason = "the panic is the event the test is about")]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    /// A pool push answers the slot it took, and a pop gives the slot back, so the
    /// push after a popped pool takes the same slot again - and the one after a
    /// pool left on the stack takes the next one.
    #[test]
    fn a_pool_is_drained_when_the_work_in_it_panics() {
        let outer = unsafe { ffi::objc_autoreleasePoolPush() };
        let slot = unsafe { ffi::objc_autoreleasePoolPush() };
        unsafe { ffi::objc_autoreleasePoolPop(slot) };

        let caught = catch_unwind(AssertUnwindSafe(|| {
            in_a_pool(|| panic!("the work in the pool failed"));
        }));
        let next = unsafe { ffi::objc_autoreleasePoolPush() };
        unsafe { ffi::objc_autoreleasePoolPop(next) };
        unsafe { ffi::objc_autoreleasePoolPop(outer) };

        assert!(caught.is_err());
        assert_eq!(next, slot, "the pool of the work was left on the stack");

        // The control: a pool pushed and not popped moves the next push on, so
        // the comparison above can tell the two apart.
        let outer = unsafe { ffi::objc_autoreleasePoolPush() };
        let left = unsafe { ffi::objc_autoreleasePoolPush() };
        let after = unsafe { ffi::objc_autoreleasePoolPush() };
        unsafe { ffi::objc_autoreleasePoolPop(outer) };
        assert_ne!(
            after, left,
            "a pool left on the stack is not visible this way"
        );
    }
}
