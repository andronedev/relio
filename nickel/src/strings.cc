#include "strings.h"

#include <cstring>

#include <QAbstractButton>
#include <QLabel>
#include <QSettings>

namespace relio {

namespace {

#include "strings_table.inc"

constexpr const char *kNickelSettings = "/mnt/onboard/.kobo/Kobo/Kobo eReader.conf";

int current = 0;

int languageIndex(const QString &code) {
    for (int i = 0; i < kLanguageCount; ++i) {
        if (code.compare(QLatin1String(kLanguages[i]), Qt::CaseInsensitive) == 0) {
            return i;
        }
    }
    return -1;
}

// Nickel stores codes such as "fr", "pt_BR" or "zh_TW".
int resolve(QString code) {
    code.replace('_', '-');
    const QString base = code.section('-', 0, 0).toLower();
    if (base == "zh") {
        const bool traditional = code.contains("TW", Qt::CaseInsensitive) || code.contains("HK", Qt::CaseInsensitive) ||
                                 code.contains("Hant", Qt::CaseInsensitive);
        code = traditional ? "zh-Hant" : "zh-Hans";
    } else if (base == "no" || base == "nn") {
        code = "nb";
    } else if (code.compare("pt-PT", Qt::CaseInsensitive) == 0) {
        code = "pt";
    }
    const int exact = languageIndex(code);
    if (exact >= 0) {
        return exact;
    }
    const int fallback = languageIndex(base);
    return fallback >= 0 ? fallback : 0;
}

}  // namespace

void Strings::reload() {
    const QString code = QSettings(kNickelSettings, QSettings::IniFormat).value("ApplicationPreferences/CurrentLocale").toString();
    current = resolve(code);
}

QString Strings::get(const char *key) {
    for (int i = 0; i < kKeyCount; ++i) {
        if (std::strcmp(kKeys[i], key) == 0) {
            return QString::fromUtf8(kStrings[current][i]);
        }
    }
    return QString::fromLatin1(key);
}

QLocale Strings::locale() {
    return QLocale(QString::fromLatin1(kLanguages[current]).replace('-', '_'));
}

void retranslate(QWidget *root) {
    for (QWidget *widget : root->findChildren<QWidget *>()) {
        const QByteArray key = widget->property(kTextKeyProperty).toByteArray();
        if (key.isEmpty()) {
            continue;
        }
        if (auto *label = qobject_cast<QLabel *>(widget)) {
            label->setText(t(key.constData()));
        } else if (auto *button = qobject_cast<QAbstractButton *>(widget)) {
            button->setText(t(key.constData()));
        }
    }
}

}  // namespace relio
