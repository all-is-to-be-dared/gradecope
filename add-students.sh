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

while IFS=, read -r EMAIL PUBKEY ; do
  # Skip empty lines and comments
  [[ -z "${EMAIL}" || "${EMAIL}" =~ ^# ]] && continue

  # Extract username from email (part before @)
  USERNAME="${EMAIL%%@*}"

  echo "Creating user: ${USERNAME}"

  ./newuser $USERNAME $PUBKEY

  echo "  -> Done"
done < "${CSV_FILE}"

echo "Finished adding students"
