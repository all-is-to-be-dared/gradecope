#!/bin/bash

SELF_PATH="$0"
SELF_DIR=$(dirname "${SELF_PATH}")
source "${SELF_DIR}/config.sh"

if [[ $# -ne 1 ]] ; then
  echo "Usage: $0 <csv-file>"
  echo "CSV format: email,public_key"
  exit 1
fi

CSV_FILE="$1"

if [[ ! -f "${CSV_FILE}" ]] ; then
  echo "Error: file '${CSV_FILE}' not found"
  exit 1
fi

if [[ $(whoami) != "root" ]] ; then
  echo "Error: must run as root"
  exit 1
fi

while IFS=, read -r EMAIL PUBKEY ; do
  # Skip empty lines and comments
  [[ -z "${EMAIL}" || "${EMAIL}" =~ ^# ]] && continue

  # Extract username from email (part before @)
  USERNAME="${EMAIL%%@*}"

  echo "Creating user: ${USERNAME}"

  # Create user if doesn't exist
  if ! id "${USERNAME}" &>/dev/null ; then
    adduser --disabled-password --gecos '' "${USERNAME}"
    usermod -aG "${GRADECOPE_STUDENTS_GROUP}" "${USERNAME}"
  fi

  # Set up SSH
  HOME_DIR="/home/${USERNAME}"
  SSH_DIR="${HOME_DIR}/.ssh"
  AUTHORIZED_KEYS="${SSH_DIR}/authorized_keys"

  sudo -u "${USERNAME}" mkdir -p "${SSH_DIR}"
  chmod 0755 "${HOME_DIR}"
  chmod 0700 "${SSH_DIR}"

  # Append key if not already present
  if ! grep -qF "${PUBKEY}" "${AUTHORIZED_KEYS}" 2>/dev/null ; then
    echo "${PUBKEY}" >> "${AUTHORIZED_KEYS}"
  fi

  chown -R "${USERNAME}:${USERNAME}" "${SSH_DIR}"
  chmod 0600 "${AUTHORIZED_KEYS}"

  echo "  -> Done"
done < "${CSV_FILE}"

echo "Finished adding students"
