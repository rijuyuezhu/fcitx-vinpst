#include "vinpst_fcitx_bridge/fcitx_addon.h"

#include <fcitx/addonfactory.h>
#include <fcitx/addonmanager.h>

namespace vinpst_fcitx_bridge {

class FcitxVinpstAddonFactory final : public fcitx::AddonFactory {
public:
  fcitx::AddonInstance *create(fcitx::AddonManager *manager) override {
    return new FcitxVinpstAddon(manager != nullptr ? manager->instance() : nullptr);
  }
};

} // namespace vinpst_fcitx_bridge

#ifdef VINPST_FCITX5_CORE_HAVE_ADDON_FACTORY_V2
FCITX_ADDON_FACTORY_V2(vinpst, vinpst_fcitx_bridge::FcitxVinpstAddonFactory);
#else
FCITX_ADDON_FACTORY(vinpst_fcitx_bridge::FcitxVinpstAddonFactory);
#endif
