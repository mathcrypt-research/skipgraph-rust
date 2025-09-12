//! Examples demonstrating CancelableContext usage patterns
//! 
//! This module shows how to use the simplified cancelable context for
//! cancellation and irrecoverable error handling.

use crate::core::context::CancelableContext;
use anyhow::{anyhow, Result};
use tokio::time::{sleep, Duration};
use tracing::Span;

/// Example: Basic cancellation usage
pub async fn basic_cancellation_example(parent_span: &Span) -> Result<()> {
    let ctx = CancelableContext::new(parent_span);
    let child_ctx = ctx.child();
    
    // Spawn background work with child context
    // this will start in a separate thread
    // it runs for a few seconds unless cancelled
    let work_handle = tokio::spawn( // spawn a new asynchronous task
        {
            // use a clone of the child context
            let work_ctx = child_ctx.clone();
            async move {
                work_ctx.run(async {
                    tracing::info!("Starting long-running operation...");
                    sleep(Duration::from_secs(5)).await;
                    tracing::info!("Operation completed");
                    Ok(())
                }).await
            }
        }
    );
    
    // Cancel after a short time
    tokio::spawn({
        let cancel_ctx = ctx.clone();
        async move {
            sleep(Duration::from_millis(100)).await;
            tracing::info!("Cancelling operation");
            cancel_ctx.cancel();
        }
    });

    // Match against an expected outcome (cancellation)
    match work_handle.await {
        Ok(Err(_)) => {
            tracing::info!("✓ Work was cancelled as expected");
            Ok(()) // SUCCESS: Cancellation worked
        }
        Ok(Ok(())) => {
            tracing::error!("✗ Work completed but should have been cancelled");
            Err(anyhow!("Expected cancellation but work completed successfully"))
        }
        Err(e) => {
            tracing::error!("✗ Work panicked unexpectedly: {}", e);
            Err(anyhow!("Work panicked: {}", e))
        }
    }

}

/// Example: Server startup with irrecoverable error handling
/// All operations are critical - if any fail, the program should terminate
/// In this example, all operations succeed for demonstration purposes
pub async fn startup_example(parent_span: &Span) -> Result<()> {
    let ctx = CancelableContext::new(parent_span);
    
    let startup_operations = vec![
        "initialize_network",
        "load_configuration",
        "setup_routing",
        "register_services",
    ];
    
    for operation in startup_operations {
        // Use run_or_throw for critical startup operations
        // If any fails, the program terminates
        ctx.run_or_throw(simulate_startup_operation(operation)).await;
        tracing::info!("Completed startup operation: {}", operation);
    }
    
    tracing::info!("Server startup completed successfully");
    Ok(())
}

/// Example: Hierarchical cancellation
pub async fn hierarchical_cancellation_example(parent_span: &Span) -> Result<()> {
    let root_ctx = CancelableContext::new(parent_span);
    let service1_ctx = root_ctx.child();
    let service2_ctx = root_ctx.child();
    
    // Spawn multiple services
    let service1_handle = tokio::spawn({
        let ctx = service1_ctx.clone();
        async move {
            ctx.run(async {
                tracing::info!("Service 1 starting");
                sleep(Duration::from_secs(2)).await;
                tracing::info!("Service 1 completed");
                Ok(())
            }).await
        }
    });
    
    let service2_handle = tokio::spawn({
        let ctx = service2_ctx.clone();
        async move {
            ctx.run(async {
                tracing::info!("Service 2 starting");
                sleep(Duration::from_secs(2)).await;
                tracing::info!("Service 2 completed");
                Ok(())
            }).await
        }
    });
    
    // Cancel root after short time - should cancel all services
    tokio::spawn({
        let ctx = root_ctx.clone();
        async move {
            sleep(Duration::from_millis(50)).await;
            tracing::info!("Shutting down all services");
            ctx.cancel();
        }
    });
    
    let (result1, result2) = tokio::join!(service1_handle, service2_handle);
    
    match result1 {
        Ok(Ok(())) => tracing::info!("Service 1 completed"),
        Ok(Err(e)) => tracing::info!("Service 1 cancelled: {}", e),
        Err(e) => tracing::error!("Service 1 panicked: {}", e),
    }
    
    match result2 {
        Ok(Ok(())) => tracing::info!("Service 2 completed"),
        Ok(Err(e)) => tracing::info!("Service 2 cancelled: {}", e),
        Err(e) => tracing::error!("Service 2 panicked: {}", e),
    }
    
    Ok(())
}

/// Example: Error propagation pattern
pub async fn error_propagation_example(parent_span: &Span) -> Result<()> {
    let root_ctx = CancelableContext::new(parent_span);
    let child_ctx = root_ctx.child();
    let _grandchild_ctx = child_ctx.child();
    
    // Simulate a critical error in deeply nested operation
    let critical_error = anyhow!("Critical database connection failed");
    
    // In a real scenario, uncommenting the line below would terminate the program:
    // grandchild_ctx.throw_irrecoverable(critical_error);
    
    // For demo purposes, just log what would happen
    tracing::info!("Would propagate error through context hierarchy: {}", critical_error);
    tracing::info!("Error would bubble from grandchild -> child -> root -> program exit");
    
    Ok(())
}

// Helper function to simulate startup operations
// Fails if operation_name is "fail_critical"
// otherwise succeeds after a short delay
async fn simulate_startup_operation(operation_name: &str) -> Result<()> {
    // Simulate work
    sleep(Duration::from_millis(20)).await;
    
    // Simulate potential critical failure
    if operation_name == "fail_critical" {
        return Err(anyhow!("Critical failure in {}", operation_name));
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::testutil::fixtures::span_fixture;

    #[tokio::test]
    async fn test_basic_cancellation_example() {
        let result = basic_cancellation_example(&span_fixture()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_startup_example() {
        let result = startup_example(&span_fixture()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_hierarchical_cancellation_example() {
        let result = hierarchical_cancellation_example(&span_fixture()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_error_propagation_example() {
        let result = error_propagation_example(&span_fixture()).await;
        assert!(result.is_ok());
    }
}