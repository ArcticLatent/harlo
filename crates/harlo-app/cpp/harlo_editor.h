// The file editor's multi-part edits (replace all, indenting or commenting
// lines, a new line's indentation): applied to a TextEdit's document in one
// edit block, so a single Ctrl+Z undoes the lot.
#pragma once

#include <QtCore/QJsonArray>
#include <QtCore/QJsonDocument>
#include <QtCore/QString>
#include <QtCore/QVariant>
#include <QtGui/QTextCursor>
#include <QtGui/QTextDocument>
#include <QtQuick/QQuickTextDocument>

#include <algorithm>
#include <vector>

namespace harlo {

// `doc` is a TextEdit's `textDocument`; `edits` is JSON, [[start, end,
// text], ...] in the text as it is now (they mustn't overlap). False if
// either can't be read.
inline bool editor_apply(const QVariant &doc, const QString &edits)
{
    auto *quick = qobject_cast<QQuickTextDocument *>(doc.value<QObject *>());
    QTextDocument *d = quick ? quick->textDocument() : nullptr;
    if (!d)
        return false;
    struct Edit { int start, end; QString text; };
    std::vector<Edit> list;
    const QJsonArray all = QJsonDocument::fromJson(edits.toUtf8()).array();
    for (const auto &v : all) {
        const QJsonArray e = v.toArray();
        if (e.size() != 3)
            return false;
        list.push_back({e[0].toInt(), e[1].toInt(), e[2].toString()});
    }
    // From the end, so each one's positions still hold.
    std::sort(list.begin(), list.end(), [](const Edit &a, const Edit &b) { return a.start > b.start; });
    const int last = d->characterCount() - 1;
    QTextCursor c(d);
    c.beginEditBlock();
    for (const Edit &e : list) {
        const int start = std::clamp(std::min(e.start, e.end), 0, last);
        const int end = std::clamp(std::max(e.start, e.end), 0, last);
        c.setPosition(start);
        c.setPosition(end, QTextCursor::KeepAnchor);
        c.insertText(e.text);
    }
    c.endEditBlock();
    return true;
}

// Adds `text` at the end of a TextEdit's document (even a read-only one),
// kept out of its undo history: a long file filled in a slice at a time.
inline bool editor_append(const QVariant &doc, const QString &text)
{
    auto *quick = qobject_cast<QQuickTextDocument *>(doc.value<QObject *>());
    QTextDocument *d = quick ? quick->textDocument() : nullptr;
    if (!d)
        return false;
    const bool undo = d->isUndoRedoEnabled();
    d->setUndoRedoEnabled(false);
    QTextCursor c(d);
    c.movePosition(QTextCursor::End);
    c.insertText(text);
    d->setUndoRedoEnabled(undo);
    return true;
}

} // namespace harlo
