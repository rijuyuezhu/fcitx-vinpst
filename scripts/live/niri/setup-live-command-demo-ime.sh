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

stop_stale_owner="${VINPST_LIVE_STOP_STALE_OWNER:-}"

cat <<'EOF'
This command mutates the current user profile for live desktop testing.
It installs the deterministic command-demo IME profile, writes the user D-Bus
activation service, writes the Fcitx env wrapper/autostart override, and then
runs the live probe.
EOF

VINPST_LIVE_INSTALL_COMMAND_DEMO=1 \
VINPST_LIVE_STOP_STALE_OWNER="${stop_stale_owner}" \
  scripts/live/niri/run-ime-fcitx-live-probe.sh || {
    status=$?
    cat >&2 <<'EOF'

Live setup did not fully pass yet. Review the classified probe output above.
If it reports stale-bus-owner and the displayed process is safe to stop, rerun:
  VINPST_LIVE_STOP_STALE_OWNER=1 just ime-fcitx-live-command-demo-setup
EOF
    exit "${status}"
  }

wrapper="${XDG_DATA_HOME:-${HOME}/.local/share}/fcitx-vinpst/fcitx5-with-vinpst-env.sh"
cat <<EOF

Live command-demo IME files are installed and the probe passed.
Restart Fcitx5 for the current session with:
  ${wrapper} -dr

Then open a text field and test:
  Right Ctrl press/release: normal command-demo commit
  F10 press/release with selected text: command replacement
EOF
