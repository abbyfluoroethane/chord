//! Spawn and timers. A thin wrapper over tokio in phase 1.
// Phase 2 adds a wasm version of this module.

#[cfg(feature = "native-session")]
use core::future::Future;

/// Spawns a task on the tokio runtime. The task runs detached.
#[cfg(feature = "native-session")]
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    drop(tokio::spawn(future));
}

/// Waits for the given duration.
#[cfg(feature = "native-session")]
pub async fn sleep(duration: core::time::Duration) {
    tokio::time::sleep(duration).await;
}
