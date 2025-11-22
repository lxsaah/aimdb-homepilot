//! KNX Switch Records
//!
//! Contains all switch-related data structures and utilities:
//! - SwitchState: Current state of a KNX switch
//! - SwitchControl: Control commands for switches

#[cfg(feature = "std")]
use std::string::String;

#[cfg(not(feature = "std"))]
use heapless::String as HeaplessString;

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::{format, vec::Vec};

// ============================================================================
// DATA TYPES
// ============================================================================

/// KNX switch state (DPT 1.001 - boolean on/off)
/// 
/// Represents the current state of a KNX switch/actuator.
/// Published by the gateway when monitoring KNX bus activity.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "std", derive(PartialEq))]
#[cfg_attr(feature = "std", derive(crate::serde::Serialize, crate::serde::Deserialize))]
#[cfg_attr(not(feature = "std"), derive(crate::serde::Serialize, crate::serde::Deserialize))]
pub struct SwitchState {
    /// KNX group address (e.g., "1/0/7")
    #[cfg(feature = "std")]
    pub address: String,
    #[cfg(not(feature = "std"))]
    pub address: HeaplessString<16>,
    
    /// Switch on/off state
    pub is_on: bool,
    
    /// Timestamp of last update (milliseconds)
    pub timestamp: u64,
}

/// KNX switch control command (DPT 1.001)
/// 
/// Represents a control command to be sent to a KNX switch/actuator.
/// Consumed by the gateway to control KNX devices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "std", derive(PartialEq))]
#[cfg_attr(feature = "std", derive(crate::serde::Serialize, crate::serde::Deserialize))]
#[cfg_attr(not(feature = "std"), derive(crate::serde::Serialize, crate::serde::Deserialize))]
pub struct SwitchControl {
    /// KNX group address to control (e.g., "1/0/6")
    #[cfg(feature = "std")]
    pub address: String,
    #[cfg(not(feature = "std"))]
    pub address: HeaplessString<16>,
    
    /// Desired on/off state
    pub is_on: bool,
    
    /// Command timestamp (milliseconds)
    pub timestamp: u64,
}

// ============================================================================
// CONSTRUCTORS (std only)
// ============================================================================

impl SwitchState {
    /// MQTT topic for publishing switch state updates
    pub const MQTT_TOPIC: &'static str = "mqtt://knx/lights/state";
}

#[cfg(feature = "std")]
impl SwitchState {
    /// Create a new SwitchState
    pub fn new(address: impl Into<String>, is_on: bool) -> Self {
        Self {
            address: address.into(),
            is_on,
            timestamp: 0,
        }
    }
}

impl SwitchControl {
    /// MQTT topic for receiving switch control commands
    pub const MQTT_TOPIC: &'static str = "mqtt://knx/lights/control";
}

#[cfg(feature = "std")]
impl SwitchControl {
    /// Create a new SwitchControl command
    pub fn new(address: impl Into<String>, is_on: bool) -> Self {
        Self {
            address: address.into(),
            is_on,
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
    
    /// Serialize SwitchState to JSON
    pub fn serialize_state(state: &SwitchState) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(state)
    }
    
    /// Deserialize SwitchState from JSON
    pub fn deserialize_state(data: &[u8]) -> Result<SwitchState, String> {
        serde_json::from_slice(data)
            .map_err(|e| format!("Failed to deserialize SwitchState: {}", e))
    }
    
    /// Serialize SwitchControl to JSON
    pub fn serialize_control(control: &SwitchControl) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(control)
    }
    
    /// Deserialize SwitchControl from JSON
    pub fn deserialize_control(data: &[u8]) -> Result<SwitchControl, String> {
        serde_json::from_slice(data)
            .map_err(|e| format!("Failed to deserialize SwitchControl: {}", e))
    }
    

}

// ============================================================================
// SERIALIZATION - NO_STD
// ============================================================================

#[cfg(not(feature = "std"))]
pub mod serde {
    use super::*;
    
    /// Serialize SwitchState to JSON (manual formatting)
    pub fn serialize_state(state: &SwitchState) -> Result<Vec<u8>, alloc::string::String> {
        let json = format!(
            r#"{{"address":"{}","is_on":{},"timestamp":{}}}"#,
            state.address.as_str(),
            state.is_on,
            state.timestamp
        );
        Ok(json.into_bytes())
    }
    
    /// Deserialize SwitchState from JSON (manual parsing)
    pub fn deserialize_state(data: &[u8]) -> Result<SwitchState, alloc::string::String> {
        let json_str = core::str::from_utf8(data)
            .map_err(|_| alloc::string::String::from("Invalid UTF-8"))?;
        
        let mut address = HeaplessString::<16>::new();
        let mut is_on = false;
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
                "is_on" => {
                    is_on = value == "true";
                }
                "timestamp" => {
                    timestamp = value.parse().unwrap_or(0);
                }
                _ => {}
            }
        }
        
        Ok(SwitchState {
            address,
            is_on,
            timestamp,
        })
    }
    
    /// Serialize SwitchControl to JSON (manual formatting)
    pub fn serialize_control(control: &SwitchControl) -> Result<Vec<u8>, alloc::string::String> {
        let json = format!(
            r#"{{"address":"{}","is_on":{},"timestamp":{}}}"#,
            control.address.as_str(),
            control.is_on,
            control.timestamp
        );
        Ok(json.into_bytes())
    }
    
    /// Deserialize SwitchControl from JSON (manual parsing)
    pub fn deserialize_control(data: &[u8]) -> Result<SwitchControl, alloc::string::String> {
        let json_str = core::str::from_utf8(data)
            .map_err(|_| alloc::string::String::from("Invalid UTF-8"))?;
        
        let mut address = HeaplessString::<16>::new();
        let mut is_on = false;
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
                "is_on" => {
                    is_on = value == "true";
                }
                "timestamp" => {
                    timestamp = value.parse().unwrap_or(0);
                }
                _ => {}
            }
        }
        
        Ok(SwitchControl {
            address,
            is_on,
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
    
    /// Monitor for SwitchState changes
    /// 
    /// Logs all incoming switch state updates to the console.
    /// Can be used as a tap in aimdb configuration.
    pub async fn state_monitor(
        _ctx: RuntimeContext<TokioAdapter>,
        consumer: Consumer<SwitchState, TokioAdapter>,
    ) {
        info!("💡 Switch state monitor started");
        
        let Ok(mut reader) = consumer.subscribe() else {
            error!("Failed to subscribe to SwitchState buffer");
            return;
        };
        
        while let Ok(state) = reader.recv().await {
            info!(
                "💡 Switch state: {} = {}",
                state.address,
                if state.is_on { "ON ✨" } else { "OFF" }
            );
        }
    }
    
    /// Monitor for SwitchControl commands
    /// 
    /// Logs all outgoing switch control commands.
    pub async fn control_monitor(
        _ctx: RuntimeContext<TokioAdapter>,
        consumer: Consumer<SwitchControl, TokioAdapter>,
    ) {
        info!("📤 Switch control monitor started");
        
        let Ok(mut reader) = consumer.subscribe() else {
            error!("Failed to subscribe to SwitchControl buffer");
            return;
        };
        
        while let Ok(control) = reader.recv().await {
            info!(
                "📤 Switch control: {} = {}",
                control.address,
                if control.is_on { "ON" } else { "OFF" }
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
    
    /// Monitor for SwitchState changes (Embassy/embedded)
    pub async fn state_monitor(
        ctx: RuntimeContext<EmbassyAdapter>,
        consumer: Consumer<SwitchState, EmbassyAdapter>,
    ) {
        let log = ctx.log();
        log.info("💡 Switch state monitor started\n");
        
        let Ok(mut reader) = consumer.subscribe() else {
            log.error("Failed to subscribe to SwitchState buffer");
            return;
        };
        
        while let Ok(state) = reader.recv().await {
            log.info(&format!(
                "💡 KNX switch: {} = {}",
                state.address.as_str(),
                if state.is_on { "ON ✨" } else { "OFF" }
            ));
        }
    }
    
    /// Monitor for SwitchControl commands (Embassy/embedded)
    pub async fn control_monitor(
        ctx: RuntimeContext<EmbassyAdapter>,
        consumer: Consumer<SwitchControl, EmbassyAdapter>,
    ) {
        let log = ctx.log();
        log.info("📥 MQTT→KNX command monitor started...");
        
        let Ok(mut reader) = consumer.subscribe() else {
            log.error("Failed to subscribe to SwitchControl buffer");
            return;
        };
        
        while let Ok(cmd) = reader.recv().await {
            log.info(&format!(
                "📥 MQTT command → KNX: {} = {}",
                cmd.address.as_str(),
                if cmd.is_on { "ON" } else { "OFF" }
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
    
    /// Deserialize SwitchState from KNX DPT 1.001 (boolean)
    /// 
    /// Decodes the raw KNX telegram bytes using DPT 1.001 format.
    /// 
    /// # Arguments
    /// * `data` - Raw KNX telegram bytes (1 byte for DPT 1.001)
    /// * `group_address` - KNX group address (e.g., "1/0/7")
    pub fn deserialize_switch_state_from_knx(
        data: &[u8],
        group_address: &str,
    ) -> Result<SwitchState, alloc::string::String> {
        use aimdb_knx_connector::dpt::{Dpt1, DptDecode};
        
        let is_on = Dpt1::Switch.decode(data).unwrap_or(false);
        
        let mut address = HeaplessString::<16>::new();
        address.push_str(group_address)
            .map_err(|_| alloc::string::String::from("Group address too long"))?;
        
        Ok(SwitchState {
            address,
            is_on,
            timestamp: 0,
        })
    }
    
    /// Serialize SwitchControl to KNX DPT 1.001 (boolean)
    /// 
    /// Converts SwitchControl command to KNX bus format using DPT 1.001 encoder.
    pub fn serialize_switch_control_to_knx(
        control: &SwitchControl,
    ) -> Result<alloc::vec::Vec<u8>, alloc::string::String> {
        use aimdb_knx_connector::dpt::{Dpt1, DptEncode};
        
        let mut buf = [0u8; 1];
        let len = Dpt1::Switch.encode(control.is_on, &mut buf)
            .map_err(|_| alloc::string::String::from("Failed to encode DPT 1.001"))?;
        
        Ok(buf[..len].to_vec())
    }
}
