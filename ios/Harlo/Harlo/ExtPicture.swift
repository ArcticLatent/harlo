import SwiftUI

/// A picture in an extension's view: no wider than the column or itself.
struct ExtPicture: View {
    let data: Data
    @State private var image: UIImage?

    var body: some View {
        Group {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFit()
                    .frame(maxWidth: image.size.width, alignment: .leading)
                    .accessibilityLabel("A picture from an extension")
            }
        }
        .task(id: data) {
            image = UIImage(data: data)
        }
    }
}
