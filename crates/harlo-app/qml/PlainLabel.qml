import QtQuick
import QtQuick.Controls.Basic

// A label for text harlo doesn't write: session titles, what an agent or
// the user wrote, file and branch names. Shown as it is, never read as
// HTML ("Fix the <div> overflow" keeps its <div>).
Label {
    textFormat: Text.PlainText
}
