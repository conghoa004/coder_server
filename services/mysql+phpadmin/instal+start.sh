#!/usr/bin/env bash
set -eo pipefail

# Check for root / sudo privileges
if [ "$EUID" -ne 0 ]; then
  echo "Please run this script with root or sudo: sudo bash $0"
  exit 1
fi

# Check for Debian/Ubuntu system
if ! command -v apt-get >/dev/null 2>&1; then
  echo "This script currently supports apt-based systems (Ubuntu/Debian)."
  exit 1
fi

# Ensure openssl is available to generate a random password
if ! command -v openssl >/dev/null 2>&1; then
  apt-get update -y
  apt-get install -y openssl
fi

# Helper function to manage services with or without systemd
manage_service() {
  local action="$1"
  local svc="$2"

  if [ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1; then
    if [ "$action" = "start" ]; then
      systemctl enable --now "$svc"
    else
      systemctl "$action" "$svc"
    fi
  else
    service "$svc" "$action"
  fi
}

echo "=== [1/5] Updating package lists and installing dependencies ==="
export DEBIAN_FRONTEND=noninteractive
apt-get update -y
apt-get install -y apache2 php libapache2-mod-php php-mysql php-mbstring php-zip php-gd php-json php-curl mysql-server debconf-utils net-tools

echo "=== [2/5] Starting MySQL & Apache ==="
manage_service start mysql
manage_service start apache2

echo "=== [3/5] Configuring MySQL admin user with random password ==="
PMA_USER="admin"
PMA_PASS="$(openssl rand -base64 24)"

mysql -e "CREATE USER IF NOT EXISTS '${PMA_USER}'@'localhost' IDENTIFIED WITH mysql_native_password BY '${PMA_PASS}';"
mysql -e "ALTER USER '${PMA_USER}'@'localhost' IDENTIFIED WITH mysql_native_password BY '${PMA_PASS}';"
mysql -e "GRANT ALL PRIVILEGES ON *.* TO '${PMA_USER}'@'localhost' WITH GRANT OPTION;"

mysql -e "CREATE USER IF NOT EXISTS '${PMA_USER}'@'127.0.0.1' IDENTIFIED WITH mysql_native_password BY '${PMA_PASS}';"
mysql -e "ALTER USER '${PMA_USER}'@'127.0.0.1' IDENTIFIED WITH mysql_native_password BY '${PMA_PASS}';"
mysql -e "GRANT ALL PRIVILEGES ON *.* TO '${PMA_USER}'@'127.0.0.1' WITH GRANT OPTION;"

mysql -e "FLUSH PRIVILEGES;"

echo "=== [4/5] Installing phpMyAdmin ==="
echo "phpmyadmin phpmyadmin/reconfigure-webserver multiselect apache2" | debconf-set-selections
echo "phpmyadmin phpmyadmin/dbconfig-install boolean true" | debconf-set-selections
echo "phpmyadmin phpmyadmin/mysql/admin-pass password ${PMA_PASS}" | debconf-set-selections
echo "phpmyadmin phpmyadmin/mysql/app-pass password ${PMA_PASS}" | debconf-set-selections
echo "phpmyadmin phpmyadmin/app-password-confirm password ${PMA_PASS}" | debconf-set-selections

apt-get install -y phpmyadmin

if [ -f /etc/phpmyadmin/config-db.php ]; then
  sed -i "s/\$dbserver\s*=\s*'.*';/\$dbserver = '127.0.0.1';/" /etc/phpmyadmin/config-db.php
fi

# Tìm đúng file cấu hình Apache của phpMyAdmin hoặc tạo fallback Alias
PMA_APACHE_CONF=""
for f in /etc/phpmyadmin/apache.conf /etc/phpmyadmin/apache2.conf; do
  if [ -f "$f" ]; then
    PMA_APACHE_CONF="$f"
    break
  fi
done

if [ -n "$PMA_APACHE_CONF" ]; then
  ln -sf "$PMA_APACHE_CONF" /etc/apache2/conf-enabled/phpmyadmin.conf
elif [ ! -f /etc/apache2/conf-enabled/phpmyadmin.conf ]; then
  cat > /etc/apache2/conf-enabled/phpmyadmin.conf <<'EOF'
Alias /phpmyadmin /usr/share/phpmyadmin

<Directory /usr/share/phpmyadmin>
    Options FollowSymLinks
    DirectoryIndex index.php
    AllowOverride All
    Require all granted
</Directory>
EOF
fi

echo "=== [5/5] Testing configuration, restarting Apache & saving credentials ==="
apache2ctl configtest
manage_service restart apache2

# Port and IP Detection
HTTP_PORT=$(ss -tulpn 2>/dev/null | grep apache2 | awk '{print $5}' | awk -F: '{print $NF}' | head -n 1 || echo "80")
MYSQL_PORT=$(ss -tulpn 2>/dev/null | grep mysqld | awk '{print $5}' | awk -F: '{print $NF}' | head -n 1 || echo "3306")
HTTP_PORT=${HTTP_PORT:-80}
MYSQL_PORT=${MYSQL_PORT:-3306}

CRED_DIR="/workspaces/services/mysql+phpadmin"
CRED_FILE="${CRED_DIR}/credentials.txt"
SERVER_IP=$(hostname -I 2>/dev/null | awk '{print $1}')

mkdir -p "$CRED_DIR"
cat > "$CRED_FILE" <<EOF
MySQL & phpMyAdmin Credentials
==============================
Service Ports:
  - Apache / Web Port : ${HTTP_PORT}
  - MySQL Database    : ${MYSQL_PORT}

Authentication:
  - Username          : ${PMA_USER}
  - Password          : ${PMA_PASS}

Access Links:
  - Local Access      : http://localhost:${HTTP_PORT}/phpmyadmin
${SERVER_IP:+  - LAN/IP Access     : http://${SERVER_IP}:${HTTP_PORT}/phpmyadmin}
EOF
chmod 600 "$CRED_FILE"

echo ""
echo "=========================================================="
echo "Installation completed successfully!"
echo "Credentials saved to: ${CRED_FILE}"
echo "=========================================================="