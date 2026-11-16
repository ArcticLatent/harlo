import HarloKit
import SwiftUI
import UIKit

/// The session's pictures full screen: swipe across for the next, pinch or
/// double-tap to zoom, swipe down (or Done) to close, Share to send or save
/// one. A tap hides the bars.
struct PictureViewer: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let paths: [String]
    @State private var current: String
    @State private var bars = true
    /// Every picture at once, as the desktop's grid (more than one).
    @State private var grid = false
    /// The top bar's height, so the grid starts below it at any text size.
    @State private var topHeight: CGFloat = 56
    @Environment(\.dynamicTypeSize) private var typeSize
    /// How far it's been pulled down (to close), 0…1.
    @State private var pulled: CGFloat = 0
    @State private var shown: [String: UIImage] = [:]

    init(paths: [String], current: String) {
        self.paths = paths
        _current = State(initialValue: current)
    }

    var index: Int { paths.firstIndex(of: current) ?? 0 }
    /// Its name as people say it ("You sent", not harlo's file name).
    func name(_ path: String) -> String { model.session?.pictureName(path) ?? PictureNames.name(path) }

    var body: some View {
        ZStack {
            Color.black.opacity(1 - Double(pulled) * 0.7).ignoresSafeArea()
            TabView(selection: $current) {
                ForEach(paths, id: \.self) { path in
                    PicturePage(path: path, pulled: $pulled, loaded: { shown[path] = $0 }, toggleBars: {
                        withAnimation(.easeOut(duration: 0.15)) { bars.toggle() }
                    }, close: { dismiss() })
                    .tag(path)
                }
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            .ignoresSafeArea()
            if grid {
                PictureGrid(paths: paths, current: current, name: name, top: topHeight) { path in
                    current = path
                    withAnimation(.easeOut(duration: 0.15)) { grid = false }
                }
                .transition(.opacity)
            }
            if (bars || grid) && pulled == 0 {
                VStack(spacing: 0) {
                    top
                    Spacer()
                    bottom
                }
                .transition(.opacity)
            }
        }
        .statusBarHidden(!bars)
        .preferredColorScheme(.dark)
    }

    var top: some View {
        HStack(spacing: 12) {
            if grid {
                Button {
                    withAnimation(.easeOut(duration: 0.15)) { grid = false }
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: "chevron.left").font(.body.weight(.semibold))
                        // (At the largest sizes the arrow alone: the word
                        // doesn't fit beside the title.)
                        if !typeSize.isAccessibilitySize { Text("Back") }
                    }
                    .font(.body)
                    .frame(minWidth: 44, minHeight: 44, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .accessibilityIdentifier("pictureGridBack")
                .accessibilityLabel("Back to the picture")
                .accessibilityShowsLargeContentViewer {
                    Label("Back", systemImage: "chevron.left")
                }
            }
            VStack(alignment: .leading, spacing: 2) {
                if grid {
                    Text("All pictures")
                        .font(.subheadline.weight(.semibold))
                        .lineLimit(1)
                        .minimumScaleFactor(0.7)
                    Text("\(paths.count) in this session")
                        .font(.caption)
                        .lineLimit(1)
                        .minimumScaleFactor(0.7)
                        .foregroundStyle(Self.caption)
                } else {
                    Text(name(current))
                        .accessibilityLabel(PictureNames.spoken(name(current)))
                        .font(.subheadline.weight(.semibold))
                        .lineLimit(1)
                        .truncationMode(.middle)
                    if paths.count > 1 {
                        Text("\(index + 1) of \(paths.count)")
                            .font(.caption)
                            .foregroundStyle(Self.caption)
                    }
                }
            }
            .layoutPriority(1)
            Spacer(minLength: 8)
            if paths.count > 1 {
                Button {
                    withAnimation(.easeOut(duration: 0.15)) { grid.toggle() }
                } label: {
                    Image(systemName: grid ? "square.grid.2x2.fill" : "square.grid.2x2")
                        .font(.body.weight(.semibold))
                        .frame(minWidth: 44, minHeight: 44)
                }
                .accessibilityIdentifier("pictureGrid")
                .accessibilityLabel(grid ? "Back to the picture" : "All pictures")
            }
            Button("Done") { dismiss() }
                .font(.body.weight(.semibold))
                // (Whole at any size: the title gives way, not Done.)
                .fixedSize()
                .frame(minHeight: 44)
                .accessibilityIdentifier("pictureDone")
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 16)
        .padding(.bottom, 8)
        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { topHeight = $0 }
        .background(LinearGradient(colors: [.black.opacity(0.7), .clear], startPoint: .top, endPoint: .bottom).ignoresSafeArea())
    }

    /// The bars' second line: a solid grey (Apple's audit misreads text at
    /// partial opacity), about 9:1 on black.
    static let caption = Color(white: 0.72)

    var bottom: some View {
        HStack {
            if !grid, let image = shown[current] {
                let name = name(current)
                ShareLink(item: Image(uiImage: image), preview: SharePreview(name, image: Image(uiImage: image))) {
                    Label("Share", systemImage: "square.and.arrow.up")
                        .font(.body.weight(.medium))
                        .frame(minHeight: 44)
                }
                .accessibilityIdentifier("pictureShare")
            }
            Spacer()
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 16)
        .padding(.top, 8)
        .background(LinearGradient(colors: [.clear, .black.opacity(0.7)], startPoint: .top, endPoint: .bottom).ignoresSafeArea())
    }
}
