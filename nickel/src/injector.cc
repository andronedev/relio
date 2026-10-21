#include "injector.h"

#include <QApplication>
#include <QBoxLayout>
#include <QEvent>
#include <QFile>
#include <QFileSystemWatcher>
#include <QImage>
#include <QPainter>
#include <QPainterPath>
#include <QPointer>
#include <QTimer>
#include <QWidget>

#include "log.h"
#include "panel.h"
#include "strings.h"

namespace relio {

namespace {

constexpr const char *kStatusBarClass = "StatusBarView";
constexpr const char *kIconRowName = "iconContainer";
constexpr const char *kAnchorIconName = "searchIcon";
constexpr const char *kInjectedProperty = "relioInjected";

// Bounding box of the dark pixels Nickel draws for an icon, in its own coordinates.
QRect glyphBounds(QWidget *icon) {
    const QImage image = icon->grab().toImage();
    QRect bounds;
    for (int y = 0; y < image.height(); ++y) {
        for (int x = 0; x < image.width(); ++x) {
            if (qGray(image.pixel(x, y)) < 128) {
                bounds |= QRect(x, y, 1, 1);
            }
        }
    }
    return bounds;
}

// reliod leaves this file while a downloaded update waits for a restart; /tmp is cleared by
// that restart, so the dot goes away once the update is installed.
constexpr const char *kUpdateFlag = "/tmp/relio-update-ready";

class StatusIcon : public QWidget {
public:
    StatusIcon(QWidget *row, QWidget *window, QWidget *anchor)
        : QWidget(row), window_(window), anchor_(anchor), watcher_(new QFileSystemWatcher(this)) {
        setObjectName("relioStatusIcon");
        setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Expanding);
        setAttribute(Qt::WA_AcceptTouchEvents);
        QObject::connect(watcher_, &QFileSystemWatcher::directoryChanged, [this] { checkUpdate(); });
        watcher_->addPath("/tmp");
        checkUpdate();
    }

    // Nickel's icons span the whole row and draw their glyph lower than the centre,
    // so the book copies the search glyph's size and position once it can be measured.
    QSize sizeHint() const override {
        const int side = anchor_ ? anchor_->sizeHint().height() : 0;
        return side > 0 ? QSize(side, side) : QSize(70, 70);
    }

protected:
    void resizeEvent(QResizeEvent *) override {
        if (glyph_.isNull()) {
            QTimer::singleShot(0, this, [this] { measureAnchor(); });
        }
    }

    void paintEvent(QPaintEvent *) override {
        const qreal side = glyph_.isValid() ? glyph_.height() : height() * 0.4;
        const qreal centerY = glyph_.isValid() ? glyph_.center().y() + 0.5 : height() / 2.0;
        drawIcon(QRectF((width() - side) / 2, centerY - side / 2, side, side));
    }

    bool event(QEvent *event) override {
        switch (event->type()) {
        case QEvent::TouchBegin:
            event->accept();
            return true;
        case QEvent::TouchEnd:
            openPanel();
            return true;
        default:
            return QWidget::event(event);
        }
    }

private:
    // Built once and reused: replacing the panel leaves Qt routing the next touches
    // to the destroyed one. open() relabels it in the e-reader's current language.
    void openPanel() {
        Strings::reload();
        if (!panel_) {
            panel_ = new Panel(window_);
        }
        panel_->open();
    }

    void checkUpdate() {
        const bool ready = QFile::exists(kUpdateFlag);
        if (ready != updateReady_) {
            updateReady_ = ready;
            update();
        }
    }

    void measureAnchor() {
        if (!glyph_.isNull() || !anchor_ || !anchor_->isVisible()) {
            return;
        }
        glyph_ = glyphBounds(anchor_);
        log(QString("search glyph %1,%2 %3x%4 in %5x%6")
                .arg(glyph_.x()).arg(glyph_.y()).arg(glyph_.width()).arg(glyph_.height())
                .arg(anchor_->width()).arg(anchor_->height()));
        update();
    }

    // An e-reader and a phone interlocked like two links, as in web/icon.svg, drawn on the
    // same 42-unit grid and 2.2-unit stroke as Nickel's status-bar icons. Each outline
    // breaks where the other passes over it. A dot in the free top-right corner means an
    // update is ready.
    void drawIcon(const QRectF &box) {
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing);
        painter.translate(box.topLeft());
        painter.scale(box.width() / kGrid, box.height() / kGrid);
        QPen pen(Qt::black, 2.2);
        pen.setCapStyle(Qt::RoundCap);
        pen.setJoinStyle(Qt::RoundJoin);
        painter.setPen(pen);

        QPainterPath reader(QPointF(17.1, 32));
        reader.arcTo(QRectF(6.5, 25, 7, 7), 270, -90);
        reader.arcTo(QRectF(6.5, 3, 7, 7), 180, -90);
        reader.arcTo(QRectF(20.5, 3, 7, 7), 90, -90);
        reader.arcTo(QRectF(20.5, 25, 7, 7), 0, -90);
        reader.lineTo(23.9, 32);
        painter.drawPath(reader);

        QPainterPath phone(QPointF(30.9, 15));
        phone.arcTo(QRectF(29.5, 15, 6, 6), 90, -90);
        phone.arcTo(QRectF(29.5, 33, 6, 6), 0, -90);
        phone.arcTo(QRectF(20.5, 33, 6, 6), 270, -90);
        phone.arcTo(QRectF(20.5, 15, 6, 6), 180, -90);
        phone.lineTo(24.1, 15);
        painter.drawPath(phone);

        if (updateReady_) {
            painter.setPen(Qt::NoPen);
            painter.setBrush(Qt::black);
            painter.drawEllipse(QPointF(36, 6.5), 5, 5);
        }
    }

    static constexpr qreal kGrid = 42;

    QWidget *window_;
    QPointer<Panel> panel_;
    QWidget *anchor_;
    QRect glyph_;
    QFileSystemWatcher *watcher_;
    bool updateReady_ = false;
};

void dumpTree(QObject *object, int depth, QString &out) {
    const QWidget *widget = qobject_cast<QWidget *>(object);
    out += QString("%1%2 \"%3\"").arg(QString(depth * 2, ' '), object->metaObject()->className(), object->objectName());
    if (widget) {
        const QRect g = widget->geometry();
        out += QString(" [%1,%2 %3x%4]%5").arg(g.x()).arg(g.y()).arg(g.width()).arg(g.height()).arg(widget->isVisible() ? "" : " hidden");
        if (widget->layout()) {
            out += QString(" layout=%1").arg(widget->layout()->metaObject()->className());
        }
    }
    out += '\n';
    for (QObject *child : object->children()) {
        dumpTree(child, depth + 1, out);
    }
}

}  // namespace

void Injector::scanExisting() {
    int found = 0;
    for (QWidget *widget : QApplication::allWidgets()) {
        if (widget->inherits(kStatusBarClass)) {
            ++found;
            injectOnce(widget);
        }
    }
    log(QString("startup scan: %1 widgets, %2 status bars").arg(QApplication::allWidgets().size()).arg(found));
}

bool Injector::eventFilter(QObject *watched, QEvent *event) {
    if (event->type() == QEvent::Show && watched->isWidgetType() && watched->inherits(kStatusBarClass)) {
        injectOnce(static_cast<QWidget *>(watched));
    }
    return false;
}

void Injector::injectOnce(QWidget *statusBar) {
    if (statusBar->property(kInjectedProperty).toBool()) {
        return;
    }
    statusBar->setProperty(kInjectedProperty, true);
    inject(statusBar);
}

void Injector::inject(QWidget *statusBar) {
    if (!dumped_) {
        dumped_ = true;
        QString tree;
        dumpTree(statusBar, 0, tree);
        log("status bar structure:\n" + tree);
    }

    QWidget *row = statusBar->findChild<QWidget *>(kIconRowName);
    auto *rowLayout = row ? qobject_cast<QBoxLayout *>(row->layout()) : nullptr;
    if (!rowLayout) {
        log("icon row not found; leaving the status bar untouched");
        return;
    }
    if (row->findChild<QWidget *>("relioStatusIcon", Qt::FindDirectChildrenOnly)) {
        return;
    }

    QWidget *anchor = row->findChild<QWidget *>(kAnchorIconName, Qt::FindDirectChildrenOnly);
    auto *icon = new StatusIcon(row, statusBar->window(), anchor);
    const int index = anchor ? rowLayout->indexOf(anchor) : -1;
    rowLayout->insertWidget(index, icon);
    icon->show();
    log(QString("icon inserted %1").arg(anchor ? "before searchIcon" : "at the end of the icon row"));
}

}  // namespace relio
