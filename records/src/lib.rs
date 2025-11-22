//! KNX Home Automation Record Types
//!
//! This module defines the core data structures for KNX home automation
//! using aimdb. All record types support both `std` and `no_std` environments.
//!
//! ## Organization
//!
//! Each record type is organized in its own module containing:
//! - Data structures
//! - Serialization/deserialization functions
//! - Monitoring utilities (for logging/debugging)
//!
//! ## Modules
//!
//! - [`switch`]: Switch-related records (SwitchState, SwitchControl)
//! - [`temperature`]: Temperature sensor records
//!
//! ## Example Usage
//!
//! ```ignore
//! use records::switch::{SwitchState, SwitchControl};
//! use records::switch::serde::{serialize_state, deserialize_state};
//! use records::temperature::Temperature;
//!
//! // Create a switch state
//! let state = SwitchState::new("1/0/7", true);
//!
//! // Serialize to JSON
//! let json = serialize_state(&state)?;
//!
//! // Use monitor in aimdb tap
//! builder.configure::<SwitchState>(|reg| {
//!     reg.buffer(BufferCfg::SingleLatest)
//!         .tap(switch::monitors::state_monitor)
//!         // ...
//! });
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

// Re-export serde so derive macros work properly in submodules
pub use serde;

// Per-record modules
pub mod switch;
pub mod temperature;

// Re-export commonly used types for convenience
pub use switch::{SwitchControl, SwitchState};
pub use temperature::Temperature;
