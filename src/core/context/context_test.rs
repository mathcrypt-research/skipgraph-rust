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
        wait_until(
            move || ctx_clone.is_cancelled(),
            Duration::from_millis(100)
        ).await.expect("context should be cancelled within 100ms");
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
            } else{
                panic!("unexpected panic payload type");
            }
        }
    }
}