//! Task supervisor with cancellation, bounded restarts and fail-closed shutdown.

use crate::error::{BeastError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RestartPolicy {
    pub max_restarts: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
}

impl RestartPolicy {
    pub fn never() -> Self {
        Self {
            max_restarts: 0,
            initial_backoff_ms: 0,
            max_backoff_ms: 0,
        }
    }

    pub fn bounded(max_restarts: u32) -> Self {
        Self {
            max_restarts,
            initial_backoff_ms: 500,
            max_backoff_ms: 30_000,
        }
    }

    fn backoff(&self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(10);
        let millis = self
            .initial_backoff_ms
            .saturating_mul(1u64 << exponent)
            .min(self.max_backoff_ms);
        Duration::from_millis(millis)
    }
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::bounded(10)
    }
}

pub struct Supervisor {
    token: CancellationToken,
    tasks: JoinSet<(String, Result<()>)>,
    started_at: DateTime<Utc>,
    completed: u64,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl Supervisor {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            tasks: JoinSet::new(),
            started_at: Utc::now(),
            completed: 0,
        }
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.token.clone()
    }

    pub fn is_shutting_down(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Spawn a supervised task. `factory` is invoked again after a retryable
    /// error. A task returning `Ok(())` is considered intentionally finished.
    pub fn spawn<F, Fut>(&mut self, name: impl Into<String>, policy: RestartPolicy, factory: F)
    where
        F: Fn(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        let name = name.into();
        let parent = self.token.clone();
        let task_name = name.clone();
        self.tasks.spawn(async move {
            let mut restarts = 0u32;
            loop {
                if parent.is_cancelled() {
                    return (task_name, Ok(()));
                }
                let run_token = parent.child_token();
                match factory(run_token).await {
                    Ok(()) => {
                        info!(task = %task_name, "supervised task finished");
                        return (task_name, Ok(()));
                    }
                    Err(_) if parent.is_cancelled() => {
                        info!(task = %task_name, "supervised task cancelled");
                        return (task_name, Ok(()));
                    }
                    Err(err) if err.is_retryable() && restarts < policy.max_restarts => {
                        restarts += 1;
                        let backoff = policy.backoff(restarts);
                        warn!(
                            task = %task_name,
                            attempt = restarts,
                            backoff_ms = backoff.as_millis(),
                            error = %err,
                            "supervised task failed; restarting"
                        );
                        tokio::select! {
                            _ = parent.cancelled() => return (task_name, Ok(())),
                            _ = tokio::time::sleep(backoff) => {}
                        }
                    }
                    Err(err) => {
                        error!(task = %task_name, error = %err, "supervised task failed permanently");
                        return (task_name, Err(err));
                    }
                }
            }
        });
        info!(task = %name, "supervised task started");
    }

    /// Spawn a task without restart. Useful for one-shot boot tasks.
    pub fn spawn_once<F, Fut>(&mut self, name: impl Into<String>, factory: F)
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        let name = name.into();
        let token = self.token.child_token();
        let task_name = name.clone();
        self.tasks.spawn(async move {
            let result = factory(token).await;
            (task_name, result)
        });
        info!(task = %name, "one-shot task started");
    }

    pub async fn wait(&mut self) -> Result<()> {
        while let Some(joined) = self.tasks.join_next().await {
            match joined {
                Ok((name, Ok(()))) => {
                    self.completed += 1;
                    info!(task = %name, "task completed");
                }
                Ok((name, Err(err))) => {
                    self.token.cancel();
                    return Err(BeastError::Internal(format!(
                        "supervised task {name} failed: {err}"
                    )));
                }
                Err(join_err) if join_err.is_cancelled() => {}
                Err(join_err) => {
                    self.token.cancel();
                    return Err(BeastError::Internal(format!(
                        "supervised task panicked: {join_err}"
                    )));
                }
            }
        }
        Ok(())
    }

    pub async fn shutdown(&mut self) {
        self.token.cancel();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while let Ok(Some(_)) = tokio::time::timeout_at(deadline, self.tasks.join_next()).await {}
        if !self.tasks.is_empty() {
            warn!(
                remaining = self.tasks.len(),
                "aborting tasks that did not stop within shutdown deadline"
            );
            self.tasks.abort_all();
        }
    }

    pub fn health(&self) -> SupervisorHealth {
        SupervisorHealth {
            started_at: self.started_at,
            tasks_running: self.tasks.len(),
            completed: self.completed,
            shutting_down: self.token.is_cancelled(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorHealth {
    pub started_at: DateTime<Utc>,
    pub tasks_running: usize,
    pub completed: u64,
    pub shutting_down: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn task_finishes_cleanly() {
        let mut supervisor = Supervisor::new();
        let count = Arc::new(AtomicU32::new(0));
        let task_count = count.clone();
        supervisor.spawn_once("one", move |_token| async move {
            task_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        supervisor.wait().await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retryable_task_restarts() {
        let mut supervisor = Supervisor::new();
        let attempts = Arc::new(AtomicU32::new(0));
        let task_attempts = attempts.clone();
        supervisor.spawn(
            "flaky",
            RestartPolicy {
                max_restarts: 3,
                initial_backoff_ms: 1,
                max_backoff_ms: 2,
            },
            move |_token| {
                let attempts = task_attempts.clone();
                async move {
                    let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                    if attempt == 0 {
                        Err(BeastError::Rpc("temporary".into()))
                    } else {
                        Ok(())
                    }
                }
            },
        );
        supervisor.wait().await.unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cancellation_stops_blocked_task() {
        let mut supervisor = Supervisor::new();
        supervisor.spawn("blocked", RestartPolicy::bounded(0), |token| async move {
            token.cancelled().await;
            Ok(())
        });
        supervisor.shutdown().await;
        assert!(supervisor.health().shutting_down);
    }
}
