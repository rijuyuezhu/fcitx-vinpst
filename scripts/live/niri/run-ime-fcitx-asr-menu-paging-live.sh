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

cli_binary="${VINPUT_LIVE_CLI_BINARY:-target/debug/vinput}"
out_dir="${VINPUT_LIVE_ASR_MENU_PAGING_OUT_DIR:-target/tmp/ime-fcitx-asr-menu-paging-live}"
if [[ "${out_dir}" == /* ]]; then
  out_dir_abs="${out_dir}"
else
  out_dir_abs="${repo_root}/${out_dir}"
fi
service_path="${VINPUT_LIVE_DBUS_SERVICE:-${HOME}/.local/share/dbus-1/services/org.fcitx.Vinput.service}"
model_source="${VINPUT_LIVE_ASR_PAGING_MODEL_SOURCE:-${repo_root}/target/models/onnx-pf-zh-sm-off}"
model_root="${VINPUT_LIVE_ASR_PAGING_MODEL_ROOT:-${out_dir_abs}/model-root}"
trigger_key="${VINPUT_LIVE_ASR_MENU_KEY:-F8}"
addon_config="${VINPUT_LIVE_FCITX_ADDON_CONFIG:-${HOME}/.config/fcitx5/conf/vinput.conf}"
page_next_key="${VINPUT_LIVE_PAGE_NEXT_KEY:-}"
page_prev_key="${VINPUT_LIVE_PAGE_PREV_KEY:-}"
probe="scripts/live/niri/probes/fcitx-live-menu-paging-probe.py"
temporary_model_count=14
config_path=""
original_provider=""
original_model=""
service_mutated=0
fcitx_restart_needed=0
backup_existed=0

call_service() {
  gdbus call --session \
    --dest org.fcitx.Vinput \
    --object-path /org/fcitx/Vinput \
    --method "org.fcitx.Vinput.Service.$1" "${@:2}"
}

wait_backend() {
  local provider="$1" model="$2" output_path="$3"
  for _ in $(seq 1 600); do
    if "${cli_binary}" daemon status --json >"${output_path}" 2>/dev/null &&
      jq -e \
        --arg provider "${provider}" \
        --arg model "${model}" '
          .status == "idle" and
          .owner.ok == true and
          .asr_backend.has_effective_backend == true and
          .asr_backend.reload_in_progress == false and
          .asr_backend.last_error == "" and
          .asr_backend.target_provider_id == $provider and
          .asr_backend.target_model_id == $model and
          .asr_backend.effective_provider_id == $provider and
          .asr_backend.effective_model_id == $model
        ' "${output_path}" >/dev/null; then
      return 0
    fi
    sleep 0.1
  done
  echo "ASR backend did not become ready for ${provider}/${model}" >&2
  cat "${output_path}" >&2 2>/dev/null || true
  return 1
}

stop_verified_owner() {
  local status pid exe cmdline proc_exe proc_cmdline
  status="$("${cli_binary}" daemon status --json 2>/dev/null || true)"
  pid="$(jq -r '.owner.unix_process_id // empty' <<<"${status}")"
  [[ -z "${pid}" ]] && return 0
  exe="$(jq -r '.owner.process.exe // empty' <<<"${status}")"
  cmdline="$(jq -r '.owner.process.cmdline | join(" ")' <<<"${status}")"
  if [[ "${exe}" != *vinput-daemon* || "${cmdline}" != *"${config_path}"* ]]; then
    echo "refusing to stop unexpected org.fcitx.Vinput owner: pid=${pid} exe=${exe}" >&2
    return 1
  fi
  proc_exe="$(readlink "/proc/${pid}/exe")"
  proc_cmdline="$(tr '\0' ' ' <"/proc/${pid}/cmdline")"
  if [[ "${proc_exe}" != *vinput-daemon* || "${proc_cmdline}" != *"${config_path}"* ]]; then
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
  local output_path="$1"
  call_service GetStatus >/dev/null
  wait_backend "${original_provider}" "${original_model}" "${output_path}"
}

restart_fcitx() {
  local pid
  fcitx5 -rd >/dev/null 2>&1
  for _ in $(seq 1 100); do
    pid="$(pgrep -n -x fcitx5 || true)"
    if [[ -n "${pid}" ]] && fcitx5-remote --check >/dev/null 2>&1 &&
      grep -q "${HOME}/.local/lib/fcitx5/fcitx5-vinput.so" "/proc/${pid}/maps"; then
      printf '%s\n' "${pid}"
      return 0
    fi
    sleep 0.1
  done
  echo "restarted Fcitx did not load the user vinput addon" >&2
  return 1
}

verify_profile_unchanged() {
  cmp "${out_dir}/config-before.json" "${config_path}"
  if [[ "${backup_existed}" == 1 ]]; then
    cmp "${out_dir}/config-backup-before.json" "${config_path}.bak"
  else
    test ! -e "${config_path}.bak"
  fi
}

restore_service() {
  [[ "${service_mutated}" == 0 ]] && return 0
  stop_verified_owner
  install -m 0644 "${out_dir}/service-before.service" "${service_path}"
  activate_and_wait "${out_dir}/restored-status.json"
  cmp "${out_dir}/service-before.service" "${service_path}"
  if jq -e --arg model_root "${model_root}" \
    '.owner.process.cmdline | index($model_root) != null' \
    "${out_dir}/restored-status.json" >/dev/null; then
    echo "temporary ASR paging model root remained in the restored daemon" >&2
    return 1
  fi
  verify_profile_unchanged
  service_mutated=0
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  set +e
  if ! restore_service; then
    exit_code=1
  fi
  if [[ "${fcitx_restart_needed}" == 1 ]]; then
    if ! restart_fcitx >"${out_dir}/fcitx-restored.pid"; then
      exit_code=1
    fi
    fcitx_restart_needed=0
  fi
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

for command in cp fcitx5 fcitx5-remote gdbus jq pgrep python3 readlink; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required ASR paging command is missing: ${command}" >&2
    exit 2
  fi
done
if [[ ! -x "${cli_binary}" ]]; then
  echo "vinput CLI is missing: ${cli_binary}" >&2
  exit 2
fi
for path in "${service_path}" "${model_source}" "${model_source}/vinput-model.json"; do
  if [[ ! -e "${path}" ]]; then
    echo "ASR paging fixture is missing: ${path}" >&2
    exit 2
  fi
done
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
  echo "could not resolve configured ASR page keys" >&2
  exit 1
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}" "${model_root}"
"${cli_binary}" daemon status --json >"${out_dir}/status-before.json"
if ! jq -e '
  .status == "idle" and
  .owner.ok == true and
  .asr_backend.has_effective_backend == true and
  .asr_backend.reload_in_progress == false and
  .asr_backend.last_error == ""
' "${out_dir}/status-before.json" >/dev/null; then
  echo "ASR backend must be idle and ready before paging" >&2
  cat "${out_dir}/status-before.json" >&2
  exit 1
fi
original_provider="$(jq -r '.asr_backend.effective_provider_id' "${out_dir}/status-before.json")"
original_model="$(jq -r '.asr_backend.effective_model_id' "${out_dir}/status-before.json")"
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
install -m 0644 "${service_path}" "${out_dir}/service-before.service"

for index in $(seq -w 0 $((temporary_model_count - 1))); do
  target="${model_root}/live-paging-${index}"
  mkdir -p "${target}"
  cp -al "${model_source}/." "${target}/"
  rm -f "${target}/vinput-model.json"
  cp "${model_source}/vinput-model.json" "${target}/vinput-model.json"
  python3 - "${target}/vinput-model.json" "${index}" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
index = sys.argv[2]
metadata = json.loads(path.read_text())
display = metadata.setdefault("display", {})
display["registry_id"] = f"model.sherpa-onnx.live-paging-{index}"
display["fallback_title"] = f"Live Paging {index}"
display["localized_titles"] = {}
path.write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + "\n")
PY
done

python3 - "${out_dir}/service-before.service" "${out_dir}/service-model-root.service" \
  "${model_root}" <<'PY'
import shlex
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
model_root = sys.argv[3]
lines = source.read_text().splitlines()
exec_rows = [index for index, line in enumerate(lines) if line.startswith("Exec=")]
if len(exec_rows) != 1:
    raise SystemExit(f"expected one D-Bus service Exec row, found {len(exec_rows)}")
index = exec_rows[0]
command = shlex.split(lines[index][len("Exec=") :])
if "--model-root" in command:
    raise SystemExit("D-Bus activation service already contains --model-root")
command.extend(["--model-root", model_root])
lines[index] = "Exec=" + " ".join(shlex.quote(value) for value in command)
target.write_text("\n".join(lines) + "\n")
PY

stop_verified_owner
install -m 0644 "${out_dir}/service-model-root.service" "${service_path}"
service_mutated=1
activate_and_wait "${out_dir}/model-root-status.json"
if ! jq -e --arg model_root "${model_root}" \
  '.owner.process.cmdline | index($model_root) != null' \
  "${out_dir}/model-root-status.json" >/dev/null; then
  echo "activated daemon did not use the temporary ASR paging model root" >&2
  exit 1
fi

call_service GetAsrDisplayMenuState >"${out_dir}/menu-state-before.txt"
python3 - "${out_dir}/menu-state-before.txt" "${original_provider}" \
  "${original_model}" "${temporary_model_count}" <<'PY'
import ast
import re
import sys
from pathlib import Path

raw = Path(sys.argv[1]).read_text()
raw = re.sub(r"\btrue\b", "True", raw)
raw = re.sub(r"\bfalse\b", "False", raw)
state = ast.literal_eval(raw)
provider = sys.argv[2]
model = sys.argv[3]
temporary_count = int(sys.argv[4])
if state[0] != provider or state[1] != model:
    raise SystemExit("temporary model root changed the configured ASR target")
if state[2] != provider or state[3] != model or state[4] or state[5]:
    raise SystemExit("temporary model root changed the effective ASR backend")
rows = state[6]
if len(rows) != temporary_count + 1:
    raise SystemExit(
        f"expected {temporary_count + 1} ASR rows, found {len(rows)}"
    )
unique_titles = {row[3] for row in rows}
if len(unique_titles) != len(rows):
    raise SystemExit("temporary ASR paging rows did not have unique display titles")
PY

restart_fcitx | tee "${out_dir}/fcitx-before-paging.pid"
fcitx_restart_needed=1
python3 "${probe}" \
  --menu asr \
  --trigger-key "${trigger_key}" \
  --page-next-key "${page_next_key}" \
  --page-prev-key "${page_prev_key}" \
  | tee "${out_dir}/asr-paging.jsonl"
jq -s -e 'any(.[];
  .event == "summary" and
  .menu == "asr" and
  .ok == true and
  .first_page_count == 10 and
  .second_page_count == 4 and
  .commit_count == 0
)' "${out_dir}/asr-paging.jsonl" >/dev/null
verify_profile_unchanged
wait_backend "${original_provider}" "${original_model}" \
  "${out_dir}/backend-after-paging.json"

first_page_count="$(jq -r 'select(.event == "summary") | .first_page_count' \
  "${out_dir}/asr-paging.jsonl")"
second_page_count="$(jq -r 'select(.event == "summary") | .second_page_count' \
  "${out_dir}/asr-paging.jsonl")"
restore_service
restart_fcitx | tee "${out_dir}/fcitx-restored.pid"
fcitx_restart_needed=0

jq -n \
  --arg provider "${original_provider}" \
  --arg model "${original_model}" \
  --arg page_next_key "${page_next_key}" \
  --arg page_prev_key "${page_prev_key}" \
  --argjson temporary_model_count "${temporary_model_count}" \
  --argjson first_page_count "${first_page_count}" \
  --argjson second_page_count "${second_page_count}" \
  '{
    event: "summary",
    menu: "asr",
    provider: $provider,
    model: $model,
    page_keys: {next: $page_next_key, previous: $page_prev_key},
    temporary_model_count: $temporary_model_count,
    first_page_count: $first_page_count,
    second_page_count: $second_page_count,
    profile_unchanged: true,
    service_restored: true,
    fcitx_restored: true,
    backend_unchanged: true,
    ok: true
  }' | tee "${out_dir}/summary.json"
