#!/bin/bash

SELF_PATH="$0"
SELF_DIR=$(dirname "${SELF_PATH}")
source "${SELF_DIR}/style.sh"
source "${SELF_DIR}/config.sh"
source "${SELF_DIR}/pubkeys.sh" # GRADECOPE_<role>_PUBKEYS : string

if [[ $# -gt 0 ]] ; then
  echo -e "${FERR}: expected no arguments"
  exit 1
fi

# -------------------------------------------------------------------------------------------------
# HELPERS

@create-ssh-user () {
  local USER, PUBKEYS, HOME_DIR, SSH_DIR, AUTHORIZED_KEYS
  USER="$1"
  PUBKEYS="$2"

  adduser --disabled-password --comment '' "${USER}"
  local AUTHORIZED_KEYS
  HOME_DIR="/home/${USER}"
  SSH_DIR="${HOME_DIR}/.ssh"
  AUTHORIZED_KEYS="/${SSH_DIR}/authorized_keys"
  sudo -u "${USER}" mkdir -p "${SSH_DIR}"
  sudo touch -u "${USER}" touch "${AUTHORIZED_KEYS}"
  sudo chmod 0755 "${HOME_DIR}"
  sudo chmod 0700 "${SSH_DIR}"
  sudo chmod 0600 "${AUTHORIZED_KEYS}"
  echo "${PUBKEYS}" | sudo -u "${USER}" tee "${AUTHORIZED_KEYS}" > /dev/null
}

# -------------------------------------------------------------------------------------------------
# INSTALLER STAGES

@stage1 () {
  if [[ $(whoami) != "${GRADECOPE_SWITCHBOARD_USER}" ]] ; then
    echo -e "${FERR}: stage1 needs to run as ${GRADECOPE_SWITCHBOARD_USER}"
    exit 1
  fi

  sudo apt update
  sudo apt upgrade -y

  # -----------------------------------------------------------------------------------------------
  # SSH hardening

  sudo apt install -y fail2ban
  sudo sed -ir 's/^#?(PasswordAuthentication) .+/\1 no/' /etc/ssh/sshd_config
  sudo sed -ir 's/^(PermitRootLogin) .+/\1 no/' /etc/ssh/sshd_config

  sudo systemctl restart ssh

  # -----------------------------------------------------------------------------------------------
  # Start postgres database

  sudo apt install -y postgresql-common
  sudo /usr/share/postgresql-common/pgdg/apt.postgresql.org.sh
  sudo apt install -y postgresql-18

  # -----------------------------------------------------------------------------------------------
  # Build essentials

  sudo apt install -y perl git build-essential
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y

  # -----------------------------------------------------------------------------------------------
  # Check git version

  MIN_GIT_VERSION="2.10"
  # Note: we need git>=2.10 for push options
  @check-git-version () {
    local GIT_VERSION, VERSION_LIST
    GIT_VERSION=$( git --version | grep -Po '(?<=git version ).+' )
    VERSION_LIST=$( ( echo "${GIT_VERSION}" ; echo "${MIN_GIT_VERSION}" ) | sort -V | head -n1 )
    # Funky trick: git version returns a X.Y.Z version, and 2.10.0 is sorted before 2.10 by sort -V
    [[ "${GIT_VERSION}" = "${MIN_GIT_VERSION}.0" ]] || [[ "${VERSION_LIST}" = "${MIN_GIT_VERSION}" ]]
  }
  if ! @check-git-version ; then
    echo -e "${FERR}: ${STPKG}gradecope${RST} requires ${STPKG}git${RST}>=${STVER}${MIN_GIT_VERSION}${RST}\n"
    exit 1
  fi

  # -----------------------------------------------------------------------------------------------
  # Group permissions

  # This is necessary for students to be able to write to submit sockets: since the sockets are
  # created by the switchboard process, they get the UID+GID of that process. Therefore, we need a
  # GID that is accessible to the student user accounts, which is precisely this:
  sudo addgroup "${GRADECOPE_STUDENTS_GROUP}"
  sudo usermod -aG "${GRADECOPE_STUDENTS_GROUP}"

  # -----------------------------------------------------------------------------------------------
  # Postgres initialization

  sudo systemctl restart system-postgresql.slice

  # Need to make sure that the switchboard user has permission to access the sockets and owns the database
  sudo -u postgres createuser "${GRADECOPE_SWITCHBOARD_USER}"
  sudo -u postgres createdb -O "${GRADECOPE_SWITCHBOARD_USER}" "${GRADECOPE_DATABASE}"

  psql -f init.sql

  # -----------------------------------------------------------------------------------------------
  # Other essential packages
  sudo apt install -y uhubctl uuid socat caddy

  # -----------------------------------------------------------------------------------------------
  # Build and install gradecope-ctl system-wide
  # This allows students to use ctl in non-interactive SSH sessions

  source "${HOME}/.cargo/env"
  cd "${SELF_DIR}"
  cargo build --release -p gradecope-ctl
  sudo cp target/release/gradecope-ctl /usr/local/bin/
  sudo chmod 755 /usr/local/bin/gradecope-ctl
}

###########
## stage0 setup script is run as root and sets up the switchboard and runner users
## It exits by sudo-ing as the switchboard user and running the stage1 setup script
@stage0 () {
  if [[ $(whoami) != "root" ]] ; then
    echo -e "${FERR}: stage0 needs to run as root"
    exit 1
  fi

  @create-ssh-user "${GRADECOPE_SWITCHBOARD_USER}" "${GRADECOPE_SWITCHBOARD_PUBKEYS}"
  @create-ssh-user "${GRADECOPE_RUNNER_USER}" "${GRADECOPE_RUNNER_PUBKEYS}"

  # Creeate socket location
  mkdir /var/run/gradecope
  chown "${GRADECOPE_SWITCHBOARD_USER}:${GRADECOPE_STUDENTS_GROUP}" /var/run/gradecope
  chmod 755 /var/run/gradecope

  sudo -u "${GRADECOPE_SWITCHBOARD_USER}" @stage1
}


@stage0
