#pragma once

#include <QEvent>
#include <QPushButton>

namespace relio {

// Nickel delivers touch events without synthesizing mouse clicks.
class TouchButton : public QPushButton {
public:
    TouchButton(const QString &text, QWidget *parent) : QPushButton(text, parent) {
        setAttribute(Qt::WA_AcceptTouchEvents);
        setFocusPolicy(Qt::NoFocus);
    }

protected:
    bool event(QEvent *event) override {
        switch (event->type()) {
        case QEvent::TouchBegin:
            setDown(isEnabled());
            event->accept();
            return true;
        case QEvent::TouchEnd:
            setDown(false);
            if (isEnabled()) {
                click();
            }
            return true;
        default:
            return QPushButton::event(event);
        }
    }
};

}  // namespace relio
