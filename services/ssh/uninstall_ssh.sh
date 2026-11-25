#!/usr/bin/env bash
set -Eeuo pipefail

echo "======================================"
echo "        OpenSSH Server Uninstaller"
echo "======================================"

if [[ "${EUID}" -ne 0 ]]; then
    echo "Error: Please run as root."
    exit 1
fi

if ! dpkg -s openssh-server >/dev/null 2>&1; then
    echo "OpenSSH Server is not installed."
    exit 0
fi

echo ""
echo "WARNING: This will remove OpenSSH Server."
echo "SSH client will remain installed."
echo ""

read -r -p "Are you sure? [y/N]: " CONFIRM

if [[ ! "$CONFIRM" =~ ^[Yy]$ ]]; then
    echo "Uninstallation cancelled."
    exit 0
fi

echo ""
echo "[1/3] Stopping SSH Server..."

pkill -TERM -x sshd 2>/dev/null || true

sleep 1

pkill -KILL -x sshd 2>/dev/null || true

echo ""
echo "[2/3] Removing OpenSSH Server..."

apt-get purge -y openssh-server

echo ""
echo "[3/3] Cleaning unused packages..."

apt-get autoremove -y

echo ""
echo "======================================"
echo "OpenSSH Server uninstalled"
echo "======================================"

echo ""
echo "SSH Client status:"

if command -v ssh >/dev/null 2>&1; then
    ssh -V
else
    echo "SSH Client is not available."
fi

echo ""
echo "OpenSSH Server has been removed."