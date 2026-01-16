#!/bin/bash

trap "exit 1" TERM
SCRIPT_PID=$$

export WORKER_ID="$1"
export SPEC="$2"
export REMOTE_REPO_PATH="$3"
export COMMIT="$4"
export LOGFILE_PATH="$5"
export DEVICE_SERIAL="$6"
export USB_PORT="$7"
export USB_HUB_PATH="$8"

# ENV: GRADECOPE_SWITCHBOARD_SERVER
# ENV: GRADECOPE_SWITCHBOARD_RUNNER_USER
# ENV: XDG_CACHE_DIR

REMOTE_NAME="upstream"

REMOTE_REPO_URL="ssh://${GRADECOPE_SWITCHBOARD_RUNNER_USER}@${GRADECOPE_SWITCHBOARD_SERVER}/${REMOTE_REPO_PATH}"
REPO_CACHE_DIR="${XDG_CACHE_DIR}/gradecope-runner"
REPO_ID="$(echo "${REMOTE_REPO_PATH}" | sha256sum | cut -f1 -d' ')"
LOCAL_REPO_PATH="${REPO_CACHE_DIR}/${REPO_ID}"

@die () {
  kill -s TERM $SCRIPT_PID
}

@log () {
  # tee "$LOGFILE_PATH"
  cat
}

@clone-repo-if-not-exists () {
  if [[ -d "${LOCAL_REPO_PATH}/.git" ]] ; then
    echo "Repo already exists..."
    return 0
  fi
  # rm -rf "${LOCAL_REPO_PATH}"
  echo "Switching to ${LOCAL_REPO_PATH}"
  cd "${LOCAL_REPO_PATH}"
  echo "Initializing git repo"
  git init
  echo "Adding remote ${REMOTE_REPO_URL}"
  git remote add "${REMOTE_NAME}" "${REMOTE_REPO_URL}"
  return 0
}

@update-repo () {
  @clone-repo-if-not-exists

  echo "Switching to ${LOCAL_REPO_PATH}"

  cd "${LOCAL_REPO_PATH}"

  echo ">>> git fetch ${REMOTE_NAME} ${COMMIT}"
  git fetch "${REMOTE_NAME}" "${COMMIT}"
  git checkout "${COMMIT}"
}

# Clear logfile

echo | @log
echo "Starting runner with invocation: $0 $*" | @log
echo | @log

# Update repo

echo "Updating repo @ ${LOCAL_REPO_PATH}" | @log
mkdir -p "${LOCAL_REPO_PATH}"
@update-repo | @log

# Get pi-install proxy ready

PATH_ADD="$(mktemp -d)"
PROXY_PI_INSTALL="${PATH_ADD}/pi-install"
ORIGINAL_PI_INSTALL="$(which pi-install)"
cat <<EOF > "${PROXY_PI_INSTALL}"
#!/bin/bash
if [[ "$#" -ne 1 ]] ; then
  echo "failed to forward pi-install to ${DEVICE_SERIAL}: expected exactly one argument, got ${#}"
  exit 1
fi
"${ORIGINAL_PI_INSTALL}" "${DEVICE_SERIAL}" $1
EOF
chmod +x "${PROXY_PI_INSTALL}"
export PATH="${PATH_ADD}:${PATH}"

echo "Installed proxied pi-install to ${PROXY_PI_INSTALL}" | @log

# Reboot USB device

echo "Cycling power to Pi" | @log

uhubctl -a cycle -p $USB_PORT -l $USB_HUB_PATH | @log

cd "${LOCAL_REPO_PATH}/labs/1-trusting-trust"
make clean
make check
exit
