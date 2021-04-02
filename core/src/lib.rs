#![cfg_attr(feature = "nightly", feature(specialization))]
//! Core structs and traits for paperclip.

#[cfg(feature = "actix2")]
extern crate actix_web2 as actix_web;
#[cfg(feature = "actix3")]
extern crate actix_web3 as actix_web;
#[cfg(feature = "actix4")]
extern crate actix_web4 as actix_web;
#[cfg_attr(feature = "v2", macro_use)]
extern crate serde;

mod error;
pub mod im;
pub mod util;
#[cfg(feature = "v2")]
pub mod v2;

pub use self::error::ValidationError;
