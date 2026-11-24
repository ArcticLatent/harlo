import Foundation

/// A part of an extension's Markdown: prose, or a code block (`html` as a
/// reply's, with `code` its text and `lang` its language).
public struct ExtMarkdownPart: Equatable, Sendable {
    public var isCode: Bool
    public var html: String
    public var code: String
    public var lang: String

    public init(_ v: JSON) {
        isCode = v["kind"].text == "code"
        html = v["html"].text
        code = v["code"].text
        lang = v["lang"].text
    }
}
