#!/usr/bin/env bash
set -Eeuo pipefail

echo "======================================"
echo "           Stopping SSH Server"
echo "======================================"

if [[ "${EUID}" -ne 0 ]]; then
    echo "Error: Please run as root."
    exit 1
fi

if ! command -v sshd >/dev/null 2>&1; then
    echo "OpenSSH Server is not installed."
    exit 1
fi

if ! pgrep -x sshd >/dev/null 2>&1; then
    echo "SSH Server is already stopped."
    exit 0
fi

echo "[1/2] Sending termination signal..."
pkill -TERM -x sshd || true

echo "[2/2] Waiting for SSH processes to stop..."

for i in {1..10}; do
    if ! pgrep -x sshd >/dev/null 2>&1; then
        break
    fi
    sleep 1
done

# Force stop if necessary
if pgrep -x sshd >/dev/null 2>&1; then
    echo "SSH did not stop gracefully. Forcing termination..."
    pkill -KILL -x sshd || true
fi

if pgrep -x sshd >/dev/null 2>&1; then
    echo "Error: Unable to stop SSH Server."
    exit 1
fi

echo ""
echo "======================================"
echo "SSH Server stopped successfully"
echo "======================================"