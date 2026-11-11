// The image viewer's Qt side: an item that shows an image as GPU textures
// (tiles, so any size fits) under a transform, so zooming, panning and
// turning only move the textures. Decoding (any thread) is here too, through
// Qt's image readers and whatever format plugins are installed. ImageView and
// ImageThumb (src/image_view.rs) derive from the item.
#pragma once

#include <QtCore/QBuffer>
#include <QtCore/QTimerEvent>
#include <QtGui/QColorSpace>
#include <QtGui/QImage>
#include <QtGui/QImageReader>
#include <QtGui/QMatrix4x4>
#include <QtGui/QMouseEvent>
#include <QtGui/QPointingDevice>
#include <QtGui/QNativeGestureEvent>
#include <QtGui/QWheelEvent>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>
#include <QtQuick/QSGImageNode>
#include <QtQuick/QSGTexture>
#include <QtQuick/QSGTransformNode>

#include <algorithm>
#include <cstdint>
#include <memory>
#include <vector>

namespace harlo {

// ---- decoding (any thread) -------------------------------------------------

inline void allow_big_images()
{
    // (Qt refuses images over 256 MB unless told otherwise.)
    static bool done = [] { QImageReader::setAllocationLimit(0); return true; }();
    Q_UNUSED(done);
}

// Ready for a texture: premultiplied RGBA in sRGB.
inline QImage for_screen(QImage img)
{
    if (img.isNull())
        return img;
    if (img.colorSpace().isValid() && img.colorSpace() != QColorSpace(QColorSpace::SRgb))
        img.convertToColorSpace(QColorSpace(QColorSpace::SRgb));
    if (img.format() != QImage::Format_RGBA8888_Premultiplied)
        img = img.convertToFormat(QImage::Format_RGBA8888_Premultiplied);
    return img;
}

// An image's size as shown (a photo's rotation applied); invalid if it
// can't be read.
inline void image_size(const QString &path, int &width, int &height)
{
    allow_big_images();
    QImageReader r(path);
    r.setAutoTransform(true);
    QSize s = r.size();
    if (s.isValid() && (r.transformation() & QImageIOHandler::TransformationRotate90))
        s.transpose();
    width = s.isValid() ? s.width() : -1;
    height = s.isValid() ? s.height() : -1;
}

// Decodes `path`, no side longer than `max_side` (0: whole). Formats that
// can (JPEG) decode straight at the smaller size, which is much faster.
inline QImage decode_image(const QString &path, int max_side)
{
    allow_big_images();
    QImageReader r(path);
    r.setAutoTransform(true);
    const QSize s = r.size();
    if (max_side > 0 && s.isValid() && std::max(s.width(), s.height()) > max_side)
        r.setScaledSize(s.scaled(max_side, max_side, Qt::KeepAspectRatio));
    QImage img = r.read();
    if (max_side > 0 && !img.isNull() && std::max(img.width(), img.height()) > max_side)
        img = img.scaled(max_side, max_side, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    return for_screen(std::move(img));
}

// A thumbnail at most `side` pixels each way, smoothly scaled.
inline QImage decode_thumbnail(const QString &path, int side)
{
    allow_big_images();
    QImageReader r(path);
    r.setAutoTransform(true);
    const QSize s = r.size();
    // (Decode at twice the size where the format can, then scale smoothly.)
    if (s.isValid() && std::max(s.width(), s.height()) > side * 2)
        r.setScaledSize(s.scaled(side * 2, side * 2, Qt::KeepAspectRatio));
    QImage img = r.read();
    if (!img.isNull() && std::max(img.width(), img.height()) > side)
        img = img.scaled(side, side, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    return for_screen(std::move(img));
}

inline QImage load_cached(const QString &path) { return for_screen(QImage(path)); }

// Whether a decoded image has any pixel that isn't opaque.
inline bool has_transparency(const QImage &img)
{
    if (!img.hasAlphaChannel())
        return false;
    if (img.format() != QImage::Format_RGBA8888_Premultiplied)
        return true;
    for (int y = 0; y < img.height(); ++y) {
        const uchar *row = img.constScanLine(y);
        for (int x = 0; x < img.width(); ++x)
            if (row[x * 4 + 3] != 255)
                return true;
    }
    return false;
}

// Saves a thumbnail: JPEG when it has no transparency, else PNG.
inline bool save_thumbnail(const QImage &img, const QString &path_without_suffix)
{
    if (img.isNull())
        return false;
    const bool alpha = has_transparency(img);
    return alpha ? img.save(path_without_suffix + ".png", "PNG") : img.convertToFormat(QImage::Format_RGB888).save(path_without_suffix + ".jpg", "JPG", 88);
}

// An image as a file's bytes for the phone: PNG when it has transparency
// (`png` set), else JPEG at `quality`.
inline QByteArray encode_for_phone(const QImage &img, int quality, bool &png)
{
    QByteArray out;
    if (img.isNull())
        return out;
    QBuffer buf(&out);
    buf.open(QIODevice::WriteOnly);
    // (`png` in: wanted lossless, a screenshot say; PNG with transparency
    // whatever was wanted.)
    png = png || has_transparency(img);
    if (png)
        img.save(&buf, "PNG");
    else
        img.convertToFormat(QImage::Format_RGB888).save(&buf, "JPG", quality);
    return out;
}

// The file suffixes Qt can read here (with the installed format plugins).
inline QString readable_suffixes()
{
    QStringList out;
    for (const QByteArray &f : QImageReader::supportedImageFormats())
        out << QString::fromLatin1(f).toLower();
    return out.join(QLatin1Char(','));
}

// The frames of an animated image, read one after another.
class FrameReader
{
public:
    explicit FrameReader(const QString &path)
        : m_reader(path)
    {
        allow_big_images();
        m_reader.setAutoTransform(true);
    }
    bool animated() const { return m_reader.supportsAnimation() && m_reader.imageCount() != 1; }
    // The next frame (null at the end), and how long it shows.
    QImage next_frame(int &delay_ms)
    {
        if (!m_reader.canRead())
            return QImage();
        QImage img = m_reader.read();
        delay_ms = m_reader.nextImageDelay();
        return for_screen(std::move(img));
    }

private:
    QImageReader m_reader;
};

inline std::unique_ptr<FrameReader> open_frames(const QString &path) { return std::make_unique<FrameReader>(path); }

// ---- the item ---------------------------------------------------------------

class ImageItem : public QQuickItem
{
    Q_OBJECT
public:
    explicit ImageItem(QQuickItem *parent = nullptr)
        : QQuickItem(parent)
    {
        setFlag(ItemHasContents, true);
        setAcceptedMouseButtons(Qt::LeftButton | Qt::MiddleButton);
        setAcceptHoverEvents(false);
    }

    double pixelRatio() const { return window() ? window()->effectiveDevicePixelRatio() : 1.0; }

    // Shows `image` (as decoded: premultiplied RGBA), uploaded on the next frame.
    void showImage(const QImage &image)
    {
        m_image = image;
        m_imageDirty = true;
        update();
    }
    void clearImage()
    {
        m_image = QImage();
        m_imageDirty = true;
        update();
    }
    // Item = image · matrix: x' = m11·x + m21·y + dx, y' = m12·x + m22·y + dy
    // (logical pixels).
    void setView(double m11, double m12, double m21, double m22, double dx, double dy)
    {
        m_view = QTransform(m11, m12, m21, m22, dx, dy);
        update();
    }
    // `nearest`: square pixels when zoomed in. (Shown smaller, the
    // mipmaps keep it smooth; zoomed in, only the whole image is sampled.)
    void setSampling(bool nearest)
    {
        if (nearest != m_nearest) {
            m_nearest = nearest;
            update();
        }
    }
    // A checkerboard under the image (shows transparency), in two colours (0xRRGGBB).
    void setChecker(bool on, std::uint32_t light, std::uint32_t dark)
    {
        m_checker = on;
        m_light = QColor::fromRgb(light);
        m_dark = QColor::fromRgb(dark);
        m_checkerDirty = true;
        update();
    }
    int startTicks(int ms) { return startTimer(ms); }
    void stopTicks(int id) { if (id > 0) killTimer(id); }
    void setGrabbing(bool on) { setCursor(on ? Qt::ClosedHandCursor : Qt::ArrowCursor); }
    void takeNoInput() { setAcceptedMouseButtons(Qt::NoButton); }

    // Hooks for Rust. kind: 0 press, 1 move, 2 release, 3 double click.
    virtual void pointerHook(int kind, double x, double y, int button, int modifiers) { Q_UNUSED(kind); Q_UNUSED(x); Q_UNUSED(y); Q_UNUSED(button); Q_UNUSED(modifiers); }
    // `pixels`: a touchpad's pixel deltas; else wheel notches (1 a notch).
    virtual bool wheelHook(double dx, double dy, double x, double y, int modifiers, bool pixels) { Q_UNUSED(dx); Q_UNUSED(dy); Q_UNUSED(x); Q_UNUSED(y); Q_UNUSED(modifiers); Q_UNUSED(pixels); return false; }
    // A touchpad pinch: `factor` (1 + change) about (x, y).
    virtual void pinchHook(double factor, double x, double y) { Q_UNUSED(factor); Q_UNUSED(x); Q_UNUSED(y); }
    virtual void sizeHook() {}
    virtual void timerHook(int id) { Q_UNUSED(id); }

protected:
    void timerEvent(QTimerEvent *e) override { timerHook(e->timerId()); }
    void mousePressEvent(QMouseEvent *e) override
    {
        forceActiveFocus(Qt::MouseFocusReason);
        pointerHook(0, e->position().x(), e->position().y(), int(e->button()), int(e->modifiers()));
    }
    void mouseMoveEvent(QMouseEvent *e) override { pointerHook(1, e->position().x(), e->position().y(), int(e->buttons()), int(e->modifiers())); }
    void mouseReleaseEvent(QMouseEvent *e) override { pointerHook(2, e->position().x(), e->position().y(), int(e->button()), int(e->modifiers())); }
    void mouseDoubleClickEvent(QMouseEvent *e) override { pointerHook(3, e->position().x(), e->position().y(), int(e->button()), int(e->modifiers())); }
    void wheelEvent(QWheelEvent *e) override
    {
        // A touchpad's pixel deltas (some systems give a wheel's too).
        const QPointF px = e->pixelDelta();
        const auto *dev = e->pointingDevice();
        const bool pixels = !px.isNull() && ((dev && dev->type() == QInputDevice::DeviceType::TouchPad) || e->phase() != Qt::NoScrollPhase);
        const QPointF d = pixels ? px : QPointF(e->angleDelta()) / 120.0;
        if (!wheelHook(d.x(), d.y(), e->position().x(), e->position().y(), int(e->modifiers()), pixels))
            e->ignore();
    }
    bool event(QEvent *e) override
    {
        if (e->type() == QEvent::NativeGesture) {
            auto *g = static_cast<QNativeGestureEvent *>(e);
            if (g->gestureType() == Qt::ZoomNativeGesture) {
                pinchHook(1.0 + g->value(), g->position().x(), g->position().y());
                return true;
            }
        }
        return QQuickItem::event(e);
    }
    void geometryChange(const QRectF &now, const QRectF &before) override
    {
        QQuickItem::geometryChange(now, before);
        if (now.size() != before.size())
            sizeHook();
    }

    QSGNode *updatePaintNode(QSGNode *old, UpdatePaintNodeData *) override
    {
        QQuickWindow *w = window();
        if (!w)
            return old;
        auto *root = old ? old : new QSGNode;
        if (!old) {
            m_checkerNode = nullptr;
            m_transform = new QSGTransformNode;
            root->appendChildNode(m_transform);
            m_imageDirty = true;
            m_checkerDirty = true;
        }
        // The image: textures of at most `tile` pixels a side.
        if (m_imageDirty) {
            m_imageDirty = false;
            while (QSGNode *n = m_transform->firstChild()) {
                m_transform->removeChildNode(n);
                delete n;
            }
            m_tiles.clear();
            m_size = m_image.size();
            const int tile = 8192;
            const bool alpha = m_image.hasAlphaChannel();
            for (int ty = 0; ty < m_image.height(); ty += tile) {
                for (int tx = 0; tx < m_image.width(); tx += tile) {
                    const QRect r(tx, ty, std::min(tile, m_image.width() - tx), std::min(tile, m_image.height() - ty));
                    const QImage part = r.size() == m_image.size() ? m_image : m_image.copy(r);
                    QQuickWindow::CreateTextureOptions opts = QQuickWindow::TextureHasMipmaps;
                    opts |= alpha ? QQuickWindow::TextureHasAlphaChannel : QQuickWindow::TextureIsOpaque;
                    QSGTexture *t = w->createTextureFromImage(part, opts);
                    if (!t)
                        continue;
                    auto *node = w->createImageNode();
                    node->setTexture(t);
                    node->setOwnsTexture(true);
                    // (Set once: the image isn't kept on the texture, so
                    // changing it later can't rebuild the mipmaps.)
                    node->setMipmapFiltering(QSGTexture::Linear);
                    node->setRect(QRectF(r));
                    node->setSourceRect(QRectF(QPointF(0, 0), QSizeF(r.size())));
                    m_transform->appendChildNode(node);
                    m_tiles.push_back(node);
                }
            }
        }
        for (QSGImageNode *n : m_tiles)
            n->setFiltering(m_nearest ? QSGTexture::Nearest : QSGTexture::Linear);
        m_transform->setMatrix(QMatrix4x4(m_view));
        // The checkerboard under it, in the item's pixels (its squares keep
        // their size at any zoom).
        if (m_checkerDirty || (m_checker && !m_checkerNode)) {
            m_checkerDirty = false;
            if (m_checkerNode) {
                root->removeChildNode(m_checkerNode);
                delete m_checkerNode;
                m_checkerNode = nullptr;
            }
            if (m_checker) {
                QImage cells(2, 2, QImage::Format_RGBA8888_Premultiplied);
                cells.setPixelColor(0, 0, m_light);
                cells.setPixelColor(1, 1, m_light);
                cells.setPixelColor(1, 0, m_dark);
                cells.setPixelColor(0, 1, m_dark);
                QSGTexture *t = w->createTextureFromImage(cells, QQuickWindow::TextureIsOpaque);
                t->setHorizontalWrapMode(QSGTexture::Repeat);
                t->setVerticalWrapMode(QSGTexture::Repeat);
                m_checkerNode = w->createImageNode();
                m_checkerNode->setTexture(t);
                m_checkerNode->setOwnsTexture(true);
                m_checkerNode->setFiltering(QSGTexture::Nearest);
                root->prependChildNode(m_checkerNode);
            }
        }
        if (m_checkerNode) {
            const QRectF shown = m_view.mapRect(QRectF(QPointF(0, 0), QSizeF(m_size))).intersected(boundingRect());
            const double cell = 8.0;
            m_checkerNode->setRect(shown);
            m_checkerNode->setSourceRect(QRectF(shown.x() / cell, shown.y() / cell, shown.width() / cell, shown.height() / cell));
        }
        return root;
    }

    void releaseResources() override
    {
        // (The scene graph deletes the nodes; they're made again from the
        // kept image.)
        m_tiles.clear();
        m_checkerNode = nullptr;
        m_transform = nullptr;
        m_imageDirty = true;
    }

private:
    QImage m_image;
    QSize m_size;
    bool m_imageDirty = false;
    QTransform m_view;
    bool m_nearest = false;
    bool m_checker = false;
    bool m_checkerDirty = false;
    QColor m_light, m_dark;
    QSGTransformNode *m_transform = nullptr;
    QSGImageNode *m_checkerNode = nullptr;
    std::vector<QSGImageNode *> m_tiles;
};

} // namespace harlo

// cxx-qt names a base class without its namespace.
using ImageItem = harlo::ImageItem;
