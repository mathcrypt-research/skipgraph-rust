//! Examples demonstrating ThrowableContext usage patterns
//! 
//! This module provides examples of how to use ThrowableContext in various scenarios,
//! similar to Go's context usage patterns but with Rust-specific adaptations.

use crate::core::context::ThrowableContext;
use anyhow::{anyhow, Result};
use tokio::time::{sleep, Duration};
use tracing::Span;

/// Example: Using ThrowableContext for startup operations
/// This demonstrates how to handle irrecoverable startup errors
pub async fn startup_with_context_example(parent_span: &Span) -> Result<()> {
    let ctx = ThrowableContext::with_timeout(parent_span, Duration::from_secs(30));
    
    // Simulate multiple startup operations
    let operations = vec![
        "initialize_network",
        "load_configuration", 
        "setup_routing_table",
        "register_services",
    ];
    
    for operation in operations {
        // Use run_or_throw for critical startup operations
        // If any operation fails, the program will terminate with an error message
        ctx.run_or_throw(simulate_startup_operation(operation)).await;
        tracing::info!("Completed startup operation: {}", operation);
    }
    
    tracing::info!("All startup operations completed successfully");
    Ok(())
}

/// Example: Using context hierarchy with value propagation
pub async fn hierarchical_context_example(parent_span: &Span) -> Result<()> {
    let root_ctx = ThrowableContext::new(parent_span);
    
    // Add configuration values to context
    let config_ctx = root_ctx
        .with_value("service_name".to_string())
        .with_value(42u16); // port number
    
    // Create child context for specific operation
    let operation_ctx = config_ctx.with_parent();
    
    // Child can access parent values
    if let Some(service_name) = operation_ctx.value::<String>() {
        tracing::info!("Running operation for service: {}", service_name);
    }
    
    if let Some(port) = operation_ctx.value::<u16>() {
        tracing::info!("Using port: {}", port);
    }
    
    // Use the context for timeout-sensitive operations
    operation_ctx.run(async {
        sleep(Duration::from_millis(100)).await;
        Ok(())
    }).await?;
    
    Ok(())
}

/// Example: Context with cancellation
pub async fn cancellation_example(parent_span: &Span) -> Result<()> {
    let ctx = ThrowableContext::new(parent_span);
    let (child_ctx, cancel) = ctx.with_cancel();
    
    // Spawn a background task
    let background_task = tokio::spawn(async move {
        child_ctx.run(async {
            // Long running operation
            sleep(Duration::from_secs(10)).await;
            Ok(())
        }).await
    });
    
    // Cancel after a short time
    tokio::spawn(async move {
        sleep(Duration::from_millis(100)).await;
        cancel();
    });
    
    // The background task will be cancelled
    match background_task.await {
        Ok(Ok(())) => tracing::info!("Task completed successfully"),
        Ok(Err(e)) => tracing::info!("Task cancelled: {}", e),
        Err(e) => tracing::error!("Task panicked: {}", e),
    }
    
    Ok(())
}

/// Example: Error propagation up context chain
pub async fn error_propagation_example(parent_span: &Span) -> Result<()> {
    let root_ctx = ThrowableContext::new(parent_span);
    let child_ctx = root_ctx.with_parent();
    let grandchild_ctx = child_ctx.with_parent();
    
    // In a real scenario, this would be called from a deeply nested operation
    // When throw_irrecoverable is called, it will propagate up to the root and terminate
    // For demo purposes, we'll just show the pattern without actually calling it
    
    let critical_error = anyhow!("Database connection failed during critical operation");
    
    // In production, this would terminate the program:
    // grandchild_ctx.throw_irrecoverable(critical_error);
    
    // Instead, we'll just log what would happen
    tracing::info!("Would propagate irrecoverable error: {}", critical_error);
    
    Ok(())
}

// Helper function to simulate startup operations
async fn simulate_startup_operation(operation_name: &str) -> Result<()> {
    // Simulate some work
    sleep(Duration::from_millis(10)).await;
    
    // Simulate potential failure
    if operation_name == "fail_example" {
        return Err(anyhow!("Simulated failure in {}", operation_name));
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::testutil::fixtures::span_fixture;

    #[tokio::test]
    async fn test_startup_example() {
        let result = startup_with_context_example(&span_fixture()).await;
        assert!(result.is_ok());
    }

    #[tokio::test] 
    async fn test_hierarchical_context_example() {
        let result = hierarchical_context_example(&span_fixture()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cancellation_example() {
        let result = cancellation_example(&span_fixture()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_error_propagation_example() {
        let result = error_propagation_example(&span_fixture()).await;
        assert!(result.is_ok());
    }
}