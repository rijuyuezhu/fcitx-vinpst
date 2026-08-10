#!/usr/bin/env bash
set -euo pipefail

if (($# != 2)); then
  echo "usage: run-flatpak-package-smoke-inner.sh MANIFEST WORK_DIR" >&2
  exit 2
fi

manifest="$1"
work_dir="$2"
app_id="org.fcitx.Fcitx5.Addon.Vinpst"
host_id="org.fcitx.Fcitx5"
branch="stable"
platform_id="org.kde.Platform"
platform_branch="6.10"
sdk_id="org.kde.Sdk"
sdk_branch="6.10"
rust_extension_id="org.freedesktop.Sdk.Extension.rust-stable"
llvm_extension_id="org.freedesktop.Sdk.Extension.llvm20"
extension_branch="25.08"
repo="${work_dir}/repo"
build1="${work_dir}/build-1"
state_dir="${work_dir}/state"
bundle="${work_dir}/fcitx-vinpst.flatpak"
home="${work_dir}/home"

for command in flatpak flatpak-builder ostree timeout; do
  command -v "${command}" >/dev/null || {
    echo "missing Flatpak package smoke tool: ${command}" >&2
    exit 1
  }
done
test -f "${manifest}" || {
  echo "missing rendered Flatpak manifest: ${manifest}" >&2
  exit 1
}

rm -rf "${repo}" "${build1}" "${bundle}"
if [[ "${VINPST_FLATPAK_REUSE_HOME:-0}" != "1" ]]; then
  rm -rf "${state_dir}" "${home}"
fi
mkdir -p "${repo}" "${state_dir}" "${home}"
export HOME="${home}"
export XDG_CACHE_HOME="${home}/.cache"
export XDG_CONFIG_HOME="${home}/.config"
export XDG_DATA_HOME="${home}/.local/share"
mkdir -p "${XDG_CACHE_HOME}" "${XDG_CONFIG_HOME}" "${XDG_DATA_HOME}"

flatpak uninstall --user -y "${app_id}//${branch}" >/dev/null 2>&1 || true
flatpak remote-delete --user --force vinpst-local >/dev/null 2>&1 || true

flathub_repo="${work_dir}/flathub.flatpakrepo"
test -s "${flathub_repo}" || {
  echo "missing downloaded Flathub repository descriptor: ${flathub_repo}" >&2
  exit 1
}
if [[ -n "${VINPST_FLATPAK_REMOTE_URL:-}" ]]; then
  sed -i "s|^Url=.*|Url=${VINPST_FLATPAK_REMOTE_URL}|" "${flathub_repo}"
fi
flatpak remote-add --user --if-not-exists --no-enumerate --no-follow_redirect \
  --from flathub "${flathub_repo}"
if [[ -n "${VINPST_FLATPAK_REMOTE_URL:-}" ]]; then
  flatpak remote-modify --user --url="${VINPST_FLATPAK_REMOTE_URL}" \
    --no-follow-redirect flathub
  actual_remote_url="$(flatpak remotes --user --columns=name,url \
    | awk -F '\t' '$1 == "flathub" { print $2 }')"
  [[ "${actual_remote_url}" == "${VINPST_FLATPAK_REMOTE_URL}" ]] || {
    echo "Flatpak remote URL mismatch: expected ${VINPST_FLATPAK_REMOTE_URL}, got ${actual_remote_url}" >&2
    exit 1
  }
fi

retry_command() {
  local attempt
  local max_attempts="${VINPST_FLATPAK_RETRY_ATTEMPTS:-5}"
  for ((attempt = 1; attempt <= max_attempts; attempt++)); do
    if "$@"; then
      return 0
    fi
    if ((attempt == max_attempts)); then
      echo "command failed after ${max_attempts} attempts: $*" >&2
      return 1
    fi
    echo "command failed (attempt ${attempt}/${max_attempts}); retrying: $*" >&2
    sleep "$((attempt * 5))"
  done
}

retry_timed_command() {
  local timeout_seconds="$1"
  shift
  retry_command timeout \
    --foreground \
    --signal=TERM \
    --kill-after=30 \
    "${timeout_seconds}" \
    "$@"
}

dependency_timeout="${VINPST_FLATPAK_DEPENDENCY_TIMEOUT_SECONDS:-900}"
build_timeout="${VINPST_FLATPAK_BUILD_TIMEOUT_SECONDS:-3600}"
transaction_timeout="${VINPST_FLATPAK_TRANSACTION_TIMEOUT_SECONDS:-600}"
for dependency in \
  "${platform_id}//${platform_branch}" \
  "${sdk_id}//${sdk_branch}" \
  "${host_id}//${branch}" \
  "${rust_extension_id}//${extension_branch}" \
  "${llvm_extension_id}//${extension_branch}"; do
  retry_timed_command "${dependency_timeout}" \
    flatpak install --user -y flathub "${dependency}"
done

build_manifest() {
  local build_dir="$1"
  local manifest="$2"
  retry_timed_command "${build_timeout}" flatpak-builder \
    --user \
    --disable-rofiles-fuse \
    --force-clean \
    --state-dir="${state_dir}" \
    --install-deps-from=flathub \
    --repo="${repo}" \
    "${build_dir}" \
    "${manifest}"
  ostree summary --repo="${repo}" --update
}

run_in_host_app() {
  flatpak run --user --command=sh "${host_id}" -lc "$1"
}

verify_revision() {
  local expected="$1"
  local revision
  revision="$(run_in_host_app \
    'cat /app/addons/Vinpst/share/fcitx-vinpst/package-revision')"
  [[ "${revision}" == "${expected}" ]] || {
    echo "Flatpak extension revision mismatch: expected ${expected}, got ${revision}" >&2
    exit 1
  }
}

verify_product() {
  # The quoted script is intentionally expanded by the inner Fcitx Flatpak shell.
  # shellcheck disable=SC2016
  run_in_host_app '
    set -eu
    test -x /app/addons/Vinpst/bin/vinpst
    test -x /app/addons/Vinpst/bin/vinpst-daemon
    test -x /app/addons/Vinpst/bin/vinpst-gui
    test -f /app/addons/Vinpst/lib/fcitx5/fcitx5-vinpst.so
    test -f /app/addons/Vinpst/lib/libsherpa-onnx-c-api.so
    test -f /app/addons/Vinpst/lib/libonnxruntime.so
    test -f /app/addons/Vinpst/share/fcitx5/addon/vinpst.conf
    test -f /app/addons/Vinpst/share/systemd/user/vinpst-daemon.service
    test -f /app/addons/Vinpst/share/dbus-1/services/org.fcitx.Vinpst.service
    test -f /app/addons/Vinpst/share/applications/vinpst-gui.desktop
    test -f /app/addons/Vinpst/share/fcitx-vinpst/vad/silero_vad.onnx
    test -f /app/addons/Vinpst/share/licenses/fcitx-vinpst/LICENSE
    grep -Fq /app/addons/Vinpst/bin/vinpst-daemon \
      /app/addons/Vinpst/share/systemd/user/vinpst-daemon.service
    service_plan="$(/app/addons/Vinpst/bin/vinpst daemon install-service --dry-run --json)"
    printf "%s\n" "${service_plan}" | grep -Fq "\"rewritten_for_flatpak\": true"
    printf "%s\n" "${service_plan}" | grep -Fq \
      "ExecStart=flatpak run --command=/app/addons/Vinpst/bin/vinpst-daemon org.fcitx.Fcitx5"
    /app/addons/Vinpst/bin/vinpst --version
    /app/addons/Vinpst/bin/vinpst-daemon --version
    /app/addons/Vinpst/bin/vinpst-gui --version
    /app/addons/Vinpst/bin/vinpst-gui --check --offline
  '
}

build_manifest "${build1}" "${manifest}"
flatpak remote-add --user --if-not-exists --no-gpg-verify \
  vinpst-local "file://${repo}"
retry_timed_command "${transaction_timeout}" \
  flatpak install --user -y vinpst-local "${app_id}//${branch}"
first_commit="$(flatpak info --user --show-commit "${app_id}//${branch}")"
test -n "${first_commit}"
verify_revision 1
verify_product

ref="$(ostree refs --repo="${repo}" | grep -E "^runtime/${app_id}/[^/]+/${branch}$" | head -n1)"
test -n "${ref}" || {
  echo "Flatpak repository does not contain the extension runtime ref" >&2
  ostree refs --repo="${repo}" >&2
  exit 1
}
architecture="$(cut -d/ -f3 <<<"${ref}")"
flatpak build-bundle --runtime \
  "${repo}" "${bundle}" "${app_id}" "${branch}"
test -s "${bundle}"

revision_marker="${build1}/files/share/fcitx-vinpst/package-revision"
test -f "${revision_marker}" || {
  echo "Flatpak build tree is missing the revision marker: ${revision_marker}" >&2
  exit 1
}
chmod u+w "${revision_marker}"
printf '2\n' >"${revision_marker}"
flatpak build-export --runtime --no-update-summary \
  --subject="Synthetic update fixture" \
  "${repo}" "${build1}" "${branch}"
ostree summary --repo="${repo}" --update
second_repo_commit="$(ostree rev-parse --repo="${repo}" "${ref}")"
if [[ "${second_repo_commit}" == "${first_commit}" ]]; then
  echo "Flatpak update fixture did not create a distinct repository commit" >&2
  exit 1
fi
retry_timed_command "${transaction_timeout}" flatpak update --user -y \
  --commit="${second_repo_commit}" "${app_id}//${branch}"
second_commit="$(flatpak info --user --show-commit "${app_id}//${branch}")"
test -n "${second_commit}"
if [[ "${first_commit}" == "${second_commit}" ]]; then
  echo "Flatpak update did not change the installed OSTree commit" >&2
  exit 1
fi
if [[ "${second_commit}" != "${second_repo_commit}" ]]; then
  echo "Flatpak update installed ${second_commit}, expected ${second_repo_commit}" >&2
  exit 1
fi
verify_revision 2
verify_product

retry_timed_command "${transaction_timeout}" \
  flatpak uninstall --user -y "${app_id}//${branch}"
if flatpak info --user "${app_id}//${branch}" >/dev/null 2>&1; then
  echo "Flatpak extension remained installed after uninstall" >&2
  exit 1
fi
retry_timed_command "${transaction_timeout}" \
  flatpak install --user -y --bundle "${bundle}"
verify_revision 1
verify_product
retry_timed_command "${transaction_timeout}" \
  flatpak uninstall --user -y "${app_id}//${branch}"

cat >"${work_dir}/summary.json" <<EOF
{
  "app_id": "${app_id}",
  "architecture": "${architecture}",
  "branch": "${branch}",
  "bundle": "${bundle}",
  "bundle_commit": "${first_commit}",
  "first_commit": "${first_commit}",
  "second_commit": "${second_commit}"
}
EOF

printf 'Flatpak package build and transaction smoke passed: %s\n' "${bundle}"
