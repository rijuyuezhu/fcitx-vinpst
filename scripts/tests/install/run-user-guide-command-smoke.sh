#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
cd "${repo_root}"

cargo build -q -p vinpst-cli -p vinpst-daemon

root="${VINPST_USER_GUIDE_SMOKE_DIR:-target/tmp/user-guide-command-smoke}"
rm -rf "${root}"
mkdir -p "${root}/home"

export HOME="${repo_root}/${root}/home"
export XDG_CONFIG_HOME="${repo_root}/${root}/config"
export XDG_DATA_HOME="${repo_root}/${root}/data"
export XDG_CACHE_HOME="${repo_root}/${root}/cache"

cli="${repo_root}/target/debug/vinpst"
daemon="${repo_root}/target/debug/vinpst-daemon"
config="${XDG_CONFIG_HOME}/fcitx-vinpst/config.json"
registry="${repo_root}/crates/vinpst-registry/tests/fixtures/live-models-sensevoice.json"
model_id=onnx-sv-zh-int8-off

"${cli}" init --dry-run --json >"${root}/init-dry-run.json"
jq -e '
  .ok == true and
  .dry_run == true and
  .config.wrote == false and
  .directories.model_root.created == false and
  .directories.cache_root.created == false
' "${root}/init-dry-run.json" >/dev/null

"${cli}" init --json >"${root}/init.json"
jq -e --arg config "${config}" '
  .ok == true and
  .dry_run == false and
  .config.wrote == true and
  .config.path == $config and
  .directories.model_root.created == true and
  .directories.cache_root.created == true
' "${root}/init.json" >/dev/null

test -f "${config}"
"${cli}" config validate "${config}" --summary-only --json >"${root}/config.json"
jq -e '.ok == true and .version == 1 and .active_provider == "sherpa-onnx"' \
  "${root}/config.json" >/dev/null

"${cli}" model list --available --registry "${registry}" --json >"${root}/models.json"
jq -e --arg id "${model_id}" '
  .ok == true and any(.models[]; .short_id == $id or .id == $id)
' "${root}/models.json" >/dev/null

"${cli}" model install "${model_id}" \
  --registry "${registry}" --model-root "${XDG_DATA_HOME}/fcitx-vinpst/models" \
  --staging-root "${XDG_CACHE_HOME}/fcitx-vinpst/model-install" \
  --dry-run --json >"${root}/model-install.json"
jq -e --arg id "${model_id}" '
  .ok == true and .dry_run == true and
  (.model.short_id == $id or .model.id == $id)
' "${root}/model-install.json" >/dev/null

"${cli}" model use "${model_id}" \
  --registry "${registry}" --config "${config}" \
  --model-root "${XDG_DATA_HOME}/fcitx-vinpst/models" \
  --dry-run --json >"${root}/model-use.json"
jq -e '
  .ok == true and .dry_run == true and
  .reload_daemon.requested == false and
  .reload_daemon.called == false
' "${root}/model-use.json" >/dev/null

"${cli}" device use default --config "${config}" --dry-run --json \
  >"${root}/device-use.json"
jq -e '
  .ok == true and .dry_run == true and
  .after == "default" and .will_write_config == false
' "${root}/device-use.json" >/dev/null

"${cli}" doctor --config "${config}" --json >"${root}/doctor.json"
jq -e '
  .ok == true and
  .config.ok == true and
  .audio.ok == true and
  .activation_service.user_service_exists == false
' "${root}/doctor.json" >/dev/null

"${cli}" activation-service \
  --daemon "${daemon}" --config "${config}" --configured-backends \
  --audio-backend pipewire --output "${root}/org.fcitx.Vinpst.service"
grep -qx 'Name=org.fcitx.Vinpst' "${root}/org.fcitx.Vinpst.service"
grep -Fq "Exec=${daemon} --dbus" "${root}/org.fcitx.Vinpst.service"
grep -Fq -- "--configured-backends" "${root}/org.fcitx.Vinpst.service"
grep -Fq -- "--config ${config}" "${root}/org.fcitx.Vinpst.service"
grep -Fq -- "--audio-backend pipewire" "${root}/org.fcitx.Vinpst.service"
grep -Fq -- "--exit-when-executable-replaced" "${root}/org.fcitx.Vinpst.service"

"${cli}" daemon status --dry-run --json >"${root}/daemon-status.json"
jq -e '.ok == true and .dry_run == true and .will_call_dbus == false' \
  "${root}/daemon-status.json" >/dev/null

"${cli}" daemon handoff --dry-run --json >"${root}/daemon-handoff.json"
jq -e '
  .ok == true and .dry_run == true and
  .will_call_dbus == false and
  .will_mutate_user_service == false and
  .will_signal_owner == false
' "${root}/daemon-handoff.json" >/dev/null

"${cli}" daemon prepare-remove --dry-run --json >"${root}/daemon-remove.json"
jq -e '
  .ok == true and .dry_run == true and
  .will_call_dbus == false and
  .will_mutate_user_service == false and
  .will_signal_owner == false
' "${root}/daemon-remove.json" >/dev/null

"${cli}" daemon log --lines 100 --dry-run --json >"${root}/daemon-log.json"
jq -e '
  .ok == true and .dry_run == true and
  .command_argv[-2:] == ["-n", "100"]
' "${root}/daemon-log.json" >/dev/null

echo "external user guide command smoke passed"
