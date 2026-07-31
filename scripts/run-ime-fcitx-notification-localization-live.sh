#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

out_dir="${VINPUT_LIVE_NOTIFICATION_LOCALIZATION_OUT_DIR:-target/tmp/ime-fcitx-notification-localization-live}"
module_path="${HOME}/.local/lib/fcitx5/fcitx5-vinput.so"
catalog_path="${HOME}/.local/share/locale/zh_CN/LC_MESSAGES/fcitx5-vinput.mo"
fcitx_wrapper="${HOME}/.local/share/fcitx-vinput/fcitx5-with-vinput-env.sh"
profile_path="${HOME}/.local/share/fcitx-vinput/sherpa-native-command-live.json"
service_path="${HOME}/.local/share/dbus-1/services/org.fcitx.Vinput.service"
addon_config="${HOME}/.config/fcitx5/conf/vinput.conf"
addon_metadata="${HOME}/.local/share/fcitx5/addon/vinput.conf"
fcitx_env="${HOME}/.local/share/fcitx-vinput/fcitx-vinput.env"
cli_binary="${repo_root}/target/debug/vinput"
fcitx_settle_seconds="${VINPUT_LIVE_FCITX_SETTLE_SECONDS:-1}"
localized_fcitx=0
success=0

for command in cmp fcitx5-remote jq pgrep python3; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required command is missing: ${command}" >&2
    exit 2
  fi
done
for required in \
  "${module_path}" \
  "${catalog_path}" \
  "${fcitx_wrapper}" \
  "${profile_path}" \
  "${service_path}" \
  "${addon_config}" \
  "${addon_metadata}" \
  "${fcitx_env}" \
  "${cli_binary}" \
  scripts/run-ime-fcitx-notification-live.sh \
  scripts/run-ime-fcitx-error-notification-live.sh; do
  if [[ ! -e "${required}" ]]; then
    echo "required localized-notification path is missing: ${required}" >&2
    exit 2
  fi
done
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set" >&2
  exit 2
fi
if ! fcitx5-remote --check >/dev/null 2>&1; then
  echo "Fcitx5 is not running in this session" >&2
  exit 1
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
cp -a "${module_path}" "${out_dir}/module-before.so"
cp -a "${catalog_path}" "${out_dir}/catalog-before.mo"
cp -a "${profile_path}" "${out_dir}/profile-before.json"
cp -a "${service_path}" "${out_dir}/service-before.service"
cp -a "${addon_config}" "${out_dir}/addon-config-before.conf"
cp -a "${addon_metadata}" "${out_dir}/addon-metadata-before.conf"
cp -a "${fcitx_env}" "${out_dir}/fcitx-env-before.sh"
"${cli_binary}" daemon status --json >"${out_dir}/status-before.json"
if ! jq -e '
  .status == "idle" and
  .owner.ok == true and
  .asr_backend.has_effective_backend == true and
  .asr_backend.reload_in_progress == false and
  .asr_backend.last_error == ""
' "${out_dir}/status-before.json" >/dev/null; then
  cat "${out_dir}/status-before.json" >&2
  echo "daemon must be idle with a healthy backend before localized notifications" >&2
  exit 1
fi
original_provider="$(jq -r '.asr_backend.effective_provider_id' "${out_dir}/status-before.json")"
original_model="$(jq -r '.asr_backend.effective_model_id' "${out_dir}/status-before.json")"
original_fcitx_pid="$(pgrep -n -x fcitx5 || true)"
if [[ -z "${original_fcitx_pid}" ]]; then
  echo "could not resolve the current Fcitx process" >&2
  exit 1
fi

locale_env() {
  local pid="$1"
  tr '\0' '\n' <"/proc/${pid}/environ" \
    | grep -E '^(LANGUAGE|LC_ALL|LC_MESSAGES|LANG)=' \
    | sort || true
}
locale_env "${original_fcitx_pid}" >"${out_dir}/locale-before.env"

wait_fcitx_locale() {
  local previous_pid="$1"
  local expected_path="$2"
  local environ_path="$3"
  local current_path="${out_dir}/locale-current.env"
  local pid
  for _ in $(seq 1 180); do
    pid="$(pgrep -n -x fcitx5 || true)"
    if [[ -n "${pid}" ]] && [[ "${pid}" != "${previous_pid}" ]] &&
      [[ ! -e "/proc/${previous_pid}" ]] &&
      fcitx5-remote --check >/dev/null 2>&1 &&
      grep -q "${module_path}" "/proc/${pid}/maps"; then
      tr '\0' '\n' <"/proc/${pid}/environ" >"${environ_path}"
      locale_env "${pid}" >"${current_path}"
      if cmp -s "${expected_path}" "${current_path}"; then
        sleep "${fcitx_settle_seconds}"
        if [[ -e "/proc/${pid}" ]] && fcitx5-remote --check >/dev/null 2>&1; then
          locale_env "${pid}" >"${current_path}"
          if cmp -s "${expected_path}" "${current_path}"; then
            printf '%s\n' "${pid}"
            return 0
          fi
        fi
      fi
    fi
    sleep 0.1
  done
  echo "Fcitx did not restart with the expected locale environment" >&2
  cat "${expected_path}" >&2 2>/dev/null || true
  cat "${current_path}" >&2 2>/dev/null || true
  return 1
}

restart_fcitx_zh_cn() {
  local previous_pid
  previous_pid="$(pgrep -n -x fcitx5 || true)"
  cat >"${out_dir}/locale-zh-cn.expected" <<'EOF'
LANG=en_US.UTF-8
LANGUAGE=zh_CN:zh
LC_MESSAGES=en_US.UTF-8
EOF
  env -u LC_ALL \
    LANGUAGE=zh_CN:zh \
    LC_MESSAGES=en_US.UTF-8 \
    LANG=en_US.UTF-8 \
    "${fcitx_wrapper}" -rd >/dev/null 2>&1
  wait_fcitx_locale \
    "${previous_pid}" \
    "${out_dir}/locale-zh-cn.expected" \
    "${out_dir}/fcitx-zh-cn.environ"
}

restart_fcitx_english() {
  local previous_pid
  previous_pid="$(pgrep -n -x fcitx5 || true)"
  cat >"${out_dir}/locale-english.expected" <<'EOF'
LANG=en_US.UTF-8
LC_MESSAGES=en_US.UTF-8
EOF
  env -u LANGUAGE -u LC_ALL \
    LC_MESSAGES=en_US.UTF-8 \
    LANG=en_US.UTF-8 \
    "${fcitx_wrapper}" -rd >/dev/null 2>&1
  wait_fcitx_locale \
    "${previous_pid}" \
    "${out_dir}/locale-english.expected" \
    "${out_dir}/fcitx-english.environ"
}

restart_fcitx_original() {
  local previous_pid
  local -a command=(env -u LANGUAGE -u LC_ALL -u LC_MESSAGES -u LANG)
  local name value
  previous_pid="$(pgrep -n -x fcitx5 || true)"
  while IFS='=' read -r name value; do
    [[ -z "${name}" ]] && continue
    command+=("${name}=${value}")
  done <"${out_dir}/locale-before.env"
  command+=("${fcitx_wrapper}" -rd)
  "${command[@]}" >/dev/null 2>&1
  wait_fcitx_locale \
    "${previous_pid}" \
    "${out_dir}/locale-before.env" \
    "${out_dir}/fcitx-restored.environ"
}

verify_files() {
  cmp "${out_dir}/module-before.so" "${module_path}"
  cmp "${out_dir}/catalog-before.mo" "${catalog_path}"
  cmp "${out_dir}/profile-before.json" "${profile_path}"
  cmp "${out_dir}/service-before.service" "${service_path}"
  cmp "${out_dir}/addon-config-before.conf" "${addon_config}"
  cmp "${out_dir}/addon-metadata-before.conf" "${addon_metadata}"
  cmp "${out_dir}/fcitx-env-before.sh" "${fcitx_env}"
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  set +e
  if [[ "${success}" == 0 && "${localized_fcitx}" == 1 ]]; then
    restart_fcitx_original >"${out_dir}/fcitx-cleanup.pid" || exit_code=1
  fi
  verify_files || exit_code=1
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

restart_fcitx_zh_cn | tee "${out_dir}/fcitx-zh-cn.pid"
localized_fcitx=1
VINPUT_LIVE_NOTIFICATION_OUT_DIR="${out_dir}/info-zh-cn" \
VINPUT_LIVE_NOTIFICATION_EXPECTED_SUMMARY='语音输入' \
VINPUT_LIVE_NOTIFICATION_EXPECTED_BODY_PREFIX='已切换场景到“' \
VINPUT_LIVE_NOTIFICATION_EXPECTED_BODY_SUFFIX='”。' \
  scripts/run-ime-fcitx-notification-live.sh
VINPUT_LIVE_ERROR_NOTIFICATION_OUT_DIR="${out_dir}/error-zh-cn" \
VINPUT_LIVE_ERROR_NOTIFICATION_EXPECTED_SUMMARY='语音输入' \
  scripts/run-ime-fcitx-error-notification-live.sh

restart_fcitx_english | tee "${out_dir}/fcitx-english.pid"
VINPUT_LIVE_NOTIFICATION_OUT_DIR="${out_dir}/info-english" \
VINPUT_LIVE_NOTIFICATION_EXPECTED_SUMMARY='Voice Input' \
VINPUT_LIVE_NOTIFICATION_EXPECTED_BODY_PREFIX="Switched scene to '" \
VINPUT_LIVE_NOTIFICATION_EXPECTED_BODY_SUFFIX="'." \
  scripts/run-ime-fcitx-notification-live.sh

restart_fcitx_original | tee "${out_dir}/fcitx-restored.pid"
localized_fcitx=0
verify_files
"${cli_binary}" daemon status --json >"${out_dir}/status-after.json"
if ! jq -e \
  --arg provider "${original_provider}" \
  --arg model "${original_model}" '
    .status == "idle" and
    .owner.ok == true and
    .asr_backend.has_effective_backend == true and
    .asr_backend.reload_in_progress == false and
    .asr_backend.last_error == "" and
    .asr_backend.target_provider_id == $provider and
    .asr_backend.target_model_id == $model and
    .asr_backend.effective_provider_id == $provider and
    .asr_backend.effective_model_id == $model
  ' "${out_dir}/status-after.json" >/dev/null; then
  cat "${out_dir}/status-after.json" >&2
  echo "localized notification gate changed daemon backend state" >&2
  exit 1
fi

python3 - \
  "${out_dir}/info-zh-cn/summary.json" \
  "${out_dir}/error-zh-cn/summary.json" \
  "${out_dir}/info-english/summary.json" \
  "${out_dir}/locale-before.env" \
  "${out_dir}/summary.json" <<'PY'
import json
import sys
from pathlib import Path

info_zh = json.loads(Path(sys.argv[1]).read_text())
error_zh = json.loads(Path(sys.argv[2]).read_text())
info_en = json.loads(Path(sys.argv[3]).read_text())
original_locale = Path(sys.argv[4]).read_text().splitlines()
out_path = Path(sys.argv[5])

failures = []
if info_zh.get("summary") != "语音输入":
    failures.append("Chinese information summary was not localized")
if not str(info_zh.get("body", "")).startswith("已切换场景到“"):
    failures.append("Chinese information body prefix was not localized")
if not str(info_zh.get("body", "")).endswith("”。"):
    failures.append("Chinese information body suffix was not localized")
if error_zh.get("summary") != "语音输入":
    failures.append("Chinese error summary was not localized")
if error_zh.get("error", "") == "":
    failures.append("daemon technical error body was empty")
if info_en.get("summary") != "Voice Input":
    failures.append("English notification summary was not restored")
if not str(info_en.get("body", "")).startswith("Switched scene to '"):
    failures.append("English information body was not restored")

result = {
    "event": "summary",
    "zh_cn": {
        "info_summary": info_zh.get("summary"),
        "info_body": info_zh.get("body"),
        "error_summary": error_zh.get("summary"),
        "error_body_preserved": error_zh.get("error", "") != "",
        "info_sender_verified": info_zh.get("sender_is_fcitx") is True,
        "error_daemon_sender_verified": error_zh.get("daemon_sender_verified") is True,
        "error_fcitx_sender_verified": error_zh.get("fcitx_sender_verified") is True,
    },
    "english": {
        "info_summary": info_en.get("summary"),
        "info_body": info_en.get("body"),
        "sender_verified": info_en.get("sender_is_fcitx") is True,
    },
    "original_locale": original_locale,
    "profile_unchanged": True,
    "service_unchanged": True,
    "addon_config_unchanged": True,
    "addon_metadata_unchanged": True,
    "module_unchanged": True,
    "catalog_unchanged": True,
    "fcitx_env_unchanged": True,
    "backend_unchanged": True,
    "original_locale_restored": True,
    "ok": not failures,
    "failures": failures,
}
out_path.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(result, ensure_ascii=False, indent=2))
if failures:
    raise SystemExit("; ".join(failures))
PY

success=1
