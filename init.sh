#!/bin/bash

SELF_PATH=$(dirname $0)

source "${SELF_PATH}/style.sh"

apt update
apt upgrade

# amd64:
#sudo apt install -y postgresql-common
#sudo /usr/share/postgresql-common/pgdg/apt.postgresql.org.sh
#sudo apt install -y postgresql-18
# arm64:
#sudo apt install postgresql-17

sudo apt update

apt install perl git

# Note: we need git>=2.10 for push options
-check-git-version () {
  local GIT_VERSION=$( git --version | grep -Po '(?<=git version ).+' )
  local tmp=$( ( echo "${GIT_VERSION}" ; echo "2.10" ) | sort -V | head -n1 )
  # Funky trick: git version returns a X.Y.Z version, and 2.10.0 is sorted before 2.10 by sort -V
  [[ "x${GIT_VERSION}" = "x2.10.0" ]] || [[ "x${tmp}" = "x2.10" ]]
}
if ! -check-git-version ; then
  printf "${FERR}: ${STPKG}gradecope${RST} requires ${STPKG}git${RST}>=${STVER}2.10${RST}\n"
  exit 1
fi

# Note: mostly for organizational purposes
addgroup gradecope-students

# Note: equivalent to CREATE DATABASE "gradecope" IF NOT EXISTS;
if ! psql -c '\l' --csv | grep 'gradecope,' ; then
  psql -c 'CREATE DATABASE "gradecope";'
fi

# Note: we expressly do NOT run `INIT.sql` because that will drop all tables if the database
#       already exists.
