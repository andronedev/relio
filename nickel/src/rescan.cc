#include "rescan.h"

#include <dlfcn.h>

#include <QFile>
#include <QFileSystemWatcher>
#include <QTimer>

#include "log.h"

namespace relio {

namespace {
constexpr const char *kTriggerDir = "/tmp";
constexpr const char *kTrigger = "/tmp/relio-rescan";
constexpr int kDebounceMs = 1500;
}

Rescanner::Rescanner(QObject *parent)
    : QObject(parent), watcher_(new QFileSystemWatcher(this)), debounce_(new QTimer(this)) {
    if (void *nickel = dlopen("libnickel.so.1.0.0", RTLD_LAZY | RTLD_NOLOAD)) {
        sharedInstance_ = reinterpret_cast<void *(*)()>(dlsym(nickel, "_ZN19PlugWorkflowManager14sharedInstanceEv"));
        sync_ = reinterpret_cast<void (*)(void *)>(dlsym(nickel, "_ZN19PlugWorkflowManager4syncEv"));
    }
    if (!sharedInstance_ || !sync_) {
        log("library rescan unavailable: PlugWorkflowManager not found");
        return;
    }

    debounce_->setSingleShot(true);
    debounce_->setInterval(kDebounceMs);
    QObject::connect(debounce_, &QTimer::timeout, [this] { rescan(); });
    QObject::connect(watcher_, &QFileSystemWatcher::directoryChanged, [this] { checkTrigger(); });
    watcher_->addPath(kTriggerDir);
    checkTrigger();
}

void Rescanner::checkTrigger() {
    if (QFile::exists(kTrigger)) {
        debounce_->start();
    }
}

void Rescanner::rescan() {
    QFile::remove(kTrigger);
    void *manager = sharedInstance_();
    if (!manager) {
        log("library rescan skipped: no PlugWorkflowManager instance");
        return;
    }
    log("asking Nickel to rescan the library");
    sync_(manager);
}

}  // namespace relio
