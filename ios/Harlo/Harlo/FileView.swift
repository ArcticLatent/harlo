import HarloKit
import SwiftUI
import UIKit

/// A file a reply mentions (`src/app.rs:42`), read-only and highlighted,
/// at that line.
struct FileView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    let path: String
    let line: Int
    @State private var file: FileText?
    @State private var problem: String?
    @AppStorage(CodeWrap.key) private var wrap = true

    var name: String { path.split(separator: "/").last.map(String.init) ?? path }

    var body: some View {
        NavigationStack {
            Group {
                if let f = file {
                    lines(f)
                } else if let problem {
                    ContentUnavailableView(name, systemImage: "doc.questionmark", description: Text(problem))
                } else {
                    ProgressView()
                }
            }
            .screen()
            .navigationTitle(name)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
                if let f = file {
                    ToolbarItem(placement: .topBarTrailing) {
                        ViewMenu(wrap: $wrap) {
                            UIPasteboard.general.string = (0..<f.lines.count).map { RichText.plain(f.spans($0)) }.joined(separator: "\n")
                        }
                    }
                }
            }
        }
        .task {
            let before = model.alert
            file = await model.file(path)
            if file == nil {
                // Said here, not as an alert over the sheet.
                problem = model.alert ?? "Couldn't open it."
                model.alert = before
            }
        }
    }

    /// A line's number, as wide as the last one's.
    func number(_ n: Int, _ width: Int) -> some View {
        Text(String(n).leftPadded(to: width))
            .font(Style.mono(12))
            .foregroundStyle(Color(p.muted))
            .fixedSize()
    }

    func lines(_ f: FileText) -> some View {
        let width = String(f.lines.count).count
        // (Scrolling both ways, a file shorter or narrower than the screen
        // sat in its middle: its content is at least the screen, from the top.)
        return GeometryReader { room in ScrollViewReader { proxy in
            ScrollView(wrap ? Axis.Set.vertical : [.vertical, .horizontal]) {
                // Lines kept whole: as wide as the longest, which a lazy
                // stack can't know (it lays out only the lines on screen;
                // narrower, its lines stuck out on both sides).
                VStack(alignment: .leading, spacing: 0) {
                    if !wrap, let i = f.longestLine {
                        HStack(spacing: 10) {
                            number(i + 1, width)
                            Text(Style.attributed(f.spans(i), p, size: 14, inCode: true))
                                .fixedSize()
                        }
                        .padding(.horizontal, 12)
                        .frame(height: 0)
                        .clipped()
                        .hidden()
                        .accessibilityHidden(true)
                    }
                    LazyVStack(alignment: .leading, spacing: 0) {
                        // Its folder, when it's in one (its name is the title).
                        if let slash = f.path.lastIndex(of: "/"), slash != f.path.startIndex {
                            Text(f.path[..<slash])
                                .font(Style.mono(11))
                                .foregroundStyle(Color(p.muted))
                                .lineLimit(1)
                                .truncationMode(.head)
                                .padding(.horizontal, 12)
                                .padding(.vertical, 8)
                                .accessibilityLabel("In " + String(f.path[..<slash]))
                        } else {
                            Color.clear.frame(height: 8)
                        }
                        ForEach(0..<f.lines.count, id: \.self) { i in
                            HStack(alignment: .firstTextBaseline, spacing: 10) {
                                number(i + 1, width)
                                if wrap, let line = CodeLines.lines(f.spans(i)).first {
                                    WrappedLine(indent: line.indent, font: Style.mono(12)) {
                                        Text(Style.attributed(line.spans.isEmpty ? [Span(" ")] : RichText.breakable(line.spans), p, size: 14, inCode: true))
                                            .accessibilityLabel(RichText.plain(line.spans))
                                    }
                                    // (Its breaks are only shown: copied, the line as it is.)
                                    .contextMenu {
                                        Button("Copy Line", systemImage: "doc.on.doc") { UIPasteboard.general.string = RichText.plain(f.spans(i)) }
                                    }
                                } else {
                                    Text(Style.attributed(f.spans(i), p, size: 14, inCode: true))
                                        .fixedSize(horizontal: true, vertical: false)
                                }
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 1)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(i + 1 == line ? Color(p.primary).opacity(0.16) : Color.clear)
                            .id(i + 1)
                        }
                        if f.cut {
                            Text("The first \(f.lines.count) of \(f.total) lines; the rest is in harlo on your computer.")
                                .font(.footnote)
                                .foregroundStyle(Color(p.muted))
                                .padding(12)
                        }
                    }
                }
                .padding(.bottom, 20)
                .frame(minWidth: room.size.width, minHeight: room.size.height, alignment: .topLeading)
                .modifier(Selectable(on: !wrap))
            }
            .background(Color(p.codeBackground))
            .onAppear {
                if line > 1 { proxy.scrollTo(line, anchor: .center) }
            }
        } }
    }
}
