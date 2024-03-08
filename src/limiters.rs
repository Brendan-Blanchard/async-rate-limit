/// A per-call rate limiter, where each call has an implied cost of 1 permit.
///
/// See [`SlidingWindowRateLimiter`] for a common use case.
///
/// [`SlidingWindowRateLimiter`]: ../sliding_window/struct.SlidingWindowRateLimiter.html
pub trait RateLimiter {
    fn wait_until_ready(&mut self) -> impl std::future::Future<Output = ()> + Send;
}

/// A cost-based rate limiter, where each call can be of a variable cost.
///
/// Calls that cost more than all potentially available permits WILL deadlock permanently.
///
///  See [`TokenBucketRateLimiter`] for a common use case.
///
///  [`TokenBucketRateLimiter`]: ../token_bucket/struct.TokenBucketRateLimiter.html
pub trait VariableCostRateLimiter {
    fn wait_with_cost(&mut self, cost: usize) -> impl std::future::Future<Output = ()> + Send;
}
