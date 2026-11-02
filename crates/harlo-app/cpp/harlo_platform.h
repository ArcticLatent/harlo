// App-level platform bits: window icon and desktop app id.
#pragma once

#include <QtGui/QClipboard>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QImage>
#include <QtGui/QPainter>
#include <QtGui/QKeySequence>
#include <QtWidgets/QApplication>
#include <QtCore/QByteArray>
#include <QtCore/QEvent>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QDateTime>
#include <QtCore/QFile>
#include <QtCore/QFileInfo>
#include <QtCore/QMimeData>
#include <QtCore/QUrl>
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

// The clipboard's files, if it holds only files on this machine.
inline QStringList clipboard_files()
{
    const QMimeData *mime = QGuiApplication::clipboard()->mimeData();
    QStringList files;
    if (!mime || !mime->hasUrls())
        return files;
    for (const QUrl &url : mime->urls()) {
        if (!url.isLocalFile())
            return {};
        files.append(url.toLocalFile());
    }
    return files;
}

// Saves the clipboard's image as `base`.png, or `base`.jpg when a PNG would be
// over what agents take (5 MB once encoded). Returns its path; "" when the
// clipboard holds text (a browser's copied image may carry its address too, a
// spreadsheet's cells carry a picture of themselves).
inline QString clipboard_image_to(const QString &base)
{
    const QMimeData *mime = QGuiApplication::clipboard()->mimeData();
    if (!mime || !mime->hasImage())
        return {};
    const QString text = mime->text().trimmed();
    if (!text.isEmpty() && !(text.startsWith(QStringLiteral("http")) && !text.contains(QLatin1Char(' '))))
        return {};
    QImage image = qvariant_cast<QImage>(mime->imageData());
    if (image.isNull())
        return {};
    const int largest = 2000; // Bigger ones are refused next to other images.
    if (image.width() > largest || image.height() > largest)
        image = image.scaled(largest, largest, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    const QString png = base + QStringLiteral(".png");
    if (!image.save(png, "PNG"))
        return {};
    if (QFileInfo(png).size() <= 3500 * 1024)
        return png;
    QImage flat(image.size(), QImage::Format_RGB32);
    flat.fill(Qt::white);
    QPainter(&flat).drawImage(0, 0, image);
    const QString jpg = base + QStringLiteral(".jpg");
    QFile::remove(png);
    return flat.save(jpg, "JPEG", 85) ? jpg : QString();
}

// Runs the event loop until the app quits.
inline int exec_application()
{
    return QApplication::exec();
}

// After the event loop and the QML engine: the objects still waiting to be
// deleted (deleteLater), then the application itself. Left to the process's
// exit, Qt deletes those after the application and the engine are gone,
// and a Quick item's destructor reads freed type data (a crash on quit).
inline void shutdown_application()
{
    QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
    delete qApp;
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
