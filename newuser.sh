#!/bin/bash

# Pull in styling library
SELF_PATH=$(dirname "$0")
source "${SELF_PATH}/style.sh"

# SUNet ID is argument 0
if [[ $# -ne 1 ]] ; then
  printf "${FERR}: expected exactly 1 argument\n"
  exit 1
fi
STUDENT=$1

# -------------------------------------------------------------------------------------------------
# Try to create the user if they don't already exist

if ! adduser --disabled-password --quiet --comment "" $STUDENT ; then
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
  $(which psql) -d gradecope -q -c "$1"
}

-cleanup () {
  deluser --remove-home "${STUDENT}"
  -pq-run "DELETE FROM users WHERE name = '${STUDENT}';"
  exit 1
}
  
# -------------------------------------------------------------------------------------------------
# Add the user to the `gradecope-students` group

GROUP=gradecope-students

if ! usermod -aG "${GROUP}" "${STUDENT}" ; then
  printf "${FERR}: couldn't add user ${STUSR}${STUDENT}${RST} to group ${STGRP}${GROUP}${RST}\n"
  -cleanup
else
  printf "Added user ${STUSR}${STUDENT}${RST} to group ${STGRP}${GROUP}${RST}\n"
fi

# -------------------------------------------------------------------------------------------------
# Create and set up git repo

REPO="/home/${STUDENT}/gradecope-repo"

if ! mkdir -p $REPO ; then
  printf "${FERR}: failed to create directory ${STPTH}${REPO}${RST}\n"
  -cleanup
else
  printf "Created directory ${STPTH}${REPO}${RST}\n"
fi

if ! GIT_DIR="${REPO}/.git" git init --quiet --bare ; then
  printf "${FERR}: failed to initialize empty git repository in ${STPTH}${REPO}/.git${RST}\n"
  -cleanup
else
  printf "Initialized empty git repository in ${STPTH}${REPO}/.git${RST}\n"
fi

-git-set-option() {
  if ! GIT_DIR="${REPO}/.git" git config set --local "$1" "$2" ; then
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
if ! cp "${SELF_PATH}/post-receive.sh" "$POST_RECEIVE_PATH" ; then
  printf "${FERR}: failed to install post-receive hook to ${STPTH}${POST_RECEIVE_PATH}${RST}\n"
  -cleanup
else
  printf "Installed ${STPTH}${POST_RECEIVE_PATH}${RST}"
fi

# -------------------------------------------------------------------------------------------------
# Fix permissions
sudo chown -R "${STUDENT}" "/home/${STUDENT}"

# -------------------------------------------------------------------------------------------------
# If everything else succeeded, add to gradecope database

UUID=$(uuid -v 4)

if ! -pq-run "INSERT INTO users (id, name) VALUES ('${UUID}', '$STUDENT');" ; then
  printf "${FERR}: failed to add user ${STUSR}${STUDENT}${RST} to database\n"
  -cleanup
else
  printf "Added user ${STUSR}${STUDENT}${RST} to database\n"
fi
