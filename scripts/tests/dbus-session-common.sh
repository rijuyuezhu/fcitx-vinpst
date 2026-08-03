#!/usr/bin/env bash

write_isolated_dbus_session_config() {
  local output_path="$1"
  local service_dir="$2"

  if [[ "${service_dir}" != /* ]]; then
    echo "D-Bus service directory must be absolute: ${service_dir}" >&2
    return 1
  fi

  cat >"${output_path}" <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
  "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <keep_umask/>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <servicedir>${service_dir}</servicedir>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF
}
