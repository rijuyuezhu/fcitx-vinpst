#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="${script_dir}"
while [[ ! -f "${repo_root}/Cargo.toml" || ! -d "${repo_root}/scripts" ]]; do
  parent="$(dirname "${repo_root}")"
  if [[ "${parent}" == "${repo_root}" ]]; then
    echo "cannot locate repository root from ${script_dir}" >&2
    exit 1
  fi
  repo_root="${parent}"
done
cd "${repo_root}"
source scripts/tests/dbus-session-common.sh

stage_dir="target/tmp/vinpst-cli-recording-no-activation"
stage_abs="${repo_root}/${stage_dir}"
service_dir="${stage_abs}/services"
bus_config="${stage_abs}/session.conf"
activation_marker="${stage_abs}/activation.marker"
activation_script="${stage_abs}/activation.sh"
cli_bin="${repo_root}/target/debug/vinpst"

cargo build -p vinpst-cli >/dev/null
rm -rf "${stage_dir}"
mkdir -p "${service_dir}"
write_isolated_dbus_session_config "${bus_config}" "${service_dir}"

cat >"${activation_script}" <<ACTIVATION
#!/bin/sh
echo activated >"${activation_marker}"
exec sleep 10
ACTIVATION
chmod +x "${activation_script}"
cat >"${service_dir}/org.fcitx.Vinpst.service" <<SERVICE
[D-BUS Service]
Name=org.fcitx.Vinpst
Exec=${activation_script}
SERVICE
rm -f "${activation_marker}"

# The quoted body is expanded by the child shell, not this script.
# shellcheck disable=SC2016
timeout 10s dbus-run-session --config-file="${bus_config}" -- bash -ceu '
  cli="$1"
  marker="$2"
  stage="$3"

  for subcommand in start status; do
    set +e
    "$cli" recording "$subcommand" >"$stage/$subcommand.stdout" 2>"$stage/$subcommand.stderr"
    code=$?
    set -e
    [[ $code -ne 0 ]] || { echo "recording $subcommand unexpectedly succeeded" >&2; exit 1; }
    grep -F "Daemon is not running." "$stage/$subcommand.stderr" >/dev/null
    [[ ! -e "$marker" ]] || {
      echo "recording $subcommand activated the daemon" >&2
      exit 1
    }
  done

  [[ "$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
    org.freedesktop.DBus NameHasOwner s org.fcitx.Vinpst)" == "b false" ]]
  [[ ! -e "$marker" ]] || { echo "owner probe activated the daemon" >&2; exit 1; }
' bash "${cli_bin}" "${activation_marker}" "${stage_abs}"
