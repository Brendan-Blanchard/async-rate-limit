# Changelog

### v0.1.1

- Add two threadsafe traits, `ThreadsafeRateLimiter` and `ThreadsafeVariableRateLimiter` as threadsafe counterparts to
  the existing traits
- Implement the threadsafe variants where possible

### v0.1.0

- impl `Clone` for `TokenBucketRateLimiter`
    - allows cloned structs containing the rate limiter to share the same underlying limiter state
