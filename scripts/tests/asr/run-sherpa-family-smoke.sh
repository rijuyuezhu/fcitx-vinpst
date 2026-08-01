#!/usr/bin/env bash
set -euo pipefail

scenario="${1:?usage: run-sherpa-family-smoke.sh <offline-transducer|dolphin|paraformer|qwen3|moonshine|moonshine-reload|online-transducer|zipformer2-ctc>}"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
cd "${repo_root}"

case "${scenario}" in
  offline-transducer)
    VINPUT_SHERPA_EXPECT_FAMILY=transducer \
    VINPUT_SHERPA_EXPECT_TEXT='对我做了介绍那么我想说的是大家如果对我的研究感兴趣' \
    VINPUT_SHERPA_SMOKE_DIR=target/tmp/sherpa-offline-transducer-local-smoke \
      scripts/tests/asr/run-sherpa-offline-local-smoke.sh
    ;;
  dolphin)
    VINPUT_SHERPA_EXPECT_FAMILY=dolphin \
    VINPUT_SHERPA_EXPECT_TEXT='对我做了介绍哈那么我想说的是呢大家如果对我的研究感兴趣呢。' \
    VINPUT_SHERPA_SMOKE_DIR=target/tmp/sherpa-dolphin-local-smoke \
      scripts/tests/asr/run-sherpa-offline-local-smoke.sh
    ;;
  paraformer)
    VINPUT_SHERPA_EXPECT_FAMILY=paraformer \
    VINPUT_SHERPA_EXPECT_TEXT='对我做了介绍啊那么我想说的是呢大家如果对我的研究感兴趣呢嗯' \
    VINPUT_SHERPA_SMOKE_DIR=target/tmp/sherpa-paraformer-local-smoke \
      scripts/tests/asr/run-sherpa-offline-local-smoke.sh
    ;;
  qwen3)
    VINPUT_SHERPA_EXPECT_FAMILY=qwen3_asr \
    VINPUT_SHERPA_SMOKE_DIR=target/tmp/sherpa-qwen3-local-smoke \
      scripts/tests/asr/run-sherpa-offline-local-smoke.sh
    ;;
  moonshine)
    VINPUT_SHERPA_EXPECT_FAMILY=moonshine \
    VINPUT_SHERPA_EXPECT_TEXT='After early nightfall, the yellow lamps would light up here and there the squalid quarter of the brothels.' \
    VINPUT_SHERPA_SMOKE_DIR=target/tmp/sherpa-moonshine-local-smoke \
      scripts/tests/asr/run-sherpa-offline-local-smoke.sh
    ;;
  moonshine-reload)
    VINPUT_SHERPA_EXPECT_FAMILY=moonshine \
    VINPUT_SHERPA_EXPECT_TEXT='After early nightfall, the yellow lamps would light up here and there the squalid quarter of the brothels.' \
    VINPUT_SHERPA_RELOAD_SMOKE_DIR=target/tmp/sherpa-moonshine-dbus-reload-smoke \
      scripts/tests/asr/run-sherpa-dbus-reload-smoke.sh
    ;;
  online-transducer)
    VINPUT_SHERPA_EXPECT_FAMILY=transducer \
    VINPUT_SHERPA_EXPECT_TEXT='THE YELLOW LAMPS WOULD LIGHT UP HERE AND THERE THE SQUALID QUARTER OF THE BRAFFLEL' \
    VINPUT_SHERPA_SMOKE_DIR=target/tmp/sherpa-online-transducer-local-smoke \
      scripts/tests/asr/run-sherpa-online-local-smoke.sh
    ;;
  zipformer2-ctc)
    VINPUT_SHERPA_EXPECT_FAMILY=zipformer2_ctc \
    VINPUT_SHERPA_EXPECT_TEXT='对我做了介绍那么我想说的是呢大家如果对我的研究感兴趣呢' \
    VINPUT_SHERPA_SMOKE_DIR=target/tmp/sherpa-zipformer2-ctc-local-smoke \
      scripts/tests/asr/run-sherpa-online-local-smoke.sh
    ;;
  *)
    echo "unknown Sherpa smoke scenario: ${scenario}" >&2
    exit 2
    ;;
esac
