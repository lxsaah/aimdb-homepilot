//! KNX Temperature Records
//!
//! Contains temperature sensor data structure and utilities.

#[cfg(feature = "std")]
use std::string::String;

#[cfg(not(feature = "std"))]
use heapless::String as HeaplessString;

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::{format, vec::Vec};

// ============================================================================
// DATA TYPE
// ============================================================================

/// KNX temperature sensor reading (DPT 9.001 - 2-byte float)
/// 
/// Represents a temperature measurement from a KNX sensor.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "std", derive(PartialEq))]
#[cfg_attr(feature = "std", derive(crate::serde::Serialize, crate::serde::Deserialize))]
#[cfg_attr(not(feature = "std"), derive(crate::serde::Serialize, crate::serde::Deserialize))]
pub struct Temperature {
    /// KNX group address (e.g., "9/1/0")
    #[cfg(feature = "std")]
    pub address: String,
    #[cfg(not(feature = "std"))]
    pub address: HeaplessString<16>,
    
    /// Temperature in Celsius
    pub celsius: f32,
    
    /// Measurement timestamp (milliseconds)
    pub timestamp: u64,
}

// ============================================================================
// CONSTRUCTOR (std only)
// ============================================================================

impl Temperature {
    /// MQTT topic for publishing temperature readings
    pub const MQTT_TOPIC: &'static str = "mqtt://knx/temperature/state";
}

#[cfg(feature = "std")]
impl Temperature {
    /// Create a new Temperature reading
    pub fn new(address: impl Into<String>, celsius: f32) -> Self {
        Self {
            address: address.into(),
            celsius,
            timestamp: 0,
        }
    }
}

// ============================================================================
// SERIALIZATION - STD
// ============================================================================

#[cfg(feature = "std")]
pub mod serde {
    use super::*;
    
    /// Serialize Temperature to JSON
    pub fn serialize(temp: &Temperature) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(temp)
    }
    
    /// Deserialize Temperature from JSON
    pub fn deserialize(data: &[u8]) -> Result<Temperature, String> {
        serde_json::from_slice(data)
            .map_err(|e| format!("Failed to deserialize Temperature: {}", e))
    }
}

// ============================================================================
// SERIALIZATION - NO_STD
// ============================================================================

#[cfg(not(feature = "std"))]
pub mod serde {
    use super::*;
    
    /// Serialize Temperature to JSON (manual formatting)
    pub fn serialize(temp: &Temperature) -> Result<Vec<u8>, alloc::string::String> {
        let json = format!(
            r#"{{"address":"{}","celsius":{:.2},"timestamp":{}}}"#,
            temp.address.as_str(),
            temp.celsius,
            temp.timestamp
        );
        Ok(json.into_bytes())
    }
    
    /// Deserialize Temperature from JSON (manual parsing)
    pub fn deserialize(data: &[u8]) -> Result<Temperature, alloc::string::String> {
        let json_str = core::str::from_utf8(data)
            .map_err(|_| alloc::string::String::from("Invalid UTF-8"))?;
        
        let mut address = HeaplessString::<16>::new();
        let mut celsius = 0.0f32;
        let mut timestamp = 0u64;
        
        for pair in json_str.trim_matches(|c| c == '{' || c == '}').split(',') {
            let parts: alloc::vec::Vec<&str> = pair.split(':').collect();
            if parts.len() != 2 {
                continue;
            }
            let key = parts[0].trim().trim_matches('"');
            let value = parts[1].trim();
            
            match key {
                "address" => {
                    let addr = value.trim_matches('"');
                    let _ = address.push_str(addr);
                }
                "celsius" => {
                    celsius = value.parse().unwrap_or(0.0);
                }
                "timestamp" => {
                    timestamp = value.parse().unwrap_or(0);
                }
                _ => {}
            }
        }
        
        Ok(Temperature {
            address,
            celsius,
            timestamp,
        })
    }
}

// ============================================================================
// MONITORS - STD with Tokio
// ============================================================================

#[cfg(feature = "std")]
pub mod monitors {
    use super::*;
    use tracing::{info, error};
    use aimdb_tokio_adapter::TokioAdapter;
    use aimdb_core::{Consumer, RuntimeContext};
    
    /// Monitor for Temperature changes
    /// 
    /// Logs all incoming temperature readings to the console.
    /// Can be used as a tap in aimdb configuration.
    pub async fn monitor(
        _ctx: RuntimeContext<TokioAdapter>,
        consumer: Consumer<Temperature, TokioAdapter>,
    ) {
        info!("🌡️  Temperature monitor started");
        
        let Ok(mut reader) = consumer.subscribe() else {
            error!("Failed to subscribe to Temperature buffer");
            return;
        };
        
        while let Ok(temp) = reader.recv().await {
            info!(
                "🌡️  Temperature: {} = {:.1}°C",
                temp.address,
                temp.celsius
            );
        }
    }
}

// ============================================================================
// MONITORS - NO_STD with Embassy
// ============================================================================

#[cfg(all(not(feature = "std"), feature = "embassy"))]
pub mod monitors {
    use super::*;
    use aimdb_embassy_adapter::EmbassyAdapter;
    use aimdb_core::{Consumer, RuntimeContext};
    
    /// Monitor for Temperature changes (Embassy/embedded)
    pub async fn monitor(
        ctx: RuntimeContext<EmbassyAdapter>,
        consumer: Consumer<Temperature, EmbassyAdapter>,
    ) {
        let log = ctx.log();
        log.info("🌡️  Temperature monitor started - watching KNX bus...\n");
        
        let Ok(mut reader) = consumer.subscribe() else {
            log.error("Failed to subscribe to temperature buffer");
            return;
        };
        
        while let Ok(temp) = reader.recv().await {
            log.info(&format!(
                "🌡️  KNX temperature: {} = {:.1}°C",
                temp.address.as_str(),
                temp.celsius
            ));
        }
    }
}

// ============================================================================
// KNX-SPECIFIC DESERIALIZATION (for gateway)
// ============================================================================

#[cfg(all(not(feature = "std"), feature = "embassy"))]
pub mod knx {
    use super::*;
    
    /// Deserialize Temperature from KNX DPT 9.001 (2-byte float)
    /// 
    /// Decodes the raw KNX telegram bytes using DPT 9.001 format.
    /// 
    /// # Arguments
    /// * `data` - Raw KNX telegram bytes (2 bytes for DPT 9.001)
    /// * `group_address` - KNX group address (e.g., "9/1/0")
    pub fn from_knx(
        data: &[u8],
        group_address: &str,
    ) -> Result<Temperature, alloc::string::String> {
        use aimdb_knx_connector::dpt::{Dpt9, DptDecode};
        
        let celsius = Dpt9::Temperature.decode(data).unwrap_or(0.0);
        
        let mut address = HeaplessString::<16>::new();
        address.push_str(group_address)
            .map_err(|_| alloc::string::String::from("Group address too long"))?;
        
        Ok(Temperature {
            address,
            celsius,
            timestamp: 0,
        })
    }
}
