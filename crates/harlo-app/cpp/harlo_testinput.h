// Synthetic input for self-tests: real mouse and keyboard events sent
// into the app's window through Qt Test, so they travel the same path as
// a user's (hover, press/release, focus, popups).
#pragma once

#include <QtCore/QElapsedTimer>
#include <algorithm>
#include <QtCore/QPoint>
#include <QtCore/QString>
#include <QtGui/QGuiApplication>
#include <QtGui/QWindow>
#include <QtGui/QImage>
#include <QtGui/QWheelEvent>
#include <QtQuick/QQuickWindow>
// The mouse and keyboard helpers are declared for GUI builds only.
#ifndef QT_GUI_LIB
#define QT_GUI_LIB
#endif
#include <QtTest/qtest_gui.h>

namespace harlo {

inline QWindow *test_window()
{
    if (QWindow *w = QGuiApplication::focusWindow())
        return w;
    for (QWindow *w : QGuiApplication::topLevelWindows())
        if (w->isVisible())
            return w;
    return nullptr;
}

// Positions are in window coordinates (QML: item.mapToItem(null, x, y)).
inline void test_move(double x, double y)
{
    if (QWindow *w = test_window())
        QTest::mouseMove(w, QPoint(qRound(x), qRound(y)));
}

inline void test_click(double x, double y, int button)
{
    QWindow *w = test_window();
    if (!w)
        return;
    const QPoint p(qRound(x), qRound(y));
    QTest::mouseMove(w, p);
    QTest::mouseClick(w, Qt::MouseButton(button), Qt::NoModifier, p);
}

inline void test_key(int key, int modifiers)
{
    if (QWindow *w = test_window())
        QTest::keyClick(w, Qt::Key(key), Qt::KeyboardModifiers(modifiers));
}

// Character by character (keyClicks is for widgets only).
inline void test_type(const QString &text)
{
    QWindow *w = test_window();
    if (!w)
        return;
    for (const QChar c : text) {
        if (c.unicode() < 128)
            QTest::keyClick(w, c.toLatin1());
        else
            QTest::sendKeyEvent(QTest::Click, w, Qt::Key_unknown, QString(c), Qt::NoModifier);
    }
}

// A mouse wheel turned `dy` (120 a notch; negative: down) at a point.
inline void test_wheel(double x, double y, int dy)
{
    QWindow *w = test_window();
    if (!w)
        return;
    const QPointF p(x, y);
    QWheelEvent ev(p, w->mapToGlobal(p), QPoint(0, 0), QPoint(0, dy), Qt::NoButton, Qt::NoModifier, Qt::NoScrollPhase, false);
    // A clock of its own, a notch every 40 ms as a hand turns it: one
    // timestamp for them all reads as a single endless spin.
    static QElapsedTimer clock;
    static qint64 last = 0;
    if (!clock.isValid())
        clock.start();
    last = std::max(last + 40, clock.elapsed());
    ev.setTimestamp(ulong(last));
    QCoreApplication::sendEvent(w, &ev);
}

// The whole window as the user sees it, popups and shadows included.
inline bool test_grab_window(const QString &path)
{
    auto *w = qobject_cast<QQuickWindow *>(test_window());
    return w && w->grabWindow().save(path);
}

// Part of the window as it is now (an item's rectangle, in window
// coordinates): taken at once, unlike an item's grabToImage, which waits
// for the next frame and so caught whatever the test did after it.
inline bool test_grab_area(const QString &path, double x, double y, double w, double h)
{
    auto *win = qobject_cast<QQuickWindow *>(test_window());
    if (!win)
        return false;
    const QImage all = win->grabWindow();
    const qreal r = all.devicePixelRatio();
    const QRect area = QRectF(x * r, y * r, w * r, h * r).toAlignedRect().intersected(all.rect());
    return !area.isEmpty() && all.copy(area).save(path);
}

} // namespace harlo
