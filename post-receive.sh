#!/bin/bash

path=$(pwd)
repo_path="${path}/.."
socket_path="${repo_path}/../gradecope-sockets/submit.sock"

user=$(whoami)

read -r _oldrev _newrev refname
commit=$(git rev-parse "$refname")
echo "> gradecope: Received branch ${commit}"

submit-job() {
  echo "{\"user\":\"${user}\", \"commit\":\"${commit}\", \"spec\":\"${1}\"}" | socat - "UNIX-CONNECT:${socket_path}"
}

option_count_minus_one="$(("$GIT_PUSH_OPTION_COUNT"-1))"

for i in $(seq 0 $option_count_minus_one) ; do
  as_git_var="GIT_PUSH_OPTION_${i}"
  submit-job "${!as_git_var}"
done
