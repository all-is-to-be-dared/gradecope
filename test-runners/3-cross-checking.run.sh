#!/bin/bash

trap "exit 1" TERM
SCRIPT_PID=$$

WORKER_ID="$1"
SPEC="$2"
REMOTE_REPO_PATH="$3"
COMMIT="$4"
LOGFILE_PATH="$5"
DEVICE_SERIAL="$6"
USB_PORT="$7"
USB_HUB_PATH="$8"

# ENV: GRADECOPE_SWITCHBOARD_SERVER
# ENV: GRADECOPE_SWITCHBOARD_RUNNER_USER
# ENV: XDG_CACHE_DIR

REMOTE_REPO_URL="ssh://${GRADECOPE_SWITCHBOARD_RUNNER_USER}@${GRADECOPE_SWITCHBOARD_SERVER}/${REMOTE_REPO_PATH}"
REPO_CACHE_DIR="${XDG_CACHE_DIR}/gradecope-runner"
REPO_ID="$(echo "${REMOTE_REPO_PATH}" | sha256sum | cut -f1 -d' ')"
LOCAL_REPO_PATH="${REPO_CACHE_DIR}/${REPO_ID}"

@die () {
  kill -s TERM $SCRIPT_PID
}

@clone-repo-if-not-exists () {
  if [[ -d "${LOCAL_REPO_PATH}" ]] ; then
    exit 1
  fi
  rm -rf "${LOCAL_REPO_PATH}"
  mkdir -p "${LOCAL_REPO_PATH}"
  cd "${LOCAL_REPO_PATH}" || @die
  git init
  git remote add upstream "${REMOTE_REPO_URL}"
  git fetch
  git
  exit 0
}

@update-repo () {
  if @clone-repo-if-not-exists ; then exit ; fi

}