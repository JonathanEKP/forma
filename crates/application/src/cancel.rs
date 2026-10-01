//! Cancellation for the operations a caller can abort (Pontis plan, section 11
//! step 5).
//!
//! Pontis threads an abort signal into every capability binding, and a binding
//! doing real work must stop when it fires. Forma's use cases had nothing to
//! receive it with. A [`Cancellation`] is that receiver: the transport owns it,
//! the use case polls it at checkpoints it chooses, and a fired signal comes
//! back as `CANCELLED` rather than as a finished result.
//!
//! **Dropping the future is not enough, and the checkpoints must yield.** A use
//! case's CPU work — usvg parsing, resvg rasterizing — runs synchronously inside
//! a poll, so a drop can only land between polls, and a future that never
//! returns `Pending` is never between polls. `validate` on a design with no
//! images awaits nothing that pends, so before this module it ran every page in
//! one poll and no client disconnect could interrupt it. That is a property of
//! the function, not of the transport, which is why [`Cancellation::checkpoint`]
//! hands control back to the executor every time it is reached. The yield is the
//! point, not noise: remove it and a drop has nowhere to land.
//!
//! The explicit signal is the other half. Today the work runs on the request's
//! own task, so the handler's drop and the signal coincide; the moment any of it
//! moves to another thread (`spawn_blocking`, say), dropping the handler stops
//! nothing and only the signal does. A transport ties the two together with
//! [`Cancellation::cancel_on_drop`].
//!
//! **Not every operation may stop at any checkpoint.** `validate` writes nothing,
//! so abandoning it anywhere loses only work. An operation with a side effect —
//! `apply_operations` persisting a version, an export writing a file and a row —
//! must check before the side effect and never between its halves, or a cancel
//! leaves the half that landed. Place each checkpoint by asking what is already
//! committed when it fires.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::error::{ApplicationError, Result};

/// A signal a caller fires to abandon an operation in flight.
///
/// Clones share one flag, so the transport keeps a clone to fire and hands a
/// reference to the use case. One that is never fired costs an atomic load per
/// checkpoint.
#[derive(Debug, Clone, Default)]
pub struct Cancellation {
    fired: Arc<AtomicBool>,
}

impl Cancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.fired.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.fired.load(Ordering::Acquire)
    }

    /// Fires this signal when the returned guard is dropped.
    ///
    /// A handler holds the guard for as long as it runs, so the handler future
    /// being dropped — a disconnect the server noticed — reaches work that no
    /// longer lives inside that future.
    #[must_use = "the signal fires when the guard is dropped, which is immediately if it is not held"]
    pub fn cancel_on_drop(&self) -> CancelOnDrop {
        CancelOnDrop(self.clone())
    }

    /// Fails with `CANCELLED` if the signal has fired, otherwise yields once to
    /// the executor and checks again.
    ///
    /// Call it between units of synchronous work, never in the middle of a side
    /// effect. The yield is what lets a dropped future actually stop: see the
    /// module documentation before removing it.
    pub async fn checkpoint(&self) -> Result<()> {
        self.check()?;
        YieldNow(false).await;
        self.check()
    }

    /// The check without the yield, for the last point before returning, where
    /// there is nothing left for the executor to interleave.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            return Err(ApplicationError::Cancelled);
        }
        Ok(())
    }
}

/// Fires its [`Cancellation`] when dropped.
#[derive(Debug)]
pub struct CancelOnDrop(Cancellation);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// Returns `Pending` exactly once, waking itself, so the executor regains
/// control. Hand-written because this crate depends on no async runtime.
struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_guard_fires_every_clone() {
        let cancel = Cancellation::new();
        let held_elsewhere = cancel.clone();
        {
            let _guard = cancel.cancel_on_drop();
            assert!(!held_elsewhere.is_cancelled());
        }
        assert!(held_elsewhere.is_cancelled());
        assert_eq!(held_elsewhere.check().unwrap_err().code(), "CANCELLED");
    }

    #[test]
    fn a_checkpoint_returns_control_before_it_completes() {
        let cancel = Cancellation::new();
        let mut checkpoint = std::pin::pin!(cancel.checkpoint());
        let mut cx = Context::from_waker(std::task::Waker::noop());

        assert!(checkpoint.as_mut().poll(&mut cx).is_pending());
        // Fired while the executor held control: the second half sees it.
        cancel.cancel();
        match checkpoint.as_mut().poll(&mut cx) {
            Poll::Ready(Err(error)) => assert_eq!(error.code(), "CANCELLED"),
            other => panic!("expected CANCELLED, got {other:?}"),
        }
    }
}
