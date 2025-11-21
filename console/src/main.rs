//! Home Automation Console
//!
//! An aimdb-based console that integrates with KNX home automation systems.
//! This console:
//! - Connects to KNX Gateway (STM32) via MQTT
//! - Provides remote access for LLM integration via AimX protocol
//! - Exposes KNX device states and controls through a Unix domain socket
//!
//! ## Architecture
//!
//! ```text
//! LLM (Pilot - Claude/Copilot)
//!   ↓ MCP protocol
//! aimdb-mcp client
//!   ↓ Unix socket (AimX v1)
//! console (this)
//!   ↓ MQTT
//! KNX Gateway (STM32)
//!   ↓ KNX bus
//! KNX devices
//! ```
//!
//! ## Usage
//!
//! ```bash
//! cargo run --release
//! ```
//!
//! The server will:
//! 1. Connect to MQTT broker for KNX gateway communication
//! 2. Enable remote access on `/tmp/knx-mcp.sock`
//! 3. Register KNX device records (lights, sensors, etc.)
//! 4. Handle bidirectional communication between LLM and KNX devices

use aimdb_core::remote::{AimxConfig, SecurityPolicy};
use aimdb_core::{buffer::BufferCfg, AimDbBuilder};
use aimdb_mqtt_connector::MqttConnector;
use aimdb_tokio_adapter::{TokioAdapter, TokioRecordRegistrarExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

/// KNX light state (DPT 1.001 - boolean on/off)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LightState {
    /// Group address (e.g., "1/0/6")
    address: String,
    /// Light on/off state
    is_on: bool,
    /// Last update timestamp
    timestamp: u64,
}

/// KNX switch state (DPT 1.001 - button press)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SwitchEvent {
    /// Group address (e.g., "1/0/7")
    address: String,
    /// Button pressed (true) or released (false)
    pressed: bool,
    /// Event timestamp
    timestamp: u64,
}

/// KNX temperature sensor (DPT 9.001 - 2-byte float)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Temperature {
    /// Group address (e.g., "9/1/0")
    address: String,
    /// Temperature in Celsius
    celsius: f32,
    /// Measurement timestamp
    timestamp: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("🚀 Starting Home Automation Console");
    info!("📡 Home automation with LLM integration");

    // Create runtime adapter
    let adapter = Arc::new(TokioAdapter);

    // Configure remote access for MCP
    let socket_path = "/tmp/console.sock";
    
    // Remove existing socket if present
    let _ = std::fs::remove_file(socket_path);

    // Configure security: read-write access for controllable devices
    let mut security_policy = SecurityPolicy::read_write();
    security_policy.allow_write::<LightState>(); // Lights can be controlled
    
    let remote_config = AimxConfig::uds_default()
        .socket_path(socket_path)
        .security_policy(security_policy)
        .max_connections(5)
        .subscription_queue_size(100);

    info!("📡 Remote access socket: {}", socket_path);
    info!("🔒 Security policy: ReadWrite (lights controllable)");

    // Initialize MQTT connector for communicating with KNX Gateway
    let mqtt_broker = std::env::var("MQTT_BROKER").unwrap_or_else(|_| "mqtt://192.168.1.7:1883".to_string());
    info!("📡 Connecting to MQTT broker: {}", mqtt_broker);
    
    let mqtt_connector = MqttConnector::new(&mqtt_broker)
        .with_client_id("home-automation-console");

    // Build database with remote access and MQTT connector
    let mut builder = AimDbBuilder::new()
        .runtime(adapter)
        .with_remote_access(remote_config)
        .with_connector(mqtt_connector);

    // Configure KNX device records (via MQTT communication with KNX Gateway)
    info!("⚙️  Configuring KNX device records...");

    // Living room light (controllable - publish control commands to MQTT)
    builder.configure::<LightState>(|reg| {
        reg.buffer(BufferCfg::SingleLatest)
            .with_serialization()
            // Publish light control commands to MQTT (consumed by KNX Gateway)
            .link_to("mqtt://knx/lights/control")
            .with_config("qos", "1")
            .with_config("retain", "false")
            .with_serializer(|state: &LightState| {
                // Serialize as JSON for MQTT
                serde_json::to_vec(state)
                    .map_err(|_| aimdb_core::connector::SerializeError::InvalidData)
            })
            .finish();
    });

    // Wall switch events (read-only - subscribe from MQTT published by KNX Gateway)
    builder.configure::<SwitchEvent>(|reg| {
        reg.buffer(BufferCfg::SpmcRing { capacity: 50 })
            .with_serialization()
            // Subscribe from MQTT topic (published by KNX Gateway)
            .link_from("mqtt://knx/lights/state")
            .with_config("qos", "1")
            .with_deserializer(|data: &[u8]| {
                // Parse JSON from KNX Gateway
                let json_str = std::str::from_utf8(data)
                    .map_err(|_| "Invalid UTF-8".to_string())?;
                
                // Parse JSON manually: {"group_address":"1/0/7","is_on":true,"timestamp":0}
                let mut address = String::new();
                let mut is_on = false;
                let mut timestamp = 0u64;
                
                for pair in json_str.trim_matches(|c| c == '{' || c == '}').split(',') {
                    let parts: Vec<&str> = pair.split(':').collect();
                    if parts.len() != 2 {
                        continue;
                    }
                    let key = parts[0].trim().trim_matches('"');
                    let value = parts[1].trim();
                    
                    match key {
                        "group_address" => {
                            address = value.trim_matches('"').to_string();
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
                
                Ok(SwitchEvent {
                    address,
                    pressed: is_on,
                    timestamp,
                })
            })
            .finish();
    });

    // Temperature sensor (read-only - subscribe from MQTT published by KNX Gateway)
    builder.configure::<Temperature>(|reg| {
        reg.buffer(BufferCfg::SingleLatest)
            .with_serialization()
            // Subscribe from MQTT topic (published by KNX Gateway)
            .link_from("mqtt://knx/temperature/state")
            .with_config("qos", "1")
            .with_deserializer(|data: &[u8]| {
                // Parse JSON from KNX Gateway
                let json_str = std::str::from_utf8(data)
                    .map_err(|_| "Invalid UTF-8".to_string())?;
                
                // Parse JSON manually: {"group_address":"9/1/0","celsius":21.5,"timestamp":0}
                let mut address = String::new();
                let mut celsius = 0.0f32;
                let mut timestamp = 0u64;
                
                for pair in json_str.trim_matches(|c| c == '{' || c == '}').split(',') {
                    let parts: Vec<&str> = pair.split(':').collect();
                    if parts.len() != 2 {
                        continue;
                    }
                    let key = parts[0].trim().trim_matches('"');
                    let value = parts[1].trim();
                    
                    match key {
                        "group_address" => {
                            address = value.trim_matches('"').to_string();
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
            })
            .finish();
    });

    let _db = builder.build().await?;

    info!("✅ Database initialized with KNX device records (via MQTT)");
    info!("   - LightState → mqtt://knx/lights/control (controllable via MCP)");
    info!("   - SwitchEvent ← mqtt://knx/lights/state (read-only monitoring)");
    info!("   - Temperature ← mqtt://knx/temperature/state (read-only monitoring)");
    info!("");
    info!("📡 MQTT Topics:");
    info!("   PUBLISH: mqtt://knx/lights/control (light commands to KNX Gateway)");
    info!("   SUBSCRIBE: mqtt://knx/lights/state (switch events from KNX Gateway)");
    info!("   SUBSCRIBE: mqtt://knx/temperature/state (temperature from KNX Gateway)");

    info!("");
    info!("🎯 Console ready!");
    info!("");
    info!("📚 Usage:");
    info!("   1. Use aimdb-mcp tool to discover this instance:");
    info!("      aimdb-mcp");
    info!("");
    info!("   2. Connect your LLM client (Claude Desktop, VS Code Copilot)");
    info!("      with the aimdb-mcp server configured");
    info!("");
    info!("   3. Ask natural language questions:");
    info!("      - 'What is the current temperature?'");
    info!("      - 'Turn on the living room light'");
    info!("      - 'Show me recent switch events'");
    info!("");
    info!("   4. Test manually:");
    info!("      echo '{{\"id\":1,\"method\":\"record.list\"}}' | socat - UNIX-CONNECT:{}", socket_path);
    info!("");
    info!("🔍 Monitoring:");
    info!("   - KNX bus activity will be logged");
    info!("   - All state changes are accessible via AimX protocol");
    info!("   - Subscribe to records for real-time updates");
    info!("");
    info!("Press Ctrl+C to stop the server");

    // Keep server running
    tokio::signal::ctrl_c().await?;

    info!("🛑 Shutting down Console...");

    Ok(())
}
