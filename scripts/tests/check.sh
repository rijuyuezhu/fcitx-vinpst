#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

scripts/tests/format.sh --check
scripts/tests/scripts-lint.sh
scripts/tests/lint.sh
scripts/tests/test.sh
scripts/tests/addon-install-smoke.sh
scripts/tests/cpp/run-cpp-dbus-smoke.sh
scripts/tests/cpp/run-cpp-dbus-asr-menu-smoke.sh
scripts/tests/toolkit-probe-check.sh

scripts/release/check-arch-install-script.sh
scripts/release/check-arch-pkgbuild.sh
scripts/release/check-release-manifest.sh
scripts/release/check-release-signature.sh
scripts/release/check-arch-release-candidate.sh

scripts/tests/asr/run-command-asr-wav-helper-smoke.sh
scripts/tests/asr/run-legacy-command-asr-wav-bridge-smoke.sh
scripts/tests/asr/run-openai-compatible-asr-fixture-smoke.sh
scripts/tests/asr/run-openai-compatible-text-provider-fixture-smoke.sh
scripts/tests/asr/run-capture-cold-start-smoke.sh

scripts/tests/daemon/run-daemon-default-config-smoke.sh
scripts/tests/daemon/run-daemon-handoff-diagnostics-smoke.sh
scripts/tests/daemon/run-daemon-handoff-smoke.sh
scripts/tests/daemon/run-daemon-removal-handoff-smoke.sh
scripts/tests/daemon/run-package-upgrade-handoff-smoke.sh
scripts/tests/daemon/run-package-remove-handoff-smoke.sh
scripts/tests/daemon/run-direct-activation-upgrade-smoke.sh
scripts/tests/daemon/run-daemon-unavailable-asr-smoke.sh
scripts/tests/daemon/run-remote-text-daemon-lifecycle-smoke.sh

scripts/tests/install/run-user-guide-command-smoke.sh
scripts/tests/install/run-user-ime-activation-owner-smoke.sh
scripts/tests/install/run-user-ime-real-command-asr-wav-smoke.sh
scripts/tests/install/run-user-ime-sherpa-sense-voice-smoke.sh
scripts/tests/install/run-user-ime-sherpa-native-smoke.sh
scripts/tests/install/run-user-ime-sherpa-native-command-smoke.sh
