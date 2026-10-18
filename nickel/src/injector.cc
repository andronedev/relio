#include "injector.h"

#include <QApplication>
#include <QBoxLayout>
#include <QEvent>
#include <QFile>
#include <QFileSystemWatcher>
#include <QImage>
#include <QPainter>
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

    // An e-reader and a phone with an arrow each way, as in web/icon.svg, drawn on the
    // same 42-unit grid and 2.2-unit stroke as Nickel's status-bar icons. A dot on the
    // phone's corner means an update is ready.
    void drawIcon(const QRectF &box) {
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing);
        painter.translate(box.topLeft());
        painter.scale(box.width() / kGrid, box.height() / kGrid);
        QPen pen(Qt::black, 2.2);
        pen.setCapStyle(Qt::RoundCap);
        pen.setJoinStyle(Qt::RoundJoin);
        painter.setPen(pen);

        painter.drawRoundedRect(QRectF(2.5, 9, 15, 24), 2.5, 2.5);
        painter.drawLine(QLineF(6.5, 14, 13.5, 14));
        painter.drawLine(QLineF(6.5, 18.5, 13.5, 18.5));
        painter.drawLine(QLineF(6.5, 23, 11, 23));
        painter.drawRoundedRect(QRectF(30.5, 15, 9, 18), 2.2, 2.2);
        painter.drawLine(QLineF(33.5, 29.5, 36.5, 29.5));

        const QPointF toPhone[] = {{25, 15.5}, {28, 18.5}, {25, 21.5}};
        const QPointF toReader[] = {{23, 23}, {20, 26}, {23, 29}};
        painter.drawLine(QLineF(20, 18.5, 28, 18.5));
        painter.drawPolyline(toPhone, 3);
        painter.drawLine(QLineF(28, 26, 20, 26));
        painter.drawPolyline(toReader, 3);

        if (updateReady_) {
            painter.setPen(QPen(Qt::white, 2.4));
            painter.setBrush(Qt::black);
            painter.drawEllipse(QPointF(39, 14), 5.4, 5.4);
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
