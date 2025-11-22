//! KNX Switch Records
//!
//! Contains all switch-related data structures and utilities:
//! - SwitchState: Current state of a KNX switch
//! - SwitchControl: Control commands for switches
//!
//! This module is no_std by default and works in both embedded and std environments.

extern crate alloc;
use alloc::string::String;
use serde::{Deserialize, Serialize};

// ============================================================================
// DATA TYPES
// ============================================================================

/// KNX switch state (DPT 1.001 - boolean on/off)
///
/// Represents the current state of a KNX switch/actuator.
/// Published by the gateway when monitoring KNX bus activity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwitchState {
    /// KNX group address (e.g., "1/0/7")
    pub address: String,

    /// Switch on/off state
    pub is_on: bool,
}

/// KNX switch control command (DPT 1.001)
///
/// Represents a control command to be sent to a KNX switch/actuator.
/// Consumed by the gateway to control KNX devices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwitchControl {
    /// KNX group address to control (e.g., "1/0/6")
    pub address: String,

    /// Desired on/off state
    pub is_on: bool,
}

// ============================================================================
// CONSTRUCTORS
// ============================================================================

impl SwitchState {
    /// MQTT topic for publishing switch state updates
    pub const MQTT_TOPIC: &'static str = "mqtt://knx/tv/state";

    /// Create a new SwitchState
    pub fn new(address: String, is_on: bool) -> Self {
        Self { address, is_on }
    }
}

impl SwitchControl {
    /// MQTT topic for receiving switch control commands
    pub const MQTT_TOPIC: &'static str = "mqtt://knx/tv/control";

    /// Create a new SwitchControl command
    pub fn new(address: String, is_on: bool) -> Self {
        Self { address, is_on }
    }
}

// ============================================================================
// SERIALIZATION
// ============================================================================

pub mod json {
    use super::*;
    use alloc::vec::Vec;

    /// Serialize SwitchState to JSON
    pub fn serialize_state(state: &SwitchState) -> Result<Vec<u8>, String> {
        #[cfg(feature = "std")]
        {
            serde_json::to_vec(state).map_err(|e| alloc::format!("Serialization failed: {}", e))
        }
        #[cfg(not(feature = "std"))]
        {
            let mut buf = [0u8; 128];
            serde_json_core::to_slice(state, &mut buf)
                .map(|len| buf[..len].to_vec())
                .map_err(|_| String::from("Serialization buffer too small"))
        }
    }

    /// Deserialize SwitchState from JSON
    pub fn deserialize_state(data: &[u8]) -> Result<SwitchState, String> {
        #[cfg(feature = "std")]
        {
            serde_json::from_slice(data).map_err(|e| alloc::format!("Deserialization failed: {}", e))
        }
        #[cfg(not(feature = "std"))]
        {
            serde_json_core::from_slice(data)
                .map(|(state, _)| state)
                .map_err(|_| String::from("Deserialization failed"))
        }
    }

    /// Serialize SwitchControl to JSON
    pub fn serialize_control(control: &SwitchControl) -> Result<Vec<u8>, String> {
        #[cfg(feature = "std")]
        {
            serde_json::to_vec(control).map_err(|e| alloc::format!("Serialization failed: {}", e))
        }
        #[cfg(not(feature = "std"))]
        {
            let mut buf = [0u8; 128];
            serde_json_core::to_slice(control, &mut buf)
                .map(|len| buf[..len].to_vec())
                .map_err(|_| String::from("Serialization buffer too small"))
        }
    }

    /// Deserialize SwitchControl from JSON
    pub fn deserialize_control(data: &[u8]) -> Result<SwitchControl, String> {
        #[cfg(feature = "std")]
        {
            serde_json::from_slice(data).map_err(|e| alloc::format!("Deserialization failed: {}", e))
        }
        #[cfg(not(feature = "std"))]
        {
            serde_json_core::from_slice(data)
                .map(|(control, _)| control)
                .map_err(|_| String::from("Deserialization failed"))
        }
    }
}

// ============================================================================
// MONITORS - Generic over runtime adapter
// ============================================================================

#[cfg(feature = "monitors")]
pub mod monitors {
    use super::*;
    use aimdb_core::{Consumer, Runtime, RuntimeContext};
    use alloc::format;

    /// Monitor for SwitchState changes
    ///
    /// Logs all incoming switch state updates.
    /// Works with any runtime adapter (Tokio, Embassy, etc.).
    /// Can be used as a tap in aimdb configuration.
    pub async fn state_monitor<R: Runtime>(
        ctx: RuntimeContext<R>,
        consumer: Consumer<SwitchState, R>,
    ) {
        let log = ctx.log();
        log.info("💡 Switch state monitor started");

        let Ok(mut reader) = consumer.subscribe() else {
            log.error("Failed to subscribe to SwitchState buffer");
            return;
        };

        while let Ok(state) = reader.recv().await {
            log.info(&format!(
                "💡 Switch: {} = {}",
                state.address,
                if state.is_on { "ON ✨" } else { "OFF" }
            ));
        }
    }

    /// Monitor for SwitchControl commands
    ///
    /// Logs all outgoing switch control commands.
    /// Works with any runtime adapter (Tokio, Embassy, etc.).
    pub async fn control_monitor<R: Runtime>(
        ctx: RuntimeContext<R>,
        consumer: Consumer<SwitchControl, R>,
    ) {
        let log = ctx.log();
        log.info("📤 Switch control monitor started");

        let Ok(mut reader) = consumer.subscribe() else {
            log.error("Failed to subscribe to SwitchControl buffer");
            return;
        };

        while let Ok(control) = reader.recv().await {
            log.info(&format!(
                "📤 Control: {} = {}",
                control.address,
                if control.is_on { "ON" } else { "OFF" }
            ));
        }
    }
}

// ============================================================================
// KNX-SPECIFIC SERIALIZATION (for gateway)
// ============================================================================

#[cfg(feature = "knx")]
pub mod knx {
    use super::*;
    use alloc::{string::String as AllocString, vec::Vec};

    /// Deserialize SwitchState from KNX DPT 1.001 (boolean)
    ///
    /// Decodes the raw KNX telegram bytes using DPT 1.001 format.
    ///
    /// # Arguments
    /// * `data` - Raw KNX telegram bytes (1 byte for DPT 1.001)
    /// * `group_address` - KNX group address (e.g., "1/0/7")
    pub fn from_knx(data: &[u8], group_address: &str) -> Result<SwitchState, String> {
        use aimdb_knx_connector::dpt::{Dpt1, DptDecode};

        let is_on = Dpt1::Switch.decode(data).unwrap_or(false);

        Ok(SwitchState {
            address: AllocString::from(group_address),
            is_on,
        })
    }

    /// Serialize SwitchControl to KNX DPT 1.001 (boolean)
    ///
    /// Converts SwitchControl command to KNX bus format using DPT 1.001 encoder.
    pub fn to_knx(control: &SwitchControl) -> Result<Vec<u8>, String> {
        use aimdb_knx_connector::dpt::{Dpt1, DptEncode};

        let mut buf = [0u8; 1];
        let len = Dpt1::Switch
            .encode(control.is_on, &mut buf)
            .map_err(|_| String::from("Failed to encode DPT 1.001"))?;

        Ok(buf[..len].to_vec())
    }
}
