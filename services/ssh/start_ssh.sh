#!/usr/bin/env bash
set -Eeuo pipefail

echo "======================================"
echo "          Starting SSH Server"
echo "======================================"

if [[ "${EUID}" -ne 0 ]]; then
    echo "Error: Please run as root."
    exit 1
fi

if ! command -v sshd >/dev/null 2>&1; then
    echo "Error: OpenSSH Server is not installed."
    exit 1
fi

# Prepare runtime directory
mkdir -p /run/sshd
chmod 0755 /run/sshd

# Generate missing host keys
ssh-keygen -A

# Validate configuration
echo "[1/3] Checking SSH configuration..."
sshd -t

# Check whether SSH is already running
if pgrep -x sshd >/dev/null 2>&1; then
    echo "SSH Server is already running."
else
    echo "[2/3] Starting SSH Server..."
    /usr/sbin/sshd

    sleep 1

    if ! pgrep -x sshd >/dev/null 2>&1; then
        echo "Error: Failed to start SSH Server."
        exit 1
    fi
fi

echo "[3/3] Checking listening ports..."

ss -lntp | grep ssh || {
    echo "Error: SSH is not listening."
    exit 1
}

echo ""
echo "======================================"
echo "SSH Server is running"
echo "======================================"

echo ""
echo "SSH processes:"
pgrep -a sshd || true

echo ""
echo "Listening ports:"
ss -lntp | grep ssh

echo ""
echo "Connect using:"
echo "ssh root@SERVER_IP -p 2222"