#![no_std]

//! Canonical codecs and hashes for the disposable generic-effect capability
//! experiment.
//!
//! Every byte, label, limit, discriminator, seed, and identity in this crate is
//! private test machinery. Nothing here is a public ABI or compatibility
//! promise.

extern crate alloc;

mod codec;
mod commitments;
mod control;
mod domain;
mod envelope;
mod error;
mod fee;
mod hashes;
mod identity;
mod loader;
mod receipt;
mod request;
mod rows;

pub use commitments::*;
pub use control::*;
pub use domain::*;
pub use envelope::*;
pub use error::*;
pub use fee::*;
pub use hashes::*;
pub use identity::*;
pub use loader::*;
pub use receipt::*;
pub use request::*;
pub use rows::*;
