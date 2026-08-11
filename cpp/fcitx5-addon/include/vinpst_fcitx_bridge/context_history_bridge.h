#pragma once

#include "vinpst_fcitx_bridge/rust_handle.h"
#include "vinpst_fcitx_bridge/rust_string.h"
#include "vinpst_fcitx_ffi.h"

#include <cstddef>
#include <string_view>

namespace vinpst_fcitx_bridge {

class ContextHistoryBridge final {
public:
  ContextHistoryBridge()
      : history_(HistoryHandle::Adopt(vinpst_fcitx_context_history_new())) {}

  void Reload() {
    vinpst_fcitx_context_history_reload(history_.mutable_raw_handle());
  }

  bool UserCommit(std::size_t context, std::string_view text) {
    return vinpst_fcitx_context_history_user_commit(history_.mutable_raw_handle(),
                                                    context, RustBytes(text),
                                                    text.size()) != 0;
  }

  void AppendEntry(std::string_view text, std::string_view source) {
    vinpst_fcitx_context_history_append_entry(history_.mutable_raw_handle(),
                                              RustBytes(text), text.size(),
                                              RustBytes(source), source.size());
  }

  void SuppressNext(std::string_view text) {
    vinpst_fcitx_context_history_suppress_next(history_.mutable_raw_handle(),
                                               RustBytes(text), text.size());
  }

  void ContextDestroyed(std::size_t context) {
    vinpst_fcitx_context_history_context_destroyed(history_.mutable_raw_handle(),
                                                   context);
  }

  void Flush() {
    vinpst_fcitx_context_history_flush(history_.mutable_raw_handle());
  }

private:
  using HistoryHandle =
      RustOwnedHandle<VinpstFcitxContextHistory, vinpst_fcitx_context_history_free>;

  HistoryHandle history_;
};

} // namespace vinpst_fcitx_bridge
