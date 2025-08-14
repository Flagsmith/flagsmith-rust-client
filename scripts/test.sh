#!/usr/bin/env bash

function run-default-tests() {
  echo "Running default tests"
  cargo test
}

function run-fastly-tests() {
  echo "Running fastly/wasm tests"
  ./scripts/fastly-unit-test.sh
}

if [ "$0" = "${BASH_SOURCE[0]}" ]; then
  set -eo pipefail

  run-default-tests && run-fastly-tests

  exit $?
fi
