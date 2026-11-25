#!/usr/bin/env bash
set -Eeuo pipefail

echo "======================================"
echo "      OpenSSH Server Installation"
echo "======================================"

if [[ "${EUID}" -ne 0 ]]; then
    echo "Error: Please run this script as root."
    echo "Example: sudo bash install_ssh.sh"
    exit 1
fi

echo ""
echo "[1/5] Updating package list..."
apt-get update

echo ""
echo "[2/5] Installing OpenSSH Server..."
apt-get install -y openssh-server openssh-client

echo ""
echo "[3/5] Configuring SSH server..."

mkdir -p /run/sshd
chmod 0755 /run/sshd

ssh-keygen -A

# Configure SSH authentication
SSHD_CONFIG="/etc/ssh/sshd_config"

# Remove existing conflicting directives from the main config
sed -i -E '/^[[:space:]]*(PasswordAuthentication|PubkeyAuthentication|PermitRootLogin)[[:space:]]+/Id' "$SSHD_CONFIG"

# Add authentication settings
cat >> "$SSHD_CONFIG" <<'EOF'

# Custom SSH Authentication
PasswordAuthentication yes
PubkeyAuthentication yes
PermitRootLogin yes
EOF

echo ""
echo "[4/5] Setting root password..."

passwd root

echo ""
echo "[5/5] Starting SSH server..."

# Validate SSH configuration
sshd -t

# Stop existing SSH daemon
pkill -x sshd 2>/dev/null || true

# Start SSH daemon
/usr/sbin/sshd

echo ""
echo "======================================"
echo "SSH installation completed"
echo "======================================"

echo ""
echo "SSH version:"
ssh -V

echo ""
echo "Authentication settings:"
sshd -T | grep -E 'passwordauthentication|pubkeyauthentication|permitrootlogin'

echo ""
echo "Listening ports:"
ss -lntp | grep ssh || {
    echo "Error: SSH is not listening."
    exit 1
}

echo ""
echo "SSH process:"
pgrep -a sshd || true

echo ""
echo "Connect with:"
echo "  ssh root@SERVER_IP -p 2222"

echo ""
echo "SSH server is ready!"