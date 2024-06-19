# Changelog

### v0.1.0

- impl `Clone` for `TokenBucketRateLimiter`
    - allows cloned structs containing the rate limiter to share the same underlying limiter state
