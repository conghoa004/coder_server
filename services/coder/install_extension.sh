#!/usr/bin/env bash

set -u

# ==========================================
# Configuration
# ==========================================

EXTENSIONS=(
    "ms-python.python"
    "ms-toolsai.jupyter"
    # "formulahendry.code-runner"
    "willbot.tunnelfy"
    "tamasfe.even-better-toml"
)

# ==========================================
# Find code-server
# ==========================================

echo "Searching for code-server..."

CODE_SERVER="$(command -v code-server 2>/dev/null || true)"

if [[ -z "$CODE_SERVER" ]]; then
    CODE_SERVER="$(find "$HOME/.local/lib" \
        -path '*/bin/code-server' \
        -type f 2>/dev/null | sort -V | tail -n 1)"
fi

if [[ -z "$CODE_SERVER" || ! -x "$CODE_SERVER" ]]; then
    echo "ERROR: code-server executable not found."
    exit 1
fi

echo "Found: $CODE_SERVER"
echo "Version: $("$CODE_SERVER" --version | head -n 1)"

# ==========================================
# Install extensions
# ==========================================

SUCCESS=0
FAILED=0

echo
echo "Starting extension installation..."
echo "=========================================="

for extension in "${EXTENSIONS[@]}"; do
    echo
    echo "Installing: $extension"

    if "$CODE_SERVER" --install-extension "$extension"; then
        echo "SUCCESS: $extension"
        ((SUCCESS+=1))
    else
        echo "FAILED: $extension"
        ((FAILED+=1))
    fi
done

# ==========================================
# Summary
# ==========================================

echo
echo "=========================================="
echo "Extension installation completed."
echo "Total:      ${#EXTENSIONS[@]}"
echo "Successful: $SUCCESS"
echo "Failed:     $FAILED"
echo "=========================================="

# ==========================================
# List installed extensions
# ==========================================

echo
echo "Installed extensions:"
"$CODE_SERVER" --list-extensions

echo
echo "Done."
