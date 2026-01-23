#!/bin/bash
# Idempotent update script for rebuilding and reinstalling gradecope components

set -e

SELF_PATH="$0"
SELF_DIR="$(dirname "${SELF_PATH}")"
cd "${SELF_DIR}"

source "${SELF_DIR}/config.sh"

echo "Building release binaries..."
cargo build --release -p gradecope-ctl -p gradecope-switchboard

echo "Installing gradecope-ctl to /usr/local/bin..."
sudo cp target/release/gradecope-ctl /usr/local/bin/
sudo chmod 755 /usr/local/bin/gradecope-ctl

echo "Installing gradecope-switchboard to /usr/local/bin..."
sudo cp target/release/gradecope-switchboard /usr/local/bin/
sudo chmod 755 /usr/local/bin/gradecope-switchboard

# Ensure socket directory exists with correct permissions
if [[ ! -d /var/run/gradecope ]]; then
  echo "Creating /var/run/gradecope..."
  sudo mkdir -p /var/run/gradecope
fi
sudo chown "${GRADECOPE_SWITCHBOARD_USER}:${GRADECOPE_STUDENTS_GROUP}" /var/run/gradecope
sudo chmod 755 /var/run/gradecope

echo "Done. You may need to restart the switchboard service."
