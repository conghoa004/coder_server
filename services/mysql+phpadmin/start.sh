#!/usr/bin/env bash
set -eo pipefail

CRED_FILE="/workspaces/services/mysql+phpadmin/credentials.txt"

if [ "$EUID" -ne 0 ]; then
  echo "Please run this script with root or sudo: sudo bash $0"
  exit 1
fi

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

ensure_socket_perms() {
  mkdir -p /var/run/mysqld
  chown -R mysql:mysql /var/run/mysqld
  chmod 755 /var/run/mysqld
}

wait_for_mysql() {
  local socket_path="/var/run/mysqld/mysqld.sock"
  local retries=30
  ensure_socket_perms

  for ((i=1; i<=retries; i++)); do
    if [ -S "$socket_path" ] && mysqladmin ping --silent 2>/dev/null; then
      chmod 755 /var/run/mysqld
      return 0
    fi
    sleep 1
  done

  echo "MySQL did not become ready in time. Recent error log:"
  tail -n 30 /var/log/mysql/error.log 2>/dev/null || true
  return 1
}

echo "=== Starting MySQL & Apache ==="
ensure_socket_perms
manage_service start mysql
wait_for_mysql

apache2ctl configtest
manage_service start apache2
chmod 755 /var/run/mysqld

echo "MySQL and Apache are running."
if [ -f "$CRED_FILE" ]; then
  echo "Credentials file: ${CRED_FILE}"
fi