import AVFoundation
import HarloKit
import SwiftUI

/// The camera's session, started and stopped off the main actor (starting
/// blocks until the camera runs). AVCaptureSession may be started and
/// stopped from any thread.
final class CameraCapture: @unchecked Sendable {
    let session = AVCaptureSession()
}
