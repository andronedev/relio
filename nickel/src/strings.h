#pragma once

#include <QLocale>
#include <QString>

class QWidget;

namespace relio {

// Translations come from the "kobo" section of i18n/*.json and follow the
// language chosen in the e-reader's settings, falling back to English.
class Strings {
public:
    static void reload();
    static QString get(const char *key);
    static QLocale locale();
};

inline QString t(const char *key) { return Strings::get(key); }

constexpr const char *kTextKeyProperty = "relioTextKey";

// Labels and buttons created through keyed() are relabelled by retranslate(),
// so a panel built once follows later language changes.
template <class Widget>
Widget *keyed(Widget *widget, const char *key) {
    widget->setProperty(kTextKeyProperty, key);
    widget->setText(t(key));
    return widget;
}

void retranslate(QWidget *root);

}  // namespace relio
