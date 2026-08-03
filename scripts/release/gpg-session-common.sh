#!/usr/bin/env bash

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
