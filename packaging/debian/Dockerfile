ARG BASE_IMAGE=debian:12
FROM ${BASE_IMAGE}

ARG APT_MIRROR=""
ARG APT_SECURITY_MIRROR=""
ENV DEBIAN_FRONTEND=noninteractive
RUN set -eux; \
    if [ -n "${APT_MIRROR}" ]; then \
      find /etc/apt -type f \( -name sources.list -o -name '*.sources' \) \
        -exec sed -i \
          -e "s|https\?://deb.debian.org/debian|${APT_MIRROR}|g" \
          -e "s|https\?://archive.ubuntu.com/ubuntu|${APT_MIRROR}|g" \
          {} +; \
    fi; \
    if [ -n "${APT_SECURITY_MIRROR}" ]; then \
      find /etc/apt -type f \( -name sources.list -o -name '*.sources' \) \
        -exec sed -i \
          -e "s|https\?://deb.debian.org/debian-security|${APT_SECURITY_MIRROR}|g" \
          -e "s|https\?://security.debian.org/debian-security|${APT_SECURITY_MIRROR}|g" \
          -e "s|https\?://security.ubuntu.com/ubuntu|${APT_SECURITY_MIRROR}|g" \
          {} +; \
    fi; \
    packages=' \
      bash \
      binutils \
      bzip2 \
      ca-certificates \
      clang \
      cmake \
      coreutils \
      curl \
      dbus \
      dbus-user-session \
      dpkg-dev \
      file \
      fcitx5 \
      fcitx5-modules-dev \
      g++ \
      gettext \
      git \
      jq \
      libfcitx5core-dev \
      libfontconfig1-dev \
      libglib2.0-bin \
      libpipewire-0.3-dev \
      libsystemd-dev \
      libwayland-dev \
      libx11-dev \
      libxkbcommon-dev \
      ninja-build \
      patchelf \
      pkg-config \
      python3 \
      procps \
      systemd \
      tar \
      util-linux \
      util-linux-extra \
      xz-utils'; \
    apt_options='-o Acquire::Retries=10 -o Acquire::http::Timeout=60 -o Acquire::https::Timeout=60'; \
    apt-get ${apt_options} update; \
    downloaded=0; \
    for attempt in 1 2 3 4 5; do \
      if apt-get ${apt_options} install -y --download-only --no-install-recommends ${packages}; then \
        downloaded=1; \
        break; \
      fi; \
      sleep "$((attempt * 5))"; \
      apt-get ${apt_options} update; \
    done; \
    test "${downloaded}" = 1; \
    apt-get install -y --no-install-recommends ${packages}; \
    rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/*.deb

ARG RUSTUP_DIST_SERVER=""
ARG RUSTUP_UPDATE_ROOT=""
RUN mkdir -p /opt/cargo /opt/rustup \
    && if [ -n "${RUSTUP_DIST_SERVER}" ]; then export RUSTUP_DIST_SERVER; fi \
    && if [ -n "${RUSTUP_UPDATE_ROOT}" ]; then export RUSTUP_UPDATE_ROOT; fi \
    && rustup_tmpdir="$(mktemp -d)" \
    && rustup_installer="${rustup_tmpdir}/rustup-init" \
    && rustup_update_root="${RUSTUP_UPDATE_ROOT:-https://static.rust-lang.org/rustup}" \
    && rustup_target="$(uname -m)-unknown-linux-gnu" \
    && curl --retry 5 --retry-all-errors --connect-timeout 30 \
      --proto '=https' --tlsv1.2 -fsSL \
      -o "${rustup_installer}" \
      "${rustup_update_root}/dist/${rustup_target}/rustup-init" \
    && chmod +x "${rustup_installer}" \
    && CARGO_HOME=/opt/cargo RUSTUP_HOME=/opt/rustup \
      "${rustup_installer}" -y --profile minimal --default-toolchain stable \
    && rm -rf "${rustup_tmpdir}" \
    && chmod -R a+rX /opt/cargo /opt/rustup

ENV PATH=/opt/cargo/bin:${PATH}
ENV RUSTUP_HOME=/opt/rustup
WORKDIR /workspace
