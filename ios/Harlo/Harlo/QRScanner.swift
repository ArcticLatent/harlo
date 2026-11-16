import HarloKit
import SwiftUI
import UIKit

struct QRScanner: UIViewControllerRepresentable {
    var onFound: (String) -> Void

    func makeUIViewController(context: Context) -> ScannerController {
        let c = ScannerController()
        c.onFound = onFound
        return c
    }

    func updateUIViewController(_ c: ScannerController, context: Context) {
        c.onFound = onFound
    }
}
