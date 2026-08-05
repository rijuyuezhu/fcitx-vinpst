#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

cargo test -p vinpst-daemon --features=pipewire-backend
cargo clippy -p vinpst-daemon --all-targets --features=pipewire-backend -- -D warnings
cargo test -p vinpst-audio --features=pipewire-backend
cargo clippy -p vinpst-audio --all-targets --features=pipewire-backend -- -D warnings
cargo test -p vinpst-cli --features=pipewire-backend --test audio_devices
cargo clippy -p vinpst-cli --all-targets --features=pipewire-backend -- -D warnings
