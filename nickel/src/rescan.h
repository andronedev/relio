#pragma once

#include <QObject>

class QFileSystemWatcher;
class QTimer;

namespace relio {

// Imports books added or removed by reliod: the daemon drops a trigger file,
// and Nickel's own library sync runs as if the USB cable had been unplugged.
class Rescanner : public QObject {
public:
    explicit Rescanner(QObject *parent);

private:
    void checkTrigger();
    void rescan();

    QFileSystemWatcher *watcher_;
    QTimer *debounce_;
    void *(*sharedInstance_)() = nullptr;
    void (*sync_)(void *) = nullptr;
};

}  // namespace relio
