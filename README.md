# async-rate-limit
A basic Tokio rate-limiting library providing two traits and common implementations.

![badge](https://github.com/Brendan-Blanchard/async-rate-limit/actions/workflows/main.yml/badge.svg) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Usage:
```rust
use tokio::time::{Instant, Duration};
use async_rate_limit::limiters::VariableCostRateLimiter;
use async_rate_limit::sliding_window::SlidingWindowRateLimiter;

#[tokio::main]
async fn main() -> () {
    let mut limiter = SlidingWindowRateLimiter::new(Duration::from_secs(1), 5);
    
    for _ in 0..3 {
        // these will proceed immediately, spending 3 units
        get_lite(&mut limiter).await;
    }
    // 3/5 units are spent, so this will wait for ~1s to proceed since it costs another 3
    get_heavy(&mut limiter).await;
}

// note the use of the `VariableCostRateLimiter` trait, rather than the direct type
async fn get_lite<T>(limiter: &mut T) where T: VariableCostRateLimiter {
    limiter.wait_with_cost(1).await;
    println!("Lite: {:?}", Instant::now());
}

async fn get_heavy<T>(limiter: &mut T) where T: VariableCostRateLimiter {
    limiter.wait_with_cost(3).await;
    println!("Heavy: {:?}", Instant::now());
}
```
