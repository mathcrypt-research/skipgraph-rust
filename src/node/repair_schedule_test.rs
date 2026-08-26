use crate::node::repair_schedule::{ManualRepairSchedule, RepairSchedule, TokioRepairSchedule};
use std::sync::Arc;
use std::time::Duration;

/// Verifies `fire` completes a `tick` call that is already pending.
#[tokio::test]
async fn test_manual_repair_schedule_fire_completes_pending_tick() {
    let schedule = Arc::new(ManualRepairSchedule::new());
    let waiter = tokio::spawn({
        let schedule = schedule.clone();
        async move { schedule.tick().await }
    });

    // give the spawned task a chance to start polling `tick()` before firing.
    tokio::task::yield_now().await;
    schedule.fire();

    tokio::time::timeout(Duration::from_secs(2), waiter)
        .await
        .expect("tick did not complete before the timeout")
        .expect("spawned task panicked");
}

/// Verifies a `fire` issued before `tick` is called is not lost: the next `tick` call
/// completes immediately instead of waiting for a subsequent `fire`.
#[tokio::test]
async fn test_manual_repair_schedule_fire_before_tick_is_not_lost() {
    let schedule = ManualRepairSchedule::new();
    schedule.fire();

    tokio::time::timeout(Duration::from_secs(2), schedule.tick())
        .await
        .expect("tick did not complete before the timeout");
}

/// Verifies a single `fire` unblocks exactly one `tick` call: a second, independent
/// `tick` call still waits for its own `fire`.
#[tokio::test(start_paused = true)]
async fn test_manual_repair_schedule_single_fire_grants_exactly_one_tick() {
    let schedule = ManualRepairSchedule::new();
    schedule.fire();

    tokio::time::timeout(Duration::from_secs(2), schedule.tick())
        .await
        .expect("first tick did not complete before the timeout");

    let second_tick = tokio::time::timeout(Duration::from_millis(50), schedule.tick()).await;
    assert!(
        second_tick.is_err(),
        "second tick completed without its own fire"
    );
}

/// Verifies `TokioRepairSchedule::tick` actually resolves once its period elapses,
/// confirming the trait is correctly wired to a real `tokio::time::Interval`.
#[tokio::test(start_paused = true)]
async fn test_tokio_repair_schedule_ticks_after_period_elapses() {
    let schedule = TokioRepairSchedule::new(Duration::from_millis(100));

    // `tokio::time::interval`'s own first tick resolves immediately; consume it so the
    // assertions below observe a tick that actually waited out the period.
    schedule.tick().await;

    // the period is 100ms, so a tick can't legitimately arrive within this 50ms window;
    // timing out here proves the schedule didn't fire early.
    let premature = tokio::time::timeout(Duration::from_millis(50), schedule.tick()).await;
    assert!(
        premature.is_err(),
        "tick completed before its period elapsed"
    );

    tokio::time::timeout(Duration::from_secs(2), schedule.tick())
        .await
        .expect("tick did not complete after its period elapsed");
}
