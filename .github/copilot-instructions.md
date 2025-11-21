# Project Overview

This project focuses on home automation with LLM integration. The architecture consists of:

- **knx-gateway**: An STM32-based KNX gateway implementation built on aimdb, providing hardware-level integration with KNX home automation systems
- **aimdb-mcp-server**: Runs on a PC and connects to the KNX gateway via MQTT, serving as the central coordination layer
- **LLM Interface**: Exposed through the aimdb-mcp crate, enabling natural language interaction with the home automation system

The system allows LLM-powered control and monitoring of KNX devices through a layered architecture: STM32 hardware → MQTT → aimdb → MCP → LLM.

# aimdb Usage Guide

For detailed information on using aimdb, please refer to the official usage guide:
https://github.com/aimdb-dev/aimdb/blob/main/docs/aimdb-usage-guide.md
