use crate::limiters::VariableCostRateLimiter;
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::Duration;

/// A classic [token bucket](https://en.wikipedia.org/wiki/Token_bucket) rate limiter that treats non-conformant calls by waiting indefinitely
/// for tokens to become available.
///
/// The token bucket scheme allows users to "burst" up to at most max-tokens during some period, but
/// replaces tokens at a fixed rate so users have some flexibility, but the overall load of the server
/// is still mediated.
///
/// The behavior of it's implementation of [`VariableCostRateLimiter`] can be seen in this
/// [`example`].
///
/// *Trying to acquire more than the possible available amount of tokens will deadlock.*
///
/// [`example`]: struct.TokenBucketRateLimiter.html#example-a-variable-cost-api-rate-limit
#[derive(Debug)]
pub struct TokenBucketRateLimiter {
    /// Potentially shared, mutable state required to implement the token bucket scheme.
    state: Arc<Mutex<TokenBucketState>>,
}

/// All required state that can be shared among many [`TokenBucketRateLimiters`](`TokenBucketRateLimiter`)
///
/// [`TokenBucketRateLimiter`]s take `Arc<Mutex<TokenBucketState>>` so a single state (including the replenishment task)
/// can be shared among many rate limiters, e.g. when a single API has multiple endpoints, each requiring different costs
/// but counting against the same user rate limit.
#[derive(Debug)]
pub struct TokenBucketState {
    /// Starting and max tokens in the bucket. This should NOT be shared outside of [`TokenBucketState`](`TokenBucketState`)
    tokens: Arc<Semaphore>,
    /// Number of tokens to be replaced every `replace_duration`
    replace_amount: usize,
    /// Duration after which `replace_amount` tokens are added to the bucket (unless it's full)
    replace_duration: Duration,
    /// Storage for acquired tokens
    acquired_tokens: Arc<Mutex<Vec<OwnedSemaphorePermit>>>,
    /// Handle to task that ticks on an interval, replacing `replace_amount` tokens every `replace_duration`
    replenish_task: Option<JoinHandle<()>>,
}

impl VariableCostRateLimiter for TokenBucketRateLimiter {
    /// # Example: A Variable Cost API Rate Limit
    ///
    /// An API enforces a rate limit by allotting 10 tokens, and replenishes used tokens at a rate of 1 per second.
    /// An endpoint being called requires 4 tokens per call.
    ///
    /// ```
    /// use tokio::time::{Instant, Duration};
    /// use async_rate_limit::limiters::VariableCostRateLimiter;
    /// use async_rate_limit::sliding_window::SlidingWindowRateLimiter;
    /// use async_rate_limit::token_bucket::{TokenBucketState, TokenBucketRateLimiter};
    /// use std::sync::Arc;
    /// use tokio::sync::Mutex;
    ///
    /// #[tokio::main]
    /// async fn main() -> () {
    ///     let state = TokenBucketState::new(10, 1, Duration::from_secs(1));
    ///     let state_mutex = Arc::new(Mutex::new(state));
    ///     let mut limiter = TokenBucketRateLimiter::new(state_mutex);
    ///     
    ///     // these calls proceed immediately, using 8 tokens
    ///     get_something(&mut limiter).await;
    ///     get_something(&mut limiter).await;
    ///
    ///     // this call waits ~2 seconds to acquire additional tokens before proceeding
    ///     get_something(&mut limiter).await;
    /// }
    ///
    /// // note the use of the `VariableCostRateLimiter` trait, rather than the direct type
    /// async fn get_something<T>(limiter: &mut T) where T: VariableCostRateLimiter {
    ///     limiter.wait_with_cost(4).await;
    ///     println!("{:?}", Instant::now());
    /// }
    /// ```
    ///
    async fn wait_with_cost(&mut self, cost: usize) {
        let mut state_guard = self.state.lock().await;
        (*state_guard).acquire_tokens(cost).await;
    }
}

impl Drop for TokenBucketState {
    fn drop(&mut self) {
        if let Some(task_handle) = self.replenish_task.take() {
            task_handle.abort();
        }
    }
}

impl TokenBucketState {
    /// Create a new [`TokenBucketState`] with a full bucket of `max_tokens` that will be replenished
    /// with `replace_amount` tokens every `replace_duration`.
    pub fn new(max_tokens: usize, replace_amount: usize, replace_duration: Duration) -> Self {
        TokenBucketState {
            tokens: Arc::new(Semaphore::new(max_tokens)),
            replace_amount,
            replace_duration,
            acquired_tokens: Arc::new(Mutex::new(vec![])),
            replenish_task: None,
        }
    }

    fn create_task_if_none(&mut self) {
        if self.replenish_task.is_none() {
            let acquired_tokens = self.acquired_tokens.clone();
            let replace_amount = self.replace_amount;
            let replace_duration = self.replace_duration;

            let handle = tokio::spawn(async move {
                Self::replenish_on_schedule(acquired_tokens, replace_amount, replace_duration)
                    .await;
            });
            self.replenish_task = Some(handle);
        }
    }

    async fn acquire_tokens(&mut self, n_tokens: usize) {
        self.create_task_if_none();

        let mut tokens = Vec::with_capacity(n_tokens);

        for _ in 0..n_tokens {
            let token = self
                .tokens
                .clone()
                .acquire_owned()
                .await
                .expect("Failed to acquire tokens.");

            tokens.push(token);
        }

        let mut acquired_tokens_guard = self.acquired_tokens.lock().await;

        (*acquired_tokens_guard).extend(tokens);
    }

    async fn replenish_on_schedule(
        acquired_tokens: Arc<Mutex<Vec<OwnedSemaphorePermit>>>,
        replace_amount: usize,
        replace_duration: Duration,
    ) {
        let mut interval = tokio::time::interval(replace_duration);

        // tick once to avoid instant replenishment on startup
        interval.tick().await;

        loop {
            interval.tick().await;
            TokenBucketState::release_tokens(replace_amount, acquired_tokens.clone()).await;
        }
    }

    async fn release_tokens(
        replace_amount: usize,
        acquired_tokens: Arc<Mutex<Vec<OwnedSemaphorePermit>>>,
    ) {
        let mut acquired_tokens_guard = acquired_tokens.lock().await;

        let release_amount = replace_amount.min(acquired_tokens_guard.len());
        let owned_tokens = (*acquired_tokens_guard).drain(0..release_amount);

        for token in owned_tokens.into_iter() {
            drop(token);
        }
    }
}

impl TokenBucketRateLimiter {
    /// Create a new [`TokenBucketRateLimiter`] using an established [`TokenBucketState`].
    ///
    /// `token_bucket_state` can be a reference for just this rate limiter, or it can be shared across
    /// many different rate limiters.
    pub fn new(token_bucket_state: Arc<Mutex<TokenBucketState>>) -> Self {
        TokenBucketRateLimiter {
            state: token_bucket_state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{pause, sleep, Instant};

    #[tokio::test]
    async fn test_proceeds_immediately_under_limit() {
        pause();
        let state = TokenBucketState::new(10, 3, Duration::from_secs(3));
        let state_mutex = Arc::new(Mutex::new(state));
        let mut limiter = TokenBucketRateLimiter::new(state_mutex);

        let start = Instant::now();

        limiter.wait_with_cost(2).await;
        limiter.wait_with_cost(2).await;
        limiter.wait_with_cost(2).await;
        limiter.wait_with_cost(2).await;
        limiter.wait_with_cost(2).await;

        let end = Instant::now();
        let elapsed = end - start;

        assert!(elapsed < Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_multi_cost_waits_at_limit() {
        pause();

        let state = TokenBucketState::new(10, 2, Duration::from_secs(3));
        let state_mutex = Arc::new(Mutex::new(state));
        let mut limiter = TokenBucketRateLimiter::new(state_mutex);

        let start = Instant::now();

        limiter.wait_with_cost(8).await;
        limiter.wait_with_cost(3).await;

        let end = Instant::now();
        let elapsed = end - start;

        assert!(elapsed > Duration::from_secs(3));
        assert!(elapsed < Duration::from_secs(4));
    }

    #[tokio::test]
    async fn test_bucket_does_not_overflow_over_time() {
        pause();

        let state = TokenBucketState::new(10, 2, Duration::from_secs(3));
        let state_mutex = Arc::new(Mutex::new(state));
        let mut limiter = TokenBucketRateLimiter::new(state_mutex);

        // bucket should not accumulate more than max tokens when not in use
        sleep(Duration::from_secs(180)).await;

        let start = Instant::now();

        limiter.wait_with_cost(8).await;
        limiter.wait_with_cost(3).await;

        let end = Instant::now();
        let elapsed = end - start;

        assert!(elapsed > Duration::from_secs(3));
        assert!(elapsed < Duration::from_secs(4));
    }

    #[tokio::test]
    async fn test_bucket_does_not_replace_over() {
        pause();


        let state = TokenBucketState::new(10, 100, Duration::from_secs(3));
        let state_mutex = Arc::new(Mutex::new(state));
        let mut limiter = TokenBucketRateLimiter::new(state_mutex);

        // bucket should not accumulate more than max tokens when not in use, and
        // replacing a large amount should not over-drain the permits Vec and panic
        sleep(Duration::from_secs(180)).await;

        let start = Instant::now();

        limiter.wait_with_cost(8).await;
        limiter.wait_with_cost(3).await;

        let end = Instant::now();
        let elapsed = end - start;

        assert!(elapsed > Duration::from_secs(3));
        assert!(elapsed < Duration::from_secs(4));
    }

    #[tokio::test]
    async fn test_many_waiters() {
        pause();
        let start = Instant::now();

        let mut tasks = vec![];

        let state = TokenBucketState::new(10, 2, Duration::from_secs(3));
        let state_mutex = Arc::new(Mutex::new(state));

        for _ in 0..10 {
            let task_mutex = state_mutex.clone();

            let task = tokio::spawn(async move {
                let mut limiter = TokenBucketRateLimiter::new(task_mutex);
                limiter.wait_with_cost(5).await;
            });
            tasks.push(task);
        }

        for task in tasks.into_iter() {
            let _ = task.await;
        }

        let end = Instant::now();
        let duration = end - start;

        // 10 tasks * 5 cost per = 50 tokens
        //  40 needed after initial 10 tokens spent
        //  40 needed / 2 replace = 20 waits of 3s = 60s

        assert!(duration > Duration::from_secs(60));
        assert!(duration < Duration::from_secs(61));
    }
}
