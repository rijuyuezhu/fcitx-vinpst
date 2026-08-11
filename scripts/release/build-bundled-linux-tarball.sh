#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 --package DEB --version VERSION --output DIR" >&2
  exit 2
}

package=""
version=""
output_dir=""
while (($#)); do
  case "$1" in
    --package)
      (($# >= 2)) || usage
      package="$2"
      shift 2
      ;;
    --version)
      (($# >= 2)) || usage
      version="$2"
      shift 2
      ;;
    --output)
      (($# >= 2)) || usage
      output_dir="$2"
      shift 2
      ;;
    *)
      usage
      ;;
  esac
done

[[ -n "${package}" && -n "${version}" && -n "${output_dir}" ]] || usage
[[ "${version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]] || {
  echo "invalid bundled tarball version: ${version@Q}" >&2
  exit 2
}
[[ -f "${package}" && ! -L "${package}" ]] || {
  echo "bundled tarball input must be a regular Debian package" >&2
  exit 1
}

for command in dpkg-deb gzip jq patchelf tar; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "missing bundled tarball tool: ${command}" >&2
    exit 1
  }
done

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
package="$(cd "$(dirname "${package}")" && pwd)/$(basename "${package}")"
output_dir="$(mkdir -p "${output_dir}" && cd "${output_dir}" && pwd)"

[[ "$(dpkg-deb -f "${package}" Package)" == fcitx-vinpst ]]
[[ "$(dpkg-deb -f "${package}" Version)" == "${version}-1" ]]
[[ "$(dpkg-deb -f "${package}" Architecture)" == amd64 ]]

stage_root="${repo_root}/target/tmp/bundled-linux-tarball"
payload_root="${stage_root}/payload"
archive_root="fcitx-vinpst_${version}-1_linux_x86_64_bundled"
archive_stage="${stage_root}/archive/${archive_root}"
output="${output_dir}/${archive_root}.tar.gz"
rm -rf "${stage_root}"
mkdir -p "${payload_root}" "${archive_stage}"
dpkg-deb -x "${package}" "${payload_root}"
[[ -d "${payload_root}/usr" && ! -L "${payload_root}/usr" ]]
mv "${payload_root}/usr" "${archive_stage}/usr"

required_files=(
  usr/bin/vinpst
  usr/bin/vinpst-daemon
  usr/bin/vinpst-gui
  usr/lib/fcitx-vinpst/libsherpa-onnx-c-api.so
  usr/lib/fcitx-vinpst/libonnxruntime.so
  usr/share/applications/vinpst-gui.desktop
  usr/share/dbus-1/services/org.fcitx.Vinpst.service
  usr/share/fcitx5/addon/vinpst.conf
  usr/share/fcitx-vinpst/default-config.json
)
for relative in "${required_files[@]}"; do
  [[ -f "${archive_stage}/${relative}" ]]
done
mapfile -t addon_files < <(
  find "${archive_stage}/usr/lib" -type f -path '*/fcitx5/fcitx5-vinpst.so' -print
)
((${#addon_files[@]} == 1)) || {
  echo "bundled tarball must contain exactly one Fcitx addon" >&2
  exit 1
}

[[ "$(patchelf --print-rpath "${archive_stage}/usr/bin/vinpst")" == '$ORIGIN/../lib/fcitx-vinpst' ]]
[[ "$(patchelf --print-rpath "${archive_stage}/usr/bin/vinpst-daemon")" == '$ORIGIN/../lib/fcitx-vinpst' ]]
[[ "$(patchelf --print-rpath "${archive_stage}/usr/lib/fcitx-vinpst/libsherpa-onnx-c-api.so")" == '$ORIGIN' ]]

"${archive_stage}/usr/bin/vinpst" --version | grep -Fq "${version}"
"${archive_stage}/usr/bin/vinpst-daemon" --help >/dev/null
"${archive_stage}/usr/bin/vinpst-gui" --version | grep -Fq "${version}"
isolated_config="${stage_root}/empty-config"
mkdir -p "${isolated_config}"
XDG_CONFIG_HOME="${isolated_config}" \
  "${archive_stage}/usr/bin/vinpst-gui" --check --offline |
  jq -e '.ok and .application == "vinpst-gui" and .daemon.skipped' >/dev/null

rm -f "${output}"
tar \
  --sort=name \
  --mtime='@0' \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  -C "${stage_root}/archive" \
  -cf - "${archive_root}" |
  gzip -n >"${output}"
[[ -s "${output}" ]]

mapfile -t roots < <(tar -tzf "${output}" | sed 's#/.*##' | LC_ALL=C sort -u)
((${#roots[@]} == 1))
[[ "${roots[0]}" == "${archive_root}" ]]
if tar -tvzf "${output}" | awk '$1 ~ /^l/ { found=1 } END { exit !found }'; then
  echo "bundled tarball unexpectedly contains a symbolic link" >&2
  exit 1
fi

printf '%s\n' "${output}"
