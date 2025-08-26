#!/usr/bin/env bash

function run-specs() {
  cargo nextest run --target wasm32-wasip1 flagsmith::client
}

if [ "$0" = "${BASH_SOURCE[0]}" ]; then
  set -eo pipefail 
  run-specs "${@:-}"
  exit $?
fi
