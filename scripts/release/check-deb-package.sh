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
tool_injection_root="${check_root}/tool-injection"
fake_bin="${tool_injection_root}/bin"
fake_image_bin="${tool_injection_root}/image-bin"
fake_sysroot="${tool_injection_root}/rust-sysroot"
docker_run_args="${tool_injection_root}/docker-run.args"
mkdir -p "${fake_bin}" "${fake_image_bin}" "${fake_sysroot}/bin"
cat >"${fake_bin}/rustc" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$*" == '--print sysroot' ]]; then
  printf '%s\n' "${VINPST_TEST_RUST_SYSROOT:?}"
  exit 0
fi
echo "unexpected fake rustc invocation: $*" >&2
exit 1
EOF
cat >"${fake_sysroot}/bin/rustc" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
cat >"${fake_sysroot}/bin/cargo" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
cat >"${fake_bin}/docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
build)
  exit 0
  ;;
run)
  shift
  has_mount=false
  for arg in "$@"; do
    case "${arg}" in
    --volume | -v | --mount | --volume=* | -v=* | --mount=*)
      has_mount=true
      ;;
    esac
  done
  if [[ "${has_mount}" == false ]]; then
    args=("$@")
    shell_index=-1
    for index in "${!args[@]}"; do
      if [[ "${args[index]}" == bash ]]; then
        shell_index="${index}"
        break
      fi
    done
    ((shell_index >= 0 && shell_index + 2 < ${#args[@]})) || {
      echo "builder preflight did not invoke a checked shell" >&2
      exit 1
    }
    mode="${args[shell_index + 1]}"
    script="${args[shell_index + 2]}"
    image_bin="${VINPST_TEST_IMAGE_BIN:?}"
    host_bash="${VINPST_TEST_HOST_BASH:?}"
    PATH="${image_bin}" "${host_bash}" "${mode}" "${script}"
    printf '#!/bin/sh\nexit 0\n' >"${image_bin}/rustc"
    printf '#!/bin/sh\nexit 0\n' >"${image_bin}/cargo"
    chmod +x "${image_bin}/rustc" "${image_bin}/cargo"
    if PATH="${image_bin}" "${host_bash}" "${mode}" "${script}" \
      >/dev/null 2>&1; then
      echo "builder preflight accepted an image-provided Rust toolchain" >&2
      exit 1
    fi
    rm -f "${image_bin}/rustc" "${image_bin}/cargo"
    exit 0
  fi
  printf '%s\0' "$@" >"${VINPST_TEST_DOCKER_RUN_ARGS:?}"
  ;;
*)
  echo "unexpected fake docker invocation: $*" >&2
  exit 1
  ;;
esac
EOF
chmod +x \
  "${fake_bin}/rustc" \
  "${fake_bin}/docker" \
  "${fake_sysroot}/bin/rustc" \
  "${fake_sysroot}/bin/cargo"
PATH="${fake_bin}:${PATH}" \
  VINPST_TEST_HOST_BASH="$(command -v bash)" \
  VINPST_TEST_IMAGE_BIN="${fake_image_bin}" \
  VINPST_TEST_RUST_SYSROOT="${fake_sysroot}" \
  VINPST_TEST_DOCKER_RUN_ARGS="${docker_run_args}" \
  VINPST_PACKAGE_SOURCE_CACHE="${tool_injection_root}/package-cache" \
  scripts/release/run-deb-package-smoke.sh \
    --image example.invalid/debian:test \
    --distribution debian-test
python3 - "${docker_run_args}" "${fake_sysroot}" <<'PY'
import pathlib
import sys

args_path = pathlib.Path(sys.argv[1])
expected_sysroot = sys.argv[2]
args = [part.decode() for part in args_path.read_bytes().split(b"\0") if part]


def option_values(name: str, short: str | None = None) -> list[str]:
    values: list[str] = []
    for index, arg in enumerate(args):
        if arg == name or (short is not None and arg == short):
            if index + 1 >= len(args):
                raise SystemExit(f"missing value after {arg}")
            values.append(args[index + 1])
        elif arg.startswith(name + "="):
            values.append(arg.split("=", 1)[1])
    return values


readonly_sysroot = False
for value in option_values("--volume", "-v"):
    fields = value.split(":")
    if len(fields) >= 3 and fields[0] == expected_sysroot and fields[1] == expected_sysroot:
        readonly_sysroot = "ro" in fields[2].split(",")
for value in option_values("--mount"):
    fields: dict[str, str] = {}
    flags: set[str] = set()
    for field in value.split(","):
        if "=" in field:
            key, item = field.split("=", 1)
            fields[key] = item
        else:
            flags.add(field)
    source = fields.get("src", fields.get("source"))
    target = fields.get("dst", fields.get("target", fields.get("destination")))
    if source == expected_sysroot and target == expected_sysroot:
        readonly_sysroot = "readonly" in flags or fields.get("readonly") == "true"
if not readonly_sysroot:
    raise SystemExit("Debian smoke did not bind the host Rust sysroot read-only")

environment: dict[str, str] = {}
for value in option_values("--env", "-e"):
    key, separator, item = value.partition("=")
    if separator:
        environment[key] = item
if environment.get("VINPST_RUST_SYSROOT") != expected_sysroot:
    raise SystemExit("Debian smoke did not pass the host Rust sysroot into the container")

try:
    bash_index = args.index("bash")
except ValueError as error:
    raise SystemExit("Debian smoke did not launch its checked container shell") from error
if args[bash_index + 1 : bash_index + 2] != ["-lc"] or bash_index + 2 >= len(args):
    raise SystemExit("Debian smoke container shell invocation is incomplete")
container_script = args[bash_index + 2]
if "VINPST_RUST_SYSROOT" not in container_script or "rustc --version" not in container_script:
    raise SystemExit("Debian smoke container does not activate and probe the injected Rust sysroot")
PY

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
