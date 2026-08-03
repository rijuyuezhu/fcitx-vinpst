#pragma once

#include "vinput_fcitx_bridge/rust_handle.h"

struct VinputFcitxAsrDisplaySnapshot;
struct VinputFcitxSceneSnapshot;

extern "C" {
void vinput_fcitx_asr_display_snapshot_free(VinputFcitxAsrDisplaySnapshot *snapshot);
void vinput_fcitx_scene_snapshot_free(VinputFcitxSceneSnapshot *snapshot);
}

namespace vinput_fcitx_bridge {

using SceneStateSnapshot =
    RustOwnedHandle<VinputFcitxSceneSnapshot, vinput_fcitx_scene_snapshot_free>;
using AsrDisplayMenuStateSnapshot =
    RustOwnedHandle<VinputFcitxAsrDisplaySnapshot,
                    vinput_fcitx_asr_display_snapshot_free>;

} // namespace vinput_fcitx_bridge
