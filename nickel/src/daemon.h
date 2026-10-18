#pragma once

#include <QByteArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>

namespace relio {

struct DaemonReply {
    int status = 0;  // 0 when reliod is unreachable
    QJsonDocument body;

    QJsonObject object() const { return body.object(); }
};

// Talks to reliod over its control socket. Calls are short and blocking.
class Daemon {
public:
    DaemonReply startPairing();
    DaemonReply currentPairing();
    void cancelPairing();
    DaemonReply clients();
    bool revokeClient(const QString &id);
    DaemonReply update();
    bool checkForUpdate();
    bool restartForUpdate();

private:
    DaemonReply request(const QByteArray &method, const QByteArray &path);
};

}  // namespace relio
