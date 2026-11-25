#!/bin/bash

set -e

export DEBIAN_FRONTEND=noninteractive

MYSQL_VERSION="8.4"
PHPMYADMIN_VERSION="5.2.3"

MYSQL_USER="kaggle"
MYSQL_PASSWORD="kaggle123"

PHPMYADMIN_DIR="/var/www/html/phpmyadmin"

echo "=========================================="
echo " MySQL ${MYSQL_VERSION} + phpMyAdmin"
echo "=========================================="

# --------------------------------------------------
# 1. Check root
# --------------------------------------------------

if [ "$(id -u)" -ne 0 ]; then
    echo "ERROR: Please run this script as root."
    echo "Example:"
    echo "  sudo bash instal+start.sh"
    exit 1
fi

# --------------------------------------------------
# 2. Detect Linux distribution
# --------------------------------------------------

if [ ! -f /etc/os-release ]; then
    echo "ERROR: Cannot detect Linux distribution."
    exit 1
fi

source /etc/os-release

echo ""
echo "[INFO] OS      : ${PRETTY_NAME}"
echo "[INFO] ID      : ${ID}"
echo "[INFO] Codename: ${VERSION_CODENAME:-unknown}"

if [ "$ID" != "debian" ] && [ "$ID" != "ubuntu" ]; then
    echo ""
    echo "ERROR: This script supports Debian/Ubuntu only."
    exit 1
fi

if [ -z "${VERSION_CODENAME:-}" ]; then
    echo "ERROR: Cannot determine Debian/Ubuntu codename."
    exit 1
fi

CODENAME="$VERSION_CODENAME"

# --------------------------------------------------
# 3. Update system packages
# --------------------------------------------------

echo ""
echo "[1/7] Updating APT..."

apt-get update

# --------------------------------------------------
# 4. Basic dependencies
# --------------------------------------------------

echo ""
echo "[2/7] Installing dependencies..."

apt-get install -y \
    ca-certificates \
    wget \
    curl \
    gnupg \
    lsb-release \
    debconf-utils \
    apache2 \
    php \
    php-cli \
    php-common \
    php-mysql \
    php-mbstring \
    php-zip \
    php-gd \
    php-curl \
    php-xml \
    php-bcmath \
    unzip

# --------------------------------------------------
# 5. Install official MySQL APT Repository
# --------------------------------------------------

echo ""
echo "[3/7] Installing official MySQL APT Repository..."

cd /tmp

MYSQL_APT_PACKAGE="mysql-apt-config_0.8.40-1_all.deb"

wget -q \
    "https://dev.mysql.com/get/${MYSQL_APT_PACKAGE}" \
    -O "${MYSQL_APT_PACKAGE}"

if [ ! -s "${MYSQL_APT_PACKAGE}" ]; then
    echo "ERROR: Failed to download MySQL APT Repository package."
    exit 1
fi

dpkg -i "${MYSQL_APT_PACKAGE}" || true

# --------------------------------------------------
# Force MySQL 8.4 LTS repository
# --------------------------------------------------

echo ""
echo "[INFO] Configuring MySQL ${MYSQL_VERSION} LTS repository..."

# Find MySQL repository files
MYSQL_REPO_FILES=$(find /etc/apt/sources.list.d \
    -maxdepth 1 \
    -type f \
    \( -name "*mysql*.list" -o -name "*mysql*.sources" \) \
    2>/dev/null || true)

if [ -z "$MYSQL_REPO_FILES" ]; then
    echo "ERROR: MySQL APT repository file was not found."
    exit 1
fi

echo "[INFO] MySQL repository files:"
echo "$MYSQL_REPO_FILES"

# Replace possible selected MySQL release with 8.4 LTS.
for FILE in $MYSQL_REPO_FILES; do

    if grep -q "mysql-" "$FILE"; then

        sed -i \
            -E 's/mysql-[0-9]+\.[0-9]+(-lts)?/mysql-8.4-lts/g' \
            "$FILE"

    fi

done

# Show repository configuration
echo ""
echo "[INFO] MySQL repository configuration:"

grep -R "repo.mysql.com" \
    /etc/apt/sources.list.d/ \
    2>/dev/null || true

# --------------------------------------------------
# 6. Install MySQL
# --------------------------------------------------

echo ""
echo "[4/7] Installing MySQL ${MYSQL_VERSION}..."

apt-get update

apt-get install -y mysql-server mysql-client

# --------------------------------------------------
# 7. Start MySQL
# --------------------------------------------------

echo ""
echo "[5/7] Starting MySQL..."

# Kaggle containers may not use systemd.
# Try systemctl first, then service.

if command -v systemctl >/dev/null 2>&1; then
    systemctl start mysql 2>/dev/null || true
fi

service mysql start 2>/dev/null || true

# Give MySQL a moment to start.
sleep 3

if mysqladmin ping >/dev/null 2>&1; then
    echo "[OK] MySQL is running."
else
    echo "[WARNING] MySQL did not respond immediately."
fi

# --------------------------------------------------
# 8. Create MySQL user
# --------------------------------------------------

echo ""
echo "[INFO] Creating MySQL user..."

mysql <<EOF

CREATE USER IF NOT EXISTS '${MYSQL_USER}'@'localhost'
IDENTIFIED BY '${MYSQL_PASSWORD}';

ALTER USER '${MYSQL_USER}'@'localhost'
IDENTIFIED BY '${MYSQL_PASSWORD}';

GRANT ALL PRIVILEGES
ON *.*
TO '${MYSQL_USER}'@'localhost';

FLUSH PRIVILEGES;

EOF

# --------------------------------------------------
# 9. Install phpMyAdmin
# --------------------------------------------------

echo ""
echo "[6/7] Installing phpMyAdmin ${PHPMYADMIN_VERSION}..."

cd /tmp

PHPMYADMIN_ARCHIVE="phpMyAdmin-${PHPMYADMIN_VERSION}-all-languages.tar.gz"

PHPMYADMIN_URL="https://www.phpmyadmin.net/downloads/${PHPMYADMIN_ARCHIVE}"

wget -q \
    "$PHPMYADMIN_URL" \
    -O "$PHPMYADMIN_ARCHIVE"

if [ ! -s "$PHPMYADMIN_ARCHIVE" ]; then
    echo "ERROR: Failed to download phpMyAdmin."
    exit 1
fi

rm -rf "/tmp/phpMyAdmin-${PHPMYADMIN_VERSION}"

tar -xzf "$PHPMYADMIN_ARCHIVE"

rm -rf "$PHPMYADMIN_DIR"

mkdir -p "$PHPMYADMIN_DIR"

cp -a \
    "/tmp/phpMyAdmin-${PHPMYADMIN_VERSION}/." \
    "$PHPMYADMIN_DIR/"

# phpMyAdmin temporary directory
mkdir -p "$PHPMYADMIN_DIR/tmp"

chmod 777 "$PHPMYADMIN_DIR/tmp"

# Apache permissions
chown -R www-data:www-data "$PHPMYADMIN_DIR"

# --------------------------------------------------
# 10. Configure phpMyAdmin
# --------------------------------------------------

echo ""
echo "[INFO] Configuring phpMyAdmin..."

CONFIG_FILE="$PHPMYADMIN_DIR/config.inc.php"

if [ ! -f "$CONFIG_FILE" ]; then

cat > "$CONFIG_FILE" <<'EOF'
<?php

$cfg['blowfish_secret'] = 'kaggle-mysql-phpmyadmin-secret-CHANGE-ME-123456789';

$i = 1;

$cfg['Servers'][$i]['auth_type'] = 'cookie';
$cfg['Servers'][$i]['host'] = '127.0.0.1';
$cfg['Servers'][$i]['port'] = '3306';
$cfg['Servers'][$i]['compress'] = false;
$cfg['Servers'][$i]['AllowNoPassword'] = false;

$cfg['TempDir'] = '/var/www/html/phpmyadmin/tmp';

?>
EOF

fi

chown www-data:www-data "$CONFIG_FILE"
chmod 640 "$CONFIG_FILE"

# --------------------------------------------------
# 11. Configure Apache
# --------------------------------------------------

echo ""
echo "[INFO] Configuring Apache..."

a2enmod php* 2>/dev/null || true

cat > /etc/apache2/conf-available/phpmyadmin.conf <<EOF
Alias /phpmyadmin ${PHPMYADMIN_DIR}

<Directory ${PHPMYADMIN_DIR}>
    Options FollowSymLinks
    DirectoryIndex index.php

    AllowOverride All

    Require all granted
</Directory>
EOF

a2enconf phpmyadmin

# --------------------------------------------------
# 12. Start Apache
# --------------------------------------------------

echo ""
echo "[7/7] Starting Apache..."

if command -v systemctl >/dev/null 2>&1; then
    systemctl start apache2 2>/dev/null || true
fi

service apache2 start 2>/dev/null || true

# Restart Apache to apply configuration
service apache2 restart 2>/dev/null || true

# --------------------------------------------------
# 13. Verify installation
# --------------------------------------------------

echo ""
echo "=========================================="
echo " Installation completed"
echo "=========================================="

echo ""
echo "MySQL:"
mysql --version

echo ""
echo "phpMyAdmin:"
echo "Version: ${PHPMYADMIN_VERSION}"

echo ""
echo "MySQL user:"
echo "  Username: ${MYSQL_USER}"
echo "  Password: ${MYSQL_PASSWORD}"

echo ""
echo "Local phpMyAdmin:"
echo "  http://127.0.0.1/phpmyadmin/"

echo ""
echo "MySQL status:"
mysqladmin ping 2>/dev/null || true

echo ""
echo "Apache status:"
service apache2 status --no-pager 2>/dev/null || true

echo ""
echo "=========================================="
echo " Done"
echo "=========================================="