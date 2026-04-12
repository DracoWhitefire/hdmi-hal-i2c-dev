//! Linux userspace `hdmi-hal` backend backed by `/dev/i2c-N` via the `i2c-dev`
//! kernel interface.
//!
//! This crate is the development, validation, and diagnostic backend for the
//! HDMI stack — not the production path. See the crate-level documentation and
//! `doc/architecture.md` for full context.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod discovery;
pub mod error;
pub mod transport;

pub use discovery::connector_ddc_adapter;
pub use error::{I2cDevError, I2cErrorKind, I2cTransactionError, MessagePhase};
pub use transport::I2cDevTransport;
