#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

just addon-build
cargo build -q -p vinput-daemon

smoke_dir="${repo_root}/target/tmp/vinput-cpp-dbus-asr-menu-smoke"
config_path="${smoke_dir}/config.json"
model_root="${smoke_dir}/models"
model_dir="${model_root}/installed-one"
log_file="${smoke_dir}/daemon.log"
rm -rf "${smoke_dir}"
mkdir -p "${model_dir}"
cat >"${model_dir}/vinput-model.json" <<'JSON'
{
  "backend": "sherpa-offline",
  "family": "moonshine"
}
JSON
printf 'tokens\n' >"${model_dir}/tokens.txt"
python3 - "${config_path}" <<'PY'
import json
import pathlib
import sys

output = pathlib.Path(sys.argv[1])
config = json.loads(pathlib.Path("data/default-config.json").read_text())
config["asr"]["providers"].append(
    {
        "id": "mock",
        "type": "local",
        "model": "mock-model",
    }
)
output.write_text(json.dumps(config, indent=2) + "\n")
PY

dbus-run-session -- bash -euo pipefail <<INNER
config_path="${config_path}"
log_file="${log_file}"
model_root="${model_root}"
model_dir="${model_dir}"
smoke_bin="target/cpp/fcitx5-addon/vinput_fcitx_bridge_dbus_smoke"

target/debug/vinput-daemon --dbus --config "\${config_path}" \
  --model-root "\${model_root}" >"\${log_file}" 2>&1 &
daemon_pid=\$!
cleanup() {
  kill "\${daemon_pid}" >/dev/null 2>&1 || true
  wait "\${daemon_pid}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

for _ in \$(seq 1 50); do
  if VINPUT_DBUS_SMOKE_SWITCH_ASR_PROVIDER=mock \
       VINPUT_DBUS_SMOKE_EXPECT_SCENE_PERSISTED=1 \
       VINPUT_DBUS_SMOKE_EXPECT_ASR_PERSISTED=1 \
       VINPUT_DBUS_SMOKE_SWITCH_ASR_TARGET_PROVIDER=mock \
       VINPUT_DBUS_SMOKE_SWITCH_ASR_TARGET_MODEL="\${model_dir}" \
       VINPUT_DBUS_SMOKE_EXPECT_ASR_TARGET_PERSISTED=1 \
       "\${smoke_bin}"; then
    python3 - "\${config_path}" "\${model_dir}" <<'PY'
import json
import pathlib
import sys

config = json.loads(pathlib.Path(sys.argv[1]).read_text())
assert config["asr"]["active_provider"] == "mock", config["asr"]
mock = next(provider for provider in config["asr"]["providers"] if provider["id"] == "mock")
assert mock["model"] == sys.argv[2], mock
PY
    exit 0
  fi
  if ! kill -0 "\${daemon_pid}" >/dev/null 2>&1; then
    cat "\${log_file}" >&2
    exit 1
  fi
  sleep 0.1
done

cat "\${log_file}" >&2
exit 1
INNER

echo "C++ ASR provider and installed-model switch smoke passed"
