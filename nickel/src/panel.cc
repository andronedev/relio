#include "panel.h"

#include <dlfcn.h>

#include <QEvent>
#include <QHBoxLayout>
#include <QLabel>
#include <QPainter>
#include <QPushButton>
#include <QStackedWidget>
#include <QTimer>
#include <QVBoxLayout>

#include "devices_page.h"
#include "log.h"
#include "strings.h"
#include "touch_button.h"

namespace relio {

namespace {
constexpr int kPollMs = 2000;

QString groupedCode(const QString &code) {
    return code.length() == 6 ? code.left(3) + "  " + code.mid(3) : code;
}

QString withoutFragment(const QString &url) {
    const int hash = url.indexOf('#');
    return hash < 0 ? url : url.left(hash);
}

// Restarts through Nickel, as its own menu does, so it saves its state and shows its screen.
bool nickelReboot() {
    void *nickel = dlopen("libnickel.so.1.0.0", RTLD_LAZY | RTLD_NOLOAD);
    if (!nickel) {
        return false;
    }
    auto sharedInstance = reinterpret_cast<void *(*)()>(dlsym(nickel, "_ZN22N3PowerWorkflowManager14sharedInstanceEv"));
    auto reboot = reinterpret_cast<void (*)(void *)>(dlsym(nickel, "_ZN22N3PowerWorkflowManager6rebootEv"));
    void *manager = sharedInstance && reboot ? sharedInstance() : nullptr;
    if (!manager) {
        return false;
    }
    reboot(manager);
    return true;
}
}  // namespace

class QrView : public QWidget {
public:
    explicit QrView(QWidget *parent) : QWidget(parent) {}

    void setModules(int size, const QString &modules) {
        size_ = size;
        modules_ = modules;
        setVisible(size_ > 0 && modules_.length() == size_ * size_);
        update();
    }

protected:
    void paintEvent(QPaintEvent *) override {
        if (size_ <= 0) {
            return;
        }
        QPainter painter(this);
        painter.fillRect(rect(), Qt::white);
        const int quiet = 4;
        const int cell = qMax(1, qMin(width(), height()) / (size_ + 2 * quiet));
        const int side = cell * (size_ + 2 * quiet);
        const QPoint origin((width() - side) / 2 + quiet * cell, (height() - side) / 2 + quiet * cell);
        for (int y = 0; y < size_; ++y) {
            for (int x = 0; x < size_; ++x) {
                if (modules_.at(y * size_ + x) == '1') {
                    painter.fillRect(origin.x() + x * cell, origin.y() + y * cell, cell, cell, Qt::black);
                }
            }
        }
    }

private:
    int size_ = 0;
    QString modules_;
};

Panel::Panel(QWidget *window)
    : QWidget(window),
      pages_(new QStackedWidget(this)),
      homeStatus_(new QLabel(this)),
      connectButton_(keyed(new TouchButton(QString(), this), "connectDevice")),
      updateStatus_(new QLabel(this)),
      restartButton_(keyed(new TouchButton(QString(), this), "restartToUpdate")),
      checkButton_(keyed(new TouchButton(QString(), this), "checkUpdates")),
      version_(new QLabel(this)),
      devices_(nullptr),
      qr_(new QrView(this)),
      code_(new QLabel(this)),
      instructions_(new QLabel(this)),
      url_(new QLabel(this)),
      pairingStatus_(new QLabel(this)),
      resultTitle_(new QLabel(this)),
      resultMessage_(new QLabel(this)),
      poll_(new QTimer(this)),
      updatePoll_(new QTimer(this)) {
    setAttribute(Qt::WA_OpaquePaintEvent);
    hide();

    const int unit = qMax(16, window->height() / 48);
    setStyleSheet(QString(
        "QLabel { color: black; background: transparent; font-size: %1px; }"
        "QLabel#title { font-size: %2px; font-weight: bold; }"
        "QLabel#code { font-size: %3px; font-weight: bold; }"
        "QLabel#url { font-weight: bold; }"
        "QLabel#version { color: #555555; font-size: %6px; }"
        "QPushButton { color: black; background: white; border: 3px solid black; border-radius: %4px;"
        " font-size: %1px; padding: %4px %5px; }"
        "QPushButton:pressed { background: black; color: white; }"
        "QPushButton:disabled { color: #8a8a8a; border-color: #8a8a8a; }")
        .arg(unit).arg(unit * 2).arg(unit * 4).arg(unit / 2).arg(unit).arg(unit * 4 / 5));

    auto *homePage = new QWidget(pages_);
    auto *homeTitle = new QLabel("Relio", homePage);
    homeTitle->setObjectName("title");
    homeStatus_->setParent(homePage);
    homeStatus_->setWordWrap(true);
    connectButton_->setParent(homePage);
    auto *devicesButton = keyed(new TouchButton(QString(), homePage), "connectedDevices");
    updateStatus_->setParent(homePage);
    updateStatus_->setWordWrap(true);
    restartButton_->setParent(homePage);
    version_->setParent(homePage);
    version_->setObjectName("version");
    checkButton_->setParent(homePage);
    auto *closeHome = keyed(new TouchButton(QString(), homePage), "close");
    auto *home = new QVBoxLayout(homePage);
    home->setContentsMargins(unit * 2, unit * 2, unit * 2, unit * 2);
    home->setSpacing(unit);
    home->addWidget(homeTitle);
    home->addWidget(homeStatus_);
    home->addSpacing(unit);
    home->addWidget(connectButton_);
    home->addWidget(devicesButton);
    home->addSpacing(unit);
    home->addWidget(updateStatus_);
    home->addWidget(restartButton_);
    home->addStretch();
    auto *about = new QHBoxLayout();
    about->setSpacing(unit);
    about->addWidget(version_, 1);
    about->addWidget(checkButton_);
    home->addLayout(about);
    home->addWidget(closeHome);

    auto *pairingPage = new QWidget(pages_);
    auto *pairingTitle = keyed(new QLabel(pairingPage), "connectDevice");
    pairingTitle->setObjectName("title");
    qr_->setParent(pairingPage);
    qr_->setMinimumSize(window->width() / 2, window->width() / 2);
    code_->setParent(pairingPage);
    code_->setObjectName("code");
    code_->setAlignment(Qt::AlignCenter);
    instructions_->setParent(pairingPage);
    instructions_->setWordWrap(true);
    instructions_->setAlignment(Qt::AlignCenter);
    url_->setParent(pairingPage);
    url_->setObjectName("url");
    url_->setAlignment(Qt::AlignCenter);
    pairingStatus_->setParent(pairingPage);
    pairingStatus_->setAlignment(Qt::AlignCenter);
    auto *done = keyed(new TouchButton(QString(), pairingPage), "done");
    auto *pairing = new QVBoxLayout(pairingPage);
    pairing->setContentsMargins(unit * 2, unit * 2, unit * 2, unit * 2);
    pairing->setSpacing(unit);
    pairing->addWidget(pairingTitle);
    pairing->addWidget(qr_, 1);
    pairing->addWidget(code_);
    pairing->addWidget(instructions_);
    pairing->addWidget(url_);
    pairing->addWidget(pairingStatus_);
    pairing->addWidget(done);

    auto *resultPage = new QWidget(pages_);
    resultTitle_->setParent(resultPage);
    resultTitle_->setObjectName("title");
    resultMessage_->setParent(resultPage);
    resultMessage_->setWordWrap(true);
    auto *resultDone = keyed(new TouchButton(QString(), resultPage), "done");
    auto *result = new QVBoxLayout(resultPage);
    result->setContentsMargins(unit * 2, unit * 2, unit * 2, unit * 2);
    result->setSpacing(unit);
    result->addWidget(resultTitle_);
    result->addWidget(resultMessage_);
    result->addStretch();
    result->addWidget(resultDone);

    pages_->addWidget(homePage);
    pages_->addWidget(pairingPage);
    devices_ = new DevicesPage(daemon_, unit, pages_);
    devices_->setOnBack([this] { showHome(); });

    pages_->addWidget(resultPage);
    pages_->addWidget(devices_);
    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->addWidget(pages_);

    poll_->setInterval(kPollMs);
    QObject::connect(poll_, &QTimer::timeout, [this] { refreshPairing(); });
    QObject::connect(connectButton_, &QPushButton::clicked, [this] { showPairing(); });
    QObject::connect(devicesButton, &QPushButton::clicked, [this] {
        devices_->refresh();
        pages_->setCurrentWidget(devices_);
    });
    QObject::connect(restartButton_, &QPushButton::clicked, [this] { restartForUpdate(); });
    QObject::connect(checkButton_, &QPushButton::clicked, [this] { checkForUpdate(); });
    updatePoll_->setInterval(kPollMs);
    QObject::connect(updatePoll_, &QTimer::timeout, [this] { refreshUpdate(); });
    QObject::connect(closeHome, &QPushButton::clicked, [this] { close(); });
    QObject::connect(done, &QPushButton::clicked, [this] {
        daemon_.cancelPairing();
        close();
    });
    QObject::connect(resultDone, &QPushButton::clicked, [this] { close(); });
}

void Panel::open() {
    checkRequested_ = false;
    retranslate(this);
    setGeometry(parentWidget()->rect());
    showHome();
    raise();
    show();
}

void Panel::paintEvent(QPaintEvent *) {
    QPainter(this).fillRect(rect(), Qt::white);
}

void Panel::showHome() {
    poll_->stop();
    const bool running = daemon_.currentPairing().status != 0;
    homeStatus_->setText(running
        ? t("homeReady")
        : t("homeOffline"));
    connectButton_->setEnabled(running);
    refreshUpdate();
    pages_->setCurrentIndex(0);
}

// The outcome of a finished check is only shown to someone who asked for it here.
void Panel::refreshUpdate() {
    const QJsonObject update = daemon_.update().object();
    const QString state = update.value("state").toString();
    const QString version = update.value("version").toString();
    const QString last = update.value("last").toString();
    const bool busy = state == "checking" || state == "downloading";

    QString text;
    if (state == "ready") {
        text = t("updateReady").replace("{version}", version);
    } else if (state == "checking") {
        text = t("checkingUpdates");
    } else if (state == "downloading") {
        text = t("downloadingUpdate").replace("{version}", version);
    } else if (checkRequested_ && last == "up_to_date") {
        text = t("upToDate");
    } else if (checkRequested_ && last == "blocked") {
        text = t("updateBlocked");
    } else if (checkRequested_ && last == "failed") {
        text = t("updateCheckFailed");
    }
    updateStatus_->setText(text);
    updateStatus_->setVisible(!text.isEmpty());
    restartButton_->setVisible(state == "ready");

    const QString current = update.value("current").toString();
    version_->setText(current.isEmpty() ? QString() : "Relio " + current);
    checkButton_->setVisible(!current.isEmpty() && state != "ready");
    checkButton_->setEnabled(!busy);
    if (busy) {
        updatePoll_->start();
    } else {
        updatePoll_->stop();
    }
}

void Panel::checkForUpdate() {
    checkRequested_ = true;
    if (!daemon_.checkForUpdate()) {
        updateStatus_->setText(t("updateCheckFailed"));
        updateStatus_->setVisible(true);
        return;
    }
    refreshUpdate();
}

void Panel::restartForUpdate() {
    log("restarting to install an update");
    if (!nickelReboot() && !daemon_.restartForUpdate()) {
        log("restart unavailable");
        homeStatus_->setText(t("homeOffline"));
    }
}

void Panel::showPairing() {
    const DaemonReply reply = daemon_.startPairing();
    if (reply.status != 200 || reply.object().value("state").toString() != "waiting") {
        log(QString("pairing request failed with status %1").arg(reply.status));
        homeStatus_->setText(t("codeUnavailable"));
        return;
    }
    const QJsonObject qr = reply.object().value("qr").toObject();
    const QString url = reply.object().value("url").toString();
    qr_->setModules(qr.value("size").toInt(), qr.value("modules").toString());
    code_->setText(groupedCode(reply.object().value("code").toString()));
    instructions_->setText(url.isEmpty()
        ? t("openAddress")
        : t("scanOrOpen"));
    url_->setText(withoutFragment(url));
    url_->setVisible(!url.isEmpty());
    const int minutes = qMax(1, (reply.object().value("expires_in").toInt() + 59) / 60);
    pairingStatus_->setText(t("validMinutes").replace("{minutes}", Strings::locale().toString(minutes)));
    pages_->setCurrentIndex(1);
    poll_->start();
}

void Panel::refreshPairing() {
    const DaemonReply reply = daemon_.currentPairing();
    const QString state = reply.object().value("state").toString();
    if (state == "waiting") {
        return;
    }
    poll_->stop();
    if (state == "paired") {
        resultTitle_->setText(t("deviceConnectedTitle"));
        resultMessage_->setText(t("deviceConnectedMessage").replace("{device}", reply.object().value("name").toString()));
    } else {
        resultTitle_->setText(t("codeExpiredTitle"));
        resultMessage_->setText(t("codeExpiredMessage"));
    }
    pages_->setCurrentIndex(2);
}

void Panel::close() {
    poll_->stop();
    updatePoll_->stop();
    hide();
}

}  // namespace relio
