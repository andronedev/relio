#pragma once

#include <QObject>

class QWidget;

namespace relio {

// Watches widget show events and adds the Relio icon to Nickel's status bar.
class Injector : public QObject {
public:
    using QObject::QObject;

    // Status bars shown before the plugin was loaded never send another Show event.
    void scanExisting();

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void injectOnce(QWidget *statusBar);
    void inject(QWidget *statusBar);
    bool dumped_ = false;
};

}  // namespace relio
