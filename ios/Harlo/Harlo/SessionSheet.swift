import HarloKit
import SwiftUI
import UIKit

/// The sheets a session opens.
enum SessionSheet: Identifiable {
    case changes, activity, models, actions
    /// The session as a Markdown file, to share.
    case share(URL)
    /// What waits on you in other sessions.
    case inbox
    /// Ask again with another model, from this message (its row key).
    case retry(String)
    /// A file a reply mentions, at a line (0: its start).
    case file(String, Int)
    case branches
    /// Undo from one of your messages (its row key).
    case undo(String)

    var id: String {
        switch self {
        case .file(let path, let line): return "file:\(path):\(line)"
        case .branches: return "branches"
        case .changes: return "changes"
        case .activity: return "activity"
        case .actions: return "actions"
        case .share(let url): return "share:\(url.path)"
        case .models: return "models"
        case .inbox: return "inbox"
        case .retry(let row): return "retry:" + row
        case .undo(let row): return "undo:" + row
        }
    }
}
