#[cfg(test)]
mod tests {
    use crate::core::context::IrrevocableContext;
    use crate::core::testutil::fixtures::{span_fixture, wait_until};
    use tokio::time::{sleep, Duration};

    /// this test ensures that cancelling a context works as expected
    #[tokio::test]
    async fn test_basic_cancellation() {
        let ctx = IrrevocableContext::new(&span_fixture(), "test_context");

        assert!(!ctx.is_cancelled());
        ctx.cancel();

        // Wait until cancellation is processed
        let ctx_clone = ctx.clone();
        wait_until(move || ctx_clone.is_cancelled(), Duration::from_millis(100))
            .await
            .expect("context should be cancelled within 100ms");
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
            Duration::from_millis(100),
        )
        .await
        .expect("child context should be cancelled within 100ms");
    }

    /// this test ensures that running an operation completes successfully if its context is not canceled
    #[tokio::test]
    async fn test_successful_operation() {
        let ctx = IrrevocableContext::new(&span_fixture(), "test_context");

        let result = ctx.run(async { Ok::<i32, anyhow::Error>(42) }).await;

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
        wait_until(move || ctx_clone.is_cancelled(), Duration::from_millis(100))
            .await
            .expect("context should be cancelled within 100ms");

        let result = ctx
            .run(async {
                // This should not execute due to cancellation
                sleep(Duration::from_millis(10)).await;
                Ok::<i32, anyhow::Error>(42)
            })
            .await;

        // The operation should return an error since the context was canceled before it could complete
        assert!(result.is_err());
        // The error should indicate cancellation
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("context cancelled"));
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
            move || {
                child1_clone.is_cancelled()
                    && child2_clone.is_cancelled()
                    && grandchild_clone.is_cancelled()
            },
            Duration::from_millis(100),
        )
        .await
        .expect("all child contexts should be cancelled within 100ms");
    }

    /// Test that we can create the error propagation hierarchy
    #[test]
    fn test_error_propagation_structure() {
        let root = IrrevocableContext::new(&span_fixture(), "test_error_propagation_root");
        let child = root.child("error_prop_child");
        let grandchild = child.child("error_prop_grandchild");

        grandchild.cancel();

        // verify the parent chain exists
        assert!(grandchild.inner.parent.is_some());
        assert!(child.inner.parent.is_some());
        assert!(root.inner.parent.is_none());
    }

    /// Test that throw_irrecoverable properly panics when called
    #[test]
    fn test_throw_irrecoverable_panics() {
        let result = std::panic::catch_unwind(|| {
            let root = IrrevocableContext::new(&span_fixture(), "test_throw_root");
            let child = root.child("test_throw_child");

            // This should panic with the irrecoverable error message
            child.throw_irrecoverable(anyhow::anyhow!("test irrecoverable error"));
        });

        // Verify that a panic occurred
        assert!(result.is_err());

        // Verify the panic message contains our error
        if let Err(panic_payload) = result {
            if let Some(panic_msg) = panic_payload.downcast_ref::<String>() {
                assert_eq!(panic_msg, "irrecoverable error: test irrecoverable error");
            } else {
                panic!("unexpected panic payload type");
            }
        }
    }

    // --- ported "extras" feature tests (candidate for main integration) ---

    /// a context created with a timeout fails a longer-running operation with a deadline error
    #[tokio::test]
    async fn test_run_respects_deadline() {
        let ctx =
            IrrevocableContext::with_timeout(&span_fixture(), "timeout_ctx", Duration::from_millis(10));

        let result = ctx
            .run(async {
                sleep(Duration::from_millis(200)).await;
                Ok::<(), anyhow::Error>(())
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("deadline exceeded"));
    }

    /// is_deadline_exceeded reflects the deadline crossing
    #[tokio::test]
    async fn test_is_deadline_exceeded() {
        let ctx =
            IrrevocableContext::with_timeout(&span_fixture(), "timeout_ctx", Duration::from_millis(5));
        assert!(!ctx.is_deadline_exceeded());
        assert!(ctx.deadline().is_some());
        sleep(Duration::from_millis(20)).await;
        assert!(ctx.is_deadline_exceeded());
        assert!(ctx.err().is_some());
    }

    /// with_cancel returns a child plus a closure that cancels it
    #[tokio::test]
    async fn test_with_cancel_closure() {
        let root = IrrevocableContext::new(&span_fixture(), "cancel_root");
        let (child, cancel) = root.with_cancel("cancel_child");

        assert!(!child.is_cancelled());
        cancel();

        let child_clone = child.clone();
        wait_until(move || child_clone.is_cancelled(), Duration::from_millis(100))
            .await
            .expect("child should be cancelled after invoking cancel closure");
    }

    /// run() on an already-expired context returns the deadline error even when the
    /// operation itself is immediately ready (guards the short-circuit / select! race)
    #[tokio::test]
    async fn test_run_short_circuits_when_deadline_already_passed() {
        let ctx =
            IrrevocableContext::with_timeout(&span_fixture(), "expired_ctx", Duration::from_millis(5));
        // let the deadline lapse before we ever call run()
        sleep(Duration::from_millis(20)).await;
        assert!(ctx.is_deadline_exceeded());

        // an immediately-ready future must NOT slip through on an expired context
        let result = ctx.run(async { Ok::<i32, anyhow::Error>(42) }).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("deadline exceeded"));
    }

    /// cancellation that fires *while* run() is in-flight wins the race inside the
    /// Some(deadline) arm — exercises select!'s `cancelled()` branch (not the short-circuit)
    #[tokio::test]
    async fn test_run_cancellation_beats_deadline() {
        // generous deadline so the timer never fires; the context is live when run() starts
        let ctx = IrrevocableContext::with_timeout(
            &span_fixture(),
            "cancel_vs_deadline",
            Duration::from_secs(60),
        );
        let canceller = ctx.clone();

        // run a long operation, and concurrently cancel shortly after it begins
        let (result, ()) = tokio::join!(
            ctx.run(async {
                sleep(Duration::from_millis(200)).await;
                Ok::<(), anyhow::Error>(())
            }),
            async move {
                sleep(Duration::from_millis(10)).await;
                canceller.cancel();
            }
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("context cancelled"));
    }
}
