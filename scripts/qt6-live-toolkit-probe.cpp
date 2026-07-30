#include <QApplication>
#include <QInputMethodEvent>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QTimer>
#include <QVBoxLayout>
#include <QWidget>

#include <cstdlib>
#include <functional>

namespace {

enum class ProbeMode {
  Normal,
  Command,
};

void EmitJson(const QJsonObject &object) {
  const auto encoded = QJsonDocument(object).toJson(QJsonDocument::Compact);
  fwrite(encoded.constData(), 1, static_cast<std::size_t>(encoded.size()), stdout);
  fputc('\n', stdout);
  fflush(stdout);
}

bool EnvFlag(const char *name, bool fallback) {
  if (!qEnvironmentVariableIsSet(name)) {
    return fallback;
  }
  const auto value = qEnvironmentVariable(name).trimmed().toLower();
  return value != "0" && value != "false" && value != "no";
}

int TimeoutMilliseconds() {
  bool ok = false;
  const int seconds =
      qEnvironmentVariableIntValue("VINPUT_TOOLKIT_TIMEOUT_SECONDS", &ok);
  if (!ok || seconds <= 0 || seconds > 3600) {
    return 60'000;
  }
  return seconds * 1000;
}

class ProbeLineEdit final : public QLineEdit {
public:
  std::function<void(const QString &)> preedit_observer;

protected:
  void inputMethodEvent(QInputMethodEvent *event) override {
    if (preedit_observer) {
      preedit_observer(event->preeditString());
    }
    QLineEdit::inputMethodEvent(event);
  }
};

ProbeMode ParseMode(const QString &mode) {
  if (mode == "normal") {
    return ProbeMode::Normal;
  }
  if (mode == "command") {
    return ProbeMode::Command;
  }
  fprintf(stderr, "mode must be `normal` or `command`\n");
  std::exit(2);
}

} // namespace

int main(int argc, char **argv) {
  QApplication app(argc, argv);
  if (argc != 2) {
    fprintf(stderr, "usage: %s normal|command\n", argv[0]);
    return 2;
  }

  const auto mode_text = QString::fromUtf8(argv[1]);
  const auto mode = ParseMode(mode_text);
  auto initial_text = qEnvironmentVariable("VINPUT_TOOLKIT_INITIAL_TEXT");
  if (initial_text.isEmpty()) {
    initial_text = QStringLiteral("selected text");
  }
  const bool require_partial = EnvFlag("VINPUT_TOOLKIT_REQUIRE_PARTIAL", true);

  bool partial_seen = false;
  bool commit_seen = false;
  bool replacement_seen = false;
  bool timed_out = false;

  QWidget window;
  window.setWindowTitle(QStringLiteral("fcitx-vinput Qt6 live probe"));
  window.resize(640, 140);

  auto *layout = new QVBoxLayout(&window);
  auto *instruction = new QLabel(
      mode == ProbeMode::Normal
          ? QStringLiteral("Focus the field, press the normal dictation key, speak, "
                           "then press it again.")
          : QStringLiteral("The text is selected. Press the command dictation key, "
                           "speak, stop, and choose the replacement candidate."),
      &window);
  instruction->setWordWrap(true);
  layout->addWidget(instruction);

  auto *entry = new ProbeLineEdit();
  layout->addWidget(entry);
  if (mode == ProbeMode::Command) {
    entry->setText(initial_text);
    entry->selectAll();
  }

  const auto finish_when_successful = [&] {
    const bool partial_ok = !require_partial || partial_seen;
    const bool outcome_ok = mode == ProbeMode::Normal ? commit_seen : replacement_seen;
    if (partial_ok && outcome_ok) {
      QTimer::singleShot(0, &app, &QApplication::quit);
    }
  };

  entry->preedit_observer = [&](const QString &preedit) {
    EmitJson({{"event", "preedit"}, {"text", preedit}});
    if (!preedit.isEmpty() && !preedit.contains(QStringLiteral("..."))) {
      partial_seen = true;
    }
  };
  QObject::connect(entry, &QLineEdit::textChanged, &app, [&](const QString &text) {
    EmitJson({{"event", "changed"}, {"text", text}});
    if (mode == ProbeMode::Normal) {
      commit_seen = !text.isEmpty();
    } else if (!text.isEmpty() && text != initial_text) {
      commit_seen = true;
      replacement_seen = true;
    }
    finish_when_successful();
  });

  QTimer timeout;
  timeout.setSingleShot(true);
  QObject::connect(&timeout, &QTimer::timeout, &app, [&] {
    timed_out = true;
    EmitJson({{"event", "timeout"}, {"text", entry->text()}});
    app.quit();
  });

  window.show();
  entry->setFocus(Qt::OtherFocusReason);
  if (mode == ProbeMode::Command) {
    entry->selectAll();
  }
  EmitJson({{"event", "ready"},
            {"toolkit", "qt6"},
            {"mode", mode_text},
            {"manual_trigger", true}});
  timeout.start(TimeoutMilliseconds());
  app.exec();

  const bool partial_ok = !require_partial || partial_seen;
  const bool outcome_ok = mode == ProbeMode::Normal ? commit_seen : replacement_seen;
  const bool ok = partial_ok && outcome_ok && !timed_out;
  EmitJson({{"event", "summary"},
            {"toolkit", "qt6"},
            {"mode", mode_text},
            {"partial", partial_seen},
            {"commit", commit_seen},
            {"replacement", replacement_seen},
            {"timed_out", timed_out},
            {"ok", ok},
            {"text", entry->text()}});
  return ok ? 0 : 1;
}
