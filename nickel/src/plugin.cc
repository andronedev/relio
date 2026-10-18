// Loaded by Nickel from /usr/local/Kobo/imageformats: Qt dlopens every library
// there, which runs the constructor below even though this is not a real
// image plugin.

#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>

#include <QApplication>
#include <QFile>
#include <QTimer>

#include "injector.h"
#include "log.h"
#include "reading.h"
#include "rescan.h"

namespace {

constexpr int kFailsafeDelayMs = 20000;

// While loading, the library is renamed so that a crash during boot leaves
// Nickel without the plugin on the next start. It is restored once Nickel has
// run normally for a while.
class Failsafe {
public:
    bool arm() {
        Dl_info info;
        if (!dladdr(reinterpret_cast<void *>(&relio::log), &info) || !info.dli_fname ||
            !realpath(info.dli_fname, path_)) {
            return false;
        }
        snprintf(disarmedPath_, sizeof(disarmedPath_), "%s.failsafe", path_);
        if (!dlopen(path_, RTLD_LAZY | RTLD_NODELETE)) {
            return false;
        }
        return rename(path_, disarmedPath_) == 0;
    }

    void disarm() const {
        if (rename(disarmedPath_, path_) != 0) {
            relio::log("failsafe: could not restore plugin");
        }
    }

private:
    char path_[PATH_MAX] = {};
    char disarmedPath_[PATH_MAX + 16] = {};
};

bool disabledByUser() {
    return QFile::exists(QString(relio::kStateDir) + "/disable-plugin");
}

__attribute__((constructor)) void start() {
    static Failsafe failsafe;
    if (!failsafe.arm()) {
        relio::log("failsafe could not be armed; not starting");
        return;
    }
    if (disabledByUser()) {
        relio::log("disabled by .adds/relio/disable-plugin");
        failsafe.disarm();
        return;
    }
    QCoreApplication *app = QCoreApplication::instance();
    if (!app) {
        relio::log("no Qt application yet; not starting");
        failsafe.disarm();
        return;
    }

    auto *injector = new relio::Injector(app);
    app->installEventFilter(injector);
    QTimer::singleShot(0, injector, [injector] { injector->scanExisting(); });
    new relio::Rescanner(app);
    app->installEventFilter(new relio::ReadingTracker(app));
    QTimer::singleShot(kFailsafeDelayMs, [] { failsafe.disarm(); });
    relio::log("plugin loaded");
}

}  // namespace
