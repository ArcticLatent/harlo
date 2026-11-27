import HarloKit
import PhotosUI
import SwiftUI
import UIKit

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
                .scaledFont(17, .semibold)
                .frame(width: size, height: size)
                .background { RoundedRectangle(cornerRadius: Corner.card).fill(Color(p.subtle)) }
                .overlay { RoundedRectangle(cornerRadius: Corner.card).strokeBorder(Color(p.border), lineWidth: 1) }
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
