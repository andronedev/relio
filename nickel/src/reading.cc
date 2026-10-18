#include "reading.h"

#include <QApplication>
#include <QEvent>
#include <QFile>
#include <QWidget>

#include "log.h"

namespace relio {

namespace {
constexpr const char *kReadingViewClass = "ReadingView";
constexpr const char *kReadingFlag = "/tmp/relio-reading";
}

ReadingTracker::ReadingTracker(QObject *parent) : QObject(parent) {
    bool reading = false;
    for (QWidget *widget : QApplication::allWidgets()) {
        reading = reading || (widget->inherits(kReadingViewClass) && widget->isVisible());
    }
    setReading(reading);
}

bool ReadingTracker::eventFilter(QObject *watched, QEvent *event) {
    const QEvent::Type type = event->type();
    if ((type == QEvent::Show || type == QEvent::Hide) && watched->isWidgetType() && watched->inherits(kReadingViewClass)) {
        setReading(type == QEvent::Show);
        if (type == QEvent::Show && !watched_.contains(watched)) {
            watched_.insert(watched);
            QObject::connect(watched, &QObject::destroyed, this, [this, watched] {
                watched_.remove(watched);
                setReading(false);
            });
        }
    }
    return false;
}

void ReadingTracker::setReading(bool reading) {
    QFile flag(kReadingFlag);
    if (reading == flag.exists()) {
        return;
    }
    if (reading) {
        flag.open(QIODevice::WriteOnly);
    } else {
        flag.remove();
    }
    log(reading ? "book opened" : "book closed");
}

}  // namespace relio
