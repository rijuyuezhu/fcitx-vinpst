set dotenv-load := false

# Format Rust and the retained Fcitx C++ boundary.
fmt:
    scripts/tests/format.sh

# Verify formatting without changing files.
fmt-check:
    scripts/tests/format.sh --check

# Run Rust, C++, shell, and Python static analysis.
lint:
    scripts/tests/scripts-lint.sh
    scripts/tests/lint.sh

# Run workspace, D-Bus integration, and retained-addon tests.
test:
    scripts/tests/test.sh

# Full deterministic development gate.
check:
    scripts/tests/check.sh

# CI uses the same deterministic gate as local development.
ci: check

# Build the Rust workspace.
build:
    cargo build --workspace

# Build the retained addon without requiring installed Fcitx development packages.
addon-build:
    scripts/tests/addon-build.sh

# Build the real Fcitx addon target.
addon-fcitx-build:
    VINPUT_ADDON_REQUIRE_FCITX=1 VINPUT_ADDON_BUILD_DIR=target/cpp/fcitx5-addon-fcitx scripts/tests/addon-build.sh

# Validate lightweight Arch/RPM/release metadata and trust boundaries.
package-check:
    scripts/release/check-arch-install-script.sh
    scripts/release/check-arch-pkgbuild.sh
    scripts/release/check-rpm-spec.sh
    scripts/release/check-release-manifest.sh
    scripts/release/check-release-signature.sh
    scripts/release/check-arch-release-candidate.sh

# Build and validate the complete Arch package/repository/release pipeline.
package-smoke:
    scripts/release/run-arch-package-smoke.sh

# Build and validate the RPM package plus isolated install/upgrade/removal transactions.
rpm-package-smoke:
    scripts/release/run-rpm-package-smoke.sh

# Install the local per-user IME profile.
install-user:
    scripts/install/install-user-ime.sh

# Show or remove the local per-user IME profile.
user-status:
    VINPUT_USER_STATUS=1 scripts/install/install-user-ime.sh

user-remove:
    VINPUT_USER_REMOVE=1 scripts/install/install-user-ime.sh

# Run the daemon on the current session bus.
dbus:
    cargo run -p vinput-daemon -- --dbus

# Deterministic file-input demo.
demo:
    python3 scripts/fixtures/write-demo-wav.py target/tmp/vinput-demo.wav
    cargo run -q -p vinput-daemon -- --config data/e2e-command-demo-config.json --configured-backends --once --wav target/tmp/vinput-demo.wav
