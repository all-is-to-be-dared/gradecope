#!/bin/bash

path=$(pwd)
repo_path="${path}/.."
socket_path="${path}/../../gradecope-sockets/gradecope-submit.sock"


user=$(whoami)
commit=$(git rev-parse HEAD)

submit-job() {
  echo "{\"user\":\"${user}\", \"commit\":\"${commit}\", \"spec\":\"${1}\"}" | socat - UNIX-CONNECT:${socket_path}
}

let option_count_minus_one=$(($GIT_PUSH_OPTION_COUNT-1))

echo "$option_count_minus_one"

for i in $(seq 0 $option_count_minus_one) ; do
  as_gitvar="GIT_PUSH_OPTION_${i}"
  submit-job "${!as_gitvar}"
done
