#!/bin/bash

# -------------------------------------------------------------------------------------------------
# Pull in styling library

SELF_PATH=$(dirname "$0")
source "${SELF_PATH}/style.sh"

# -------------------------------------------------------------------------------------------------
# Settings

source "${SELF_PATH}/config.env"

# -------------------------------------------------------------------------------------------------
# Database helper

@pg-run () {
  psql -d "${GRADECOPE_SWITCHBOARD_USER}" -q -c "$1"
}

# -------------------------------------------------------------------------------------------------
# Parse script arguments

# SUNet ID is argument 1
# SSH pubkey is argument 2
if [[ $# -ne 2 ]] ; then
  echo -e "${FERR}: expected exactly 2 arguments"
  exit 1
fi
STUDENT="$1"
SSH_PUBKEY="$2"

# -------------------------------------------------------------------------------------------------
# Try to create the user if they don't already exist

if ! sudo adduser --disabled-password --quiet --comment '' "${STUDENT}" ; then
  # Special case for `adduser` failure: if the user already exists, we don't want to do super
  # aggressive cleanup (e.g. deleting the user) since that could produce unexpected results in the
  # event of e.g. a mistype
  echo -e "${FERR}: failed to create user ${UND}${STUDENT}${RST}"
  exit 1
else
  echo -e "Created user ${UND}${STUDENT}${RST}"
fi

# -------------------------------------------------------------------------------------------------
# At this point, we know that the user does not exist, so performing aggressive cleanup is OK, so
# we define a couple of utility functions.

@cleanup () {
  sudo usermod -rG "${STUDENT}" gradecope
  sudo deluser --remove-home "${STUDENT}"
  @pg-run "DELETE FROM users WHERE name = '${STUDENT}';"
  exit 1
}
  
# -------------------------------------------------------------------------------------------------
# Add the user to the `gradecope-students` group
#
# This is mostly for accounting purposes and for future-proofing. It does not currently have any
# function.

@add-group () {
  local user="$1"
  local group="$2"

  if ! sudo usermod -aG "${group}" "${user}" ; then
    echo -e "${FERR}: couldn't add user ${STUSR}${user}${RST} to group ${STGRP}${group}${RST}\n"
    @cleanup
  else
    echo -e "Added user ${STUSR}${user}${RST} to group ${STGRP}${group}${RST}\n"
  fi
}

@add-group "${STUDENT}" "${GRADECOPE_STUDENTS_GROUP}}"

# -------------------------------------------------------------------------------------------------
# Create and set up git repo
#
# This is a bit messy, but basically we need to
#  1) create & initialize the repo
#  2) make sure that students are able to push to the repo (with push options)
#  3) install the post-receive hook and ensure its permissions are properly set

REPO="/home/${STUDENT}/gradecope-repo"

if ! sudo -u "${STUDENT}" mkdir -p "$REPO" ; then
  echo -e "${FERR}: failed to create directory ${STPTH}${REPO}${RST}\n"
  @cleanup
else
  echo -e "Created directory ${STPTH}${REPO}${RST}\n"
fi

if ! sudo -u "${STUDENT}" env GIT_DIR="${REPO}/.git" git init --quiet --bare ; then
  echo -e "${FERR}: failed to initialize empty git repository in ${STPTH}${REPO}/.git${RST}\n"
  @cleanup
else
  echo -e "Initialized empty git repository in ${STPTH}${REPO}/.git${RST}\n"
fi

@git-set-option() {
  if ! sudo -u "${STUDENT}" env GIT_DIR="${REPO}/.git" git config --local "$1" "$2" ; then
    echo -e "${FERR}: failed to set ${STGOP}${1}${RST}=${ITA}${2}${RST}\n"
    @cleanup
  else
    echo -e "Set ${STGOP}${1}${RST}=${ITA}${2}${RST}\n"
  fi
}

# needed in order for push to work properly
@git-set-option receive.denyCurrentBranch ignore
# needed in order for push options to work properly: even if git version is sufficient, they won't
# actually work unless the server has this in the config
@git-set-option receive.advertisePushOptions true

POST_RECEIVE_PATH="${REPO}/.git/hooks/post-receive"
if ! sudo cp "${SELF_PATH}/post-receive.sh" "${POST_RECEIVE_PATH}" ; then
  echo -e "${FERR}: failed to install post-receive hook to ${STPTH}${POST_RECEIVE_PATH}${RST}\n"
  @cleanup
else
  echo -e "Installed ${STPTH}${POST_RECEIVE_PATH}${RST}\n"
fi

sudo chown "${STUDENT}":"${STUDENT}" "${POST_RECEIVE_PATH}"
sudo chmod -wx "${POST_RECEIVE_PATH}"
sudo chmod u+x "${POST_RECEIVE_PATH}"

# -------------------------------------------------------------------------------------------------
# Prepare sockets directory that's gradecope-accessible.
#
# In order for the switchboard to run properly, we need to make sure that whatever directory the
# socket will go in has g+w permission, but that disagrees with SSH's requirements for home
# directory permissions, which require 0755 perms for /home/$student, so we make a separate
# directory, with group write permissions, and navigable by the $student group.
sudo -u "${STUDENT}" mkdir -p "/home/${STUDENT}/gradecope-sockets"
sudo chmod g+x "/home/${STUDENT}/gradecope-sockets"

# -------------------------------------------------------------------------------------------------
# Fix permissions
#
# This is a just-in-case to fix any issues that might've cropped up with user/group ownership of
# $student's files. There are a few specific things that this fixes.

sudo chown -R "${STUDENT}" "/home/${STUDENT}"
echo -e "Fixed permissions for ${STPTH}/home/${STUDENT}${RST}\n"

# -------------------------------------------------------------------------------------------------
# Add gradecope users to the user's group so that we can access their files
#
# This is required so that
#  1) the switchboard is able to create the submission sockets + post-receive hooks are able to
#     write to them
#  2) the runner is able to pull from the upstream crates
@add-group "${GRADECOPE_SWITCHBOARD_USER}" "${STUDENT}"
@add-group "${GRADECOPE_RUNNER_USER}" "${STUDENT}"

# -------------------------------------------------------------------------------------------------
# If everything else succeeded, add to gradecope database

# user id doesn't really matter
UUID=$(uuid -v 4)

if ! @pg-run "INSERT INTO users (id, name) VALUES ('${UUID}', '$STUDENT');" ; then
  echo -e "${FERR}: failed to add user ${STUSR}${STUDENT}${RST} to database\n"
  @cleanup
else
  echo -e "Added user ${STUSR}${STUDENT}${RST} to database\n"
fi

# -------------------------------------------------------------------------------------------------
# Notify currently running instance, if it exists
#
# TODO: what's the protocol?

#{"jsonrpc": "2.0", "method": "add-user", "params": {"user": "${STUDENT}"}}
cat <<- HEREDOC | socat - "UNIX-CONNECT:~${GRADECOPE_SWITCHBOARD_USER}/${GRADECOPE_SOCKETS_DIR}/admin.sock"
HEREDOC

# -------------------------------------------------------------------------------------------------
# Get SSH ready
#
# Need to ensure that students can actually SSH into their repos. This is the last thing we do
# because student SSH is also one of the things that can really break atomicity by adding
# observability.

SSH_DIR="/home/${STUDENT}/.ssh"
sudo -u "${STUDENT}" mkdir -p "${SSH_DIR}"
echo "${SSH_PUBKEY}" >> "${SSH_DIR}/authorized_keys"
sudo chmod 0700 "${SSH_DIR}"
sudo chmod 0600 "${SSH_DIR}/authorized_keys"

C
