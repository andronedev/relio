#pragma once

#include <functional>

#include <QString>
#include <QWidget>

class QLabel;
class QStackedWidget;
class QVBoxLayout;

namespace relio {

class Daemon;

// Lists the browsers and phones paired with this e-reader and disconnects them.
class DevicesPage : public QWidget {
public:
    DevicesPage(Daemon &daemon, int unit, QWidget *parent);

    void refresh();
    void setOnBack(std::function<void()> onBack) { onBack_ = std::move(onBack); }

private:
    void clearRows();
    void addRow(const QString &id, const QString &name, const QString &pairedOn);
    void confirm(const QString &id, const QString &name);

    Daemon &daemon_;
    int unit_;
    QStackedWidget *views_;
    QVBoxLayout *rows_;
    QLabel *summary_;
    QLabel *confirmTitle_;
    QString pendingId_;
    std::function<void()> onBack_;
};

}  // namespace relio
