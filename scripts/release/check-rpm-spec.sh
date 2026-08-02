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

version="$(
  cargo metadata --no-deps --format-version 1 |
    jq -r '.packages[] | select(.name == "vinput-cli") | .version'
)"
test -n "${version}"

check_root="${repo_root}/target/tmp/rpm-spec-check"
rm -rf "${check_root}"
mkdir -p "${check_root}"
spec="${check_root}/fcitx-vinput-rs.spec"

scripts/release/render-rpm-spec.py \
  --version "${version}" \
  --source-name "fcitx-vinput-rs-${version}.tar.gz" \
  --source-sha256 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef \
  --source-dir "fcitx-vinput-rs-${version}" \
  --output "${spec}"

rpmspec -P "${spec}" >"${check_root}/expanded.spec"
query="$(rpmspec -q --qf '%{NAME}\n%{VERSION}\n%{RELEASE}\n%{ARCH}\n' "${spec}")"
test "${query}" = $'fcitx-vinput-rs\n'"${version}"$'\n1\nx86_64'
rpmspec -q --provides "${spec}" | grep -Eq '^fcitx5-vinput = '
rpmspec -q --requires "${spec}" | grep -qx 'fcitx5'
rpmspec -q --requires "${spec}" | grep -qx 'pipewire-libs'
rpmspec -q --requires "${spec}" | grep -qx 'systemd'

grep -Fq 'cargo build --frozen --release' "${check_root}/expanded.spec"
grep -Fq -- '-p vinput-gui' "${check_root}/expanded.spec"
grep -Fq -- '-DCMAKE_INSTALL_LIBDIR=lib64' "${check_root}/expanded.spec"
grep -Fq -- '-DVINPUT_SYSTEMD_USER_UNIT_DIR=lib/systemd/user' \
  "${check_root}/expanded.spec"
grep -Fq -- '--target fcitx5_vinput_addon' "${check_root}/expanded.spec"
grep -Fq "/usr/lib/fcitx-vinput/package-upgrade-handoff" \
  "${check_root}/expanded.spec"
grep -Fq "/usr/lib/fcitx-vinput/package-remove-handoff" \
  "${check_root}/expanded.spec"
grep -Fq 'install -Dm644 LICENSE' "${check_root}/expanded.spec"
grep -Fq '/usr/share/licenses/fcitx-vinput-rs/LICENSE' \
  "${check_root}/expanded.spec"
grep -Fq '%post -p /bin/bash' "${spec}"
grep -Fq '%preun -p /bin/bash' "${spec}"
grep -Fq '%postun -p /bin/bash' "${spec}"
if grep -Fq '@VINPUT_' "${spec}"; then
  echo "RPM spec still contains unresolved placeholders" >&2
  exit 1
fi

expect_render_failure() {
  local expected="$1"
  shift
  local stderr_path="${check_root}/render-failure.stderr"
  rm -f "${check_root}/rejected.spec" "${stderr_path}"
  set +e
  scripts/release/render-rpm-spec.py \
    --version "${version}" \
    --source-name "fcitx-vinput-rs-${version}.tar.gz" \
    --source-sha256 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef \
    --source-dir "fcitx-vinput-rs-${version}" \
    --output "${check_root}/rejected.spec" \
    "$@" 2>"${stderr_path}"
  status=$?
  set -e
  test "${status}" -ne 0
  grep -Fq "${expected}" "${stderr_path}"
  test ! -e "${check_root}/rejected.spec"
}

expect_render_failure 'invalid RPM renderer release' --release '1;touch-injected'
expect_render_failure 'invalid RPM renderer source name' --source-name '../source.tar.gz'
expect_render_failure 'invalid RPM renderer source SHA-256' --source-sha256 invalid
expect_render_failure 'unknown runtime bundle: missing' --runtime-bundle missing

test ! -e "${check_root}/touch-injected"
echo "RPM spec metadata check passed"
