#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

out_dir="${VINPUT_LIVE_ASR_NOTIFICATION_LOCALIZATION_OUT_DIR:-target/tmp/ime-fcitx-asr-notification-localization-live}"
child_out_dir="${out_dir}/cross-provider-failure"
module_path="${HOME}/.local/lib/fcitx5/fcitx5-vinput.so"
catalog_path="${HOME}/.local/share/locale/zh_CN/LC_MESSAGES/fcitx5-vinput.mo"
fcitx_wrapper="${HOME}/.local/share/fcitx-vinput/fcitx5-with-vinput-env.sh"
profile_path="${HOME}/.local/share/fcitx-vinput/sherpa-native-command-live.json"
service_path="${HOME}/.local/share/dbus-1/services/org.fcitx.Vinput.service"
addon_config="${HOME}/.config/fcitx5/conf/vinput.conf"
addon_metadata="${HOME}/.local/share/fcitx5/addon/vinput.conf"
fcitx_env="${HOME}/.local/share/fcitx-vinput/fcitx-vinput.env"
cli_binary="${repo_root}/target/debug/vinput"
child_runner="${repo_root}/scripts/run-ime-fcitx-cross-provider-failure-live.sh"
fcitx_settle_seconds="${VINPUT_LIVE_FCITX_SETTLE_SECONDS:-1}"
locale_may_have_changed=0
success=0

for command in cmp fcitx5-remote jq pgrep; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required ASR-notification localization command is missing: ${command}" >&2
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
  "${child_runner}"; do
  if [[ ! -e "${required}" ]]; then
    echo "required ASR-notification localization path is missing: ${required}" >&2
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
  echo "daemon must be idle with a healthy backend before localized ASR notifications" >&2
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
      { [[ -z "${previous_pid}" ]] || [[ ! -e "/proc/${previous_pid}" ]]; } &&
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

stop_fcitx() {
  local pid
  if ! pgrep -x fcitx5 >/dev/null 2>&1; then
    return 0
  fi
  fcitx5-remote -e >/dev/null 2>&1 || true
  for _ in $(seq 1 180); do
    if ! pgrep -x fcitx5 >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.1
  done
  pid="$(pgrep -n -x fcitx5 || true)"
  echo "Fcitx did not exit before restoring the original locale/config: ${pid}" >&2
  return 1
}

restart_fcitx_original() {
  local previous_pid
  local -a command=(env -u LANGUAGE -u LC_ALL -u LC_MESSAGES -u LANG)
  local name value
  previous_pid="$(pgrep -n -x fcitx5 || true)"
  stop_fcitx
  install -m 0644 "${out_dir}/addon-config-before.conf" "${addon_config}"
  while IFS='=' read -r name value; do
    [[ -z "${name}" ]] && continue
    command+=("${name}=${value}")
  done <"${out_dir}/locale-before.env"
  command+=("${fcitx_wrapper}" -d)
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
  if [[ "${success}" == 0 && "${locale_may_have_changed}" == 1 ]]; then
    restart_fcitx_original >"${out_dir}/fcitx-cleanup.pid" || exit_code=1
    locale_may_have_changed=0
  fi
  verify_files || exit_code=1
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

locale_may_have_changed=1
env -u LC_ALL \
  LANGUAGE=zh_CN:zh \
  LC_MESSAGES=en_US.UTF-8 \
  LANG=en_US.UTF-8 \
  VINPUT_LIVE_CROSS_PROVIDER_FAILURE_OUT_DIR="${child_out_dir}" \
  VINPUT_LIVE_FAILURE_EXPECTED_NOTIFICATION_SUMMARY='语音输入' \
  VINPUT_LIVE_FAILURE_EXPECTED_SWITCH_BODY_PREFIX='已请求切换语音识别到“' \
  VINPUT_LIVE_FAILURE_EXPECTED_SWITCH_BODY_SUFFIX='”。' \
  "${child_runner}"

current_fcitx_pid="$(pgrep -n -x fcitx5 || true)"
if [[ -z "${current_fcitx_pid}" ]]; then
  echo "localized cross-provider gate left no Fcitx process" >&2
  exit 1
fi
locale_env "${current_fcitx_pid}" >"${out_dir}/locale-after-child.env"
cat >"${out_dir}/locale-zh-cn.expected" <<'EOF'
LANG=en_US.UTF-8
LANGUAGE=zh_CN:zh
LC_MESSAGES=en_US.UTF-8
EOF
cmp "${out_dir}/locale-zh-cn.expected" "${out_dir}/locale-after-child.env"

notification_json="${child_out_dir}/notification.json"
child_summary="${child_out_dir}/summary.json"
if ! jq -e '
  .ok == true and
  .expected_summary == "语音输入" and
  .switch.icon == "dialog-information" and
  .switch.summary == "语音输入" and
  .switch.body == "已请求切换语音识别到“remote-failure-fixture”。" and
  .switch.expected_body == .switch.body and
  .switch.timeout_ms == 3000 and
  .failure.icon == "dialog-error" and
  .failure.summary == "语音输入" and
  .failure.body_preserved == true and
  .failure.timeout_ms == 5000
' "${notification_json}" >/dev/null; then
  cat "${notification_json}" >&2
  echo "ASR switch notifications did not match the zh_CN contract" >&2
  exit 1
fi
if ! jq -e '
  .ok == true and
  .failure.previous_backend_preserved == true and
  .failure.info_fcitx_sender_verified == true and
  .failure.error_fcitx_sender_verified == true and
  .recovery.recognition == true and
  .profile_restored == true and
  .backup_restored == true and
  .service_unchanged == true and
  .addon_config_unchanged == true and
  .fcitx_restored == true and
  .backend_restored == true
' "${child_summary}" >/dev/null; then
  cat "${child_summary}" >&2
  echo "localized ASR switch child did not preserve and recover state" >&2
  exit 1
fi

restart_fcitx_original | tee "${out_dir}/fcitx-restored.pid"
locale_may_have_changed=0
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
  echo "localized ASR switch gate changed the final daemon backend" >&2
  exit 1
fi

jq -n \
  --slurpfile notification "${notification_json}" \
  --slurpfile child "${child_summary}" \
  --rawfile original_locale "${out_dir}/locale-before.env" '
  {
    event: "summary",
    locale: "zh_CN",
    notification: {
      summary: $notification[0].switch.summary,
      switch_body: $notification[0].switch.body,
      error_summary: $notification[0].failure.summary,
      error_body_preserved: $notification[0].failure.body_preserved,
      info_sender_verified: $child[0].failure.info_fcitx_sender_verified,
      error_sender_verified: $child[0].failure.error_fcitx_sender_verified
    },
    failure_preserved: $child[0].failure.previous_backend_preserved,
    recovery_partial_count: $child[0].recovery.partial_count,
    recovery_commit: $child[0].recovery.commit,
    original_locale: ($original_locale | split("\n") | map(select(length > 0))),
    profile_unchanged: true,
    backup_restored: true,
    service_unchanged: true,
    addon_config_unchanged: true,
    addon_metadata_unchanged: true,
    module_unchanged: true,
    catalog_unchanged: true,
    fcitx_env_unchanged: true,
    backend_unchanged: true,
    original_locale_restored: true,
    ok: (
      $notification[0].ok and
      $child[0].ok and
      $child[0].failure.previous_backend_preserved and
      ($child[0].recovery.partial_count > 0) and
      (($child[0].recovery.commit | length) > 0)
    )
  }' | tee "${out_dir}/summary.json"

success=1
