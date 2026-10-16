// App-level platform bits: window icon and desktop app id.
#pragma once

#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QKeySequence>
#include <QtWidgets/QApplication>
#include <QtCore/QByteArray>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QDateTime>
#include <QtCore/QFile>
#include <QtCore/QMutex>
#include <QtCore/QTextStream>

#include <vector>

namespace harlo {

// Creates the application object: a QApplication (widgets-capable), so the
// desktop's tray integration (KDE's needs widgets) and native dialogs work.
// `args` become argv, kept alive for the application's lifetime.
inline void create_application(const QStringList &args)
{
    static std::vector<QByteArray> storage;
    static std::vector<char *> argv;
    static int argc = 0;
    for (const QString &a : args)
        storage.push_back(a.toLocal8Bit());
    for (QByteArray &s : storage)
        argv.push_back(s.data());
    argv.push_back(nullptr);
    argc = int(storage.size());
    new QApplication(argc, argv.data());
    QCoreApplication::setOrganizationName(QStringLiteral("harlo"));
    QCoreApplication::setApplicationName(QStringLiteral("harlo"));
}

// Runs the event loop until the app quits.
inline int exec_application()
{
    return QApplication::exec();
}

// The icon of every window (from a resource or file path).
inline void set_window_icon(const QString &path)
{
    QGuiApplication::setWindowIcon(QIcon(path));
}

// The desktop entry name (Wayland app id), so the desktop shows the right icon.
inline void set_desktop_file_name(const QString &name)
{
    QGuiApplication::setDesktopFileName(name);
}

// Copies Qt and QML messages (warnings, binding errors…) into the log file
// at `path`, keeping the normal output on stderr.
inline void install_message_log(const QString &path)
{
    static QFile file;
    static QMutex mutex;
    static QtMessageHandler previous = nullptr;
    file.setFileName(path);
    if (!file.open(QIODevice::Append | QIODevice::Text))
        return;
    previous = qInstallMessageHandler([](QtMsgType type, const QMessageLogContext &ctx, const QString &msg) {
        const char *level = type == QtDebugMsg ? "DEBUG" : type == QtInfoMsg ? "INFO" : type == QtWarningMsg ? "WARN"
                          : type == QtCriticalMsg ? "ERROR" : "FATAL";
        {
            QMutexLocker lock(&mutex);
            QTextStream out(&file);
            out << QDateTime::currentDateTimeUtc().toString(Qt::ISODateWithMs) << "  " << level << " qt"
                << (ctx.category && qstrcmp(ctx.category, "default") != 0 ? QStringLiteral(" [%1]").arg(QString::fromUtf8(ctx.category)) : QString())
                << ": " << msg;
            if (ctx.file)
                out << " (" << ctx.file << ":" << ctx.line << ")";
            out << "\n";
            out.flush();
        }
        if (previous)
            previous(type, ctx, msg);
    });
}

// A key press as shortcut text ("Ctrl+Shift+T"), as Shortcut.sequence takes it.
inline QString key_sequence_text(int key, int modifiers)
{
    const Qt::KeyboardModifiers mods = Qt::KeyboardModifiers(modifiers) & (Qt::ControlModifier | Qt::AltModifier | Qt::ShiftModifier | Qt::MetaModifier);
    return QKeySequence(QKeyCombination(mods, Qt::Key(key))).toString(QKeySequence::PortableText);
}

} // namespace harlo
