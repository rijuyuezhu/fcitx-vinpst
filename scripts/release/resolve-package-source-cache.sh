#!/usr/bin/env bash
set -euo pipefail

cache_dir="${1:?usage: resolve-package-source-cache.sh <cache-dir>}"
mkdir -p -- "${cache_dir}"
realpath -- "${cache_dir}"
