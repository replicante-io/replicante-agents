#!/usr/bin/env sh
set -ex

if [ "$TRAVIS_RUST_VERSION" == "nightly" ]; then
  rustup component add clippy-preview
else 
  rustup component add clippy
fi