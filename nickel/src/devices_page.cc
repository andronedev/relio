#include "devices_page.h"

#include <QDateTime>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
#include <QLocale>
#include <QRegularExpression>
#include <QStackedWidget>
#include <QVBoxLayout>

#include "daemon.h"
#include "log.h"
#include "strings.h"
#include "touch_button.h"

namespace relio {

DevicesPage::DevicesPage(Daemon &daemon, int unit, QWidget *parent)
    : QWidget(parent),
      daemon_(daemon),
      unit_(unit),
      views_(new QStackedWidget(this)),
      rows_(new QVBoxLayout),
      summary_(new QLabel(this)),
      confirmTitle_(new QLabel(this)) {
    auto *listView = new QWidget(views_);
    auto *title = keyed(new QLabel(listView), "connectedDevices");
    title->setObjectName("title");
    summary_->setParent(listView);
    summary_->setWordWrap(true);
    auto *back = keyed(new TouchButton(QString(), listView), "back");
    auto *list = new QVBoxLayout(listView);
    list->setContentsMargins(unit * 2, unit * 2, unit * 2, unit * 2);
    list->setSpacing(unit);
    list->addWidget(title);
    list->addWidget(summary_);
    rows_->setSpacing(unit);
    list->addLayout(rows_);
    list->addStretch();
    list->addWidget(back);

    auto *confirmView = new QWidget(views_);
    confirmTitle_->setParent(confirmView);
    confirmTitle_->setObjectName("title");
    confirmTitle_->setWordWrap(true);
    auto *explanation = keyed(new QLabel(confirmView), "disconnectExplanation");
    explanation->setWordWrap(true);
    auto *cancel = keyed(new TouchButton(QString(), confirmView), "cancel");
    auto *disconnect = keyed(new TouchButton(QString(), confirmView), "disconnect");
    auto *confirmLayout = new QVBoxLayout(confirmView);
    confirmLayout->setContentsMargins(unit * 2, unit * 2, unit * 2, unit * 2);
    confirmLayout->setSpacing(unit);
    confirmLayout->addWidget(confirmTitle_);
    confirmLayout->addWidget(explanation);
    confirmLayout->addStretch();
    confirmLayout->addWidget(disconnect);
    confirmLayout->addWidget(cancel);

    views_->addWidget(listView);
    views_->addWidget(confirmView);
    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->addWidget(views_);

    QObject::connect(back, &QPushButton::clicked, [this] {
        if (onBack_) {
            onBack_();
        }
    });
    QObject::connect(cancel, &QPushButton::clicked, [this] { views_->setCurrentIndex(0); });
    QObject::connect(disconnect, &QPushButton::clicked, [this] {
        if (!daemon_.revokeClient(pendingId_)) {
            log("could not disconnect a device");
        }
        refresh();
    });
}

void DevicesPage::refresh() {
    clearRows();
    views_->setCurrentIndex(0);
    const DaemonReply reply = daemon_.clients();
    if (reply.status != 200) {
        summary_->setText(t("devicesUnavailable"));
        return;
    }
    const QJsonArray clients = reply.body.array();
    summary_->setText(clients.isEmpty()
        ? t("devicesEmpty")
        : t("devicesIntro"));
    const QLocale locale = Strings::locale();
    QString dateFormat = locale.dateFormat(QLocale::LongFormat);
    dateFormat.remove(QRegularExpression("dddd[,.]?\\s*"));
    for (const QJsonValue &value : clients) {
        const QJsonObject client = value.toObject();
        const QDateTime paired = QDateTime::fromTime_t(static_cast<uint>(client.value("paired_at").toDouble()));
        addRow(client.value("id").toString(), client.value("name").toString(),
               t("connectedOn").replace("{date}", locale.toString(paired.date(), dateFormat)));
    }
}

void DevicesPage::clearRows() {
    while (QLayoutItem *item = rows_->takeAt(0)) {
        delete item->widget();
        delete item;
    }
}

void DevicesPage::addRow(const QString &id, const QString &name, const QString &pairedOn) {
    auto *row = new QWidget(this);
    auto *label = new QLabel(QString("<b>%1</b><br>%2").arg(name.toHtmlEscaped(), pairedOn), row);
    label->setWordWrap(true);
    auto *disconnect = new TouchButton(t("disconnect"), row);
    disconnect->setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Fixed);
    auto *layout = new QHBoxLayout(row);
    layout->setContentsMargins(0, unit_ / 2, 0, unit_ / 2);
    layout->setSpacing(unit_);
    layout->addWidget(label, 1);
    layout->addWidget(disconnect);
    rows_->addWidget(row);
    QObject::connect(disconnect, &QPushButton::clicked, [this, id, name] { confirm(id, name); });
}

void DevicesPage::confirm(const QString &id, const QString &name) {
    pendingId_ = id;
    confirmTitle_->setText(t("disconnectQuestion").replace("{device}", name));
    views_->setCurrentIndex(1);
}

}  // namespace relio
