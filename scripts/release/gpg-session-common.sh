#!/usr/bin/env bash

# Prepare the short runtime socket directory for one ephemeral GnuPG home.
# This avoids Unix-domain socket path limits when release sources are nested
# under long CI checkout and package-build paths.
gpg_session_prepare() {
  local home="${1:-}"
  if [[ -z "${home}" || ! -d "${home}" ]]; then
    echo "GnuPG home must exist before socket preparation: ${home@Q}" >&2
    return 1
  fi
  if [[ -n "${XDG_RUNTIME_DIR:-}" ]]; then
    gpgconf --homedir "${home}" --create-socketdir
  fi
}

# Stop every GnuPG component bound to an ephemeral home before deleting it.
gpg_session_stop() {
  local home="${1:-}"
  if [[ -z "${home}" || ! -d "${home}" ]]; then
    return 0
  fi
  if ! command -v gpgconf >/dev/null 2>&1; then
    return 0
  fi
  gpgconf --homedir "${home}" --kill all >/dev/null 2>&1 || true
}
