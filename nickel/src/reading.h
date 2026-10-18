#pragma once

#include <QObject>
#include <QSet>

namespace relio {

// Keeps /tmp/relio-reading present while a book is open, so reliod never
// edits annotations Nickel holds in memory.
class ReadingTracker : public QObject {
public:
    explicit ReadingTracker(QObject *parent);

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void setReading(bool reading);

    QSet<QObject *> watched_;
};

}  // namespace relio
