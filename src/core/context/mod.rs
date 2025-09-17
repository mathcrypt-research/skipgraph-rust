//! Cancelable context with irrecoverable error propagation
//!
//! This module provides a simplified context implementation focused on:
//! - Cancellation support via tokio's CancellationToken
//! - Parent-child context hierarchies  
//! - Irrecoverable error propagation that terminates the application
//!
//! Unlike the full Go context API, this implementation focuses only on the core
//! functionality needed: cancellation and error propagation.

pub mod examples;

use anyhow::Result;
use std::sync::Arc;
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
                parent: None,
                span,
            }),
        }
    }

    /// Create a child context that inherits cancellation from the parent
    pub fn child(&self, tag: &str) -> Self {
        let span = tracing::span!(parent: &self.inner.span, tracing::Level::TRACE, "irrevocable_context_child", tag = tag);
        Self {
            inner: Arc::new(ContextInner {
                token: self.inner.token.child_token(),
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

    /// Run an operation with cancellation support
    /// If the context is cancelled before the operation completes, it returns an error.
    /// otherwise, it returns the operation's result.
    pub async fn run<F, T>(&self, future: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        let _enter = self.inner.span.enter();
        
        tokio::select! {
            result = future => result,
            _ = self.cancelled() => {
                Err(anyhow::anyhow!("context cancelled"))
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
}

// Custom Debug implementation for better visibility into the context state
impl std::fmt::Debug for IrrevocableContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IrrevocableContext")
            .field("is_cancelled", &self.is_cancelled())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::testutil::fixtures::{span_fixture, wait_until};
    use tokio::time::{sleep, Duration};

    /// this test ensures that cancelling a context works as expected
    #[tokio::test]
    async fn test_basic_cancellation() {
        let ctx = IrrevocableContext::new(&span_fixture(), "test_context");
        
        assert!(!ctx.is_cancelled());
        ctx.cancel();
        assert!(ctx.is_cancelled());
    }

    /// this test ensures that cancelling a parent context cancels its children
    #[tokio::test]
    async fn test_child_cancellation() {
        let parent = IrrevocableContext::new(&span_fixture(), "test_context");
        let child = parent.child("test_child");
        
        assert!(!child.is_cancelled());
        parent.cancel(); // Cancel parent

        // Wait for cancellation to propagate
        let child_clone = child.clone();
        wait_until(
            move || child_clone.is_cancelled(),
            Duration::from_millis(100)
        ).await.expect("child context should be cancelled within 100ms");
    }

    /// this test ensures that running an operation completes successfully if its context is not canceled
    #[tokio::test]
    async fn test_successful_operation() {
        let ctx = IrrevocableContext::new(&span_fixture(), "test_context");

        let result = ctx.run(async {
            Ok::<i32, anyhow::Error>(42)
        }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    /// this test ensures that running an operation respects cancellation
    /// the operation should not complete if the context is canceled
    #[tokio::test]
    async fn test_run_with_cancellation() {
        let ctx = IrrevocableContext::new(&span_fixture(), "test_context");
        
        // Cancel the context
        ctx.cancel();

        // Wait for cancellation to be processed
        let ctx_clone = ctx.clone();
        wait_until(
            move || ctx_clone.is_cancelled(),
            Duration::from_millis(100)
        ).await.expect("context should be cancelled within 100ms");
        
        let result = ctx.run(async {
            // This should not execute due to cancellation
            sleep(Duration::from_millis(10)).await;
            Ok::<i32, anyhow::Error>(42)

        }).await;

        // The operation should return an error since the context was canceled before it could complete
        assert!(result.is_err());
        // The error should indicate cancellation
        assert!(result.unwrap_err().to_string().contains("context cancelled"));
    }


    /// this test ensures that nested child contexts are canceled when the root context is canceled
    #[tokio::test]
    async fn test_nested_children() {
        let root = IrrevocableContext::new(&span_fixture(), "test_nested_children_root");
        let child1 = root.child("child1");
        let child2 = child1.child("child2");
        let grandchild = child2.child("grandchild");

        // Initially, none should be cancelled
        assert!(!grandchild.is_cancelled());
        
        // Cancel root - should propagate to all children
        root.cancel();

        // Wait for cancellation to propagate to all children
        let child1_clone = child1.clone();
        let child2_clone = child2.clone();
        let grandchild_clone = grandchild.clone();
        wait_until(
            move || child1_clone.is_cancelled() && child2_clone.is_cancelled() && grandchild_clone.is_cancelled(),
            Duration::from_millis(100)
        ).await.expect("all child contexts should be cancelled within 100ms");
    }

    /// Test that we can create the error propagation hierarchy
    /// (We can't test throw_irrecoverable since it exits the program)
    #[test]
    fn test_error_propagation_structure() {
        let root = IrrevocableContext::new(&span_fixture(), "test_error_propagation_root");
        let child = root.child("error_prop_child");
        let grandchild = child.child("error_prop_grandchild");
        
        // verify the parent chain exists
        assert!(grandchild.inner.parent.is_some());
        assert!(child.inner.parent.is_some());
        assert!(root.inner.parent.is_none());
    }
}