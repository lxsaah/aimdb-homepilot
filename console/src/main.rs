//! Home Automation Console
//!
//! An aimdb-based console that integrates with KNX home automation systems.
//! This console:
//! - Connects to a KNX/IP gateway via MQTT bridge
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
use aimdb_knx_connector::dpt::{Dpt1, Dpt9, DptDecode, DptEncode};
use aimdb_knx_connector::KnxConnector;
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

    // Initialize KNX connector (KNX/IP gateway communication)
    let knx_gateway = std::env::var("KNX_GATEWAY").unwrap_or_else(|_| "knx://192.168.1.19:3671".to_string());
    info!("🔌 Connecting to KNX/IP gateway: {}", knx_gateway);
    
    let knx_connector = KnxConnector::new(&knx_gateway);

    // Build database with remote access and KNX connector
    let mut builder = AimDbBuilder::new()
        .runtime(adapter)
        .with_remote_access(remote_config)
        .with_connector(knx_connector);

    // Configure KNX device records
    info!("⚙️  Configuring KNX device records...");

    // Living room light (controllable - outbound to KNX bus)
    builder.configure::<LightState>(|reg| {
        reg.buffer(BufferCfg::SingleLatest)
            .with_serialization()
            // Publish to KNX group address 1/0/6 (outbound)
            .link_to("knx://1/0/6")
            .with_serializer(|state: &LightState| {
                // Use DPT 1.001 (Switch) to encode boolean value
                let mut buf = [0u8; 1];
                let len = Dpt1::Switch.encode(state.is_on, &mut buf).unwrap_or(0);
                Ok(buf[..len].to_vec())
            })
            .finish();
    });

    // Wall switch events (read-only - inbound from KNX bus)
    builder.configure::<SwitchEvent>(|reg| {
        reg.buffer(BufferCfg::SpmcRing { capacity: 50 })
            .with_serialization()
            // Subscribe from KNX group address 1/0/7 (inbound)
            .link_from("knx://1/0/7")
            .with_deserializer(|data: &[u8]| {
                // Use DPT 1.001 (Switch) to decode boolean value
                let pressed = Dpt1::Switch.decode(data).unwrap_or(false);
                Ok(SwitchEvent {
                    address: "1/0/7".to_string(),
                    pressed,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                })
            })
            .finish();
    });

    // Temperature sensor (read-only - inbound from KNX bus)
    builder.configure::<Temperature>(|reg| {
        reg.buffer(BufferCfg::SingleLatest)
            .with_serialization()
            // Subscribe from KNX group address 9/1/0 (inbound)
            .link_from("knx://9/1/0")
            .with_deserializer(|data: &[u8]| {
                // Use DPT 9.001 (Temperature) to decode 2-byte float
                let celsius = Dpt9::Temperature.decode(data).unwrap_or(0.0);
                Ok(Temperature {
                    address: "9/1/0".to_string(),
                    celsius,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                })
            })
            .finish();
    });

    let _db = builder.build().await?;

    info!("✅ Database initialized with KNX device records");
    info!("   - LightState (1/0/6) - controllable via MCP");
    info!("   - SwitchEvent (1/0/7) - read-only monitoring");
    info!("   - Temperature (9/1/0) - read-only monitoring");

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
