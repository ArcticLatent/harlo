// The code view's Qt side: an item that shows an image Rust drew and hands
// Rust its keys, mouse, wheel and input method as plain values. CodeView
// (src/code_view.rs) derives from it and overrides the hooks; so does the
// terminal (src/terminal_session.rs), which only draws.
#pragma once

#include <QtCore/QTimerEvent>
#include <QtGui/QClipboard>
#include <QtGui/QGuiApplication>
#include <QtGui/QImage>
#include <QtGui/QInputMethodEvent>
#include <QtGui/QKeyEvent>
#include <QtGui/QPainter>
#include <QtQuick/QQuickPaintedItem>
#include <QtQuick/QQuickWindow>

#include <cstdint>

namespace harlo {

class CodeItem : public QQuickPaintedItem
{
    Q_OBJECT
public:
    explicit CodeItem(QQuickItem *parent = nullptr)
        : QQuickPaintedItem(parent)
    {
        setFlag(ItemAcceptsInputMethod, true);
        setFlag(ItemIsFocusScope, false);
        setAcceptedMouseButtons(Qt::LeftButton | Qt::MiddleButton | Qt::RightButton);
        setAcceptHoverEvents(false);
        setCursor(Qt::IBeamCursor);
        setOpaquePainting(true);
        setAntialiasing(false);
        // The image is drawn pixel for pixel: shown unfiltered, so an item
        // at a fractional position stays sharp (filtered, it blurs).
        setSmooth(false);
    }

    // The window's device pixels per logical pixel.
    double pixelRatio() const { return window() ? window()->effectiveDevicePixelRatio() : 1.0; }

    // Shows `width`×`height` RGBA (premultiplied) pixels at the item's
    // top left, one image pixel to a device pixel.
    void blit(QPainter *painter, const std::uint8_t *data, int width, int height) const
    {
        QImage image(data, width, height, width * 4, QImage::Format_RGBA8888_Premultiplied);
        image.setDevicePixelRatio(pixelRatio());
        painter->setCompositionMode(QPainter::CompositionMode_Source);
        painter->drawImage(QPointF(0, 0), image);
    }

    // A repeating timer (`timerHook` with its id); a stopped one's id is 0.
    int startTicks(int ms) { return startTimer(ms); }
    void stopTicks(int id) { if (id > 0) killTimer(id); }
    // The input method asks again where the cursor is.
    void inputMethodMoved() const
    {
        if (hasActiveFocus())
            QGuiApplication::inputMethod()->update(Qt::ImCursorRectangle | Qt::ImEnabled);
    }
    // Only shows: the mouse and the keys go to the items around it.
    void takeNoInput()
    {
        setAcceptedMouseButtons(Qt::NoButton);
        setFlag(ItemAcceptsInputMethod, false);
        unsetCursor();
    }
    // The mouse pointer: an I-beam over the text, an arrow over the gutter.
    void setTextCursor(bool text) { setCursor(text ? Qt::IBeamCursor : Qt::ArrowCursor); }

    // Hooks for Rust. kind: 0 press, 1 move, 2 release, 3 double click.
    virtual bool keyHook(int key, int modifiers, const QString &text, bool press) { Q_UNUSED(key); Q_UNUSED(modifiers); Q_UNUSED(text); Q_UNUSED(press); return false; }
    virtual void pointerHook(int kind, double x, double y, int button, int modifiers) { Q_UNUSED(kind); Q_UNUSED(x); Q_UNUSED(y); Q_UNUSED(button); Q_UNUSED(modifiers); }
    // `pixels`: a touchpad's pixel deltas; else wheel notches (1 a notch).
    virtual bool wheelHook(double dx, double dy, int modifiers, bool pixels) { Q_UNUSED(dx); Q_UNUSED(dy); Q_UNUSED(modifiers); Q_UNUSED(pixels); return false; }
    virtual void inputHook(const QString &commit, const QString &preedit, int replaceStart, int replaceLength) { Q_UNUSED(commit); Q_UNUSED(preedit); Q_UNUSED(replaceStart); Q_UNUSED(replaceLength); }
    virtual void focusHook(bool in) { Q_UNUSED(in); }
    virtual void sizeHook() {}
    virtual void timerHook(int id) { Q_UNUSED(id); }
    // A key the view handles itself, kept from the app's shortcuts.
    virtual bool claimHook(int key, int modifiers, const QString &text) { Q_UNUSED(key); Q_UNUSED(modifiers); Q_UNUSED(text); return false; }
    virtual bool editable() const { return true; }
    // The cursor's rectangle in the item (for the input method's popup).
    virtual double cursorX() const { return 0; }
    virtual double cursorY() const { return 0; }
    virtual double cursorHeight() const { return 0; }

protected:
    bool event(QEvent *e) override
    {
        if (e->type() == QEvent::ShortcutOverride) {
            // (QML's Keys.onShortcutOverride first.)
            QQuickPaintedItem::event(e);
            auto *k = static_cast<QKeyEvent *>(e);
            if (!k->isAccepted() && claimHook(k->key(), int(k->modifiers()), k->text()))
                k->accept();
            return true;
        }
        return QQuickPaintedItem::event(e);
    }
    void timerEvent(QTimerEvent *e) override { timerHook(e->timerId()); }
    void keyPressEvent(QKeyEvent *e) override
    {
        if (!keyHook(e->key(), int(e->modifiers()), e->text(), true))
            e->ignore();
    }
    void keyReleaseEvent(QKeyEvent *e) override
    {
        if (!keyHook(e->key(), int(e->modifiers()), e->text(), false))
            e->ignore();
    }
    void mousePressEvent(QMouseEvent *e) override
    {
        if (!hasActiveFocus())
            forceActiveFocus(Qt::MouseFocusReason);
        pointerHook(0, e->position().x(), e->position().y(), int(e->button()), int(e->modifiers()));
    }
    void mouseMoveEvent(QMouseEvent *e) override
    {
        pointerHook(1, e->position().x(), e->position().y(), int(e->buttons()), int(e->modifiers()));
    }
    void mouseReleaseEvent(QMouseEvent *e) override
    {
        pointerHook(2, e->position().x(), e->position().y(), int(e->button()), int(e->modifiers()));
    }
    void mouseDoubleClickEvent(QMouseEvent *e) override
    {
        pointerHook(3, e->position().x(), e->position().y(), int(e->button()), int(e->modifiers()));
    }
    void wheelEvent(QWheelEvent *e) override
    {
        const QPointF px = e->pixelDelta();
        const bool pixels = !px.isNull();
        const QPointF d = pixels ? px : QPointF(e->angleDelta()) / 120.0;
        if (!wheelHook(d.x(), d.y(), int(e->modifiers()), pixels))
            e->ignore();
    }
    void inputMethodEvent(QInputMethodEvent *e) override
    {
        inputHook(e->commitString(), e->preeditString(), e->replacementStart(), e->replacementLength());
        e->accept();
    }
    QVariant inputMethodQuery(Qt::InputMethodQuery query) const override
    {
        switch (query) {
        case Qt::ImEnabled:
            return editable();
        case Qt::ImCursorRectangle:
            return QRectF(cursorX(), cursorY(), 1, cursorHeight());
        case Qt::ImHints:
            return int(Qt::ImhNoPredictiveText | Qt::ImhNoAutoUppercase);
        default:
            return QQuickPaintedItem::inputMethodQuery(query);
        }
    }
    void focusInEvent(QFocusEvent *e) override
    {
        QQuickPaintedItem::focusInEvent(e);
        focusHook(true);
    }
    void focusOutEvent(QFocusEvent *e) override
    {
        QQuickPaintedItem::focusOutEvent(e);
        focusHook(false);
    }
    void geometryChange(const QRectF &now, const QRectF &before) override
    {
        QQuickPaintedItem::geometryChange(now, before);
        if (now.size() != before.size())
            sizeHook();
    }
};

// The clipboard (`selection` the X11/Wayland primary selection).
inline QString clipboard_text(bool selection)
{
    QClipboard *c = QGuiApplication::clipboard();
    return c ? c->text(selection ? QClipboard::Selection : QClipboard::Clipboard) : QString();
}
inline void set_clipboard_text(const QString &text, bool selection)
{
    if (QClipboard *c = QGuiApplication::clipboard()) {
        if (!selection || c->supportsSelection())
            c->setText(text, selection ? QClipboard::Selection : QClipboard::Clipboard);
    }
}

} // namespace harlo

// cxx-qt names a base class without its namespace.
using CodeItem = harlo::CodeItem;
