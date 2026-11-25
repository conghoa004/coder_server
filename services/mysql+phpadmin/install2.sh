#!/usr/bin/env bash
set -Eeuo pipefail

# ==========================================================
# MySQL + phpMyAdmin + Apache Installer
# Supports Ubuntu/Debian and non-systemd environments
# ==========================================================

# ----------------------------------------------------------
# Configuration
# ----------------------------------------------------------

PMA_USER="admin"

CRED_DIR="/workspaces/services/mysql+phpadmin"
CRED_FILE="${CRED_DIR}/credentials.txt"

HTTP_PORT="${HTTP_PORT:-80}"
MYSQL_PORT="${MYSQL_PORT:-3306}"

export DEBIAN_FRONTEND=noninteractive

# ----------------------------------------------------------
# Colors / logging
# ----------------------------------------------------------

log() {
    echo "[INFO] $*"
}

warn() {
    echo "[WARN] $*" >&2
}

error() {
    echo "[ERROR] $*" >&2
    exit 1
}

# ----------------------------------------------------------
# Root check
# ----------------------------------------------------------

if [ "${EUID}" -ne 0 ]; then
    error "Please run this script with root or sudo: sudo bash $0"
fi

# ----------------------------------------------------------
# OS check
# ----------------------------------------------------------

if ! command -v apt-get >/dev/null 2>&1; then
    error "This script supports Debian/Ubuntu systems only."
fi

# ----------------------------------------------------------
# Generate random password
# ----------------------------------------------------------

generate_password() {
    if command -v openssl >/dev/null 2>&1; then
        openssl rand -hex 24
    else
        tr -dc 'A-Za-z0-9' </dev/urandom | head -c 48
        echo
    fi
}

PMA_PASS="$(generate_password)"

# ----------------------------------------------------------
# Service manager
# ----------------------------------------------------------

manage_service() {
    local action="$1"
    local service_name="$2"

    if [ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1; then
        case "$action" in
            start)
                systemctl enable --now "$service_name"
                ;;
            restart)
                systemctl restart "$service_name"
                ;;
            stop)
                systemctl stop "$service_name"
                ;;
            status)
                systemctl status "$service_name" --no-pager
                ;;
            *)
                systemctl "$action" "$service_name"
                ;;
        esac
    elif command -v service >/dev/null 2>&1; then
        service "$service_name" "$action"
    else
        warn "No service manager detected for $service_name."
    fi
}

# ----------------------------------------------------------
# MySQL readiness
# ----------------------------------------------------------

wait_for_mysql() {
    local retries=60

    log "Waiting for MySQL to become ready..."

    mkdir -p /var/run/mysqld

    if id mysql >/dev/null 2>&1; then
        chown mysql:mysql /var/run/mysqld
    fi

    chmod 755 /var/run/mysqld

    for ((i=1; i<=retries; i++)); do
        if mysqladmin --protocol=socket ping --silent >/dev/null 2>&1; then
            log "MySQL is ready."
            return 0
        fi

        sleep 1
    done

    warn "MySQL did not become ready in time."

    if [ -f /var/log/mysql/error.log ]; then
        echo ""
        echo "----- MySQL error log -----"
        tail -n 50 /var/log/mysql/error.log || true
        echo "---------------------------"
    fi

    return 1
}

# ----------------------------------------------------------
# Check Apache
# ----------------------------------------------------------

wait_for_apache() {
    local retries=30

    for ((i=1; i<=retries; i++)); do
        if apache2ctl -t >/dev/null 2>&1; then
            return 0
        fi

        sleep 1
    done

    return 1
}

# ==========================================================
# 1. Install dependencies
# ==========================================================

echo ""
echo "=========================================================="
echo "[1/5] Installing Apache, PHP, MySQL and phpMyAdmin"
echo "=========================================================="

apt-get update -y

apt-get install -y \
    openssl \
    apache2 \
    mysql-server \
    php \
    libapache2-mod-php \
    php-mysql \
    php-mbstring \
    php-zip \
    php-gd \
    php-curl \
    php-json \
    php-xml \
    php-cli \
    phpmyadmin \
    dbconfig-mysql \
    debconf-utils \
    net-tools

# ==========================================================
# 2. Start MySQL and Apache
# ==========================================================

echo ""
echo "=========================================================="
echo "[2/5] Starting MySQL and Apache"
echo "=========================================================="

manage_service start mysql
wait_for_mysql

manage_service start apache2

# ==========================================================
# 3. Configure MySQL admin user
# ==========================================================

echo ""
echo "=========================================================="
echo "[3/5] Configuring MySQL admin user"
echo "=========================================================="

log "Creating MySQL user: ${PMA_USER}"

# MySQL 8.4:
# Do NOT force mysql_native_password.
# Let MySQL use its default authentication plugin.
#
# mysql client connects through local Unix socket as root
# on standard Ubuntu/Debian MySQL installations.

mysql --protocol=socket <<SQL
CREATE USER IF NOT EXISTS '${PMA_USER}'@'localhost'
    IDENTIFIED BY '${PMA_PASS}';

ALTER USER '${PMA_USER}'@'localhost'
    IDENTIFIED BY '${PMA_PASS}';

GRANT ALL PRIVILEGES ON *.* TO '${PMA_USER}'@'localhost'
    WITH GRANT OPTION;

CREATE USER IF NOT EXISTS '${PMA_USER}'@'127.0.0.1'
    IDENTIFIED BY '${PMA_PASS}';

ALTER USER '${PMA_USER}'@'127.0.0.1'
    IDENTIFIED BY '${PMA_PASS}';

GRANT ALL PRIVILEGES ON *.* TO '${PMA_USER}'@'127.0.0.1'
    WITH GRANT OPTION;
SQL

log "MySQL admin user configured successfully."

# ==========================================================
# 4. Configure phpMyAdmin
# ==========================================================

echo ""
echo "=========================================================="
echo "[4/5] Configuring phpMyAdmin"
echo "=========================================================="

# ----------------------------------------------------------
# Configure phpMyAdmin package
# ----------------------------------------------------------

if [ -f /etc/phpmyadmin/apache.conf ]; then
    ln -sf \
        /etc/phpmyadmin/apache.conf \
        /etc/apache2/conf-enabled/phpmyadmin.conf
elif [ -f /etc/phpmyadmin/apache2.conf ]; then
    ln -sf \
        /etc/phpmyadmin/apache2.conf \
        /etc/apache2/conf-enabled/phpmyadmin.conf
else
    cat > /etc/apache2/conf-available/phpmyadmin.conf <<'EOF'
Alias /phpmyadmin /usr/share/phpmyadmin

<Directory /usr/share/phpmyadmin>
    Options FollowSymLinks
    DirectoryIndex index.php
    AllowOverride All
    Require all granted
</Directory>
EOF

    a2enconf phpmyadmin >/dev/null
fi

# ----------------------------------------------------------
# Enable required Apache modules
# ----------------------------------------------------------

a2enmod rewrite >/dev/null 2>&1 || true
a2enmod headers >/dev/null 2>&1 || true
a2enmod dir >/dev/null 2>&1 || true

# ----------------------------------------------------------
# Configure phpMyAdmin server
# ----------------------------------------------------------

if [ -f /etc/phpmyadmin/config.inc.php ]; then

    # Avoid duplicate settings if script is executed again.
    if ! grep -q "blowfish_secret" /etc/phpmyadmin/config.inc.php; then
        SECRET="$(openssl rand -base64 32 | tr -d '\n')"

        cat >> /etc/phpmyadmin/config.inc.php <<EOF

/* Added by MySQL/phpMyAdmin installer */
\$cfg['blowfish_secret'] = '${SECRET}';
EOF
    fi

    # Make phpMyAdmin connect to local MySQL explicitly.
    if ! grep -q "127.0.0.1" /etc/phpmyadmin/config.inc.php; then
        cat >> /etc/phpmyadmin/config.inc.php <<'EOF'

/* Use TCP connection to local MySQL */
$cfg['Servers'][$i]['host'] = '127.0.0.1';
$cfg['Servers'][$i]['port'] = 3306;
EOF
    fi
fi

# ----------------------------------------------------------
# Redirect / -> /phpmyadmin
# ----------------------------------------------------------

ROOT_REDIRECT_CONF="/etc/apache2/conf-available/root-to-phpmyadmin.conf"

cat > "$ROOT_REDIRECT_CONF" <<'EOF'
<IfModule mod_rewrite.c>
    RewriteEngine On

    RewriteCond %{REQUEST_URI} ^/$
    RewriteRule ^$ /phpmyadmin [R=302,L]
</IfModule>
EOF

a2enconf root-to-phpmyadmin >/dev/null

# ----------------------------------------------------------
# Cloudflare Tunnel / reverse proxy compatibility
# ----------------------------------------------------------

CLOUDFLARE_CONF="/etc/apache2/conf-available/cloudflare-proxy.conf"

cat > "$CLOUDFLARE_CONF" <<'EOF'
<IfModule mod_setenvif.c>
    SetEnvIf X-Forwarded-Proto "^https$" HTTPS=on
</IfModule>

<IfModule mod_headers.c>
    UseCanonicalName Off
</IfModule>
EOF

a2enconf cloudflare-proxy >/dev/null

# ==========================================================
# 5. Test Apache and save credentials
# ==========================================================

echo ""
echo "=========================================================="
echo "[5/5] Testing Apache and saving credentials"
echo "=========================================================="

if ! apache2ctl configtest; then
    error "Apache configuration test failed."
fi

manage_service restart apache2

# ----------------------------------------------------------
# Detect actual ports
# ----------------------------------------------------------

DETECTED_HTTP_PORT="$(
    awk '
        $1 == "Listen" && $2 ~ /^[0-9]+$/ {
            print $2
            exit
        }
    ' /etc/apache2/ports.conf 2>/dev/null || true
)"

if [ -n "$DETECTED_HTTP_PORT" ]; then
    HTTP_PORT="$DETECTED_HTTP_PORT"
fi

DETECTED_MYSQL_PORT="$(
    mysql --protocol=socket -Nse "SELECT @@port;" 2>/dev/null || true
)"

if [ -n "$DETECTED_MYSQL_PORT" ]; then
    MYSQL_PORT="$DETECTED_MYSQL_PORT"
fi

# ----------------------------------------------------------
# Detect local IP
# ----------------------------------------------------------

SERVER_IP="$(hostname -I 2>/dev/null | awk '{print $1}' || true)"

# ----------------------------------------------------------
# Create credential directory
# ----------------------------------------------------------

mkdir -p "$CRED_DIR"

# ----------------------------------------------------------
# Save credentials
# ----------------------------------------------------------

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
  - Root Redirect     : http://localhost:${HTTP_PORT}/ -> /phpmyadmin
  - Local Access      : http://localhost:${HTTP_PORT}/phpmyadmin
EOF

if [ -n "$SERVER_IP" ]; then
    cat >> "$CRED_FILE" <<EOF
  - LAN/IP Access     : http://${SERVER_IP}:${HTTP_PORT}/phpmyadmin
EOF
fi

# Only root can read the credential file.
chmod 600 "$CRED_FILE"

# ----------------------------------------------------------
# Final verification
# ----------------------------------------------------------

echo ""
echo "Checking services..."

if mysqladmin --protocol=socket ping --silent >/dev/null 2>&1; then
    echo "[OK] MySQL is running"
else
    warn "MySQL health check failed."
fi

if apache2ctl -t >/dev/null 2>&1; then
    echo "[OK] Apache configuration is valid"
else
    warn "Apache configuration test failed."
fi

if command -v curl >/dev/null 2>&1; then
    if curl -fsS \
        --max-time 5 \
        "http://127.0.0.1:${HTTP_PORT}/phpmyadmin/" \
        >/dev/null 2>&1; then

        echo "[OK] phpMyAdmin is reachable"

    else
        warn "phpMyAdmin HTTP check failed."
    fi
fi

echo ""
echo "=========================================================="
echo "Installation completed successfully"
echo "=========================================================="

cat "$CRED_FILE"

echo "=========================================================="
echo "Credentials saved to:"
echo "  ${CRED_FILE}"
echo "=========================================================="