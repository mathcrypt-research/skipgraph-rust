//! Throwable context implementation for skipgraph, similar to Go's context with irrecoverable error propagation.
//!
//! This module provides a context implementation that combines:
//! - Cancellation support via tokio's CancellationToken
//! - Timeout/deadline functionality
//! - Value storage and retrieval
//! - Parent-child context hierarchies
//! - Irrecoverable error propagation that terminates the application

pub mod examples;

use anyhow::{anyhow, Result};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::Span;

/// A context that can propagate irrecoverable errors up the context chain,
/// similar to Go's context.Context but with throwable error handling.
/// 
/// If an irrecoverable error is thrown, it will propagate to the parent context if it exists.
/// If there is no parent context, it will log the error and terminate the program.
/// This is useful for components that need to signal fatal errors that should stop the entire application.
#[derive(Clone)]
pub struct ThrowableContext {
    inner: Arc<ContextInner>,
}

struct ContextInner {
    token: CancellationToken,
    deadline: Option<Instant>,
    values: Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>,
    parent: Option<ThrowableContext>,
    span: Span,
}

impl ThrowableContext {
    /// Create a new root context.
    pub fn new(parent_span: &Span) -> Self {
        let span = tracing::span!(parent: parent_span, tracing::Level::TRACE, "throwable_context");
        
        Self {
            inner: Arc::new(ContextInner {
                token: CancellationToken::new(),
                deadline: None,
                values: Arc::new(RwLock::new(HashMap::new())),
                parent: None,
                span,
            }),
        }
    }

    /// Create a new context with a timeout deadline.
    pub fn with_timeout(parent_span: &Span, timeout: Duration) -> Self {
        let span = tracing::span!(parent: parent_span, tracing::Level::TRACE, "throwable_context_timeout");
        
        Self {
            inner: Arc::new(ContextInner {
                token: CancellationToken::new(),
                deadline: Some(Instant::now() + timeout),
                values: Arc::new(RwLock::new(HashMap::new())),
                parent: None,
                span,
            }),
        }
    }

    /// Create a child context that inherits from a parent context.
    pub fn with_parent(&self) -> Self {
        let span = tracing::span!(parent: &self.inner.span, tracing::Level::TRACE, "throwable_context_child");
        
        Self {
            inner: Arc::new(ContextInner {
                token: self.inner.token.child_token(),
                deadline: self.inner.deadline,
                values: Arc::clone(&self.inner.values),
                parent: Some(self.clone()),
                span,
            }),
        }
    }

    /// Create a child context with a value added.
    pub fn with_value<T: Clone + Send + Sync + 'static>(&self, value: T) -> Self {
        let span = tracing::span!(parent: &self.inner.span, tracing::Level::TRACE, "throwable_context_value");
        let values: Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>> = Arc::new(RwLock::new(HashMap::new()));
        {
            let mut values_guard = values.write().unwrap();
            values_guard.insert(TypeId::of::<T>(), Box::new(value) as Box<dyn Any + Send + Sync>);
        }
        
        Self {
            inner: Arc::new(ContextInner {
                token: self.inner.token.child_token(),
                deadline: self.inner.deadline,
                values,
                parent: Some(self.clone()),
                span,
            }),
        }
    }

    /// Propagates an irrecoverable error up the context chain.
    /// When it reaches the top-level context, it logs the error and terminates the program.
    pub fn throw_irrecoverable(&self, err: anyhow::Error) -> ! {
        let _enter = self.inner.span.enter();
        
        // Propagate the error to the parent context if it exists
        if let Some(parent) = &self.inner.parent {
            tracing::trace!("propagating irrecoverable error to parent context");
            parent.throw_irrecoverable(err);
        }
        
        // If there is no parent context, log and terminate the program
        tracing::error!("irrecoverable error: {}", err);
        std::process::exit(1);
    }

    /// Returns true if the context has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.inner.token.is_cancelled()
    }

    /// Cancels the context and all its children.
    pub fn cancel(&self) {
        let _enter = self.inner.span.enter();
        tracing::trace!("cancelling context");
        self.inner.token.cancel();
    }

    /// Wait for the context to be cancelled.
    pub async fn cancelled(&self) {
        self.inner.token.cancelled().await;
    }

    /// Returns the deadline for this context, if any.
    pub fn deadline(&self) -> Option<Instant> {
        self.inner.deadline
    }

    /// Returns true if the context has exceeded its deadline.
    pub fn is_deadline_exceeded(&self) -> bool {
        self.inner.deadline.map_or(false, |d| Instant::now() >= d)
    }

    /// Gets a value from the context by type, searching up the parent chain.
    /// Note: This method returns a cloned value, not a reference, due to lifetime constraints.
    pub fn value<T: Clone + 'static>(&self) -> Option<T> {
        {
            let values_guard = self.inner.values.read().unwrap();
            if let Some(boxed_value) = values_guard.get(&TypeId::of::<T>()) {
                if let Some(typed_value) = boxed_value.downcast_ref::<T>() {
                    return Some(typed_value.clone());
                }
            }
        }
        
        // Search in parent if not found in current context
        self.inner
            .parent
            .as_ref()
            .and_then(|parent| parent.value::<T>())
    }

    /// Returns the current context error if cancelled or deadline exceeded.
    pub fn err(&self) -> Option<anyhow::Error> {
        if self.is_cancelled() {
            Some(anyhow!("context cancelled"))
        } else if self.is_deadline_exceeded() {
            Some(anyhow!("context deadline exceeded"))
        } else {
            None
        }
    }

    /// Runs a future with context cancellation and timeout support.
    /// Returns an error if the context is cancelled or deadline is exceeded.
    pub async fn run<F, T>(&self, future: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        let _enter = self.inner.span.enter();
        
        if let Some(deadline) = self.inner.deadline {
            let timeout_duration = deadline.saturating_duration_since(Instant::now());
            
            tokio::select! {
                result = future => result,
                _ = tokio::time::sleep(timeout_duration) => {
                    Err(anyhow!("context deadline exceeded"))
                }
                _ = self.cancelled() => {
                    Err(anyhow!("context cancelled"))
                }
            }
        } else {
            tokio::select! {
                result = future => result,
                _ = self.cancelled() => {
                    Err(anyhow!("context cancelled"))
                }
            }
        }
    }

    /// Runs a future with context support, throwing irrecoverable error on failure.
    /// This combines `run` with `throw_irrecoverable` for convenience.
    pub async fn run_or_throw<F, T>(&self, future: F) -> T
    where
        F: std::future::Future<Output = Result<T>>,
    {
        match self.run(future).await {
            Ok(value) => value,
            Err(err) => self.throw_irrecoverable(err),
        }
    }

    /// Helper method similar to Go's context.WithCancel - returns (context, cancel_fn).
    pub fn with_cancel(&self) -> (Self, impl Fn()) {
        let child = self.with_parent();
        let token = child.inner.token.clone();
        let cancel_fn = move || token.cancel();
        (child, cancel_fn)
    }
}

impl std::fmt::Debug for ThrowableContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThrowableContext")
            .field("is_cancelled", &self.is_cancelled())
            .field("deadline", &self.deadline())
            .field("has_parent", &self.inner.parent.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::testutil::fixtures::span_fixture;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_context_cancellation() {
        let ctx = ThrowableContext::new(&span_fixture());
        let child = ctx.with_parent();

        assert!(!child.is_cancelled());
        ctx.cancel();
        
        // Small delay to allow cancellation to propagate
        sleep(Duration::from_millis(1)).await;
        assert!(child.is_cancelled());
    }

    #[tokio::test]
    async fn test_context_timeout() {
        let ctx = ThrowableContext::with_timeout(&span_fixture(), Duration::from_millis(10));
        
        let result = ctx.run(async {
            sleep(Duration::from_millis(50)).await;
            Ok::<(), anyhow::Error>(())
        }).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("deadline exceeded"));
    }

    #[tokio::test]
    async fn test_context_values() {
        let ctx = ThrowableContext::new(&span_fixture());
        let ctx_with_value = ctx.with_value("test_key".to_string());
        let child = ctx_with_value.with_parent();

        assert_eq!(child.value::<String>(), Some("test_key".to_string()));
        assert_eq!(ctx.value::<String>(), None);
    }

    #[tokio::test]
    async fn test_context_with_cancel() {
        let ctx = ThrowableContext::new(&span_fixture());
        let (child, cancel) = ctx.with_cancel();

        assert!(!child.is_cancelled());
        cancel();
        
        // Small delay to allow cancellation to propagate
        sleep(Duration::from_millis(1)).await;
        assert!(child.is_cancelled());
    }

    #[tokio::test]
    async fn test_successful_operation() {
        let ctx = ThrowableContext::new(&span_fixture());
        
        let result = ctx.run(async {
            Ok::<i32, anyhow::Error>(42)
        }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }
}