import HarloKit
import SwiftUI
import UIKit

/// A ```mockup block in a reply: a picture of the HTML file the agent
/// wrote, taken by harlo, fitted to the width (tap to zoom); with one of how
/// it is now, Before and After switch between them. The code a tap away;
/// when there's no picture, the code with why.
struct MockupBlock: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let source: String
    let code: CodeBlock
    @State private var pictures: MockupPictures?
    @State private var why = ""
    @State private var loading = true
    @State private var showCode = false
    @State private var showBefore = false
    @State private var zoomed = false

    var picture: DiagramPicture? { showBefore ? pictures?.before : pictures?.after }
    var pictureWhy: String { (showBefore ? pictures?.beforeWhy : pictures?.afterWhy) ?? why }

    var body: some View {
        Group {
            if loading && pictures == nil {
                HStack(spacing: 8) {
                    ProgressView()
                    Text("Taking a picture of it…").font(.subheadline).foregroundStyle(Color(p.muted))
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .card(p, radius: 10, fill: p.codeBackground)
            } else if showCode || pictures == nil || (pictures?.after == nil && pictures?.before == nil) {
                VStack(alignment: .leading, spacing: 6) {
                    code
                    if pictures?.after != nil || pictures?.before != nil {
                        VisualToggle(title: "Show the picture", icon: "photo") { showCode = false }
                    } else if !pictureWhy.isEmpty {
                        Text(pictureWhy)
                            .font(.footnote)
                            .foregroundStyle(Color(p.muted))
                            .padding(.horizontal, 4)
                    }
                }
            } else {
                VStack(alignment: .leading, spacing: 8) {
                    if let m = pictures, !m.title.isEmpty {
                        Text(m.title)
                            .font(.footnote.weight(.medium))
                            .foregroundStyle(Color(p.muted))
                            .padding(.horizontal, 2)
                    }
                    if pictures?.hasBefore == true {
                        Picker("Show", selection: $showBefore) {
                            Text("Before").tag(true)
                            Text("After").tag(false)
                        }
                        .pickerStyle(.segmented)
                        .accessibilityIdentifier("mockupBeforeAfter")
                    }
                    if let d = picture, let image = UIImage(data: d.png) {
                        Button { zoomed = true } label: {
                            Image(uiImage: image)
                                .resizable()
                                .interpolation(.high)
                                .aspectRatio(CGSize(width: max(d.width, 1), height: max(d.height, 1)), contentMode: .fit)
                                .frame(maxWidth: d.width)
                                .clipShape(RoundedRectangle(cornerRadius: 6))
                                .overlay { RoundedRectangle(cornerRadius: 6).strokeBorder(Color(p.hairline), lineWidth: 1) }
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(showBefore ? "Mockup, before" : "Mockup")
                        .accessibilityHint("Opens it to zoom")
                        .fullScreenCover(isPresented: $zoomed) {
                            DiagramZoom(image: image, size: CGSize(width: d.width, height: d.height), title: "Mockup")
                        }
                    } else {
                        Text(pictureWhy.isEmpty ? "There's no picture of it." : pictureWhy)
                            .font(.footnote)
                            .foregroundStyle(Color(p.warning))
                            .padding(.vertical, 8)
                    }
                    VisualToggle(title: "Code", icon: "chevron.left.forwardslash.chevron.right") { showCode = true }
                }
            }
        }
        // Asked each time it comes on screen, so an edited file shows
        // (harlo keeps the pictures until the file changes).
        .task(id: source) {
            let r = await model.mockup(source)
            if r.pictures != nil || pictures == nil {
                pictures = r.pictures
                why = r.why
            }
            loading = false
        }
    }
}
