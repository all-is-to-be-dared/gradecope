#!/bin/bash

# Pull in styling library
SELF_PATH=$(dirname "$0")
source "${SELF_PATH}/style.sh"

# SUNet ID is argument 1
# SSH pubkey is argument 2
if [[ $# -ne 2 ]] ; then
  printf "${FERR}: expected exactly 2 argument\n"
  exit 1
fi
STUDENT="$1"
SSH_PUBKEY="$2"

# -------------------------------------------------------------------------------------------------
# Try to create the user if they don't already exist

if ! sudo adduser --disabled-password --quiet --comment "" $STUDENT ; then
  # Special case for `adduser` failure: if the user already exists, we don't want to do super
  # aggressive cleanup (e.g. deleting the user) since that could produce unexpected results in the
  # event of e.g. a mistype
  printf "${FERR}: failed to create user ${UND}${STUDENT}${RST}\n"
  exit 1
else
  printf "Created user ${UND}${STUDENT}${RST}\n"
fi

# -------------------------------------------------------------------------------------------------
# At this point, we know that the user does not exist, so performing aggressive cleanup is OK

-pq-run () {
  psql -d gradecope -q -c "$1"
}

-cleanup () {
  sudo usermod -rG "${STUDENT}" gradecope
  sudo deluser --remove-home "${STUDENT}"
  -pq-run "DELETE FROM users WHERE name = '${STUDENT}';"
  exit 1
}
  
# -------------------------------------------------------------------------------------------------
# Add the user to the `gradecope-students` group

GROUP=gradecope-students

if ! sudo usermod -aG "${GROUP}" "${STUDENT}" ; then
  printf "${FERR}: couldn't add user ${STUSR}${STUDENT}${RST} to group ${STGRP}${GROUP}${RST}\n"
  -cleanup
else
  printf "Added user ${STUSR}${STUDENT}${RST} to group ${STGRP}${GROUP}${RST}\n"
fi

# -------------------------------------------------------------------------------------------------
# Create and set up git repo

REPO="/home/${STUDENT}/gradecope-repo"

if ! sudo -u "${STUDENT}" mkdir -p $REPO ; then
  printf "${FERR}: failed to create directory ${STPTH}${REPO}${RST}\n"
  -cleanup
else
  printf "Created directory ${STPTH}${REPO}${RST}\n"
fi

if ! sudo -u "${STUDENT}" env GIT_DIR="${REPO}/.git" git init --quiet --bare ; then
  printf "${FERR}: failed to initialize empty git repository in ${STPTH}${REPO}/.git${RST}\n"
  -cleanup
else
  printf "Initialized empty git repository in ${STPTH}${REPO}/.git${RST}\n"
fi

-git-set-option() {
  if ! sudo -u "${STUDENT}" env GIT_DIR="${REPO}/.git" git config --local "$1" "$2" ; then
    printf "${FERR}: failed to set ${STGOP}${1}${RST}=${ITA}${2}${RST}\n"
    -cleanup
  else
    printf "Set ${STGOP}${1}${RST}=${ITA}${2}${RST}\n"
  fi
}

# needed in order for push to work properly
-git-set-option receive.denyCurrentBranch ignore
# needed in order for push options to work properly: even if git version is sufficient, they won't
# actually work unless the server has this in the config
-git-set-option receive.advertisePushOptions true

POST_RECEIVE_PATH="${REPO}/.git/hooks/post-receive"
if ! sudo cp "${SELF_PATH}/post-receive.sh" "$POST_RECEIVE_PATH" ; then
  printf "${FERR}: failed to install post-receive hook to ${STPTH}${POST_RECEIVE_PATH}${RST}\n"
  -cleanup
else
  printf "Installed ${STPTH}${POST_RECEIVE_PATH}${RST}\n"
fi

# -------------------------------------------------------------------------------------------------
# Get SSH ready

SSH_DIR="/home/${STUDENT}/.ssh"
sudo -u "${STUDENT}" mkdir -p "${SSH_DIR}"
echo "${SSH_PUBKEY}" >> "${SSH_DIR}/authorized_keys"
sudo chmod 0700 "${SSH_DIR}"
sudo chmod 0600 "${SSH_DIR}/authorized_keys"

# -------------------------------------------------------------------------------------------------
# Prepare sockets directory that's gradecope-accessible
sudo -u "${STUDENT}" mkdir -p "/home/${STUDENT}/gradecope-sockets"
sudo chmod g+x "/home/${STUDENT}/gradecope-sockets"

# -------------------------------------------------------------------------------------------------
# Fix permissions

sudo chown -R "${STUDENT}" "/home/${STUDENT}"
printf "Fixed permissions for ${STPTH}/home/${STUDENT}${RST}\n"

# -------------------------------------------------------------------------------------------------
# Add gradecope user to the user's group so that we can access their files
sudo usermod -aG "${STUDENT}" gradecope

# -------------------------------------------------------------------------------------------------
# If everything else succeeded, add to gradecope database

UUID=$(uuid -v 4)

if ! -pq-run "INSERT INTO users (id, name) VALUES ('${UUID}', '$STUDENT');" ; then
  printf "${FERR}: failed to add user ${STUSR}${STUDENT}${RST} to database\n"
  -cleanup
else
  printf "Added user ${STUSR}${STUDENT}${RST} to database\n"
fi

# -------------------------------------------------------------------------------------------------
# Notify currently running instance, if it exists

cat <<- HEREDOC | socat - UNIX-CONNECT:~gradecope/gradecope-admin.sock
{"jsonrpc": "2.0", "method": "add-user", "params": {"user": "${STUDENT}"}}
HEREDOC
