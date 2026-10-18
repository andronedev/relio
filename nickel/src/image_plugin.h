#pragma once

#include <QImageIOPlugin>

// Qt only loads libraries from imageformats/ that carry plugin metadata, so
// Relio presents itself as an image plugin that handles no format.
class RelioImagePlugin : public QImageIOPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "org.qt-project.Qt.QImageIOHandlerFactoryInterface" FILE "image_plugin.json")

public:
    Capabilities capabilities(QIODevice *, const QByteArray &) const override { return 0; }
    QImageIOHandler *create(QIODevice *, const QByteArray & = QByteArray()) const override { return nullptr; }
};
