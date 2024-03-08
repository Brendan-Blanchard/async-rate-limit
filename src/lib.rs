//! Common traits for specific rate limiting, with implementations of common rate limiting schemes.
//!
//! These traits and implementations were developed for a single set of use cases, and as
//! such may be missing behavior you would like to see implemented. Please reach out!

pub mod limiters;
pub mod sliding_window;
pub mod token_bucket;
