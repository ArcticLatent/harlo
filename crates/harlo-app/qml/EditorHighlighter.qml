import QtQuick
import org.kde.syntaxhighlighting
import harlo

// Live syntax highlighting for a TextEdit/TextArea via KDE's
// SyntaxHighlighting framework. Loaded through a Loader: where the
// framework is missing, the editor simply stays uncoloured.
Item {
    id: root
    property var textEdit: null
    property string fileName: ""
    readonly property string language: highlighter.definition && highlighter.definition.name !== "None" ? highlighter.definition.name : ""
    // KDE's closest match to the app theme, else its default dark/light one.
    readonly property var kdeNames: ({
        "Ayu": ["ayu Dark", "ayu Light"], "Catppuccin": ["Catppuccin Mocha", "Catppuccin Latte"],
        "Dracula": ["Dracula", ""], "GitHub": ["GitHub Dark", "GitHub Light"],
        "Gruvbox": ["gruvbox Dark", "gruvbox Light"], "Monokai": ["Monokai", ""], "Nord": ["Nord", ""],
        "One Dark Pro": ["Atom One Dark", "Atom One Light"], "Solarized": ["Solarized Dark", "Solarized Light"],
        "Tokyonight": ["Tokyo Night", "Tokyo Night Light"]
    })
    function kdeTheme() {
        const names = kdeNames[Workspace.themeFamily]
        const name = names ? names[Theme.isLight ? 1 : 0] : ""
        if (name !== "") {
            const t = Repository.theme(name)
            if (t.isValid) return t
        }
        return Repository.defaultTheme(Theme.isLight ? Repository.LightTheme : Repository.DarkTheme)
    }
    // SyntaxHighlighter is not an Item, so refer to the wrapper by id, not `parent`.
    SyntaxHighlighter {
        id: highlighter
        textEdit: root.textEdit
        definition: Repository.definitionForFileName(root.fileName)
        theme: root.kdeTheme()
    }
}
