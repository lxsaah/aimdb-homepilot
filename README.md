# aimdb-homepilot

AimDB-powered smart home demo using KNX on STM32, MQTT linking and LLM/MCP integration — a home automation "co-pilot" built in Rust.

## Architecture

This project demonstrates a layered home automation system:

- **knx-gateway**: STM32H563ZI-based KNX gateway using Embassy and AimDB
- **console**: Control and monitoring console for the system
- **aimdb-mcp-server**: MCP server providing LLM integration via the Model Context Protocol

The system enables natural language control of KNX devices through: STM32 hardware → MQTT → AimDB → MCP → LLM.

## Development

### Prerequisites

This project uses a DevContainer for a consistent development environment. Simply reuse the DevContainer configuration from:
https://github.com/aimdb-dev/aimdb/blob/main/.devcontainer/devcontainer.json

### Documentation

For detailed information on using AimDB, refer to the official usage guide:
https://github.com/aimdb-dev/aimdb/blob/main/docs/aimdb-usage-guide.md

## KNX Gateway (STM32)

### Hardware

- **Board**: STM32H563ZI
- **Framework**: Embassy (async Rust for embedded)

### Getting Started

Embassy documentation: https://embassy.dev/book/index.html#_starting_a_new_project

Embassy dependencies are sourced directly from git:

```toml
embassy-stm32 = { git = "https://github.com/embassy-rs/embassy", branch = "main", features = ["defmt", "stm32h563zi", "memory-x", "time-driver-any", "exti", "unstable-pac", "low-power"] }
embassy-sync = { git = "https://github.com/embassy-rs/embassy", branch = "main", features = ["defmt"] }
embassy-executor = { git = "https://github.com/embassy-rs/embassy", branch = "main", features = ["arch-cortex-m", "executor-thread", "defmt"] }
embassy-time = { git = "https://github.com/embassy-rs/embassy", branch = "main", features = ["defmt", "defmt-timestamp-uptime", "tick-hz-32_768"] }
```

### Building and Flashing

Build and run:

```bash
cd knx-gateway
cargo run --release
```

**Note**: I use `flash.sh` on macOS hosts, this is used as a workaround for DevContainer USB passthrough issues.

## Console

Control and monitoring console powered by AimDB.

```bash
cd console
cargo run
```

## AimDB-MCP Server

### Installation

Install the MCP server:

```bash
cargo install aimdb-mcp
```

### Configuration

Add the following to `.vscode/mcp.json`:

```json
{
    "servers": {
        "aimdb": {
            "type": "stdio",
            "command": "/aimdb/aimdb-mcp",
            "args": [],
            "env": {
                "RUST_LOG": "info"
            }
        }
    }
}
```

Run the MCP server directly from the `mcp.json` file or restart VS Code.

## Utilities

### AimDB CLI

The AimDB CLI provides command-line tools for development and administration.

Install:

```bash
cargo install aimdb-cli
```

#### Available Commands

**Instance Management**
- `aimdb instance list` - List all running AimDB instances
- `aimdb instance info <socket>` - Show detailed instance information
- `aimdb instance ping <socket>` - Test connection to an instance

**Record Management**
- `aimdb record list <socket>` - List all registered records
- `aimdb record get <socket> <record>` - Get current value of a record
- `aimdb record set <socket> <record> <value>` - Set value of a writable record

**Real-time Monitoring**
- `aimdb watch <record>` - Watch a record in real-time
  - `-s, --socket <SOCKET>` - Specify socket path (auto-discovery if omitted)
  - `-c, --count <COUNT>` - Maximum number of events (0 = unlimited)
  - `-f, --full` - Show full JSON for each event

Example usage:
```bash
# List all instances
aimdb instance list

# Watch a temperature record
aimdb watch server::Temperature

# Get a record value
aimdb record get /tmp/aimdb.sock server::Temperature
```

## License

See [LICENSE](LICENSE) for details.
