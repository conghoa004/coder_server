#!/bin/bash

set -e

echo "=========================================="
echo " Installing MySQL + phpMyAdmin"
echo "=========================================="

export DEBIAN_FRONTEND=noninteractive

# Update package lists
apt-get update

# Install required packages
apt-get install -y \
    mysql-server \
    mysql-client \
    apache2 \
    php \
    php-mysql \
    php-mbstring \
    php-zip \
    php-gd \
    php-json \
    php-curl \
    wget \
    unzip

echo ""
echo "[1/4] Starting MySQL..."

service mysql start || true

# Check MySQL
mysql --version

echo ""
echo "[2/4] Creating MySQL database user..."

MYSQL_USER="kaggle"
MYSQL_PASSWORD="kaggle123"

mysql <<EOF
CREATE USER IF NOT EXISTS '${MYSQL_USER}'@'localhost'
IDENTIFIED WITH mysql_native_password BY '${MYSQL_PASSWORD}';

GRANT ALL PRIVILEGES ON *.* TO '${MYSQL_USER}'@'localhost';

FLUSH PRIVILEGES;
EOF

echo ""
echo "[3/4] Installing phpMyAdmin..."

PHPMYADMIN_DIR="/var/www/html/phpmyadmin"

if [ ! -d "$PHPMYADMIN_DIR" ]; then

    cd /tmp

    wget -q \
        https://www.phpmyadmin.net/downloads/phpMyAdmin-latest-all-languages.tar.gz \
        -O phpmyadmin.tar.gz

    tar -xzf phpmyadmin.tar.gz

    EXTRACTED_DIR=$(find /tmp -maxdepth 1 -type d -name "phpMyAdmin-*" | head -n 1)

    mkdir -p "$PHPMYADMIN_DIR"

    cp -r "$EXTRACTED_DIR"/* "$PHPMYADMIN_DIR/"

fi

# PHP temporary directory
mkdir -p "$PHPMYADMIN_DIR/tmp"
chmod 777 "$PHPMYADMIN_DIR/tmp"

echo ""
echo "[4/4] Starting Apache..."

service apache2 start || true

echo ""
echo "=========================================="
echo " Installation completed!"
echo "=========================================="

echo ""
echo "MySQL:"
echo "  User     : $MYSQL_USER"
echo "  Password : $MYSQL_PASSWORD"
echo ""
echo "phpMyAdmin:"
echo "  Path     : $PHPMYADMIN_DIR"
echo ""
echo "Local URL:"
echo "  http://127.0.0.1/phpmyadmin"
echo ""

echo "MySQL status:"
mysqladmin ping || true

echo ""
echo "Apache status:"
service apache2 status --no-pager || true