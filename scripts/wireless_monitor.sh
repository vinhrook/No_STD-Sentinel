#!/usr/bin/env bash
# Sentinel Wireless Monitor - Zero Cable Serial Console
# Streams live telemetry and log diagnostics over Wi-Fi directly from ESP32-S3

set -e

BROKER_HOST="${1:-localhost}"
BROKER_PORT="${2:-1883}"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  📡 SENTINEL Wireless Console (Cable-Free Network Monitor)         "
echo "  Broker: ${BROKER_HOST}:${BROKER_PORT}                              "
echo "  Filter: sentinel/# (All telemetry and log streams)                "
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Listening for live wireless frames from ESP32-S3 over Wi-Fi..."
echo "Press Ctrl+C to exit."
echo ""

mosquitto_sub -h "${BROKER_HOST}" -p "${BROKER_PORT}" -t "sentinel/#" -v
