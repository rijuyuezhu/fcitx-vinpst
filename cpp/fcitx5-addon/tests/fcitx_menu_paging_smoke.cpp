#include "vinput_fcitx_bridge/fcitx_menu_paging.h"

#include <fcitx/candidatelist.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputcontextmanager.h>
#include <fcitx/inputpanel.h>
#include <fcitx/text.h>

#include <cassert>
#include <memory>
#include <string>

namespace {

class TestInputContext final : public fcitx::InputContext {
public:
  explicit TestInputContext(fcitx::InputContextManager &manager)
      : fcitx::InputContext(manager, "vinput-menu-paging-smoke") {
    created();
  }

  ~TestInputContext() override {
    destroy();
  }

  const char *frontend() const override {
    return "vinput-menu-paging-smoke";
  }

protected:
  void commitStringImpl(const std::string &) override {}
  void deleteSurroundingTextImpl(int, unsigned int) override {}
  void forwardKeyImpl(const fcitx::ForwardKeyEvent &) override {}
  void updatePreeditImpl() override {}
};

class Word final : public fcitx::CandidateWord {
public:
  explicit Word(std::string value)
      : fcitx::CandidateWord(fcitx::Text(std::move(value))) {}

  void select(fcitx::InputContext *) const override {}
};

std::unique_ptr<fcitx::CommonCandidateList> BuildCandidates() {
  auto candidates = std::make_unique<fcitx::CommonCandidateList>();
  candidates->setPageSize(10);
  candidates->setCursorPositionAfterPaging(
      fcitx::CursorPositionAfterPaging::ResetToFirst);
  for (int index = 0; index < 14; ++index) {
    candidates->append<Word>("item-" + std::to_string(index));
  }
  candidates->setGlobalCursorIndex(0);
  return candidates;
}

} // namespace

int main() {
  fcitx::InputContextManager manager;
  TestInputContext input_context(manager);

  auto second_page = BuildCandidates();
  vinput_fcitx_bridge::SetMenuCandidatePage(*second_page, 1);
  vinput_fcitx_bridge::PublishMenuCandidateList(&input_context, std::move(second_page));

  auto published = input_context.inputPanel().candidateList();
  assert(published != nullptr);
  auto *pageable = published->toPageable();
  assert(pageable != nullptr);
  assert(pageable->currentPage() == 1);
  assert(pageable->totalPages() == 2);
  assert(published->size() == 4);
  assert(published->candidate(0).text().toString() == "item-10");
  assert(published->candidate(3).text().toString() == "item-13");

  auto clamped_first = BuildCandidates();
  vinput_fcitx_bridge::SetMenuCandidatePage(*clamped_first, -1);
  assert(clamped_first->currentPage() == 0);

  auto clamped_last = BuildCandidates();
  vinput_fcitx_bridge::SetMenuCandidatePage(*clamped_last, 99);
  assert(clamped_last->currentPage() == 1);

  return 0;
}
