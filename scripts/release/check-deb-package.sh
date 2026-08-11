#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="${script_dir}"
while [[ ! -f "${repo_root}/Cargo.toml" || ! -d "${repo_root}/packaging" ]]; do
  parent="$(dirname "${repo_root}")"
  if [[ "${parent}" == "${repo_root}" ]]; then
    echo "cannot locate repository root from ${script_dir}" >&2
    exit 1
  fi
  repo_root="${parent}"
done
cd "${repo_root}"

check_root="${repo_root}/target/tmp/deb-package-check"
rm -rf "${check_root}"
mkdir -p "${check_root}"

scripts/release/render-deb-control.py \
  --version 0.1.0 \
  --release 1 \
  --architecture amd64 \
  --output "${check_root}/control"

grep -qx 'Package: fcitx-vinpst' "${check_root}/control"
grep -qx 'Version: 0.1.0-1' "${check_root}/control"
grep -qx 'Architecture: amd64' "${check_root}/control"
grep -q '^Depends: .*fcitx5' "${check_root}/control"
grep -q '^Depends: .*libglib2.0-bin' "${check_root}/control"
grep -q '^Depends: .*procps' "${check_root}/control"
grep -q '^Depends: .*systemd' "${check_root}/control"
grep -q '^Depends: .*util-linux-extra' "${check_root}/control"
! grep -qE '^(Provides|Conflicts|Replaces):' "${check_root}/control"
if grep -Eq '@[A-Z0-9_]+@' "${check_root}/control"; then
  echo "Debian control still contains placeholders" >&2
  exit 1
fi

for script in postinst prerm postrm; do
  bash -n "packaging/debian/${script}"
  test -x "packaging/debian/${script}"
done

grep -q 'package-upgrade-handoff' packaging/debian/postinst
grep -q 'package-remove-handoff' packaging/debian/prerm
grep -q 'intentionally preserved' packaging/debian/postrm
grep -q 'License: GPL-3+' packaging/debian/copyright
test -s LICENSE
! grep -qE 'rustup|static\.rust-lang\.org|sh\.rustup\.rs' packaging/debian/Dockerfile
grep -q 'rust_sysroot="$(rustc --print sysroot)"' scripts/release/run-deb-package-smoke.sh
grep -q -- '--volume "${rust_sysroot}:${rust_sysroot}:ro"' scripts/release/run-deb-package-smoke.sh
grep -q -- '--env "VINPST_RUST_SYSROOT=${rust_sysroot}"' scripts/release/run-deb-package-smoke.sh
grep -q -- '--env "PATH=${rust_sysroot}/bin:' scripts/release/run-deb-package-smoke.sh
grep -q 'rustc --version' scripts/release/run-deb-package-smoke.sh
grep -q 'cargo --version' scripts/release/run-deb-package-smoke.sh

after_failure() {
  local name="$1"
  shift
  if scripts/release/render-deb-control.py "$@" \
    --output "${check_root}/${name}" >"${check_root}/${name}.out" 2>&1; then
    echo "expected Debian renderer failure: ${name}" >&2
    exit 1
  fi
}

after_failure bad-version --version '1;touch-injected' --release 1 --architecture amd64
after_failure bad-release --version 1.0.0 --release '../2' --architecture amd64
after_failure bad-arch --version 1.0.0 --release 1 --architecture 'amd64;id'

cp packaging/debian/control.in "${check_root}/unresolved.in"
printf '%s\n' '@VINPST_UNKNOWN@' >>"${check_root}/unresolved.in"
if scripts/release/render-deb-control.py \
  --template "${check_root}/unresolved.in" \
  --version 1.0.0 \
  --release 1 \
  --architecture amd64 \
  --output "${check_root}/unresolved" >"${check_root}/unresolved.out" 2>&1; then
  echo "expected unresolved Debian placeholder failure" >&2
  exit 1
fi
grep -q 'unresolved Debian control placeholders' "${check_root}/unresolved.out"

PYTHONPYCACHEPREFIX="${check_root}/pycache" \
  python3 -m py_compile scripts/release/render-deb-control.py

echo "Debian package metadata check passed"
