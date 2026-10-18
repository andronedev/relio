#include "log.h"

#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QTextStream>

namespace relio {

namespace {
constexpr qint64 kMaxLogBytes = 256 * 1024;
}

void log(const QString &message) {
    QDir().mkpath(kStateDir);
    QFile file(QString(kStateDir) + "/plugin.log");
    const bool tooBig = file.exists() && file.size() > kMaxLogBytes;
    if (!file.open(QIODevice::WriteOnly | QIODevice::Text | (tooBig ? QIODevice::Truncate : QIODevice::Append))) {
        return;
    }
    QTextStream(&file) << QDateTime::currentDateTime().toString(Qt::ISODate) << ' ' << message << '\n';
}

}  // namespace relio
