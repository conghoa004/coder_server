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

manage_service() {
  local action="$1"
  local svc="$2"

  if [ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1; then
    systemctl "$action" "$svc" >/dev/null 2>&1 || true
  else
    service "$svc" "$action" >/dev/null 2>&1 || true
  fi
}

echo "=== [1/4] Stopping Apache & MySQL services ==="
manage_service stop apache2
manage_service stop mysql

echo "=== [2/4] Purging phpMyAdmin, MySQL, Apache & PHP packages ==="
export DEBIAN_FRONTEND=noninteractive
apt-get purge -y \
  phpmyadmin \
  mysql-server mysql-client mysql-common mysql-server-core-* mysql-client-core-* \
  apache2 apache2-bin apache2-data apache2-utils libapache2-mod-php \
  php php-common php-mysql php-mbstring php-zip php-gd php-json php-curl php[0-9.]*-* || true

apt-get autoremove -y --purge || true
apt-get autoclean -y || true

echo "=== [3/4] Removing custom configs, databases & socket directories ==="
rm -f /etc/apache2/conf-enabled/root-to-phpmyadmin.conf
rm -f /etc/apache2/conf-enabled/phpmyadmin.conf
rm -rf /etc/phpmyadmin
rm -rf /var/lib/mysql
rm -rf /var/log/mysql
rm -rf /var/run/mysqld
rm -rf /var/log/apache2

echo "=== [4/4] Removing saved credentials file ==="
CRED_FILE="/workspaces/services/mysql+phpadmin/credentials.txt"
if [ -f "$CRED_FILE" ]; then
    rm -f "$CRED_FILE"
    echo "Removed: ${CRED_FILE}"
else
    echo "Credentials file not found. Skipping."
fi

echo ""
echo "=========================================================="
echo "Uninstallation completed successfully!"
echo "=========================================================="