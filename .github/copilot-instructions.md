# Project Overview

This project focuses on home automation with LLM integration. The architecture consists of:

- **ground**: An STM32-based KNX gateway implementation built on aimdb, providing hardware-level integration with KNX home automation systems
- **tower**: Control and monitoring console that runs on a PC and connects to the ground via MQTT, serving as the central coordination layer
- **LLM Interface**: Exposed through the aimdb-mcp crate, enabling natural language interaction with the home automation system

The system allows LLM-powered control and monitoring of KNX devices through a layered architecture: STM32 hardware → MQTT → aimdb → MCP → LLM.

# aimdb Usage Guide

For detailed information on using aimdb, please refer to the official usage guide:
https://github.com/aimdb-dev/aimdb/blob/main/docs/aimdb-usage-guide.md

## Guidelines

### Dependency Management

When adding dependencies to `Cargo.toml`:
- **Always use crates.io versions** - do not add git dependencies unless explicitly documented below
- **Verify API availability** - check that features and APIs exist in the specified version before use
- **Avoid deprecated APIs** - consult the usage guide for current best practices

### aimdb API Usage

**IMPORTANT: The `.link()` method is deprecated and must NOT be used.**

Use the following pattern instead:
- **Outbound links**: Use `.link_to()` (requires serialization on the sending side)
- **Inbound links**: Use `.link_from()` (requires deserialization on the receiving side)

### Required Cargo Patches

Projects using MQTT or KNX connectors require specific patches in `Cargo.toml`:

```toml
[patch.crates-io]
# Required if using KNX connector - bug fixes not yet on crates.io
knx-pico = { git = "https://github.com/aimdb-dev/knx-pico.git", branch = "master" }

# Only required for Embassy-based projects (not needed for Tokio-only projects)
# mountain-mqtt = { git = "https://github.com/aimdb-dev/mountain-mqtt.git", branch = "main" }
# mountain-mqtt-embassy = { git = "https://github.com/aimdb-dev/mountain-mqtt.git", branch = "main" }
```

**Note**: The MQTT patches are only necessary for Embassy-based embedded projects. Tokio projects do not require these patches.
