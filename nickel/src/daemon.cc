#include "daemon.h"

#include <QJsonDocument>
#include <QLocalSocket>
#include <QUrl>

namespace relio {

namespace {
constexpr const char *kControlSocket = "/tmp/reliod.sock";
constexpr int kTimeoutMs = 1500;
}

DaemonReply Daemon::startPairing() { return request("POST", "/pairing"); }

DaemonReply Daemon::currentPairing() { return request("GET", "/pairing"); }

void Daemon::cancelPairing() { request("DELETE", "/pairing"); }

DaemonReply Daemon::clients() { return request("GET", "/clients"); }

bool Daemon::revokeClient(const QString &id) {
    return request("DELETE", "/clients/" + QUrl::toPercentEncoding(id)).status == 204;
}

DaemonReply Daemon::update() { return request("GET", "/update"); }

bool Daemon::checkForUpdate() { return request("POST", "/update/check").status == 202; }

bool Daemon::restartForUpdate() { return request("POST", "/update/restart").status == 202; }

DaemonReply Daemon::request(const QByteArray &method, const QByteArray &path) {
    DaemonReply reply;
    QLocalSocket socket;
    socket.connectToServer(kControlSocket);
    if (!socket.waitForConnected(kTimeoutMs)) {
        return reply;
    }
    socket.write(method + " " + path + " HTTP/1.1\r\nHost: reliod\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    socket.waitForBytesWritten(kTimeoutMs);

    QByteArray raw;
    while (socket.state() == QLocalSocket::ConnectedState && socket.waitForReadyRead(kTimeoutMs)) {
        raw += socket.readAll();
    }
    raw += socket.readAll();

    const int headerEnd = raw.indexOf("\r\n\r\n");
    const QList<QByteArray> statusLine = raw.left(raw.indexOf("\r\n")).split(' ');
    if (headerEnd < 0 || statusLine.size() < 2) {
        return reply;
    }
    reply.status = statusLine.at(1).toInt();
    reply.body = QJsonDocument::fromJson(raw.mid(headerEnd + 4));
    return reply;
}

}  // namespace relio
