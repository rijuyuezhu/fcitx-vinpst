#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

cargo test -p vinput-daemon --features=pipewire-backend
cargo clippy -p vinput-daemon --all-targets --features=pipewire-backend -- -D warnings
cargo test -p vinput-audio --features=pipewire-backend
cargo clippy -p vinput-audio --all-targets --features=pipewire-backend -- -D warnings
cargo test -p vinput-cli --features=pipewire-backend --test audio_devices
cargo clippy -p vinput-cli --all-targets --features=pipewire-backend -- -D warnings
