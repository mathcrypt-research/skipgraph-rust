//! Cancelable context with irrecoverable error propagation
//!
//! This module provides a context implementation focused on:
//! - Cancellation support via tokio's CancellationToken
//! - Parent-child context hierarchies
//! - Irrecoverable error propagation that terminates the application
//! - Optional deadline/timeout support
//!
//! The deadline feature is additive: a context created with `new` behaves
//! exactly like the previous minimal API (no deadline).

#[cfg(test)]
mod context_test;

use anyhow::Result;
use std::sync::Arc;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::Span;

/// A cancelable context that supports parent-child hierarchies and irrecoverable error propagation.
///
/// When an irrecoverable error is thrown, it propagates up to the root context and terminates the program.
/// Children automatically get cancelled when their parent is cancelled.
pub struct IrrevocableContext {
    inner: Arc<ContextInner>,
}

struct ContextInner {
    token: CancellationToken,
    /// Absolute deadline after which the context is considered expired, if any.
    deadline: Option<Instant>,
    parent: Option<IrrevocableContext>,
    span: Span,
}

impl IrrevocableContext {
    /// Create a new root context
    pub fn new(parent_span: &Span, tag: &str) -> Self {
        let span = tracing::span!(parent: parent_span, tracing::Level::TRACE, "irrevocable_context", tag = tag);

        Self {
            inner: Arc::new(ContextInner {
                token: CancellationToken::new(),
                deadline: None,
                parent: None,
                span,
            }),
        }
    }

    /// Create a new root context that expires after `timeout` elapses.
    pub fn with_timeout(parent_span: &Span, tag: &str, timeout: std::time::Duration) -> Self {
        let span = tracing::span!(parent: parent_span, tracing::Level::TRACE, "irrevocable_context_timeout", tag = tag);

        Self {
            inner: Arc::new(ContextInner {
                token: CancellationToken::new(),
                deadline: Some(Instant::now() + timeout),
                parent: None,
                span,
            }),
        }
    }

    /// Create a child context that inherits cancellation and deadline from the parent.
    pub fn child(&self, tag: &str) -> Self {
        let span = tracing::span!(parent: &self.inner.span, tracing::Level::TRACE, "irrevocable_context_child", tag = tag);
        Self {
            inner: Arc::new(ContextInner {
                token: self.inner.token.child_token(),
                deadline: self.inner.deadline,
                parent: Some(self.clone()),
                span,
            }),
        }
    }

    /// triggers Cancel in this context and all its children
    /// to check if cancellation is complete, use `cancelled().await`
    pub fn cancel(&self) {
        let _enter = self.inner.span.enter();
        tracing::trace!("cancelling context");
        self.inner.token.cancel();
    }

    /// Check if the context is cancelled (non-blocking, private)
    fn is_cancelled(&self) -> bool {
        self.inner.token.is_cancelled()
    }

    /// Wait for the context to be cancelled (async)
    pub async fn cancelled(&self) {
        self.inner.token.cancelled().await;
    }

    /// Returns the absolute deadline for this context, if one was set.
    pub fn deadline(&self) -> Option<Instant> {
        self.inner.deadline
    }

    /// Returns true if the context has a deadline that has already passed.
    pub fn is_deadline_exceeded(&self) -> bool {
        self.inner.deadline.is_some_and(|d| Instant::now() >= d)
    }

    /// Returns an error describing why the context is unusable (cancelled or past deadline), if so.
    pub fn err(&self) -> Option<anyhow::Error> {
        if self.is_cancelled() {
            Some(anyhow::anyhow!("context cancelled"))
        } else if self.is_deadline_exceeded() {
            Some(anyhow::anyhow!("context deadline exceeded"))
        } else {
            None
        }
    }

    /// Run an operation with cancellation and deadline support.
    /// Returns an error if the context is cancelled or its deadline elapses before the operation completes.
    pub async fn run<F, T>(&self, future: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        let _enter = self.inner.span.enter();

        // Short-circuit if the context is already unusable, so an already-cancelled
        // or already-expired context never races an immediately-ready future (which
        // `select!` would otherwise resolve pseudo-randomly).
        if let Some(err) = self.err() {
            return Err(err);
        }

        match self.inner.deadline {
            Some(deadline) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                tokio::select! {
                    result = future => result,
                    _ = tokio::time::sleep(remaining) => Err(anyhow::anyhow!("context deadline exceeded")),
                    _ = self.cancelled() => Err(anyhow::anyhow!("context cancelled")),
                }
            }
            None => {
                tokio::select! {
                    result = future => result,
                    _ = self.cancelled() => Err(anyhow::anyhow!("context cancelled")),
                }
            }
        }
    }

    /// Propagate an irrecoverable error up the context chain.
    /// When it reaches the root context, it terminates the program.
    /// there is no return from this function.
    pub fn throw_irrecoverable(&self, err: anyhow::Error) -> ! {
        let _enter = self.inner.span.enter();

        // Propagate to parent if it exists
        if let Some(parent) = &self.inner.parent {
            tracing::error!("propagating irrecoverable error to parent context");
            parent.throw_irrecoverable(err);
        }

        // Root context - panic with the error
        panic!("irrecoverable error: {}", err);
    }

    /// Run an operation, throwing irrecoverable error on failure
    /// This is a convenience method that combines `run` and `throw_irrecoverable`.
    /// If the operation succeeds, it returns the result.
    /// If it fails, it propagates the error irrecoverably, terminating the program.
    pub async fn run_or_throw<F, T>(&self, future: F) -> T
    where
        F: std::future::Future<Output = Result<T>>,
    {
        match self.run(future).await {
            Ok(value) => value,
            Err(err) => self.throw_irrecoverable(err),
        }
    }

    /// Returns a cancellable child context together with a function that cancels it.
    /// Mirrors Go's `context.WithCancel`.
    ///
    /// The returned closure is `Send + Sync + Clone`, so it can be moved across
    /// threads/tasks or stored — the intended way to hand out cancel authority.
    pub fn with_cancel(&self, tag: &str) -> (Self, impl Fn() + Send + Sync + Clone) {
        let child = self.child(tag);
        let token = child.inner.token.clone();
        (child, move || token.cancel())
    }
}

// Custom Debug implementation for better visibility into the context state
impl std::fmt::Debug for IrrevocableContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IrrevocableContext")
            .field("is_cancelled", &self.is_cancelled())
            .field("deadline", &self.inner.deadline)
            .field("has_parent", &self.inner.parent.is_some())
            .finish()
    }
}

/// Custom Clone implementation to ensure shallow cloning behavior.
/// This implementation explicitly controls cloning to ensure that:
/// - Only Arc pointer is cloned (shallow), not the underlying data
/// - If future changes add non-shallow-clonable fields, this implementation
///   must be updated to maintain the shallow cloning semantics
impl Clone for IrrevocableContext {
    fn clone(&self) -> Self {
        // Shallow clone: cloned instances share the same underlying data via Arc
        IrrevocableContext {
            inner: Arc::clone(&self.inner),
        }
    }
}
