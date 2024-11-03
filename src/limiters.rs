use std::future::Future;

/// A per-call rate limiter, where each call has an implied cost of 1 permit.
///
/// See [`SlidingWindowRateLimiter`] for a common use case.
///
/// [`SlidingWindowRateLimiter`]: ../sliding_window/struct.SlidingWindowRateLimiter.html
pub trait RateLimiter {
    fn wait_until_ready(&mut self) -> impl Future<Output = ()> + Send;
}

/// A cost-based rate limiter, where each call can be of a variable cost.
///
/// Calls that cost more than all potentially available permits WILL deadlock permanently.
///
///  See [`TokenBucketRateLimiter`] for a common use case.
///
///  [`TokenBucketRateLimiter`]: ../token_bucket/struct.TokenBucketRateLimiter.html
pub trait VariableCostRateLimiter {
    fn wait_with_cost(&mut self, cost: usize) -> impl Future<Output = ()> + Send;
}

/// A threadsafe per-call rate limiter, where each call has an implied cost of 1 permit.
///
/// See [`SlidingWindowRateLimiter`] for a common use case.
///
/// [`SlidingWindowRateLimiter`]: ../sliding_window/struct.SlidingWindowRateLimiter.html
pub trait ThreadsafeRateLimiter {
    fn wait_until_ready(&self) -> impl Future<Output = ()> + Send;
}

/// All [`RateLimiter`]s are [`ThreadsafeRateLimiter`]s, since they can pass `&mut self` as `&self`.
///
/// This blanket implementation saves implementers from having to define the trait manually.
impl<L: ThreadsafeRateLimiter> RateLimiter for L {
    fn wait_until_ready(&mut self) -> impl Future<Output = ()> + Send {
        <Self as ThreadsafeRateLimiter>::wait_until_ready(self)
    }
}

/// A threadsafe cost-based rate limiter, where each call can be of a variable cost.
///
/// Calls that cost more than all potentially available permits WILL deadlock permanently.
///
///  See [`TokenBucketRateLimiter`] for a common use case.
///
///  [`TokenBucketRateLimiter`]: ../token_bucket/struct.TokenBucketRateLimiter.html
pub trait ThreadsafeVariableRateLimiter {
    fn wait_with_cost(&self, cost: usize) -> impl Future<Output = ()> + Send;
}

/// All [`VariableCostRateLimiter`]s are [`ThreadsafeVariableRateLimiter`]s, since they can pass `&mut self` as `&self`.
///
/// This blanket implementation saves implementers from having to define the trait manually.
impl<L: ThreadsafeVariableRateLimiter> VariableCostRateLimiter for L {
    fn wait_with_cost(&mut self, cost: usize) -> impl Future<Output = ()> + Send {
        <Self as ThreadsafeVariableRateLimiter>::wait_with_cost(self, cost)
    }
}
