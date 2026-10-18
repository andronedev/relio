#pragma once

#include <QWidget>

#include "daemon.h"

class QLabel;
class QPushButton;
class QStackedWidget;
class QTimer;

namespace relio {

class DevicesPage;
class QrView;

// Full-window overlay shown from the status-bar icon.
class Panel : public QWidget {
public:
    explicit Panel(QWidget *window);

    void open();

protected:
    void paintEvent(QPaintEvent *event) override;

private:
    void showHome();
    void showPairing();
    void refreshUpdate();
    void checkForUpdate();
    void restartForUpdate();
    void refreshPairing();
    void close();

    Daemon daemon_;
    QStackedWidget *pages_;
    QLabel *homeStatus_;
    QPushButton *connectButton_;
    QLabel *updateStatus_;
    QPushButton *restartButton_;
    QPushButton *checkButton_;
    QLabel *version_;
    DevicesPage *devices_;
    QrView *qr_;
    QLabel *code_;
    QLabel *instructions_;
    QLabel *url_;
    QLabel *pairingStatus_;
    QLabel *resultTitle_;
    QLabel *resultMessage_;
    QTimer *poll_;
    QTimer *updatePoll_;
    bool checkRequested_ = false;
};

}  // namespace relio
