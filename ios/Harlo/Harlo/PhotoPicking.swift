import HarloKit
import PhotosUI
import SwiftUI
import UIKit

/// Photos for a message: from the library or the camera, made small
/// enough for the agent (and the relay) on the phone.
enum Photos {
    /// The longest side agents use.
    static let maxSide: CGFloat = 1568

    static func photo(_ image: UIImage) -> Photo? {
        let size = image.size
        let scale = min(1, maxSide / max(size.width, size.height, 1))
        let target = CGSize(width: (size.width * scale).rounded(), height: (size.height * scale).rounded())
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let small = UIGraphicsImageRenderer(size: target, format: format).image { _ in
            image.draw(in: CGRect(origin: .zero, size: target))
        }
        guard let data = small.jpegData(compressionQuality: 0.8) else { return nil }
        return Photo(data: data, mimeType: "image/jpeg")
    }
}

/// Adds photos: a menu with the library and the camera.
struct PhotoButton: View {
    @Environment(\.palette) private var p
    @Binding var photos: [Photo]
    var size: CGFloat = 34
    @State private var library = false
    @State private var camera = false
    @State private var picked: [PhotosPickerItem] = []

    var body: some View {
        Menu {
            Button {
                library = true
            } label: {
                Label("Photo library", systemImage: "photo.on.rectangle")
            }
            if UIImagePickerController.isSourceTypeAvailable(.camera) {
                Button {
                    camera = true
                } label: {
                    Label("Take a photo", systemImage: "camera")
                }
            }
        } label: {
            Image(systemName: "plus")
                .font(.system(size: 17, weight: .semibold))
                .frame(width: size, height: size)
                .background(Circle().fill(Color(p.subtle)))
                .overlay(Circle().strokeBorder(Color(p.border), lineWidth: 1))
                .foregroundStyle(Color(p.muted))
        }
        .accessibilityLabel("Add a photo")
        .photosPicker(isPresented: $library, selection: $picked, maxSelectionCount: 4, matching: .images)
        .onChange(of: picked) { _, items in
            guard !items.isEmpty else { return }
            picked = []
            Task {
                for item in items {
                    if let data = try? await item.loadTransferable(type: Data.self), let image = UIImage(data: data), let photo = Photos.photo(image) {
                        photos.append(photo)
                    }
                }
            }
        }
        .fullScreenCover(isPresented: $camera) {
            CameraView { image in
                if let image, let photo = Photos.photo(image) { photos.append(photo) }
                camera = false
            }
            .ignoresSafeArea()
        }
    }
}

/// The photos going with the next message, each removable.
struct PhotoStrip: View {
    @Environment(\.palette) private var p
    @Binding var photos: [Photo]

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(photos) { photo in
                    ZStack(alignment: .topTrailing) {
                        if let image = UIImage(data: photo.data) {
                            Image(uiImage: image)
                                .resizable()
                                .scaledToFill()
                                .frame(width: 56, height: 56)
                                .clipShape(RoundedRectangle(cornerRadius: 8))
                                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Color(p.border), lineWidth: 1))
                        }
                        Button {
                            photos.removeAll { $0.id == photo.id }
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .font(.system(size: 18))
                                .symbolRenderingMode(.palette)
                                .foregroundStyle(Color.white, Color.black.opacity(0.6))
                        }
                        .offset(x: 6, y: -6)
                        .accessibilityLabel("Remove photo")
                    }
                }
            }
            .padding(.top, 6)
            .padding(.horizontal, 2)
        }
    }
}

/// The camera, for one photo (nil: cancelled).
struct CameraView: UIViewControllerRepresentable {
    var done: (UIImage?) -> Void

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        picker.sourceType = .camera
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ controller: UIImagePickerController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(done: done) }

    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let done: (UIImage?) -> Void
        init(done: @escaping (UIImage?) -> Void) { self.done = done }

        func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            done(info[.originalImage] as? UIImage)
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
            done(nil)
        }
    }
}
