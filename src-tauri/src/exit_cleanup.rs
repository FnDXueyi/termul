use std::future::Future;
use std::time::Duration;

pub(crate) const EXIT_CLEANUP_TIMEOUT: Duration = Duration::from_secs(15);

/// Run `cleanup` and give up on it after `timeout`, so the caller always gets
/// to exit or relaunch.
pub(crate) async fn run_bounded<F>(cleanup: F, timeout: Duration)
where
    F: Future<Output = ()> + Send + 'static,
{
    // Its own task, so the deadline still fires when a teardown step blocks
    // its worker thread instead of awaiting.
    let task = tokio::spawn(cleanup);
    match tokio::time::timeout(timeout, task).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => log::error!("[exit] cleanup task failed: {error}"),
        Err(_) => log::error!(
            "[exit] cleanup still running after {}s; exiting without it",
            timeout.as_secs()
        ),
    }
}

#[cfg(test)]
#[path = "exit_cleanup_tests.rs"]
mod tests;
