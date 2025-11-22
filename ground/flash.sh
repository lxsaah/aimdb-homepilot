#!/bin/bash
# Flash script for ground
# 
# This script should be run on the HOST machine where probe-rs and hardware are accessible.
# The binary must be built first in the dev container using: cargo build

set -e

BINARY="./target/thumbv8m.main-none-eabihf/release/ground"

if [ ! -f "$BINARY" ]; then
    echo "Error: Binary not found at $BINARY"
    echo "Please build it first in the dev container:"
    echo "  cd ground && cargo build"
    exit 1
fi

echo "Flashing ground to STM32H563ZITx..."
probe-rs run --chip STM32H563ZITx "$BINARY"
