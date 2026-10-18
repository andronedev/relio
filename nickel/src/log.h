#pragma once

#include <QString>

namespace relio {

constexpr const char *kStateDir = "/mnt/onboard/.adds/relio";

void log(const QString &message);

}  // namespace relio
