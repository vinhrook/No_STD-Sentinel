#!/usr/bin/env bash
# Sentinel Wireless OTA Flasher
# Compiles firmware and pushes it over Wi-Fi directly to the ESP32-S3 (Port 8032)
# Zero cables, zero USB required!

set -e

ESP_IP="${1}"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  🚀 SENTINEL Wireless OTA Firmware Flasher (Cable-Free)            "
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ -z "$ESP_IP" ]; then
    echo "Usage: ./scripts/flash_ota.sh <ESP32_IP_ADDRESS>"
    echo "Example: ./scripts/flash_ota.sh 10.161.233.152"
    echo ""
    echo "Tip: You can find your ESP32's IP from your wireless monitor logs"
    echo "     or router DHCP table."
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
FIRMWARE_DIR="${WORKSPACE_ROOT}/crates/sentinel-firmware"
OUTPUT_BIN="${WORKSPACE_ROOT}/target/firmware.bin"

echo "[1/3] 🔨 Compiling release firmware for ESP32-S3..."
cd "${FIRMWARE_DIR}"
if [ -f "${HOME}/export-esp.sh" ]; then
    source "${HOME}/export-esp.sh"
fi

cargo build --release

echo "[2/3] 📦 Generating raw ESP32-S3 flash image..."
mkdir -p "${WORKSPACE_ROOT}/target"
ELF_PATH="${WORKSPACE_ROOT}/target/xtensa-esp32s3-none-elf/release/sentinel-firmware"
if [ ! -f "${ELF_PATH}" ]; then
    ELF_PATH="target/xtensa-esp32s3-none-elf/release/sentinel-firmware"
fi
espflash save-image --chip esp32s3 "${ELF_PATH}" "${OUTPUT_BIN}"


BIN_SIZE=$(stat -c%s "${OUTPUT_BIN}" 2>/dev/null || stat -f%z "${OUTPUT_BIN}")
echo "       Generated ${OUTPUT_BIN} (${BIN_SIZE} bytes / $((BIN_SIZE / 1024)) KB)"

echo "[3/3] 📡 Streaming firmware over Wi-Fi to ${ESP_IP}:8032..."
if nc -w 20 "${ESP_IP}" 8032 < "${OUTPUT_BIN}"; then
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  ✅ OTA UPDATE SUCCESSFUL!                                          "
    echo "  The ESP32-S3 is now rebooting into the new firmware.               "
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
else
    echo ""
    echo "❌ Failed to connect to ${ESP_IP}:8032. Ensure ESP32 is on Wi-Fi."
    exit 1
fi
