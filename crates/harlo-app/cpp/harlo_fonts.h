// System fonts and installed font families, for Settings → Appearance.
#pragma once

#include <QtGui/QFont>
#include <QtGui/QFontDatabase>
#include <QtGui/QGuiApplication>
#include <QtCore/QString>
#include <QtCore/QRegularExpression>
#include <QtCore/QStringList>

namespace harlo {

// The desktop's interface / monospace font family.
inline QString system_font_family(bool fixed)
{
    return QFontDatabase::systemFont(fixed ? QFontDatabase::FixedFont : QFontDatabase::GeneralFont).family();
}

// Installed families (only monospace ones when `fixed`), without the
// private "." families. Qt's fixed-pitch flag misses fonts with a few wide
// glyphs (JuliaMono, Maple Mono, Monaspace…) and flags some symbol fonts,
// so names are consulted too.
inline QStringList font_families(bool fixed)
{
    static const QRegularExpression monoName(QStringLiteral("mono|code|console|courier|term|fixed|typewriter"),
                                             QRegularExpression::CaseInsensitiveOption);
    static const QRegularExpression notText(QStringLiteral("emoji|signwriting|symbol|icons?\\b"),
                                            QRegularExpression::CaseInsensitiveOption);
    QStringList out;
    const QStringList all = QFontDatabase::families();
    for (const QString &family : all) {
        if (family.startsWith(QLatin1Char('.')) || QFontDatabase::isPrivateFamily(family))
            continue;
        if (fixed && (notText.match(family).hasMatch() || !(QFontDatabase::isFixedPitch(family) || monoName.match(family).hasMatch())))
            continue;
        out.append(family);
    }
    return out;
}

// Adds a font file (a resource) to the application's fonts; false if it
// couldn't be read.
inline bool add_application_font(const QString &path)
{
    return QFontDatabase::addApplicationFont(path) >= 0;
}

// Sets the application font family ("" for the desktop's), keeping its size.
inline void set_application_font_family(const QString &family)
{
    QFont font = QGuiApplication::font();
    font.setFamily(family.isEmpty() ? system_font_family(false) : family);
    QGuiApplication::setFont(font);
}

} // namespace harlo
