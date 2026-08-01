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

probe="${repo_root}/scripts/live/niri/probes/fcitx-live-localization-probe.py"
out_dir="${VINPUT_LIVE_LOCALIZATION_OUT_DIR:-${repo_root}/target/tmp/ime-fcitx-localization-live}"
build_dir="${repo_root}/target/cpp/fcitx5-user-ime-localization-live"
module_path="${HOME}/.local/lib/fcitx5/fcitx5-vinput.so"
addon_metadata="${HOME}/.local/share/fcitx5/addon/vinput.conf"
catalog_root="${HOME}/.local/share/locale"
catalog_path="${catalog_root}/zh_CN/LC_MESSAGES/fcitx5-vinput.mo"
fcitx_env="${HOME}/.local/share/fcitx-vinput/fcitx-vinput.env"
fcitx_wrapper="${HOME}/.local/share/fcitx-vinput/fcitx5-with-vinput-env.sh"
profile_path="${HOME}/.local/share/fcitx-vinput/sherpa-native-command-live.json"
service_path="${HOME}/.local/share/dbus-1/services/org.fcitx.Vinput.service"
addon_config="${HOME}/.config/fcitx5/conf/vinput.conf"
cli_binary="${repo_root}/target/debug/vinput"
daemon_path="${HOME}/.local/bin/vinput-daemon"

for command in cmake cmp fcitx5-remote gdbus grep install jq pgrep python3 ruff strings; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required command is missing: ${command}" >&2
    exit 2
  fi
done
for required in \
  "${probe}" \
  "${module_path}" \
  "${addon_metadata}" \
  "${fcitx_env}" \
  "${fcitx_wrapper}" \
  "${profile_path}" \
  "${service_path}" \
  "${addon_config}" \
  "${cli_binary}" \
  "${daemon_path}"; do
  if [[ ! -e "${required}" ]]; then
    echo "required live localization path is missing: ${required}" >&2
    exit 2
  fi
done
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set" >&2
  exit 2
fi

rm -rf "${out_dir}" "${build_dir}"
mkdir -p "${out_dir}"
cp -a "${module_path}" "${out_dir}/module-before.so"
cp -a "${profile_path}" "${out_dir}/profile-before.json"
cp -a "${service_path}" "${out_dir}/service-before.service"
cp -a "${addon_config}" "${out_dir}/addon-config-before.conf"
cp -a "${addon_metadata}" "${out_dir}/addon-metadata-before.conf"
cp -a "${fcitx_env}" "${out_dir}/fcitx-env-before.sh"
if [[ -f "${catalog_path}" ]]; then
  catalog_existed=1
  cp -a "${catalog_path}" "${out_dir}/catalog-before.mo"
else
  catalog_existed=0
fi
"${cli_binary}" daemon status --json >"${out_dir}/status-before.json"
original_provider="$(jq -r '.asr_backend.effective_provider_id' "${out_dir}/status-before.json")"
original_model="$(jq -r '.asr_backend.effective_model_id' "${out_dir}/status-before.json")"
if ! jq -e '.status == "idle" and .asr_backend.reload_in_progress == false and .asr_backend.last_error == ""' \
  "${out_dir}/status-before.json" >/dev/null; then
  cat "${out_dir}/status-before.json" >&2
  echo "daemon must be idle with a healthy backend before localization proof" >&2
  exit 2
fi

module_installed=0
success=0

wait_fcitx() {
  local expected_language="$1"
  local output_path="$2"
  local previous_pid="$3"
  local pid
  for _ in $(seq 1 120); do
    pid="$(pgrep -n -x fcitx5 || true)"
    if [[ -n "${pid}" ]] && fcitx5-remote --check >/dev/null 2>&1 &&
      grep -q "${module_path}" "/proc/${pid}/maps"; then
      if [[ -n "${previous_pid}" ]] &&
        { [[ "${pid}" == "${previous_pid}" ]] || [[ -e "/proc/${previous_pid}" ]]; }; then
        sleep 0.1
        continue
      fi
      tr '\0' '\n' <"/proc/${pid}/environ" >"${output_path}"
      if [[ -n "${expected_language}" ]]; then
        if ! grep -qx "LANGUAGE=${expected_language}" "${output_path}"; then
          sleep 0.1
          continue
        fi
      elif grep -q '^LANGUAGE=' "${output_path}"; then
        sleep 0.1
        continue
      fi
      printf '%s\n' "${pid}"
      return 0
    fi
    sleep 0.1
  done
  echo "Fcitx did not restart with the expected locale environment" >&2
  return 1
}

restart_fcitx_zh_cn() {
  local previous_pid
  previous_pid="$(pgrep -n -x fcitx5 || true)"
  LANGUAGE=zh_CN:zh \
  LC_MESSAGES=en_US.UTF-8 \
  LANG=en_US.UTF-8 \
    "${fcitx_wrapper}" -rd >/dev/null 2>&1
  wait_fcitx "zh_CN:zh" "${out_dir}/fcitx-zh-cn.environ" "${previous_pid}"
}

restart_fcitx_english() {
  local previous_pid
  previous_pid="$(pgrep -n -x fcitx5 || true)"
  env -u LANGUAGE -u LC_ALL \
    LC_MESSAGES=en_US.UTF-8 \
    LANG=en_US.UTF-8 \
    "${fcitx_wrapper}" -rd >/dev/null 2>&1
  wait_fcitx "" "${out_dir}/fcitx-restored.environ" "${previous_pid}"
}

stop_fcitx() {
  local pid
  if ! pgrep -x fcitx5 >/dev/null 2>&1; then
    return 0
  fi
  fcitx5-remote -e >/dev/null 2>&1 || true
  for _ in $(seq 1 120); do
    if ! pgrep -x fcitx5 >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.1
  done
  pid="$(pgrep -n -x fcitx5 || true)"
  echo "Fcitx did not exit before exact state verification: ${pid}" >&2
  return 1
}

restore_installed_files() {
  [[ "${module_installed}" == 0 ]] && return 0
  install -m 0755 "${out_dir}/module-before.so" "${module_path}"
  if [[ "${catalog_existed}" == 1 ]]; then
    install -m 0644 "${out_dir}/catalog-before.mo" "${catalog_path}"
  else
    rm -f "${catalog_path}"
  fi
  module_installed=0
}

wait_file_equal() {
  local expected="$1"
  local actual="$2"
  for _ in $(seq 1 50); do
    if cmp -s "${expected}" "${actual}"; then
      return 0
    fi
    sleep 0.1
  done
  cmp "${expected}" "${actual}"
}

verify_mutable_state() {
  wait_file_equal "${out_dir}/profile-before.json" "${profile_path}"
  wait_file_equal "${out_dir}/service-before.service" "${service_path}"
  wait_file_equal "${out_dir}/addon-config-before.conf" "${addon_config}"
  wait_file_equal "${out_dir}/addon-metadata-before.conf" "${addon_metadata}"
  wait_file_equal "${out_dir}/fcitx-env-before.sh" "${fcitx_env}"
}

cleanup() {
  local exit_code=$?
  set +e
  if [[ "${success}" == 0 ]]; then
    stop_fcitx || exit_code=1
    restore_installed_files || exit_code=1
    verify_mutable_state || exit_code=1
    restart_fcitx_english >"${out_dir}/fcitx-cleanup.pid" || exit_code=1
  fi
  find scripts -type d -name __pycache__ -prune -exec rm -rf {} +
  trap - EXIT INT TERM
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

ruff check "${probe}"
ruff format --check "${probe}"
python3 -m py_compile "${probe}"

cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
  -DVINPUT_DAEMON_EXECUTABLE="${daemon_path}" \
  -DVINPUT_FCITX_RUNTIME_BUILD_LOCALEDIR= \
  -DVINPUT_FCITX_RUNTIME_INSTALL_LOCALEDIR="${catalog_root}"
cmake --build "${build_dir}" --target fcitx5_vinput_addon --parallel

module_candidate="${build_dir}/fcitx5-vinput.so"
catalog_candidate="${build_dir}/locale/zh_CN/LC_MESSAGES/fcitx5-vinput.mo"
test -f "${module_candidate}"
test -f "${catalog_candidate}"
strings "${module_candidate}" >"${out_dir}/module-candidate-strings.txt"
grep -Fxq "${catalog_root}" "${out_dir}/module-candidate-strings.txt"
if grep -Fq "${build_dir}/locale" "${out_dir}/module-candidate-strings.txt"; then
  echo "localization addon candidate retained its build-tree locale fallback" >&2
  exit 1
fi

install -Dm755 "${module_candidate}" "${module_path}"
install -Dm644 "${catalog_candidate}" "${catalog_path}"
module_installed=1
sha256sum "${module_path}" "${catalog_path}" >"${out_dir}/installed-sha256.txt"

restart_fcitx_zh_cn | tee "${out_dir}/fcitx-zh-cn.pid"
if grep -q '^VINPUT_FCITX_LOCALEDIR=' "${out_dir}/fcitx-zh-cn.environ"; then
  echo "localization proof must not use VINPUT_FCITX_LOCALEDIR override" >&2
  exit 1
fi

XDG_DATA_HOME="${HOME}/.local/share" LANGUAGE=zh_CN:zh LC_MESSAGES=en_US.UTF-8 LANG=en_US.UTF-8 \
  python3 "${probe}" \
    --menu scene \
    --trigger-key F7 \
    --expected-title '场景 /过滤' \
    --expected-status-prefix '当前：' \
    --expected-candidate Raw \
  | tee "${out_dir}/scene-zh-cn.jsonl"
XDG_DATA_HOME="${HOME}/.local/share" LANGUAGE=zh_CN:zh LC_MESSAGES=en_US.UTF-8 LANG=en_US.UTF-8 \
  python3 "${probe}" \
    --menu asr \
    --trigger-key F8 \
    --expected-title '模型 /过滤' \
    --expected-status-prefix '当前：' \
  | tee "${out_dir}/asr-zh-cn.jsonl"

restart_fcitx_english | tee "${out_dir}/fcitx-restored.pid"
XDG_DATA_HOME="${HOME}/.local/share" python3 "${probe}" \
  --menu scene \
  --trigger-key F7 \
  --expected-title 'Scenes /filter' \
  --expected-status-prefix 'Current: ' \
  --expected-candidate Raw \
  | tee "${out_dir}/scene-restored-en.jsonl"

stop_fcitx
verify_mutable_state
"${cli_binary}" daemon status --json >"${out_dir}/status-after.json"
if ! jq -e \
  --arg provider "${original_provider}" \
  --arg model "${original_model}" \
  '.status == "idle" and
   .asr_backend.reload_in_progress == false and
   .asr_backend.last_error == "" and
   .asr_backend.effective_provider_id == $provider and
   .asr_backend.effective_model_id == $model and
   .asr_backend.target_provider_id == $provider and
   .asr_backend.target_model_id == $model' \
  "${out_dir}/status-after.json" >/dev/null; then
  cat "${out_dir}/status-after.json" >&2
  echo "localization proof changed daemon backend state" >&2
  exit 1
fi

scene_title="$(jq -r 'select(.event == "summary") | .title' "${out_dir}/scene-zh-cn.jsonl")"
asr_title="$(jq -r 'select(.event == "summary") | .title' "${out_dir}/asr-zh-cn.jsonl")"
restored_title="$(jq -r 'select(.event == "summary") | .title' "${out_dir}/scene-restored-en.jsonl")"
module_before_sha="$(sha256sum "${out_dir}/module-before.so" | cut -d' ' -f1)"
module_after_sha="$(sha256sum "${module_path}" | cut -d' ' -f1)"
catalog_sha="$(sha256sum "${catalog_path}" | cut -d' ' -f1)"

restart_fcitx_english | tee "${out_dir}/fcitx-final.pid"

jq -n \
  --arg event summary \
  --arg locale zh_CN \
  --arg scene_title "${scene_title}" \
  --arg asr_title "${asr_title}" \
  --arg restored_title "${restored_title}" \
  --arg catalog_root "${catalog_root}" \
  --arg module_before_sha256 "${module_before_sha}" \
  --arg module_after_sha256 "${module_after_sha}" \
  --arg catalog_sha256 "${catalog_sha}" \
  --arg provider "${original_provider}" \
  --arg model "${original_model}" \
  '{
    event: $event,
    locale: $locale,
    scene_title: $scene_title,
    asr_title: $asr_title,
    restored_title: $restored_title,
    catalog_root: $catalog_root,
    build_fallback_disabled: true,
    locale_override_absent: true,
    module_before_sha256: $module_before_sha256,
    module_after_sha256: $module_after_sha256,
    catalog_sha256: $catalog_sha256,
    provider: $provider,
    model: $model,
    profile_unchanged: true,
    service_unchanged: true,
    addon_config_unchanged: true,
    addon_config_verified_without_fcitx_writer: true,
    addon_metadata_unchanged: true,
    fcitx_env_unchanged: true,
    backend_unchanged: true,
    english_locale_restored: true,
    ok: true
  }' | tee "${out_dir}/summary.json"

success=1
