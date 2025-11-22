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
use records::{SwitchControl, SwitchState, Temperature};
use std::sync::Arc;
use tracing::info;

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
    security_policy.allow_write::<SwitchControl>(); // Switch control commands can be sent

    let remote_config = AimxConfig::uds_default()
        .socket_path(socket_path)
        .security_policy(security_policy)
        .max_connections(5)
        .subscription_queue_size(100);

    info!("📡 Remote access socket: {}", socket_path);
    info!("🔒 Security policy: ReadWrite (switches controllable)");

    // Initialize MQTT connector for communicating with KNX Gateway
    let mqtt_broker =
        std::env::var("MQTT_BROKER").unwrap_or_else(|_| "mqtt://192.168.1.7:1883".to_string());
    info!("📡 Connecting to MQTT broker: {}", mqtt_broker);

    let mqtt_connector = MqttConnector::new(&mqtt_broker).with_client_id("home-automation-console");

    // Build database with remote access and MQTT connector
    let mut builder = AimDbBuilder::new()
        .runtime(adapter)
        .with_remote_access(remote_config)
        .with_connector(mqtt_connector);

    // Configure KNX device records (via MQTT communication with KNX Gateway)
    info!("⚙️  Configuring KNX device records...");

    // Switch state (read-only - subscribe from MQTT published by KNX Gateway)
    builder.configure::<SwitchState>(|reg| {
        reg.buffer(BufferCfg::SingleLatest)
            .with_serialization()
            // Subscribe from MQTT topic (published by KNX Gateway)
            .link_from(SwitchState::MQTT_TOPIC)
            .with_config("qos", "1")
            .with_deserializer(|data: &[u8]| records::switch::serde::deserialize_state(data))
            .finish();
    });

    // Switch control (controllable - publish control commands to MQTT)
    builder.configure::<SwitchControl>(|reg| {
        reg.buffer(BufferCfg::SpmcRing { capacity: 50 })
            .with_serialization()
            // Publish switch control commands to MQTT (consumed by KNX Gateway)
            .link_to(SwitchControl::MQTT_TOPIC)
            .with_config("qos", "1")
            .with_config("retain", "false")
            .with_serializer(|control: &SwitchControl| {
                records::switch::serde::serialize_control(control)
                    .map_err(|_| aimdb_core::connector::SerializeError::InvalidData)
            })
            .finish();
    });

    // Temperature sensor (read-only - subscribe from MQTT published by KNX Gateway)
    builder.configure::<Temperature>(|reg| {
        reg.buffer(BufferCfg::SingleLatest)
            .with_serialization()
            // Subscribe from MQTT topic (published by KNX Gateway)
            .link_from(Temperature::MQTT_TOPIC)
            .with_config("qos", "1")
            .with_deserializer(|data: &[u8]| records::temperature::serde::deserialize(data))
            .finish();
    });

    let _db = builder.build().await?;

    info!("✅ Database initialized with KNX device records (via MQTT)");
    info!(
        "   - SwitchState ← {} (read-only monitoring)",
        SwitchState::MQTT_TOPIC
    );
    info!(
        "   - SwitchControl → {} (controllable via MCP)",
        SwitchControl::MQTT_TOPIC
    );
    info!(
        "   - Temperature ← {} (read-only monitoring)",
        Temperature::MQTT_TOPIC
    );
    info!("");
    info!("📡 MQTT Topics:");
    info!(
        "   PUBLISH: {} (switch commands to KNX Gateway)",
        SwitchControl::MQTT_TOPIC
    );
    info!(
        "   SUBSCRIBE: {} (light state from KNX Gateway)",
        SwitchState::MQTT_TOPIC
    );
    info!(
        "   SUBSCRIBE: {} (temperature from KNX Gateway)",
        Temperature::MQTT_TOPIC
    );

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
    info!(
        "      echo '{{\"id\":1,\"method\":\"record.list\"}}' | socat - UNIX-CONNECT:{}",
        socket_path
    );
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
