#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
check_root="${repo_root}/target/tmp/github-release-publish-check"
rm -rf "${check_root}"
mkdir -p "${check_root}/bin" "${check_root}/inputs"

cat >"${check_root}/bin/gh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

state_dir="${FAKE_GH_STATE_DIR:?}"
command_name="${1:-}"
shift || true

write_release_json() {
  local state
  state="$(cat "${state_dir}/release-state")"
  python3 - "${state}" "${state_dir}/assets.tsv" "${FAKE_GH_ASSET_MUTATION:-none}" <<'PY'
import json
import pathlib
import sys

state, assets_path, mutation = sys.argv[1:]
assets = []
path = pathlib.Path(assets_path)
if path.exists():
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line:
            continue
        name, size, digest = line.split("\t", 2)
        assets.append({"name": name, "size": int(size), "digest": digest})
if mutation == "extra":
    assets.append({"name": "unexpected.bin", "size": 1, "digest": "sha256:" + "0" * 64})
elif mutation == "missing" and assets:
    assets.pop()
elif mutation == "size" and assets:
    assets[0]["size"] += 1
elif mutation == "digest" and assets:
    assets[0]["digest"] = "sha256:" + "f" * 64
force_public = __import__("os").environ.get("FAKE_GH_FORCE_PUBLIC") == "1" and bool(assets)
print(json.dumps({"tag_name": "v0.1.0", "draft": state == "draft" and not force_public, "assets": assets}))
PY
}

case "${command_name}" in
  api)
    endpoint="${1:-}"
    if [[ "${endpoint}" == "repos/example/fcitx-vinpst" ]]; then
      if [[ "${2:-}" == --jq ]]; then
        [[ "${3:-}" == .full_name ]]
        printf 'example/fcitx-vinpst\n'
      else
        printf '{"full_name":"example/fcitx-vinpst"}\n'
      fi
      exit 0
    fi
    [[ "${endpoint}" == "repos/example/fcitx-vinpst/releases?per_page=100" ]]
    case "${FAKE_GH_API_ERROR:-none}" in
      none)
        ;;
      server)
        echo "gh: Server Error (HTTP 500)" >&2
        exit 1
        ;;
      auth)
        echo "gh: Resource not accessible (HTTP 403)" >&2
        exit 1
        ;;
      *)
        echo "unsupported fake API error" >&2
        exit 2
        ;;
    esac
    if [[ ! -f "${state_dir}/release-state" ]]; then
      printf '[]\n'
      exit 0
    fi
    printf '['
    write_release_json
    printf ']\n'
    ;;
  release)
    subcommand="${1:-}"
    shift || true
    case "${subcommand}" in
      create)
        tag="${1:-}"
        [[ "${tag}" == v0.1.0 ]]
        [[ ! -f "${state_dir}/release-state" ]]
        printf 'draft\n' >"${state_dir}/release-state"
        : >"${state_dir}/assets.tsv"
        printf 'create\n' >>"${state_dir}/events"
        ;;
      edit)
        tag="${1:-}"
        [[ "${tag}" == v0.1.0 ]]
        shift || true
        [[ -f "${state_dir}/release-state" ]]
        publish=false
        while (($#)); do
          case "$1" in
            --draft=false)
              publish=true
              shift
              ;;
            --repo|--title|--notes-file)
              shift 2
              ;;
            --draft|--latest)
              shift
              ;;
            *)
              echo "unsupported fake edit argument: $1" >&2
              exit 2
              ;;
          esac
        done
        if [[ "${publish}" == true ]]; then
          printf 'public\n' >"${state_dir}/release-state"
          printf 'publish\n' >>"${state_dir}/events"
        else
          printf 'edit-draft\n' >>"${state_dir}/events"
        fi
        ;;
      upload)
        tag="${1:-}"
        [[ "${tag}" == v0.1.0 ]]
        shift || true
        [[ "$(cat "${state_dir}/release-state")" == draft ]]
        : >"${state_dir}/assets.tsv"
        while (($#)); do
          case "$1" in
            --repo)
              shift 2
              ;;
            --clobber)
              shift
              ;;
            *)
              [[ -f "$1" && ! -L "$1" ]]
              printf '%s\t%s\tsha256:%s\n' \
                "${1##*/}" \
                "$(stat -c '%s' "$1")" \
                "$(sha256sum "$1" | cut -d ' ' -f1)" >>"${state_dir}/assets.tsv"
              shift
              ;;
          esac
        done
        LC_ALL=C sort -o "${state_dir}/assets.tsv" "${state_dir}/assets.tsv"
        printf 'upload\n' >>"${state_dir}/events"
        ;;
      *)
        echo "unsupported fake release command: ${subcommand}" >&2
        exit 2
        ;;
    esac
    ;;
  *)
    echo "unsupported fake gh command: ${command_name}" >&2
    exit 2
    ;;
esac
SH
chmod 0755 "${check_root}/bin/gh"

printf 'source archive\n' >"${check_root}/inputs/fcitx-vinpst-0.1.0.tar.gz"
printf 'native package\n' >"${check_root}/inputs/fcitx-vinpst_0.1.0-1_ubuntu24.04_amd64.deb"
python3 "${repo_root}/scripts/release/release_manifest.py" assemble \
  --package-name fcitx-vinpst \
  --version 0.1.0 \
  --architecture x86_64 \
  --output-dir "${check_root}/bundle" \
  --artifact "source-archive=${check_root}/inputs/fcitx-vinpst-0.1.0.tar.gz" \
  --artifact "deb-ubuntu24.04=${check_root}/inputs/fcitx-vinpst_0.1.0-1_ubuntu24.04_amd64.deb"
cat >"${check_root}/notes.md" <<'EOF'
# Vinpst 0.1.0

Fixture release notes.
EOF

publisher="${repo_root}/scripts/release/publish-github-release.sh"
run_publisher() {
  local state_dir="$1"
  shift
  env \
    PATH="${check_root}/bin:${PATH}" \
    FAKE_GH_STATE_DIR="${state_dir}" \
    "$@" \
    "${publisher}" \
      --tag v0.1.0 \
      --version 0.1.0 \
      --bundle-dir "${check_root}/bundle" \
      --notes-file "${check_root}/notes.md" \
      --repo example/fcitx-vinpst
}

new_state="${check_root}/new-state"
mkdir -p "${new_state}"
run_publisher "${new_state}"
[[ "$(cat "${new_state}/release-state")" == public ]]
[[ "$(tr '\n' ' ' <"${new_state}/events")" == "create upload publish " ]]

existing_state="${check_root}/existing-state"
mkdir -p "${existing_state}"
printf 'draft\n' >"${existing_state}/release-state"
: >"${existing_state}/assets.tsv"
run_publisher "${existing_state}"
[[ "$(cat "${existing_state}/release-state")" == public ]]
[[ "$(tr '\n' ' ' <"${existing_state}/events")" == "edit-draft upload publish " ]]

public_state="${check_root}/public-state"
mkdir -p "${public_state}"
printf 'public\n' >"${public_state}/release-state"
: >"${public_state}/assets.tsv"
if run_publisher "${public_state}" >"${check_root}/public.out" 2>"${check_root}/public.err"; then
  echo "publisher accepted an already-public release" >&2
  exit 1
fi
grep -Fq 'already public' "${check_root}/public.err"
[[ ! -e "${public_state}/events" ]]

for api_error in server auth; do
  error_state="${check_root}/${api_error}-state"
  mkdir -p "${error_state}"
  if run_publisher "${error_state}" "FAKE_GH_API_ERROR=${api_error}" \
    >"${check_root}/${api_error}.out" 2>"${check_root}/${api_error}.err"; then
    echo "publisher treated a GitHub ${api_error} failure as a missing release" >&2
    exit 1
  fi
  grep -Fq 'failed to query GitHub Releases' "${check_root}/${api_error}.err"
  [[ ! -e "${error_state}/release-state" && ! -e "${error_state}/events" ]]
done

for mutation in extra missing size digest; do
  mismatch_state="${check_root}/mismatch-${mutation}-state"
  mkdir -p "${mismatch_state}"
  if run_publisher "${mismatch_state}" "FAKE_GH_ASSET_MUTATION=${mutation}" \
    >"${check_root}/mismatch-${mutation}.out" \
    2>"${check_root}/mismatch-${mutation}.err"; then
    echo "publisher accepted remote asset ${mutation} mismatch" >&2
    exit 1
  fi
  [[ "$(cat "${mismatch_state}/release-state")" == draft ]]
  grep -Fq 'asset names, sizes, or SHA-256 digests do not match' "${check_root}/mismatch-${mutation}.err"
done

race_state="${check_root}/publish-race-state"
mkdir -p "${race_state}"
if run_publisher "${race_state}" FAKE_GH_FORCE_PUBLIC=1 \
  >"${check_root}/publish-race.out" 2>"${check_root}/publish-race.err"; then
  echo "publisher accepted a release that became public during upload" >&2
  exit 1
fi
[[ "$(cat "${race_state}/release-state")" == draft ]]
grep -Fq 'is no longer a draft after upload' "${check_root}/publish-race.err"

corrupt_root="${check_root}/corrupt"
cp -a "${check_root}/bundle" "${corrupt_root}"
printf 'tampered\n' >>"${corrupt_root}/fcitx-vinpst-0.1.0.tar.gz"
corrupt_state="${check_root}/corrupt-state"
mkdir -p "${corrupt_state}"
if env \
  PATH="${check_root}/bin:${PATH}" \
  FAKE_GH_STATE_DIR="${corrupt_state}" \
  "${publisher}" \
    --tag v0.1.0 \
    --version 0.1.0 \
    --bundle-dir "${corrupt_root}" \
    --notes-file "${check_root}/notes.md" \
    --repo example/fcitx-vinpst \
    >"${check_root}/corrupt.out" 2>"${check_root}/corrupt.err"; then
  echo "publisher accepted a corrupted local bundle" >&2
  exit 1
fi
grep -Eq 'size mismatch|digest mismatch' "${check_root}/corrupt.err"
[[ ! -e "${corrupt_state}/release-state" ]]

printf 'GitHub release publication check passed\n'
