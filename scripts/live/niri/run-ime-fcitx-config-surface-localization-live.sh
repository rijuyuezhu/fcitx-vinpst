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

out_dir="${VINPUT_LIVE_CONFIG_SURFACE_OUT_DIR:-target/tmp/ime-fcitx-config-surface-localization-live}"
addon_build_dir="${VINPUT_LIVE_CONFIG_SURFACE_ADDON_BUILD_DIR:-target/cpp/fcitx5-config-surface-localization-live}"
configtool_source_dir="${VINPUT_LIVE_CONFIGTOOL_SOURCE_DIR:-target/tmp/fcitx5-configtool-5.1.14}"
configtool_build_dir="${VINPUT_LIVE_CONFIGTOOL_BUILD_DIR:-target/tmp/fcitx5-configtool-config-surface-build}"
ecm_root="${VINPUT_LIVE_CONFIGTOOL_ECM_ROOT:-target/tmp/fcitx-config-surface-ecm}"
configtool_tag="5.1.14"
configtool_commit="691c73e08844127ce74a4348776ee9596d7e7ec3"
configtool_url="https://github.com/fcitx/fcitx5-configtool.git"
config_uri="fcitx://config/addon/vinput"
probe_source="${repo_root}/scripts/live/niri/probes/fcitx-config-surface-probe.cpp"

module_path="${HOME}/.local/lib/fcitx5/fcitx5-vinput.so"
catalog_path="${HOME}/.local/share/locale/zh_CN/LC_MESSAGES/fcitx5-vinput.mo"
fcitx_wrapper="${HOME}/.local/share/fcitx-vinput/fcitx5-with-vinput-env.sh"
profile_path="${HOME}/.local/share/fcitx-vinput/sherpa-native-command-live.json"
service_path="${HOME}/.local/share/dbus-1/services/org.fcitx.Vinput.service"
addon_config="${HOME}/.config/fcitx5/conf/vinput.conf"
addon_metadata="${HOME}/.local/share/fcitx5/addon/vinput.conf"
fcitx_env="${HOME}/.local/share/fcitx-vinput/fcitx-vinput.env"
cli_binary="${repo_root}/target/debug/vinput"
daemon_path="${HOME}/.local/bin/vinput-daemon"
fcitx_settle_seconds="${VINPUT_LIVE_FCITX_SETTLE_SECONDS:-1}"

candidate_module="${addon_build_dir}/fcitx5-vinput.so"
candidate_catalog="${addon_build_dir}/locale/zh_CN/LC_MESSAGES/fcitx5-vinput.mo"
probe_binary="${configtool_build_dir}/bin/vinput-config-surface-probe"

locale_may_have_changed=0
candidate_installed=0
ecm_dir=""
ecm_package_url=""
ecm_package_sha256=""

for command in \
  bash bsdtar cmake cmp curl fcitx5-remote gdbus git install jq msgfmt pacman \
  pgrep python3 sha256sum; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required configuration-surface command is missing: ${command}" >&2
    exit 2
  fi
done
for required in \
  "${probe_source}" \
  "${module_path}" \
  "${catalog_path}" \
  "${fcitx_wrapper}" \
  "${profile_path}" \
  "${service_path}" \
  "${addon_config}" \
  "${addon_metadata}" \
  "${fcitx_env}" \
  "${cli_binary}" \
  "${daemon_path}"; do
  if [[ ! -e "${required}" ]]; then
    echo "required configuration-surface path is missing: ${required}" >&2
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

prepare_configtool_source() {
  local current_commit=""
  if [[ -d "${configtool_source_dir}/.git" ]]; then
    current_commit="$(git -C "${configtool_source_dir}" rev-parse HEAD 2>/dev/null || true)"
  fi
  if [[ "${current_commit}" != "${configtool_commit}" ]]; then
    rm -rf "${configtool_source_dir}"
    git clone --filter=blob:none --depth 1 --branch "${configtool_tag}" \
      "${configtool_url}" "${configtool_source_dir}"
  fi
  if [[ "$(git -C "${configtool_source_dir}" rev-parse HEAD)" != "${configtool_commit}" ]]; then
    echo "fcitx5-configtool checkout did not match the pinned commit" >&2
    exit 1
  fi
  git -C "${configtool_source_dir}" reset --hard "${configtool_commit}" >/dev/null
  git -C "${configtool_source_dir}" clean -fdx >/dev/null
  install -m 0644 "${probe_source}" \
    "${configtool_source_dir}/vinput-config-surface-probe.cpp"
  cat >>"${configtool_source_dir}/CMakeLists.txt" <<'EOF'

add_executable(vinput-config-surface-probe vinput-config-surface-probe.cpp)
set_target_properties(vinput-config-surface-probe PROPERTIES AUTOMOC TRUE)
target_link_libraries(vinput-config-surface-probe PRIVATE
    Qt${QT_MAJOR_VERSION}::Widgets
    configwidgetslib
    configlib)
target_compile_options(vinput-config-surface-probe PRIVATE -Wall -Wextra -Werror)
EOF
}

prepare_ecm() {
  local package_path="${ecm_root}/extra-cmake-modules.pkg.tar.zst"
  if [[ -f /usr/share/ECM/cmake/ECMConfig.cmake ]]; then
    ecm_dir="/usr/share/ECM/cmake"
    return 0
  fi
  if [[ ! -f "${ecm_root}/usr/share/ECM/cmake/ECMConfig.cmake" ]]; then
    rm -rf "${ecm_root}"
    mkdir -p "${ecm_root}"
    ecm_package_url="$(pacman -Sp --print-format '%l' extra-cmake-modules | head -n 1)"
    if [[ -z "${ecm_package_url}" ]]; then
      echo "could not resolve the extra-cmake-modules package URL" >&2
      exit 1
    fi
    curl -fL "${ecm_package_url}" -o "${package_path}"
    ecm_package_sha256="$(sha256sum "${package_path}" | awk '{print $1}')"
    bsdtar -xf "${package_path}" -C "${ecm_root}"
  elif [[ -f "${package_path}" ]]; then
    ecm_package_sha256="$(sha256sum "${package_path}" | awk '{print $1}')"
  fi
  ecm_dir="${repo_root}/${ecm_root}/usr/share/ECM/cmake"
  if [[ ! -f "${ecm_dir}/ECMConfig.cmake" ]]; then
    echo "local extra-cmake-modules extraction is incomplete: ${ecm_dir}" >&2
    exit 1
  fi
}

build_probe() {
  rm -rf "${configtool_build_dir}"
  cmake -S "${configtool_source_dir}" -B "${configtool_build_dir}" \
    -DECM_DIR="${ecm_dir}" \
    -DCMAKE_BUILD_TYPE=Debug \
    -DQT_MAJOR_VERSION=6 \
    -DENABLE_KCM=OFF \
    -DENABLE_CONFIG_QT=ON \
    -DENABLE_TEST=OFF
  cmake --build "${configtool_build_dir}" \
    --target vinput-config-surface-probe --parallel
  test -x "${probe_binary}"
}

build_candidate() {
  rm -rf "${addon_build_dir}"
  cmake -S cpp/fcitx5-addon -B "${addon_build_dir}" \
    -DCMAKE_BUILD_TYPE=Debug \
    -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
    -DVINPUT_DAEMON_EXECUTABLE="${daemon_path}" \
    -DVINPUT_FCITX_RUNTIME_BUILD_LOCALEDIR= \
    -DVINPUT_FCITX_RUNTIME_INSTALL_LOCALEDIR="${HOME}/.local/share/locale"
  cmake --build "${addon_build_dir}" --parallel
  ctest --test-dir "${addon_build_dir}" \
    -R 'vinput_fcitx_bridge_(config|i18n)_smoke' --output-on-failure
  msgfmt --check -o /dev/null cpp/fcitx5-addon/po/zh_CN.po
  test -f "${candidate_module}"
  test -f "${candidate_catalog}"
}

prepare_configtool_source
prepare_ecm
build_probe
build_candidate

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
  echo "daemon must be idle with a healthy backend before configuration-surface proof" >&2
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
  echo "Fcitx did not exit before configuration-surface restoration: ${pid}" >&2
  return 1
}

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
  echo "Fcitx did not restart with the expected configuration-surface locale" >&2
  cat "${expected_path}" >&2 2>/dev/null || true
  cat "${current_path}" >&2 2>/dev/null || true
  return 1
}

install_candidate() {
  stop_fcitx
  install -m 0755 "${candidate_module}" "${module_path}"
  install -m 0644 "${candidate_catalog}" "${catalog_path}"
  install -m 0644 "${out_dir}/addon-config-before.conf" "${addon_config}"
  candidate_installed=1
}

start_english() {
  local previous_pid
  previous_pid="$(pgrep -n -x fcitx5 || true)"
  stop_fcitx
  install -m 0644 "${out_dir}/addon-config-before.conf" "${addon_config}"
  cat >"${out_dir}/locale-english.expected" <<'EOF'
LANG=en_US.UTF-8
LC_MESSAGES=en_US.UTF-8
EOF
  env -u LANGUAGE -u LC_ALL \
    LC_MESSAGES=en_US.UTF-8 LANG=en_US.UTF-8 \
    "${fcitx_wrapper}" -d >/dev/null 2>&1 &
  wait_fcitx_locale "${previous_pid}" "${out_dir}/locale-english.expected" \
    "${out_dir}/fcitx-english.environ"
}

start_zh_cn() {
  local previous_pid
  previous_pid="$(pgrep -n -x fcitx5 || true)"
  stop_fcitx
  install -m 0644 "${out_dir}/addon-config-before.conf" "${addon_config}"
  cat >"${out_dir}/locale-zh-cn.expected" <<'EOF'
LANG=en_US.UTF-8
LANGUAGE=zh_CN:zh
LC_MESSAGES=en_US.UTF-8
EOF
  env -u LC_ALL \
    LANGUAGE=zh_CN:zh LC_MESSAGES=en_US.UTF-8 LANG=en_US.UTF-8 \
    "${fcitx_wrapper}" -d >/dev/null 2>&1 &
  wait_fcitx_locale "${previous_pid}" "${out_dir}/locale-zh-cn.expected" \
    "${out_dir}/fcitx-zh-cn.environ"
}

start_original() {
  local previous_pid name value
  local -a command=(env -u LANGUAGE -u LC_ALL -u LC_MESSAGES -u LANG)
  previous_pid="$(pgrep -n -x fcitx5 || true)"
  stop_fcitx
  install -m 0755 "${out_dir}/module-before.so" "${module_path}"
  install -m 0644 "${out_dir}/catalog-before.mo" "${catalog_path}"
  install -m 0644 "${out_dir}/addon-config-before.conf" "${addon_config}"
  candidate_installed=0
  while IFS='=' read -r name value; do
    [[ -z "${name}" ]] && continue
    command+=("${name}=${value}")
  done <"${out_dir}/locale-before.env"
  command+=("${fcitx_wrapper}" -d)
  "${command[@]}" >/dev/null 2>&1 &
  wait_fcitx_locale "${previous_pid}" "${out_dir}/locale-before.env" \
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
  if [[ "${locale_may_have_changed}" == 1 || "${candidate_installed}" == 1 ]]; then
    start_original >"${out_dir}/fcitx-cleanup.pid" || exit_code=1
    locale_may_have_changed=0
  fi
  verify_files || exit_code=1
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

install_candidate
locale_may_have_changed=1
start_english | tee "${out_dir}/fcitx-english.pid"
gdbus call --session \
  --dest org.fcitx.Fcitx5 \
  --object-path /controller \
  --method org.fcitx.Fcitx.Controller1.GetConfig "${config_uri}" \
  >"${out_dir}/get-config-english.gdbus"
env -u LANGUAGE -u LC_ALL \
  LC_MESSAGES=en_US.UTF-8 LANG=en_US.UTF-8 QT_QPA_PLATFORM=offscreen \
  "${probe_binary}" "${config_uri}" | tee "${out_dir}/form-english.jsonl"

start_zh_cn | tee "${out_dir}/fcitx-zh-cn.pid"
gdbus call --session \
  --dest org.fcitx.Fcitx5 \
  --object-path /controller \
  --method org.fcitx.Fcitx.Controller1.GetConfig "${config_uri}" \
  >"${out_dir}/get-config-zh-cn.gdbus"
env -u LC_ALL \
  LANGUAGE=zh_CN:zh LC_MESSAGES=en_US.UTF-8 LANG=en_US.UTF-8 \
  QT_QPA_PLATFORM=offscreen \
  "${probe_binary}" "${config_uri}" | tee "${out_dir}/form-zh-cn.jsonl"

python3 - \
  "${out_dir}/form-english.jsonl" \
  "${out_dir}/form-zh-cn.jsonl" \
  "${out_dir}/form-summary.json" <<'PY'
import json
import sys
from pathlib import Path

english_path = Path(sys.argv[1])
zh_path = Path(sys.argv[2])
out_path = Path(sys.argv[3])


def summary(path: Path) -> dict:
    events = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    matches = [event for event in events if event.get("event") == "summary"]
    if len(matches) != 1:
        raise SystemExit(f"expected one form summary in {path}, found {len(matches)}")
    return matches[0]


def labels(form: dict) -> list[str]:
    return [
        widget["text"]
        for widget in form["widgets"]
        if widget.get("class") == "QLabel" and widget.get("text")
    ]


def combos(form: dict) -> list[dict]:
    return [widget for widget in form["widgets"] if widget.get("class") == "QComboBox"]

english = summary(english_path)
zh = summary(zh_path)
expected_english_labels = [
    "Normal Dictation Keys:",
    "Command Dictation Keys:",
    "Scene Menu Keys:",
    "ASR Menu Keys:",
    "Previous Page Keys:",
    "Next Page Keys:",
    "Trigger Mode:",
]
expected_zh_labels = [
    "普通听写快捷键：",
    "命令听写快捷键：",
    "场景菜单快捷键：",
    "语音识别菜单快捷键：",
    "上一页快捷键：",
    "下一页快捷键：",
    "触发模式：",
]
failures = []
for name, form in (("English", english), ("zh_CN", zh)):
    if form.get("ok") is not True:
        failures.append(f"{name} official form did not report ok")
    if form.get("changed") is not False:
        failures.append(f"{name} official form changed configuration")
    if form.get("save_called") is not False:
        failures.append(f"{name} official form called save")
    if form.get("widget_count", 0) < 70:
        failures.append(f"{name} official form did not construct the full widget tree")
if labels(english) != expected_english_labels:
    failures.append("English configuration labels did not match the official form")
if labels(zh) != expected_zh_labels:
    failures.append("zh_CN configuration labels did not match the official form")
english_combos = combos(english)
zh_combos = combos(zh)
if len(english_combos) != 1 or english_combos[0].get("items") != ["Tap", "Hold", "Both"]:
    failures.append("English trigger-mode choices were not Tap/Hold/Both")
if len(zh_combos) != 1 or zh_combos[0].get("items") != ["单击", "长按", "两者"]:
    failures.append("zh_CN trigger-mode choices were not 单击/长按/两者")
if zh_combos and zh_combos[0].get("text") != "两者":
    failures.append("zh_CN current trigger mode was not 两者")
result = {
    "event": "form-summary",
    "official_configwidget": True,
    "config_uri": "fcitx://config/addon/vinput",
    "english": {
        "labels": labels(english),
        "trigger_mode": english_combos[0] if english_combos else None,
        "widget_count": english.get("widget_count"),
        "changed": english.get("changed"),
        "save_called": english.get("save_called"),
    },
    "zh_cn": {
        "labels": labels(zh),
        "trigger_mode": zh_combos[0] if zh_combos else None,
        "widget_count": zh.get("widget_count"),
        "changed": zh.get("changed"),
        "save_called": zh.get("save_called"),
    },
    "ok": not failures,
    "failures": failures,
}
out_path.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(result, ensure_ascii=False, indent=2))
if failures:
    raise SystemExit("; ".join(failures))
PY

start_original | tee "${out_dir}/fcitx-restored.pid"
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
  echo "configuration-surface gate changed the final daemon backend" >&2
  exit 1
fi
cmp "${out_dir}/locale-before.env" <(locale_env "$(pgrep -n -x fcitx5)")

configtool_source_sha256="$(sha256sum "${probe_source}" | awk '{print $1}')"
candidate_module_sha256="$(sha256sum "${candidate_module}" | awk '{print $1}')"
candidate_catalog_sha256="$(sha256sum "${candidate_catalog}" | awk '{print $1}')"
installed_before_module_sha256="$(sha256sum "${out_dir}/module-before.so" | awk '{print $1}')"
installed_before_catalog_sha256="$(sha256sum "${out_dir}/catalog-before.mo" | awk '{print $1}')"

jq -n \
  --slurpfile form "${out_dir}/form-summary.json" \
  --rawfile original_locale "${out_dir}/locale-before.env" \
  --arg configtool_tag "${configtool_tag}" \
  --arg configtool_commit "${configtool_commit}" \
  --arg configtool_url "${configtool_url}" \
  --arg probe_sha256 "${configtool_source_sha256}" \
  --arg ecm_package_url "${ecm_package_url}" \
  --arg ecm_package_sha256 "${ecm_package_sha256}" \
  --arg candidate_module_sha256 "${candidate_module_sha256}" \
  --arg candidate_catalog_sha256 "${candidate_catalog_sha256}" \
  --arg installed_before_module_sha256 "${installed_before_module_sha256}" \
  --arg installed_before_catalog_sha256 "${installed_before_catalog_sha256}" '
  {
    event: "summary",
    official_fcitx_configtool: true,
    configtool: {
      tag: $configtool_tag,
      commit: $configtool_commit,
      source: $configtool_url,
      probe_sha256: $probe_sha256,
      ecm_package_url: $ecm_package_url,
      ecm_package_sha256: $ecm_package_sha256
    },
    form: $form[0],
    candidate: {
      module_sha256: $candidate_module_sha256,
      catalog_sha256: $candidate_catalog_sha256
    },
    restored_installation: {
      module_sha256: $installed_before_module_sha256,
      catalog_sha256: $installed_before_catalog_sha256
    },
    original_locale: ($original_locale | split("\n") | map(select(length > 0))),
    offscreen_qt_widgets: true,
    save_called: false,
    profile_unchanged: true,
    service_unchanged: true,
    addon_config_unchanged: true,
    addon_metadata_unchanged: true,
    module_restored: true,
    catalog_restored: true,
    fcitx_env_unchanged: true,
    backend_unchanged: true,
    original_locale_restored: true,
    ok: $form[0].ok
  }' | tee "${out_dir}/summary.json"
