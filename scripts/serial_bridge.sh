#!/usr/bin/env bash
# ==============================================================================
# SENTINEL - Live USB Serial to MQTT Bridge
# Reads telemetry JSON frames directly from ESP32-S3 over /dev/ttyUSB0
# and forwards them into local Mosquitto MQTT broker.
# ==============================================================================

PORT="${1:-/dev/ttyUSB0}"
BAUD="115200"
MQTT_HOST="localhost"
MQTT_PORT="1883"
TOPIC_BASE="sentinel"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  ⚡ SENTINEL Live USB Serial Bridge"
echo "  Target Port: $PORT @ $BAUD baud"
echo "  MQTT Broker: $MQTT_HOST:$MQTT_PORT"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# 1. Verify device exists
if [ ! -e "$PORT" ]; then
    echo "❌ Error: Port $PORT not found! Make sure ESP32 is plugged in."
    exit 1
fi

# 2. Configure UART baud rate and raw mode
sudo chmod 666 "$PORT" 2>/dev/null
stty -F "$PORT" "$BAUD" cs8 -cstopb -parenb raw -echo min 1 time 1

echo "📡 Bridge listening on $PORT... Touch the sensor to see live temperature changes!"
echo ""

# Read stream line by line and forward JSON packets
while IFS= read -r raw_line; do
    # Strip carriage returns from serial
    line="${raw_line//$'\r'/}"

    if [[ "$line" == *'{"d":'* ]]; then
        json="{\"d\":${line#*\{\"d\":}"
        topic="sentinel/0A892648/telemetry"
        
        mosquitto_pub -h "$MQTT_HOST" -p "$MQTT_PORT" -t "$topic" -m "$json"
        
        echo "[$(date '+%H:%M:%S')] 🚀 Live Frame Forwarded -> $topic"
        echo "           Payload: $json"
    elif [[ "$line" == *"[MQTT]"* || "$line" == *"[BOOT]"* || "$line" == *"[SILICON]"* ]]; then
        echo "[ESP32-S3] $line"
    fi
done < "$PORT"
