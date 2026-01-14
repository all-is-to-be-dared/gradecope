#!/bin/bash

SELF_PATH="$0"
SELF_DIR="$(dirname "$SELF_PATH")"
source "${SELF_DIR}/style.sh"
source "${SELF_DIR}/config.env"

COMMAND="$(cat <<HEREDOC
env PGDATABASE="${GRADECOPE_DATABASE}" RUST_LOG="gradecope=debug" \
  cargo run --bin gradecope-switchboard -- \
    --bind-server 127.0.0.1:${GRADECOPE_RUNNER_PORT}
HEREDOC
)"
sg "${GRADECOPE_STUDENTS_GROUP}" "${COMMAND}"