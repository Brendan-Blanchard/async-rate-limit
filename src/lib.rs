//! Common traits for specific rate limiting, with implementations of common rate limiting schemes.
//!
//! This crate is in it's infancy and is currently nightly-only due to the need for async traits.
//! These traits and implementations were developed for a single set of use cases, and as
//! such may be missing behavior you would like to see implemented. Please reach out!

#![feature(async_fn_in_trait)]

pub mod limiters;
pub mod sliding_window;
pub mod token_bucket;
