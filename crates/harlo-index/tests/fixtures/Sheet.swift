import SwiftUI

/// A sheet.
@MainActor
struct WorkflowSheet: View {
    let run: Run
    var body: some View {
        Text(run.name)
    }
    private func open(_ key: String) {
        store.load(key)
    }
    init(run: Run) { self.run = run }
}

extension Run {
    static func make() -> Run { Run() }
}

protocol Store {
    func load(_ key: String)
}

typealias Id = String
