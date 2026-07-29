set dotenv-load := false

addon-sources := `find cpp/fcitx5-addon -type f \( -name '*.cpp' -o -name '*.h' \) | sort | tr '\n' ' '`
addon-lint-sources := `find cpp/fcitx5-addon -type f -name '*.cpp' | sort | tr '\n' ' '`

fmt:
    clang-format -i {{addon-sources}}
    cargo fmt --all

fmt-check:
    clang-format --dry-run --Werror {{addon-sources}}
    cargo fmt --all -- --check

lint:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
    clang-tidy -p target/cpp/fcitx5-addon {{addon-lint-sources}}
    cargo clippy --workspace --all-targets -- -D warnings

dbus-lint:
    cargo clippy -p vinput-daemon --all-targets --features dbus-integration -- -D warnings

test:
    cargo test --workspace --all-targets

dbus-test:
    dbus-run-session -- cargo test -p vinput-daemon --features dbus-integration --test dbus_integration

check: fmt-check lint test dbus-test dbus-lint addon-test addon-dbus-smoke addon-dbus-asr-menu-smoke command-asr-wav-helper-smoke capture-cold-start-smoke remote-text-daemon-lifecycle-smoke user-ime-activation-owner-smoke user-ime-real-command-asr-wav-smoke user-ime-sherpa-sense-voice-smoke user-ime-sherpa-native-smoke

addon-format:
    clang-format -i {{addon-sources}}

addon-format-check:
    clang-format --dry-run --Werror {{addon-sources}}

addon-configure:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json

addon-build:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
    cmake --build target/cpp/fcitx5-addon --parallel

addon-fcitx-build:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon-fcitx -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
    cmake --build target/cpp/fcitx5-addon-fcitx --parallel

addon-install-smoke: addon-fcitx-build
    rm -rf target/tmp/fcitx-addon-install-smoke
    cmake --install target/cpp/fcitx5-addon-fcitx --prefix target/tmp/fcitx-addon-install-smoke
    test -f target/tmp/fcitx-addon-install-smoke/usr/local/lib/fcitx5/fcitx5-vinput.so
    test -f target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    test -f target/tmp/fcitx-addon-install-smoke/share/locale/zh_CN/LC_MESSAGES/fcitx5-vinput.mo
    grep -qx 'Library=fcitx5-vinput' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    grep -qx 'Type=SharedLibrary' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    grep -qx 'OnDemand=False' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    grep -qx 'Configurable=True' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    grep -qx '0=dbus' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    grep -qx '1=clipboard' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    ! grep -qE '^(Name|Comment)\[' target/tmp/fcitx-addon-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    test -f target/tmp/fcitx-addon-install-smoke/share/dbus-1/services/org.fcitx.Vinput.service
    grep -qx 'Name=org.fcitx.Vinput' target/tmp/fcitx-addon-install-smoke/share/dbus-1/services/org.fcitx.Vinput.service
    grep -qx 'Exec=/usr/local/bin/vinput-daemon --dbus' target/tmp/fcitx-addon-install-smoke/share/dbus-1/services/org.fcitx.Vinput.service

# Stage the Rust daemon, Fcitx addon, metadata, and DBus activation service together.
ime-install-smoke: addon-fcitx-build
    cargo build -p vinput-daemon
    rm -rf target/tmp/fcitx-ime-install-smoke
    install -Dm755 target/debug/vinput-daemon target/tmp/fcitx-ime-install-smoke/usr/local/bin/vinput-daemon
    cmake --install target/cpp/fcitx5-addon-fcitx --prefix target/tmp/fcitx-ime-install-smoke
    test -x target/tmp/fcitx-ime-install-smoke/usr/local/bin/vinput-daemon
    test -f target/tmp/fcitx-ime-install-smoke/usr/local/lib/fcitx5/fcitx5-vinput.so
    test -f target/tmp/fcitx-ime-install-smoke/usr/local/share/fcitx5/addon/vinput.conf
    test -f target/tmp/fcitx-ime-install-smoke/share/dbus-1/services/org.fcitx.Vinput.service
    grep -qx 'Exec=/usr/local/bin/vinput-daemon --dbus' target/tmp/fcitx-ime-install-smoke/share/dbus-1/services/org.fcitx.Vinput.service

# Stage a configured demo IME install that activates command ASR/text backends.
ime-configured-install-smoke:
    cargo build -p vinput-daemon
    rm -rf target/cpp/fcitx5-addon-fcitx-configured target/tmp/fcitx-ime-configured-install-smoke
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon-fcitx-configured -DCMAKE_BUILD_TYPE=Debug -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON -DVINPUT_DAEMON_ARGS='--dbus --configured-backends --config /usr/local/share/fcitx-vinput/e2e-command-demo-config.json --wav /usr/local/share/fcitx-vinput/e2e-command-demo.wav'
    cmake --build target/cpp/fcitx5-addon-fcitx-configured --target fcitx5_vinput_addon --parallel
    install -Dm755 target/debug/vinput-daemon target/tmp/fcitx-ime-configured-install-smoke/usr/local/bin/vinput-daemon
    install -Dm644 data/e2e-command-demo-config.json target/tmp/fcitx-ime-configured-install-smoke/usr/local/share/fcitx-vinput/e2e-command-demo-config.json
    python3 scripts/write-demo-wav.py target/tmp/fcitx-ime-configured-install-smoke/usr/local/share/fcitx-vinput/e2e-command-demo.wav
    cmake --install target/cpp/fcitx5-addon-fcitx-configured --prefix target/tmp/fcitx-ime-configured-install-smoke
    test -x target/tmp/fcitx-ime-configured-install-smoke/usr/local/bin/vinput-daemon
    test -f target/tmp/fcitx-ime-configured-install-smoke/usr/local/share/fcitx-vinput/e2e-command-demo-config.json
    test -f target/tmp/fcitx-ime-configured-install-smoke/usr/local/share/fcitx-vinput/e2e-command-demo.wav
    test -f target/tmp/fcitx-ime-configured-install-smoke/usr/local/lib/fcitx5/fcitx5-vinput.so
    grep -qx 'Exec=/usr/local/bin/vinput-daemon --dbus --configured-backends --config /usr/local/share/fcitx-vinput/e2e-command-demo-config.json --wav /usr/local/share/fcitx-vinput/e2e-command-demo.wav' target/tmp/fcitx-ime-configured-install-smoke/share/dbus-1/services/org.fcitx.Vinput.service

addon-lint:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
    clang-tidy -p target/cpp/fcitx5-addon {{addon-lint-sources}}

addon-test:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
    cmake --build target/cpp/fcitx5-addon --parallel
    ctest --test-dir target/cpp/fcitx5-addon --output-on-failure

addon-smoke:
    clang-format --dry-run --Werror {{addon-sources}}
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
    ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
    clang-tidy -p target/cpp/fcitx5-addon {{addon-lint-sources}}
    cmake --build target/cpp/fcitx5-addon --parallel
    ctest --test-dir target/cpp/fcitx5-addon --output-on-failure

addon-dbus-smoke:
    scripts/run-cpp-dbus-smoke.sh

addon-dbus-asr-menu-smoke:
    scripts/run-cpp-dbus-asr-menu-smoke.sh

addon-dbus-adapter-lifecycle-smoke:
    scripts/run-cpp-dbus-adapter-lifecycle-smoke.sh

# Explicit live PipeWire D-Bus smoke. Requires a user PipeWire session.
addon-dbus-pipewire-live:
    scripts/run-cpp-dbus-pipewire-live-smoke.sh

# Explicit staged IME activation smoke with live PipeWire capture. Requires a user PipeWire session.
ime-pipewire-live:
    scripts/run-ime-pipewire-live-smoke.sh

# Explicit staged IME activation smoke with configured command backends and live PipeWire capture.
ime-configured-pipewire-live:
    scripts/run-ime-configured-pipewire-live-smoke.sh

# Install per-user D-Bus activation service for local desktop testing. Writes under XDG_DATA_HOME or ~/.local/share.
user-activation-service:
    scripts/install-user-activation-service.sh

# Install per-user D-Bus activation service with deterministic command demo backends.
user-command-demo-activation-service:
    VINPUT_USER_PROFILE=command-demo scripts/install-user-activation-service.sh

# Install per-user D-Bus activation service for configured command backends plus live PipeWire capture.
user-pipewire-activation-service:
    VINPUT_USER_PROFILE=configured-pipewire-live scripts/install-user-activation-service.sh

# Install daemon, retained Fcitx addon, metadata, and D-Bus activation for local desktop testing.
user-ime-install:
    scripts/install-user-ime.sh

# Install a deterministic command-demo user IME profile for local desktop testing.
user-ime-command-demo:
    VINPUT_USER_PROFILE=command-demo scripts/install-user-ime.sh

# Install a configured PipeWire user IME profile for local desktop testing.
user-ime-pipewire-live:
    VINPUT_USER_PROFILE=configured-pipewire-live scripts/install-user-ime.sh

# Show user IME install status.
user-ime-status:
    VINPUT_USER_STATUS=1 scripts/install-user-ime.sh

# Remove user IME addon files and D-Bus activation service.
user-ime-clear:
    VINPUT_USER_REMOVE=1 scripts/install-user-ime.sh

# Clear the per-user D-Bus activation service installed for local desktop testing.
user-activation-service-clear:
    VINPUT_USER_REMOVE=1 scripts/install-user-activation-service.sh

# Show per-user D-Bus activation service status for local desktop testing.
user-activation-service-status:
    VINPUT_USER_STATUS=1 scripts/install-user-activation-service.sh

addon-dbus-activation-smoke:
    scripts/run-cpp-dbus-activation-smoke.sh

addon-dbus-configured-activation-smoke:
    scripts/run-cpp-dbus-configured-activation-smoke.sh

ci: check

smoke:
    cargo run -q -p vinput-cli -- protocol
    cargo run -q -p vinput-cli -- init --dry-run --json
    cargo run -q -p vinput-cli -- config
    cargo run -q -p vinput-cli -- config validate data/default-config.json --summary-only
    cargo run -q -p vinput-cli -- config get /global/default_language --config data/default-config.json --json
    cargo run -q -p vinput-cli -- config get /global/missing --exists --config data/default-config.json --json
    cargo run -q -p vinput-cli -- config get /global/missing --default false --config data/default-config.json --json
    cargo run -q -p vinput-cli -- config get /global/missing --default false --default-string --config data/default-config.json --json
    cargo run -q -p vinput-cli -- config set /global/default_language en --config data/default-config.json --dry-run --json
    cargo run -q -p vinput-cli -- config set /global/capture_device true --string --config data/default-config.json --dry-run --json
    cargo run -q -p vinput-cli -- config edit --dry-run --editor true --json
    cargo run -q -p vinput-cli -- asr-state
    cargo run -q -p vinput-cli -- asr-state --config data/default-config.json
    cargo run -q -p vinput-cli -- audio-devices
    cargo run -q -p vinput-cli -- device list --json
    cargo run -q -p vinput-cli -- device use default --dry-run --json
    cargo run -q -p vinput-cli -- provider list --json
    cargo run -q -p vinput-cli -- provider use sherpa-onnx --dry-run --json
    cargo run -q -p vinput-cli -- provider edit sherpa-onnx --model sherpa-onnx --dry-run --json
    cargo run -q -p vinput-cli -- scene list --json
    cargo run -q -p vinput-cli -- scene use __raw__ --dry-run --json
    cargo run -q -p vinput-cli -- scene add scratch --label Scratch --dry-run --json
    cargo run -q -p vinput-cli -- scene edit __raw__ --label __label_raw__ --dry-run --json
    ! cargo run -q -p vinput-cli -- scene remove __command__ --dry-run --json
    cargo run -q -p vinput-cli -- llm list --json
    cargo run -q -p vinput-cli -- llm add scratch --base-url https://llm.example.test/v1 --dry-run --json
    cargo run -q -p vinput-cli -- adapter list --json
    cargo run -q -p vinput-cli -- adapter add scratch --command true --dry-run --json
    cargo run -q -p vinput-cli -- adapter start lifecycle-adapter --config data/e2e-adapter-lifecycle-config.json --dry-run --json
    cargo run -q -p vinput-cli -- adapter stop lifecycle-adapter --config data/e2e-adapter-lifecycle-config.json --dry-run --json
    cargo run -q -p vinput-cli -- adapter status lifecycle-adapter --config data/e2e-adapter-lifecycle-config.json --dry-run --json
    cargo run -q -p vinput-cli -- hotword get --json
    cargo run -q -p vinput-cli -- hotword set /tmp/hotwords.txt --dry-run --json
    cargo run -q -p vinput-cli -- hotword clear --dry-run --json
    ! cargo run -q -p vinput-cli -- hotword edit --dry-run --editor true --json
    cargo run -q -p vinput-cli -- registry
    cargo run -q -p vinput-cli -- registry validate data/sample-registry-index.json
    cargo run -q -p vinput-cli -- registry plan data/sample-registry-index.json --summary-only
    cargo run -q -p vinput-cli -- model list --registry crates/vinput-registry/tests/fixtures/live-models-sensevoice.json --json
    cargo run -q -p vinput-cli -- model info onnx-sv-zh-int8-off --registry crates/vinput-registry/tests/fixtures/live-models-sensevoice.json --json
    cargo run -q -p vinput-cli -- daemon start --dry-run --json
    cargo run -q -p vinput-cli -- daemon status --dry-run --json
    cargo run -q -p vinput-cli -- daemon reload-asr --dry-run --json
    cargo run -q -p vinput-cli -- daemon stop --dry-run --json
    cargo run -q -p vinput-cli -- daemon restart --dry-run --json
    cargo run -q -p vinput-cli -- daemon log --lines 100 --dry-run --json
    cargo run -q -p vinput-cli -- model use onnx-sv-zh-int8-off --registry crates/vinput-registry/tests/fixtures/live-models-sensevoice.json --model-root /tmp/vinput-models --reload-daemon --dry-run --json
    cargo run -q -p vinput-cli -- model use onnx-sv-zh-int8-off --installed --model-root /tmp/vinput-models --dry-run --json
    cargo run -q -p vinput-cli -- model remove onnx-sv-zh-int8-off --installed --model-root /tmp/vinput-models --dry-run --json
    cargo run -q -p vinput-cli -- recording start --dry-run --json
    cargo run -q -p vinput-cli -- recording start --selected-text demo --dry-run --json
    cargo run -q -p vinput-cli -- recording stop --scene demo --dry-run --json
    cargo run -q -p vinput-cli -- recording toggle --dry-run --json
    cargo run -q -p vinput-cli -- recording status --dry-run --json
    cargo run -q -p vinput-cli -- mock-result '你好'
    cargo run -q -p vinput-daemon -- print-config
    cargo run -q -p vinput-daemon -- asr-state
    cargo run -q -p vinput-daemon -- text-adapters
    cargo run -q -p vinput-daemon -- audio-devices
    cargo run -q -p vinput-daemon -- --once

# Compile the feature-gated official sherpa-onnx ASR backend without running model inference.
sherpa-onnx-check:
    cargo check -p vinput-asr --features=sherpa-onnx-backend
    cargo check -p vinput-daemon --features=sherpa-onnx-backend

# Run native sherpa offline inference using typed registry metadata or the legacy SenseVoice layout.
sherpa-offline-local-smoke:
    scripts/run-sherpa-offline-local-smoke.sh

# Preserve the original metadata-free SenseVoice smoke entry point.
sherpa-sense-voice-local-smoke:
    scripts/run-sherpa-sense-voice-local-smoke.sh

# Run native offline transducer inference using registry-generated typed metadata.
sherpa-offline-transducer-local-smoke:
    VINPUT_SHERPA_EXPECT_FAMILY=transducer VINPUT_SHERPA_EXPECT_TEXT="对我做了介绍那么我想说的是大家如果对我的研究感兴趣" VINPUT_SHERPA_SMOKE_DIR=${VINPUT_SHERPA_SMOKE_DIR:-target/tmp/sherpa-offline-transducer-local-smoke} scripts/run-sherpa-offline-local-smoke.sh

# Run native Dolphin inference using registry-generated typed metadata.
sherpa-dolphin-local-smoke:
    VINPUT_SHERPA_EXPECT_FAMILY=dolphin VINPUT_SHERPA_EXPECT_TEXT="对我做了介绍哈那么我想说的是呢大家如果对我的研究感兴趣呢。" VINPUT_SHERPA_SMOKE_DIR=${VINPUT_SHERPA_SMOKE_DIR:-target/tmp/sherpa-dolphin-local-smoke} scripts/run-sherpa-offline-local-smoke.sh

# Run native Paraformer inference using registry-generated typed metadata.
sherpa-paraformer-local-smoke:
    VINPUT_SHERPA_EXPECT_FAMILY=paraformer VINPUT_SHERPA_EXPECT_TEXT="对我做了介绍啊那么我想说的是呢大家如果对我的研究感兴趣呢嗯" VINPUT_SHERPA_SMOKE_DIR=${VINPUT_SHERPA_SMOKE_DIR:-target/tmp/sherpa-paraformer-local-smoke} scripts/run-sherpa-offline-local-smoke.sh

# Run native Qwen3 ASR inference using registry-generated vinput-model.json metadata.
sherpa-qwen3-local-smoke:
    VINPUT_SHERPA_EXPECT_FAMILY=qwen3_asr VINPUT_SHERPA_SMOKE_DIR=${VINPUT_SHERPA_SMOKE_DIR:-target/tmp/sherpa-qwen3-local-smoke} scripts/run-sherpa-offline-local-smoke.sh

# Run native Moonshine v1 inference using registry-generated typed metadata.
sherpa-moonshine-local-smoke:
    VINPUT_SHERPA_EXPECT_FAMILY=moonshine VINPUT_SHERPA_EXPECT_TEXT="After early nightfall, the yellow lamps would light up here and there the squalid quarter of the brothels." VINPUT_SHERPA_SMOKE_DIR=${VINPUT_SHERPA_SMOKE_DIR:-target/tmp/sherpa-moonshine-local-smoke} scripts/run-sherpa-offline-local-smoke.sh

# Reload an already-running mock daemon to a real Moonshine backend over D-Bus.
sherpa-moonshine-dbus-reload-smoke:
    VINPUT_SHERPA_EXPECT_FAMILY=moonshine VINPUT_SHERPA_EXPECT_TEXT="After early nightfall, the yellow lamps would light up here and there the squalid quarter of the brothels." VINPUT_SHERPA_RELOAD_SMOKE_DIR=${VINPUT_SHERPA_RELOAD_SMOKE_DIR:-target/tmp/sherpa-moonshine-dbus-reload-smoke} scripts/run-sherpa-dbus-reload-smoke.sh

# Run native sherpa online inference using typed live-registry metadata.
sherpa-online-local-smoke:
    scripts/run-sherpa-online-local-smoke.sh

# Run the registry-backed online transducer model smoke.
sherpa-online-transducer-local-smoke:
    VINPUT_SHERPA_EXPECT_FAMILY=transducer VINPUT_SHERPA_EXPECT_TEXT="THE YELLOW LAMPS WOULD LIGHT UP HERE AND THERE THE SQUALID QUARTER OF THE BRAFFLEL" VINPUT_SHERPA_SMOKE_DIR=${VINPUT_SHERPA_SMOKE_DIR:-target/tmp/sherpa-online-transducer-local-smoke} scripts/run-sherpa-online-local-smoke.sh

# Run the small live-registry Zipformer2 CTC streaming model smoke.
sherpa-zipformer2-ctc-local-smoke:
    VINPUT_SHERPA_EXPECT_FAMILY=zipformer2_ctc VINPUT_SHERPA_EXPECT_TEXT="对我做了介绍那么我想说的是呢大家如果对我的研究感兴趣呢" VINPUT_SHERPA_SMOKE_DIR=${VINPUT_SHERPA_SMOKE_DIR:-target/tmp/sherpa-zipformer2-ctc-local-smoke} scripts/run-sherpa-online-local-smoke.sh

# Compile and test optional PipeWire feature paths without requiring a live daemon.
pipewire-check:
    cargo test -p vinput-daemon --features=pipewire-backend
    cargo clippy -p vinput-daemon --all-targets --features=pipewire-backend -- -D warnings
    cargo test -p vinput-audio --features=pipewire-backend
    cargo clippy -p vinput-audio --all-targets --features=pipewire-backend -- -D warnings
    cargo test -p vinput-cli --features=pipewire-backend --test audio_devices
    cargo clippy -p vinput-cli --all-targets --features=pipewire-backend -- -D warnings

# Run explicit local PipeWire probes. Requires a live user PipeWire session.
pipewire-live:
    VINPUT_TEST_PIPEWIRE_CONTEXT=1 VINPUT_TEST_PIPEWIRE_ENUMERATE=1 VINPUT_TEST_PIPEWIRE_RECORD=1 cargo test -p vinput-audio --features pipewire-backend pipewire_ -- --nocapture

# Run a deterministic file-input E2E demo through command ASR and text adapter.
e2e-demo:
    python3 scripts/write-demo-wav.py target/tmp/vinput-demo.wav
    cargo run -q -p vinput-daemon -- --config data/e2e-command-demo-config.json --configured-backends --once --wav target/tmp/vinput-demo.wav

# Run the mock legacy D-Bus service on the current session bus.
dbus:
    cargo run -p vinput-daemon -- --dbus

ime-configured-activation-smoke:
    scripts/run-ime-configured-activation-smoke.sh

# Run deterministic staged IME activation plus fake outcome sink coverage.
ime-e2e-smoke:
    scripts/run-ime-e2e-smoke.sh

# Run command ASR WAV helper smoke.
command-asr-wav-helper-smoke:
    scripts/run-command-asr-wav-helper-smoke.sh

# Verify the Rust capture timing journal analyzer with a deterministic fixture.
capture-cold-start-smoke:
    scripts/run-capture-cold-start-smoke.sh

remote-text-daemon-lifecycle-smoke:
    scripts/run-remote-text-daemon-lifecycle-smoke.sh

# Prove that the initial daemon-status call reports a newly D-Bus-activated user daemon owner.
user-ime-activation-owner-smoke:
    scripts/run-user-ime-activation-owner-smoke.sh

# Run user-profile IME install smoke for a real command-ASR WAV helper profile.
user-ime-real-command-asr-wav-smoke:
    scripts/run-user-ime-real-command-asr-wav-smoke.sh

# Run user-profile IME install smoke for a generic typed native sherpa profile.
user-ime-sherpa-native-smoke:
    scripts/run-user-ime-sherpa-native-smoke.sh

# Prove a temporary native user install through D-Bus auto-activation and exact WAV recognition.
user-ime-sherpa-native-activation-smoke:
    scripts/run-user-ime-sherpa-native-activation-smoke.sh

# Run the proven online transducer through the temporary user activation path.
sherpa-online-transducer-user-activation-smoke:
    VINPUT_SHERPA_MODEL=target/models/onnx-zf-en-20m-stream VINPUT_SHERPA_WAV=target/models/onnx-zf-en-20m-stream/test_wavs/0.wav VINPUT_SHERPA_EXPECT_TEXT='THE YELLOW LAMPS WOULD LIGHT UP HERE AND THERE THE SQUALID QUARTER OF THE BRAFFLEL' scripts/run-user-ime-sherpa-native-activation-smoke.sh

# Make FrontendBridge the first D-Bus client and require a native Commit outcome.
sherpa-online-transducer-user-frontend-smoke:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
    cmake --build target/cpp/fcitx5-addon --target vinput_fcitx_bridge_native_dbus_smoke --parallel
    VINPUT_NATIVE_ACTIVATION_FRONTEND_BIN=target/cpp/fcitx5-addon/vinput_fcitx_bridge_native_dbus_smoke VINPUT_SHERPA_MODEL=target/models/onnx-zf-en-20m-stream VINPUT_SHERPA_WAV=target/models/onnx-zf-en-20m-stream/test_wavs/0.wav VINPUT_SHERPA_EXPECT_TEXT='THE YELLOW LAMPS WOULD LIGHT UP HERE AND THERE THE SQUALID QUARTER OF THE BRAFFLEL' scripts/run-user-ime-sherpa-native-activation-smoke.sh

# Make FcitxVinputAddon the first D-Bus client and capture its applied native Commit.
sherpa-online-transducer-user-addon-smoke:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
    cmake --build target/cpp/fcitx5-addon --target vinput_fcitx_native_addon_dbus_smoke --parallel
    VINPUT_NATIVE_ACTIVATION_FRONTEND_BIN=target/cpp/fcitx5-addon/vinput_fcitx_native_addon_dbus_smoke VINPUT_SHERPA_MODEL=target/models/onnx-zf-en-20m-stream VINPUT_SHERPA_WAV=target/models/onnx-zf-en-20m-stream/test_wavs/0.wav VINPUT_SHERPA_EXPECT_TEXT='THE YELLOW LAMPS WOULD LIGHT UP HERE AND THERE THE SQUALID QUARTER OF THE BRAFFLEL' scripts/run-user-ime-sherpa-native-activation-smoke.sh

# Prove selected-text command mode without a configured text adapter.
sherpa-online-transducer-user-command-addon-smoke:
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
    cmake --build target/cpp/fcitx5-addon --target vinput_fcitx_native_addon_dbus_smoke --parallel
    VINPUT_NATIVE_ADDON_SELECTED_TEXT='replace this text' VINPUT_NATIVE_ACTIVATION_FRONTEND_BIN=target/cpp/fcitx5-addon/vinput_fcitx_native_addon_dbus_smoke VINPUT_SHERPA_MODEL=target/models/onnx-zf-en-20m-stream VINPUT_SHERPA_WAV=target/models/onnx-zf-en-20m-stream/test_wavs/0.wav VINPUT_SHERPA_EXPECT_TEXT='THE YELLOW LAMPS WOULD LIGHT UP HERE AND THERE THE SQUALID QUARTER OF THE BRAFFLEL' scripts/run-user-ime-sherpa-native-activation-smoke.sh

# Run user-profile IME install smoke for the native sherpa SenseVoice profile.
user-ime-sherpa-sense-voice-smoke:
    scripts/run-user-ime-sherpa-sense-voice-smoke.sh

# Run user-profile IME install plus D-Bus activation smoke with command-demo backends.
user-ime-command-demo-smoke:
    scripts/run-user-ime-command-demo-smoke.sh

# Test live Fcitx probe diagnostics with stubbed desktop commands.
ime-fcitx-live-probe-smoke:
    scripts/run-ime-fcitx-live-probe-smoke.sh

# Probe an explicit live desktop Fcitx5 session without adding CI dependencies.
ime-fcitx-live-probe:
    scripts/run-ime-fcitx-live-probe.sh

# Mutating live setup for the deterministic command-demo IME profile.
ime-fcitx-live-command-demo-setup:
    scripts/setup-live-command-demo-ime.sh
