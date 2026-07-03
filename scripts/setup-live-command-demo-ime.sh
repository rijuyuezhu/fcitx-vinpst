#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

stop_stale_owner="${VINPUT_LIVE_STOP_STALE_OWNER:-}"

cat <<'EOF'
This command mutates the current user profile for live desktop testing.
It installs the deterministic command-demo IME profile, writes the user D-Bus
activation service, writes the Fcitx env wrapper/autostart override, and then
runs the live probe.
EOF

VINPUT_LIVE_INSTALL_COMMAND_DEMO=1 \
VINPUT_LIVE_STOP_STALE_OWNER="${stop_stale_owner}" \
  scripts/run-ime-fcitx-live-probe.sh || {
    status=$?
    cat >&2 <<'EOF'

Live setup did not fully pass yet. Review the classified probe output above.
If it reports stale-bus-owner and the displayed process is safe to stop, rerun:
  VINPUT_LIVE_STOP_STALE_OWNER=1 just ime-fcitx-live-command-demo-setup
EOF
    exit "${status}"
  }

wrapper="${XDG_DATA_HOME:-${HOME}/.local/share}/fcitx-vinput/fcitx5-with-vinput-env.sh"
cat <<EOF

Live command-demo IME files are installed and the probe passed.
Restart Fcitx5 for the current session with:
  ${wrapper} -r

Then open a text field and test:
  Right Ctrl press/release: normal command-demo commit
  F10 press/release with selected text: command replacement
EOF
