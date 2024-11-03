use std::sync::Arc;

use crate::limiters::{
    ThreadsafeRateLimiter, ThreadsafeVariableRateLimiter, VariableCostRateLimiter,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Duration;

/// A rate limiter that records calls (optionally with a specified cost) during a sliding window `Duration`.
///
/// [`SlidingWindowRateLimiter`] implements both [`RateLimiter`] and [`VariableCostRateLimiter`], so both
/// [`SlidingWindowRateLimiter::wait_until_ready()`] and [`SlidingWindowRateLimiter::wait_with_cost()`] can be used (even together). For instance, `limiter.wait_until_ready().await`
///  and `limiter.wait_with_cost(1).await` would have the same effect.
///
/// # Example: Simple Rate Limiter
///
/// A method that calls an external API with a rate limit should not be called more than three times per second.
/// ```
/// use tokio::time::{Instant, Duration};
/// use async_rate_limit::limiters::RateLimiter;
/// use async_rate_limit::sliding_window::SlidingWindowRateLimiter;
///
/// #[tokio::main]
/// async fn main() -> () {
///     let mut limiter = SlidingWindowRateLimiter::new(Duration::from_secs(1), 3);
///     
///     for _ in 0..4 {
///         // the 4th call will take place ~1 second after the first call
///         limited_method(&mut limiter).await;
///     }
/// }
///
/// // note the use of the `RateLimiter` trait, rather than the direct type
/// async fn limited_method<T>(limiter: &mut T) where T: RateLimiter {
///     limiter.wait_until_ready().await;
///     println!("{:?}", Instant::now());
/// }
/// ```
///
#[derive(Clone, Debug)]
pub struct SlidingWindowRateLimiter {
    /// Length of the window, i.e. a permit acquired at T is released at T + window
    window: Duration,
    /// Number of calls (or cost thereof) allowed during a given sliding window
    permits: Arc<Semaphore>,
}

impl SlidingWindowRateLimiter {
    /// Creates a new `SlidingWindowRateLimiter` that allows `limit` calls during any `window` Duration.
    pub fn new(window: Duration, limit: usize) -> Self {
        let permits = Arc::new(Semaphore::new(limit));

        SlidingWindowRateLimiter { window, permits }
    }

    /// Creates a new `SlidingWindowRateLimiter` with an externally provided `Arc<Semaphore>` for permits.
    /// # Example: A Shared Variable Cost Rate Limiter
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tokio::sync::Semaphore;
    /// use async_rate_limit::limiters::VariableCostRateLimiter;
    /// use async_rate_limit::sliding_window::SlidingWindowRateLimiter;
    /// use tokio::time::{Instant, Duration};
    ///
    /// #[tokio::main]
    /// async fn main() -> () {
    ///     let permits = Arc::new(Semaphore::new(5));
    ///     let mut limiter1 =
    ///     SlidingWindowRateLimiter::new_with_permits(Duration::from_secs(2), permits.clone());
    ///     let mut limiter2 =
    ///     SlidingWindowRateLimiter::new_with_permits(Duration::from_secs(2), permits.clone());
    ///
    ///     // Note: the above is semantically equivalent to creating `limiter1` with
    ///     //  `SlidingWindowRateLimiter::new`, then cloning it.
    ///
    ///     limiter1.wait_with_cost(3).await;
    ///     // the second call will wait 2s, since the first call consumed 3/5 shared permits
    ///     limiter2.wait_with_cost(3).await;
    /// }
    /// ```
    pub fn new_with_permits(window: Duration, permits: Arc<Semaphore>) -> Self {
        SlidingWindowRateLimiter { window, permits }
    }

    async fn drop_permit_after_window(window: Duration, permit: OwnedSemaphorePermit) {
        tokio::time::sleep(window).await;
        drop(permit);
    }
}

impl ThreadsafeRateLimiter for SlidingWindowRateLimiter {
    /// Wait with an implied cost of 1, see the [initial example](#example-simple-rate-limiter)
    async fn wait_until_ready(&self) {
        let permit = self
            .permits
            .clone()
            .acquire_owned()
            .await
            .expect("Failed to acquire permit for call");

        tokio::spawn(Self::drop_permit_after_window(self.window, permit));
    }
}

impl ThreadsafeVariableRateLimiter for SlidingWindowRateLimiter {
    /// Wait with some variable cost per usage.
    ///
    /// # Example: A Shared Variable Cost Rate Limiter
    ///
    /// An API specifies that you may make 5 "calls" per second, but some endpoints cost more than one call.
    /// - `/lite` costs 1 unit per call
    /// - `/heavy` costs 3 units per call
    /// ```
    /// use tokio::time::{Instant, Duration};
    /// use async_rate_limit::limiters::VariableCostRateLimiter;
    /// use async_rate_limit::sliding_window::SlidingWindowRateLimiter;
    ///
    /// #[tokio::main]
    /// async fn main() -> () {
    ///     let mut limiter = SlidingWindowRateLimiter::new(Duration::from_secs(1), 5);
    ///     
    ///     for _ in 0..3 {
    ///         // these will proceed immediately, spending 3 units
    ///         get_lite(&mut limiter).await;
    ///     }
    ///
    ///     // 3/5 units are spent, so this will wait for ~1s to proceed since it costs another 3
    ///     get_heavy(&mut limiter).await;
    /// }
    ///
    /// // note the use of the `VariableCostRateLimiter` trait, rather than the direct type
    /// async fn get_lite<T>(limiter: &mut T) where T: VariableCostRateLimiter {
    ///     limiter.wait_with_cost(1).await;
    ///     println!("Lite: {:?}", Instant::now());
    /// }
    ///
    /// async fn get_heavy<T>(limiter: &mut T) where T: VariableCostRateLimiter {
    ///     limiter.wait_with_cost(3).await;
    ///     println!("Heavy: {:?}", Instant::now());
    /// }
    /// ```
    async fn wait_with_cost(&self, cost: usize) {
        let permits = self
            .permits
            .clone()
            .acquire_many_owned(cost as u32)
            .await
            .unwrap_or_else(|_| panic!("Failed to acquire {} permits for call", cost));

        tokio::spawn(Self::drop_permit_after_window(self.window, permits));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{pause, Instant};

    mod rate_limiter_tests {
        use super::*;
        use crate::limiters::{RateLimiter, ThreadsafeRateLimiter};

        #[tokio::test]
        async fn test_proceeds_immediately_below_limit() {
            let limiter = SlidingWindowRateLimiter::new(Duration::from_secs(3), 7);

            let start = Instant::now();

            for _ in 0..7 {
                limiter.wait_until_ready().await;
            }

            let end = Instant::now();

            let duration = end - start;

            assert!(duration > Duration::from_secs(0));
            assert!(duration < Duration::from_millis(100));
        }

        #[tokio::test]
        async fn test_waits_at_limit() {
            pause();

            let limiter = SlidingWindowRateLimiter::new(Duration::from_secs(1), 3);

            let start = Instant::now();

            for _ in 0..10 {
                limiter.wait_until_ready().await;
            }

            let end = Instant::now();

            let duration = end - start;

            assert!(duration > Duration::from_secs(3));
            assert!(duration < Duration::from_secs(4));
        }

        #[tokio::test]
        async fn test_many_simultaneous_waiters() {
            pause();

            let limiter = SlidingWindowRateLimiter::new(Duration::from_secs(1), 3);

            let start = Instant::now();

            let mut tasks = vec![];

            for _ in 0..10 {
                let limiter_clone = Arc::new(tokio::sync::Mutex::new(limiter.clone()));

                let task = tokio::spawn(async move {
                    let limiter = limiter_clone.lock().await;

                    (*limiter).wait_until_ready().await;
                });
                tasks.push(task);
            }

            for task in tasks.into_iter() {
                let _ = task.await;
            }

            let end = Instant::now();

            let duration = end - start;

            assert!(duration > Duration::from_secs(3));
            assert!(duration < Duration::from_secs(4));
        }

        #[tokio::test]
        async fn test_trait_threadsafe_bounds() {
            let limiter = SlidingWindowRateLimiter::new(Duration::from_secs(3), 7);

            assert_threadsafe(&limiter).await;
        }

        #[tokio::test]
        async fn test_trait_non_threadsafe_bounds() {
            let mut limiter = SlidingWindowRateLimiter::new(Duration::from_secs(3), 7);

            assert_non_threadsafe(&mut limiter).await;
        }

        async fn assert_threadsafe<T: ThreadsafeRateLimiter>(limiter: &T) {
            let start = Instant::now();

            for _ in 0..7 {
                limiter.wait_until_ready().await;
            }

            let end = Instant::now();

            let duration = end - start;

            assert!(duration > Duration::from_secs(0));
            assert!(duration < Duration::from_millis(100));
        }

        async fn assert_non_threadsafe<T: RateLimiter>(limiter: &mut T) {
            let start = Instant::now();

            for _ in 0..7 {
                limiter.wait_until_ready().await;
            }

            let end = Instant::now();

            let duration = end - start;

            assert!(duration > Duration::from_secs(0));
            assert!(duration < Duration::from_millis(100));
        }
    }

    mod variable_cost_rate_limiter_tests {
        use super::*;
        use crate::limiters::ThreadsafeVariableRateLimiter;

        #[tokio::test]
        async fn test_proceeds_immediately_below_limit() {
            let limiter = SlidingWindowRateLimiter::new(Duration::from_secs(3), 7);

            let start = Instant::now();

            for _ in 0..3 {
                limiter.wait_with_cost(2).await;
            }

            let end = Instant::now();

            let duration = end - start;

            assert!(duration > Duration::from_secs(0));
            assert!(duration < Duration::from_millis(100));
        }

        #[tokio::test]
        async fn test_waits_at_limit() {
            pause();

            let limiter = SlidingWindowRateLimiter::new(Duration::from_secs(1), 3);

            let start = Instant::now();

            limiter.wait_with_cost(3).await;
            limiter.wait_with_cost(3).await;
            limiter.wait_with_cost(3).await;

            let end = Instant::now();

            let duration = end - start;

            assert!(duration > Duration::from_secs(2));
            assert!(duration < Duration::from_secs(3));
        }

        #[tokio::test]
        async fn test_with_threadsafe_bound() {
            pause();

            let limiter = SlidingWindowRateLimiter::new(Duration::from_secs(1), 3);

            assert_threadsafe(&limiter).await;
        }

        async fn assert_threadsafe<T>(limiter: &T)
        where
            T: ThreadsafeVariableRateLimiter,
        {
            let start = Instant::now();

            limiter.wait_with_cost(3).await;
            limiter.wait_with_cost(3).await;
            limiter.wait_with_cost(3).await;

            let end = Instant::now();

            let duration = end - start;

            assert!(duration > Duration::from_secs(2));
            assert!(duration < Duration::from_secs(3));
        }

        #[tokio::test]
        async fn test_waiters_with_shared_permits() {
            pause();

            let permits = Arc::new(Semaphore::new(5));
            let limiter1 =
                SlidingWindowRateLimiter::new_with_permits(Duration::from_secs(2), permits.clone());
            let limiter2 =
                SlidingWindowRateLimiter::new_with_permits(Duration::from_secs(2), permits.clone());

            let start = Instant::now();

            limiter1.wait_with_cost(3).await;
            limiter2.wait_with_cost(3).await;

            let end = Instant::now();

            let duration = end - start;

            assert!(duration > Duration::from_secs(2));
            assert!(duration < Duration::from_secs(3));
        }

        #[tokio::test]
        async fn test_many_waiters() {
            pause();

            let limiter = SlidingWindowRateLimiter::new(Duration::from_secs(1), 3);

            let start = Instant::now();

            let mut tasks = vec![];

            for _ in 0..10 {
                let limiter_clone = Arc::new(tokio::sync::Mutex::new(limiter.clone()));

                let task = tokio::spawn(async move {
                    let limiter = limiter_clone.lock().await;

                    (*limiter).wait_with_cost(3).await;
                });
                tasks.push(task);
            }

            for task in tasks.into_iter() {
                let _ = task.await;
            }

            let end = Instant::now();

            let duration = end - start;

            assert!(duration > Duration::from_secs(9));
            assert!(duration < Duration::from_secs(10));
        }
    }
}
