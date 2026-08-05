#include "configwidget.h"
#include "dbusprovider.h"

#include <QAbstractButton>
#include <QApplication>
#include <QComboBox>
#include <QDialog>
#include <QGroupBox>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QSpinBox>
#include <QTextStream>
#include <QTimer>
#include <QWidget>

namespace {

QJsonObject describeWidget(QWidget *widget) {
  QJsonObject object;
  object.insert("class", widget->metaObject()->className());
  object.insert("object_name", widget->objectName());
  object.insert("accessible_name", widget->accessibleName());
  object.insert("accessible_description", widget->accessibleDescription());
  object.insert("visible", widget->isVisible());
  object.insert("enabled", widget->isEnabled());

  QString text;
  QJsonArray items;
  if (const auto *label = qobject_cast<QLabel *>(widget)) {
    text = label->text();
  } else if (const auto *button = qobject_cast<QAbstractButton *>(widget)) {
    text = button->text();
  } else if (const auto *group = qobject_cast<QGroupBox *>(widget)) {
    text = group->title();
  } else if (const auto *line_edit = qobject_cast<QLineEdit *>(widget)) {
    text = line_edit->text();
  } else if (const auto *spin_box = qobject_cast<QSpinBox *>(widget)) {
    text = spin_box->text();
  } else if (const auto *combo = qobject_cast<QComboBox *>(widget)) {
    text = combo->currentText();
    for (int index = 0; index < combo->count(); ++index) {
      items.append(combo->itemText(index));
    }
  }
  object.insert("text", text);
  object.insert("items", items);
  return object;
}

void printJson(const QJsonObject &object) {
  QTextStream stream(stdout);
  stream << QJsonDocument(object).toJson(QJsonDocument::Compact) << '\n';
  stream.flush();
}

} // namespace

int main(int argc, char **argv) {
  QApplication application(argc, argv);
  application.setApplicationName("fcitx-vinpst-config-surface-probe");

  const QString uri = argc > 1 ? QString::fromLocal8Bit(argv[1])
                               : QStringLiteral("fcitx://config/addon/vinpst");
  auto *dbus = new fcitx::kcm::DBusProvider(&application);
  QDialog *dialog = nullptr;
  bool changed = false;
  bool started = false;

  const auto start = [&]() {
    if (started || !dbus->available()) {
      return;
    }
    started = true;
    dialog = fcitx::kcm::ConfigWidget::configDialog(
        nullptr, dbus, uri, QStringLiteral("Vinpst Configuration"));
    QJsonObject startup;
    startup.insert("event", "startup");
    startup.insert("uri", uri);
    startup.insert("dbus_available", true);
    startup.insert("created", dialog != nullptr);
    printJson(startup);
    if (dialog == nullptr) {
      application.exit(2);
      return;
    }
    if (auto *config = dialog->findChild<fcitx::kcm::ConfigWidget *>()) {
      QObject::connect(config, &fcitx::kcm::ConfigWidget::changed,
                       [&changed]() { changed = true; });
    }
    dialog->resize(760, 560);
    dialog->show();
    QTimer::singleShot(1500, &application, [&]() {
      const auto descendants = dialog->findChildren<QWidget *>();
      QJsonArray widgets;
      for (QWidget *widget : descendants) {
        widgets.append(describeWidget(widget));
      }
      QJsonObject summary;
      summary.insert("event", "summary");
      summary.insert("uri", uri);
      summary.insert("window_title", dialog->windowTitle());
      summary.insert("dbus_available", dbus->available());
      summary.insert("created", true);
      summary.insert("changed", changed);
      summary.insert("widget_count", descendants.size());
      summary.insert("widgets", widgets);
      summary.insert("save_called", false);
      summary.insert("ok", dbus->available() && !changed && !descendants.isEmpty());
      printJson(summary);
      dialog->reject();
      application.quit();
    });
  };

  QObject::connect(dbus, &fcitx::kcm::DBusProvider::availabilityChanged, &application,
                   [&, start](bool available) {
                     if (available) {
                       start();
                     }
                   });
  QTimer::singleShot(0, &application, start);
  QTimer::singleShot(6000, &application, [&]() {
    if (!started) {
      QJsonObject failure;
      failure.insert("event", "timeout");
      failure.insert("uri", uri);
      failure.insert("dbus_available", dbus->available());
      failure.insert("ok", false);
      printJson(failure);
      application.exit(3);
    }
  });
  return application.exec();
}
