//! KNX Temperature Records
//!
//! Contains temperature sensor data structure and utilities.
//!
//! This module is no_std by default and works in both embedded and std environments.

extern crate alloc;
use alloc::string::String;
use serde::{Deserialize, Serialize};

// ============================================================================
// DATA TYPE
// ============================================================================

/// KNX temperature sensor reading (DPT 9.001 - 2-byte float)
///
/// Represents a temperature measurement from a KNX sensor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Temperature {
    /// KNX group address (e.g., "9/1/0")
    pub address: String,

    /// Temperature in Celsius
    pub celsius: f32,
}

// ============================================================================
// CONSTRUCTOR
// ============================================================================

impl Temperature {
    /// MQTT topic for publishing temperature readings
    pub const MQTT_TOPIC: &'static str = "mqtt://knx/temperature/state";

    /// Create a new Temperature reading
    pub fn new(address: String, celsius: f32) -> Self {
        Self { address, celsius }
    }
}

// ============================================================================
// SERIALIZATION
// ============================================================================

pub mod json {
    use super::*;
    use alloc::vec::Vec;

    /// Serialize Temperature to JSON
    pub fn serialize(temp: &Temperature) -> Result<Vec<u8>, String> {
        let mut buf = [0u8; 128];
        serde_json_core::to_slice(temp, &mut buf)
            .map(|len| buf[..len].to_vec())
            .map_err(|_| String::from("Serialization buffer too small"))
    }

    /// Deserialize Temperature from JSON
    pub fn deserialize(data: &[u8]) -> Result<Temperature, String> {
        serde_json_core::from_slice(data)
            .map(|(temp, _)| temp)
            .map_err(|_| String::from("Deserialization failed"))
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

    /// Monitor for Temperature changes
    ///
    /// Logs all incoming temperature readings.
    /// Works with any runtime adapter (Tokio, Embassy, etc.).
    /// Can be used as a tap in aimdb configuration.
    pub async fn monitor<R: Runtime>(ctx: RuntimeContext<R>, consumer: Consumer<Temperature, R>) {
        let log = ctx.log();
        log.info("🌡️  Temperature monitor started");

        let Ok(mut reader) = consumer.subscribe() else {
            log.error("Failed to subscribe to Temperature buffer");
            return;
        };

        while let Ok(temp) = reader.recv().await {
            log.info(&format!(
                "🌡️  Temperature: {} = {:.1}°C",
                temp.address, temp.celsius
            ));
        }
    }
}

// ============================================================================
// KNX-SPECIFIC DESERIALIZATION (for gateway)
// ============================================================================

#[cfg(feature = "knx")]
pub mod knx {
    use super::*;
    use alloc::string::String as AllocString;

    /// Deserialize Temperature from KNX DPT 9.001 (2-byte float)
    ///
    /// Decodes the raw KNX telegram bytes using DPT 9.001 format.
    ///
    /// # Arguments
    /// * `data` - Raw KNX telegram bytes (2 bytes for DPT 9.001)
    /// * `group_address` - KNX group address (e.g., "9/1/0")
    pub fn from_knx(data: &[u8], group_address: &str) -> Result<Temperature, String> {
        use aimdb_knx_connector::dpt::{Dpt9, DptDecode};

        let celsius = Dpt9::Temperature.decode(data).unwrap_or(0.0);

        Ok(Temperature {
            address: AllocString::from(group_address),
            celsius,
        })
    }
}
