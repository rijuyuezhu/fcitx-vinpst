# shellcheck shell=bash

vinpst_network_require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'required command not found: %s\n' "$1" >&2
    return 1
  }
}

vinpst_network_find_chromium() {
  local override="${1:-}"
  local candidate

  if [[ -n "${override}" ]]; then
    [[ -x "${override}" ]] || {
      echo "configured Chromium executable is not executable: ${override}" >&2
      return 1
    }
    printf '%s\n' "${override}"
    return 0
  fi
  for candidate in \
    google-chrome-unstable \
    google-chrome-stable \
    google-chrome \
    chromium \
    chromium-browser; do
    if command -v "${candidate}" >/dev/null 2>&1; then
      command -v "${candidate}"
      return 0
    fi
  done
  echo "a Chromium-family browser is required" >&2
  return 1
}

vinpst_network_select_lan_ipv4() {
  local override="${1:-}"
  local address="${override}"

  if [[ -z "${address}" ]]; then
    address="$(
      ip -j -4 route get 1.1.1.1 2>/dev/null |
        jq -r '.[0].prefsrc // .[0].src // empty' || true
    )"
  fi
  if [[ -z "${address}" ]]; then
    address="$(
      ip -j -4 address show up scope global |
        jq -r '[.[]?.addr_info[]? | select(.scope == "global") | .local][0] // empty'
    )"
  fi
  if [[ -z "${address}" || "${address}" == 127.* ]]; then
    echo "an operational non-loopback IPv4 address is required" >&2
    return 1
  fi
  if ! ip -j -4 address show up scope global |
    jq -e --arg address "${address}" \
      'any(.[]?.addr_info[]?; .local == $address)' >/dev/null; then
    echo "selected LAN address is not assigned to an up interface: ${address}" >&2
    return 1
  fi
  printf '%s\n' "${address}"
}

vinpst_network_reserve_ports() {
  local count="$1"

  python3 - "${count}" <<'PY'
import socket
import sys

count = int(sys.argv[1])
ports = []
while len(ports) < count:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    if port not in ports:
        ports.append(port)
print(*ports)
PY
}

vinpst_network_random_token() {
  python3 - <<'PY'
import secrets
print(secrets.token_urlsafe(32))
PY
}

vinpst_remote_text_write_config() {
  local config_path="$1"
  local port="$2"
  local api_key="$3"
  local debounce_ms="$4"

  jq -n \
    --arg key "${api_key}" \
    --arg port "${port}" \
    --arg debounce_ms "${debounce_ms}" \
    '{
      version: 1,
      asr: {
        active_provider: "provider.vinpst.remote.streaming",
        providers: [{
          id: "provider.vinpst.remote.streaming",
          type: "command",
          command: "python3",
          args: ["unused-remote-text-provider.py"],
          env: {
            VINPST_ASR_API_KEY: $key,
            VINPST_ASR_PORT: $port,
            VINPST_ASR_DEBOUNCE_MS: $debounce_ms
          }
        }]
      },
      scenes: {
        active_scene: "__raw__",
        definitions: [
          {id: "__raw__", label: "Raw", candidate_count: 0},
          {id: "__command__", label: "Command", candidate_count: 1}
        ]
      }
    }' >"${config_path}"
}

vinpst_remote_text_wait_health() {
  local server_pid="$1"
  local health_url="$2"
  local server_log="$3"
  local output_path="$4"

  for _ in $(seq 1 100); do
    if ! kill -0 "${server_pid}" 2>/dev/null; then
      cat "${server_log}" >&2
      echo "remote text server exited before health became ready" >&2
      return 1
    fi
    if curl --noproxy '*' --silent --show-error --fail \
      --max-time 1 "${health_url}" >"${output_path}"; then
      jq -e '.ok == true' "${output_path}" >/dev/null
      return 0
    fi
    sleep 0.05
  done
  echo "remote text server health check timed out: ${health_url}" >&2
  return 1
}

vinpst_network_require_listener_released() {
  local port="$1"

  if ss -Hln "( sport = :${port} )" | grep -q .; then
    echo "remote text listener remained after shutdown: ${port}" >&2
    return 1
  fi
}
