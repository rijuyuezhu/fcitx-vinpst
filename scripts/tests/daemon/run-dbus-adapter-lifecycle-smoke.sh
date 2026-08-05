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

stage_dir="target/tmp/vinpst-dbus-adapter-lifecycle-smoke"
config_path="${repo_root}/${stage_dir}/config.json"
wav_path="${repo_root}/${stage_dir}/demo.wav"
log_file="${repo_root}/${stage_dir}/daemon.log"
rm -rf "${stage_dir}"
mkdir -p "${stage_dir}"
install -Dm644 data/e2e-adapter-lifecycle-config.json "${config_path}"
python3 scripts/fixtures/write-demo-wav.py "${wav_path}"
cargo build -q -p vinpst-daemon -p vinpst-cli

timeout 20s dbus-run-session -- bash -euo pipefail <<INNER
config_path="${config_path}"
wav_path="${wav_path}"
log_file="${log_file}"
daemon_bin="${repo_root}/target/debug/vinpst-daemon"
cli_bin="${repo_root}/target/debug/vinpst"
stage_dir="${repo_root}/${stage_dir}"

"\${daemon_bin}" --dbus --configured-backends --config "\${config_path}" \
  --wav "\${wav_path}" >"\${log_file}" 2>&1 &
daemon_pid=\$!
cleanup() {
  kill "\${daemon_pid}" >/dev/null 2>&1 || true
  wait "\${daemon_pid}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

ready=0
for _ in \$(seq 1 50); do
  if "\${cli_bin}" adapter status lifecycle-adapter --config "\${config_path}" \
      --json >"\${stage_dir}/initial.json" 2>"\${stage_dir}/initial.err"; then
    ready=1
    break
  fi
  if ! kill -0 "\${daemon_pid}" >/dev/null 2>&1; then
    cat "\${log_file}" >&2
    exit 1
  fi
  sleep 0.1
done
if ((ready == 0)); then
  cat "\${stage_dir}/initial.err" >&2
  cat "\${log_file}" >&2
  exit 1
fi

python3 - "\${stage_dir}/initial.json" <<'PY'
import json
import pathlib
import sys

value = json.loads(pathlib.Path(sys.argv[1]).read_text())
assert value["ok"] is True, value
assert value["adapter_id"] == "lifecycle-adapter", value
assert value["adapter"]["is_running"] is False, value
assert value["adapter"]["pid"] is None, value
PY

"\${cli_bin}" adapter start lifecycle-adapter --config "\${config_path}" \
  --json >"\${stage_dir}/start.json"
python3 - "\${stage_dir}/start.json" <<'PY'
import json
import pathlib
import sys

value = json.loads(pathlib.Path(sys.argv[1]).read_text())
assert value["ok"] is True, value
assert value["action"] == "start", value
assert value["adapter_id"] == "lifecycle-adapter", value
assert value["called"] is True, value
PY

"\${cli_bin}" adapter status lifecycle-adapter --config "\${config_path}" \
  --json >"\${stage_dir}/running.json"
python3 - "\${stage_dir}/running.json" <<'PY'
import json
import pathlib
import sys

value = json.loads(pathlib.Path(sys.argv[1]).read_text())
assert value["adapter"]["is_running"] is True, value
assert isinstance(value["adapter"]["pid"], int), value
assert value["adapter"]["pid"] > 0, value
PY

if "\${cli_bin}" adapter start lifecycle-adapter --config "\${config_path}" \
    --json >"\${stage_dir}/duplicate.json" 2>"\${stage_dir}/duplicate.err"; then
  echo "duplicate adapter start unexpectedly succeeded" >&2
  exit 1
fi
grep -q "already running" "\${stage_dir}/duplicate.err"

"\${cli_bin}" adapter stop lifecycle-adapter --config "\${config_path}" \
  --json >"\${stage_dir}/stop.json"
"\${cli_bin}" adapter status lifecycle-adapter --config "\${config_path}" \
  --json >"\${stage_dir}/stopped.json"
python3 - "\${stage_dir}/stop.json" "\${stage_dir}/stopped.json" <<'PY'
import json
import pathlib
import sys

stop = json.loads(pathlib.Path(sys.argv[1]).read_text())
state = json.loads(pathlib.Path(sys.argv[2]).read_text())
assert stop["ok"] is True, stop
assert stop["action"] == "stop", stop
assert stop["called"] is True, stop
assert state["adapter"]["is_running"] is False, state
assert state["adapter"]["pid"] is None, state
PY
INNER

echo "Rust CLI D-Bus adapter lifecycle smoke passed"
