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

cli_binary="${VINPST_LIVE_CLI_BINARY:-target/debug/vinpst}"
out_dir="${VINPST_LIVE_MENU_PAGING_OUT_DIR:-target/tmp/ime-fcitx-menu-paging-live}"
trigger_key="${VINPST_LIVE_SCENE_MENU_KEY:-F7}"
addon_config="${VINPST_LIVE_FCITX_ADDON_CONFIG:-${HOME}/.config/fcitx5/conf/vinpst.conf}"
page_next_key="${VINPST_LIVE_PAGE_NEXT_KEY:-}"
page_prev_key="${VINPST_LIVE_PAGE_PREV_KEY:-}"
probe="scripts/live/niri/probes/fcitx-live-menu-paging-probe.py"
config_path=""
profile_mutated=0
backup_existed=0
original_scene=""

call_service() {
  gdbus call --session \
    --dest org.fcitx.Vinpst \
    --object-path /org/fcitx/Vinpst \
    --method "org.fcitx.Vinpst.Service.$1" "${@:2}"
}

stop_verified_owner() {
  local status pid exe cmdline proc_exe proc_cmdline
  status="$("${cli_binary}" daemon status --json 2>/dev/null || true)"
  pid="$(jq -r '.owner.unix_process_id // empty' <<<"${status}")"
  [[ -z "${pid}" ]] && return 0
  exe="$(jq -r '.owner.process.exe // empty' <<<"${status}")"
  cmdline="$(jq -r '.owner.process.cmdline | join(" ")' <<<"${status}")"
  if [[ "${exe}" != *vinpst-daemon* || "${cmdline}" != *"${config_path}"* ]]; then
    echo "refusing to stop unexpected org.fcitx.Vinpst owner: pid=${pid} exe=${exe}" >&2
    return 1
  fi
  proc_exe="$(readlink "/proc/${pid}/exe")"
  proc_cmdline="$(tr '\0' ' ' <"/proc/${pid}/cmdline")"
  if [[ "${proc_exe}" != *vinpst-daemon* || "${proc_cmdline}" != *"${config_path}"* ]]; then
    echo "live owner changed during verification: pid=${pid} exe=${proc_exe}" >&2
    return 1
  fi
  kill -TERM "${pid}"
  for _ in $(seq 1 100); do
    if ! kill -0 "${pid}" 2>/dev/null; then
      return 0
    fi
    sleep 0.05
  done
  echo "verified daemon did not terminate after SIGTERM: ${pid}" >&2
  return 1
}

activate_and_wait() {
  call_service GetStatus >/dev/null
  for _ in $(seq 1 200); do
    if "${cli_binary}" daemon status --json >"${out_dir}/status-current.json" 2>/dev/null &&
      jq -e \
        --arg config_path "${config_path}" '
          .status == "idle" and
          .owner.ok == true and
          (.owner.process.exe | endswith("vinpst-daemon")) and
          (.owner.process.cmdline | index($config_path)) != null
        ' "${out_dir}/status-current.json" >/dev/null; then
      return 0
    fi
    sleep 0.05
  done
  echo "D-Bus activation did not restore an idle verified daemon" >&2
  cat "${out_dir}/status-current.json" >&2 2>/dev/null || true
  return 1
}

restore_profile() {
  [[ "${profile_mutated}" == 0 ]] && return 0
  stop_verified_owner
  install -m 0644 "${out_dir}/config-before.json" "${config_path}"
  if [[ "${backup_existed}" == 1 ]]; then
    install -m 0644 "${out_dir}/config-backup-before.json" "${config_path}.bak"
  else
    rm -f "${config_path}.bak"
  fi
  activate_and_wait
  cmp "${out_dir}/config-before.json" "${config_path}"
  if [[ "${backup_existed}" == 1 ]]; then
    cmp "${out_dir}/config-backup-before.json" "${config_path}.bak"
  else
    test ! -e "${config_path}.bak"
  fi
  restored_scene="$(call_service GetSceneState | python3 -c 'import ast,sys; print(ast.literal_eval(sys.stdin.read())[0])')"
  if [[ "${restored_scene}" != "${original_scene}" ]]; then
    echo "active scene was not restored: expected=${original_scene} actual=${restored_scene}" >&2
    return 1
  fi
  profile_mutated=0
}

cleanup() {
  local exit_code=$?
  trap - EXIT
  set +e
  if ! restore_profile; then
    exit_code=1
  fi
  exit "${exit_code}"
}
trap cleanup EXIT

for command in python3 jq gdbus fcitx5-remote readlink; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required command is missing: ${command}" >&2
    exit 2
  fi
done
if [[ ! -x "${cli_binary}" ]]; then
  echo "vinpst CLI is missing: ${cli_binary}" >&2
  exit 2
fi
if ! fcitx5-remote --check >/dev/null 2>&1; then
  echo "Fcitx5 is not running in this session" >&2
  exit 1
fi
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set" >&2
  exit 1
fi
if [[ -z "${page_next_key}" || -z "${page_prev_key}" ]]; then
  if [[ ! -f "${addon_config}" ]]; then
    echo "Fcitx addon config is missing: ${addon_config}" >&2
    exit 1
  fi
  mapfile -t configured_page_keys < <(python3 - "${addon_config}" <<'PY'
import sys
from pathlib import Path

sections = {"PageNextKeys": [], "PagePrevKeys": []}
current = None
for raw_line in Path(sys.argv[1]).read_text().splitlines():
    line = raw_line.strip()
    if line.startswith("[") and line.endswith("]"):
        current = line[1:-1]
        continue
    if current in sections and "=" in line:
        _index, value = line.split("=", 1)
        value = value.strip()
        if value:
            sections[current].append(value)
for section in ("PageNextKeys", "PagePrevKeys"):
    if not sections[section]:
        raise SystemExit(f"{section} has no configured key")
    print(sections[section][0])
PY
  )
  [[ -z "${page_next_key}" ]] && page_next_key="${configured_page_keys[0]:-}"
  [[ -z "${page_prev_key}" ]] && page_prev_key="${configured_page_keys[1]:-}"
fi
if [[ -z "${page_next_key}" || -z "${page_prev_key}" ]]; then
  echo "could not resolve configured page keys" >&2
  exit 1
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
"${cli_binary}" daemon status --json >"${out_dir}/status-before.json"
jq -e '.status == "idle" and .owner.ok == true' \
  "${out_dir}/status-before.json" >/dev/null
config_path="$(jq -r '
  .owner.process.cmdline as $args |
  ($args | index("--config")) as $index |
  if $index == null then empty else $args[$index + 1] end
' "${out_dir}/status-before.json")"
if [[ -z "${config_path}" || ! -f "${config_path}" ]]; then
  echo "could not resolve the active daemon config path" >&2
  exit 1
fi
install -m 0644 "${config_path}" "${out_dir}/config-before.json"
if [[ -f "${config_path}.bak" ]]; then
  backup_existed=1
  install -m 0644 "${config_path}.bak" "${out_dir}/config-backup-before.json"
fi
original_scene="$(call_service GetSceneState | python3 -c 'import ast,sys; print(ast.literal_eval(sys.stdin.read())[0])')"

python3 - "${out_dir}/config-before.json" "${out_dir}/config-paged.json" <<'PY'
import copy
import json
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
config = json.loads(source.read_text())
definitions = config["scenes"]["definitions"]
if not definitions:
    raise SystemExit("profile has no scene template")
existing = {scene["id"] for scene in definitions}
for index in range(12):
    scene_id = f"__live_paging_{index:02d}__"
    if scene_id in existing:
        raise SystemExit(f"temporary paging scene already exists: {scene_id}")
    scene = copy.deepcopy(definitions[0])
    scene["id"] = scene_id
    scene["label"] = f"Paging {index:02d}"
    scene["prompt"] = None
    scene["provider_id"] = None
    scene["model"] = None
    scene["candidate_count"] = 0
    scene["context_lines"] = 0
    definitions.append(scene)
target.write_text(json.dumps(config, ensure_ascii=False, indent=2) + "\n")
PY
"${cli_binary}" config validate "${out_dir}/config-paged.json" --json \
  | tee "${out_dir}/config-validate.json"
install -m 0644 "${out_dir}/config-paged.json" "${config_path}"
profile_mutated=1
stop_verified_owner
activate_and_wait
jq -e '.runtime_status.config.scene_count >= 15' \
  "${out_dir}/status-current.json" >/dev/null

python3 "${probe}" \
  --trigger-key "${trigger_key}" \
  --page-next-key "${page_next_key}" \
  --page-prev-key "${page_prev_key}" \
  | tee "${out_dir}/scene-paging.jsonl"
jq -e 'select(.event == "summary" and .ok == true and .first_page_count == 10 and .second_page_count > 0)' \
  "${out_dir}/scene-paging.jsonl" >/dev/null

restore_profile
jq -n \
  --arg config_path "${config_path}" \
  --arg original_scene "${original_scene}" \
  '{
    event: "summary",
    config_path: $config_path,
    original_scene: $original_scene,
    temporary_scene_count: 12,
    profile_restored: true,
    ok: true
  }' | tee "${out_dir}/summary.json"
