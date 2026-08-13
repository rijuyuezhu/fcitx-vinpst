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
    jq -r '.packages[] | select(.name == "vinpst-cli") | .version'
)"
test -n "${version}"

check_root="${repo_root}/target/tmp/rpm-spec-check"
rm -rf "${check_root}"
mkdir -p "${check_root}"
spec="${check_root}/fcitx-vinpst.spec"
opensuse_spec="${check_root}/fcitx-vinpst-opensuse.spec"

scripts/release/render-rpm-spec.py \
  --version "${version}" \
  --source-name "fcitx-vinpst-${version}.tar.gz" \
  --source-sha256 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef \
  --source-dir "fcitx-vinpst-${version}" \
  --output "${spec}"
scripts/release/render-rpm-spec.py \
  --distribution opensuse16.0 \
  --version "${version}" \
  --source-name "fcitx-vinpst-${version}.tar.gz" \
  --source-sha256 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef \
  --source-dir "fcitx-vinpst-${version}" \
  --output "${opensuse_spec}"

query="$(rpmspec -q --qf '%{NAME}\n%{VERSION}\n%{RELEASE}\n%{ARCH}\n' "${spec}")"
test "${query}" = $'fcitx-vinpst\n'"${version}"$'\n1\nx86_64'
opensuse_query="$(rpmspec -q --qf '%{NAME}\n%{VERSION}\n%{RELEASE}\n%{ARCH}\n' "${opensuse_spec}")"
test "${opensuse_query}" = $'fcitx-vinpst\n'"${version}"$'\n1\nx86_64'
! rpmspec -q --provides "${spec}" | grep -Eq '^fcitx5-vinpst = '
rpmspec -q --requires "${spec}" | grep -qx 'fcitx5'
rpmspec -q --requires "${spec}" | grep -qx '/usr/bin/systemctl'
! rpmspec -q --requires "${spec}" | grep -Eq '^lib(onnxruntime|sherpa-onnx-c-api)\.so'
rpmspec -q --requires "${opensuse_spec}" | grep -qx '/usr/bin/systemctl'
! rpmspec -q --requires "${opensuse_spec}" | grep -Eq '^lib(onnxruntime|sherpa-onnx-c-api)\.so'
fedora_buildrequires="$(rpmspec -q --buildrequires "${spec}")"
opensuse_buildrequires="$(rpmspec -q --buildrequires "${opensuse_spec}")"
grep -qx 'ninja-build' <<<"${fedora_buildrequires}"
grep -qx 'clang' <<<"${opensuse_buildrequires}"
grep -qx 'ninja' <<<"${opensuse_buildrequires}"

for rpm_spec in "${spec}" "${opensuse_spec}"; do
  test "$(rpmspec -q --qf '%{LICENSE}' "${rpm_spec}")" = \
    'GPL-3.0-or-later AND Apache-2.0 AND MIT'
  test "$(rpmspec -q --qf '%{POSTINPROG}' "${rpm_spec}")" = /bin/bash
  test "$(rpmspec -q --qf '%{PREUNPROG}' "${rpm_spec}")" = '(none)'
  test "$(rpmspec -q --qf '%{POSTUNPROG}' "${rpm_spec}")" = /bin/bash
  test "$(rpmspec -q --qf '%{PREUN}' "${rpm_spec}")" = '(none)'
done
if grep -Fq '@VINPST_' "${spec}"; then
  echo "RPM spec still contains unresolved placeholders" >&2
  exit 1
fi
if grep -Fq '@VINPST_' "${opensuse_spec}"; then
  echo "openSUSE RPM spec still contains unresolved placeholders" >&2
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
    --source-name "fcitx-vinpst-${version}.tar.gz" \
    --source-sha256 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef \
    --source-dir "fcitx-vinpst-${version}" \
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
