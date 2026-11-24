import HarloKit
import SwiftUI
import UIKit

/// The transcript's measures: the agent's tile (on the reply's first
/// line, the reply full width under it), and a run's timeline: the rail its
/// marks sit on, and how far its steps are from the edge.
enum Transcript {
    static let tile: CGFloat = 28
    static let rail: CGFloat = 24
    static let step: CGFloat = 32
}
