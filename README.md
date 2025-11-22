# aimdb-homepilot

AimDB-powered smart home demo using KNX on STM32, MQTT linking and LLM/MCP integration — a home automation "co-pilot" built in Rust.

## Architecture

This project demonstrates a layered home automation system with three main components:

- **ground**: STM32H563ZI-based KNX/IP gateway that bridges KNX devices to MQTT
  - Built with Embassy (async embedded Rust)
  - Monitors KNX bus and publishes state updates to MQTT
  - Receives MQTT commands and forwards to KNX devices
  - Runs directly on STM32 hardware

- **tower**: Control and monitoring console (PC application)
  - Connects to MQTT broker to communicate with ground
  - Exposes Unix domain socket for LLM integration via AimX protocol
  - Provides bidirectional bridge between MQTT and LLM interface
  - Built with Tokio async runtime

- **pilot**: LLM interface via aimdb-mcp server
  - Enables natural language control and monitoring
  - Connects to tower via Unix socket
  - Works with Claude Desktop, VS Code Copilot, and other MCP clients

**Data Flow**: KNX devices ↔ ground (STM32) ↔ MQTT ↔ tower (PC) ↔ Unix socket ↔ LLM

## Project Structure

```
aimdb-homepilot/
├── ground/          # STM32 KNX gateway (Embassy, no_std)
├── tower/           # PC console (Tokio, std)
├── records/         # Shared data types (no_std by default)
└── README.md
```

### Records Module

The `records` crate defines shared data types used across all components:

- **SwitchState**: Current state of KNX switches (monitoring)
- **SwitchControl**: Commands to control KNX switches
- **Temperature**: Temperature sensor readings

Each record type includes:
- Serde-compatible data structures (no_std)
- JSON serialization/deserialization
- KNX DPT encoding/decoding (for ground)
- Runtime-agnostic monitors (optional, for debugging)

## Development

### Prerequisites

This project uses a DevContainer for a consistent development environment. Use the DevContainer configuration from:
https://github.com/aimdb-dev/aimdb/blob/main/.devcontainer/devcontainer.json

### Documentation

For detailed information on using AimDB, refer to the official usage guide:
https://github.com/aimdb-dev/aimdb/blob/main/docs/aimdb-usage-guide.md

## Ground (KNX Gateway on STM32)

The ground component is an embedded KNX/IP gateway that runs on STM32H563ZI hardware.

### Hardware Requirements

- **Board**: STM32H563ZI Nucleo board
- **Network**: Ethernet connection to KNX/IP gateway and MQTT broker
- **Debug**: Integrated ST-Link debugger (on Nucleo board)

### Features

- **KNX/IP Integration**: Connects to KNX bus via IP gateway (tunneling mode)
- **MQTT Bridge**: Publishes KNX events and receives control commands via MQTT
- **Async Runtime**: Built with Embassy for efficient embedded async execution
- **Real-time Monitoring**: Tracks KNX device states and temperature sensors

### Configuration

Edit `ground/src/main.rs` to configure network addresses:

```rust
const KNX_GATEWAY_IP: &str = "192.168.1.19";  // Your KNX/IP gateway
const MQTT_BROKER_IP: &str = "192.168.1.7";    // Your MQTT broker
```

### Building and Flashing

Build and flash to STM32:

```bash
cd ground
cargo run --release
```

The firmware will:
1. Initialize Ethernet with DHCP
2. Connect to KNX/IP gateway
3. Connect to MQTT broker
4. Start bridging KNX ↔ MQTT
5. Blink LED to indicate operation

**Note**: On macOS hosts, use `flash.sh` as a workaround for DevContainer USB passthrough issues.

### KNX Device Configuration

The gateway is currently configured for:

**Monitored Devices** (KNX → MQTT):
- Group address `1/0/7`: Switch state monitoring (DPT 1.001)
  - Publishes to MQTT topic: `knx/tv/state`
- Group address `9/1/0`: Temperature sensor (DPT 9.001)
  - Publishes to MQTT topic: `knx/temperature/state`

**Controlled Devices** (MQTT → KNX):
- Group address `1/0/6`: Switch control (DPT 1.001)
  - Subscribes to MQTT topic: `knx/tv/control`

Modify these in `ground/src/main.rs` to match your KNX installation.

### Technical Notes

**Network Socket Configuration**: The Embassy network stack requires 8 sockets for:
- DHCP client (1 socket)
- KNX/IP connector (1-2 sockets)
- MQTT connector (1 socket)
- Protocol overhead (2-3 sockets)

**Task Pool**: Embassy executor uses 32 task slots (via `embassy-task-pool-32` feature) for concurrent async operations.

**Memory**: 64KB heap allocation for MQTT/KNX protocol buffers and JSON serialization.

## Tower (Control Console)

The tower is a PC application that provides the interface between MQTT and LLM clients.

### Features

- **MQTT Integration**: Connects to MQTT broker to communicate with ground
- **Remote Access**: Exposes Unix domain socket (`/tmp/console.sock`) using AimX protocol
- **Security**: Configurable read/write permissions for LLM access
- **Real-time Updates**: Streams KNX device states to connected LLM clients

### Running

Start the console:

```bash
cd tower
cargo run
```

The console will:
1. Connect to MQTT broker
2. Create Unix socket at `/tmp/console.sock`
3. Subscribe to KNX state topics (`knx/tv/state`, `knx/temperature/state`)
4. Publish control commands to `knx/tv/control`
5. Accept connections from MCP clients

### Data Flow

```
LLM Client (Claude/Copilot)
    ↓ MCP protocol
aimdb-mcp server
    ↓ AimX protocol (Unix socket)
tower (/tmp/console.sock)
    ↓ MQTT
MQTT Broker
    ↓ MQTT
ground (STM32)
    ↓ KNX/IP
KNX devices
```

## Pilot (LLM Interface via MCP)

The pilot provides natural language control through aimdb-mcp server integration.

### Installation

Install the aimdb-mcp server:

```bash
cargo install aimdb-mcp
```

### Configuration for VS Code

Add to `.vscode/mcp.json`:

```json
{
    "servers": {
        "aimdb": {
            "type": "stdio",
            "command": "aimdb-mcp",
            "args": [],
            "env": {
                "RUST_LOG": "info"
            }
        }
    }
}
```

Restart VS Code or run the MCP server directly from the mcp.json file.

### Usage with GitHub Copilot

Once tower is running and aimdb-mcp is configured, you can:

1. **Discover instances**: "Show me available AimDB instances"
2. **List records**: "What records are available in the console instance?"
3. **Read states**: "What's the current temperature?" or "Is the TV on?"
4. **Control devices**: "Turn on the TV" (sends command to KNX via MQTT)
5. **Subscribe**: "Subscribe to temperature updates for 50 samples"

## Quick Start

### 1. Start MQTT Broker

Ensure you have an MQTT broker running (e.g., Mosquitto):

```bash
# On the host or in a container
mosquitto -v
```

### 2. Flash and Run Ground (STM32)

```bash
cd ground
# Edit src/main.rs to configure KNX_GATEWAY_IP and MQTT_BROKER_IP
cargo run --release
```

### 3. Start Tower Console

```bash
cd tower
# Optional: export MQTT_BROKER=mqtt://your-broker:1883
cargo run
```

### 4. Test with MQTT

Monitor KNX events:

```bash
mosquitto_sub -h 192.168.1.7 -t 'knx/#' -v
```

Send control commands:

```bash
mosquitto_pub -h 192.168.1.7 -t 'knx/tv/control' \
  -m '{"address":"1/0/6","is_on":true}'
```

### 5. Connect LLM

Configure aimdb-mcp in VS Code (see Pilot section) and ask natural language questions:
- "What's the temperature?"
- "Turn on the TV"
- "Show me the switch state"

## Utilities

### AimDB CLI

Command-line tools for development and debugging:

```bash
cargo install aimdb-cli
```

**Common commands**:

```bash
# List running instances
aimdb instance list

# Watch temperature in real-time
aimdb watch records::Temperature

# Get switch state
aimdb record get /tmp/console.sock records::SwitchState
```

See [aimdb-cli documentation](https://github.com/aimdb-dev/aimdb) for full command reference.

## License

See [LICENSE](LICENSE) for details.
