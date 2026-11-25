#!/usr/bin/env bash
set -eo pipefail

if [ "$EUID" -ne 0 ]; then
  echo "Please run this script with root or sudo: sudo bash $0"
  exit 1
fi

manage_service() {
  local action="$1"
  local svc="$2"

  if [ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1; then
    systemctl "$action" "$svc"
  else
    service "$svc" "$action"
  fi
}

echo "=== Stopping Apache & MySQL ==="
manage_service stop apache2 || true
manage_service stop mysql || true
echo "Apache and MySQL have been stopped."