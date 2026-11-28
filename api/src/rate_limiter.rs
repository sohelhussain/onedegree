use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::errors::ApiError;

#[derive(Clone, Default)]
pub struct RateLimiter {
    attempts: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn check(
        &self,
        key: &str,
        max_attempts: usize,
        window: Duration,
    ) -> Result<(), ApiError> {
        let mut map = self.attempts.lock().await;
        let now = Instant::now();
        let entries = map.entry(key.to_string()).or_default();

        // Evict timestamps outside the sliding window
        entries.retain(|&t| now.duration_since(t) < window);

        if entries.len() >= max_attempts {
            return Err(ApiError::TooManyRequests(
                "Too many requests. Please wait a moment before trying again.".into(),
            ));
        }

        entries.push(now);
        Ok(())
    }
}
