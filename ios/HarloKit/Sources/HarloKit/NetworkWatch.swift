#if canImport(Network)
import Network

/// Tells the model when the phone's network changes (Apple's path monitor).
@MainActor
public final class NetworkWatch {
    private let watching: Task<Void, Never>

    /// Calls `changed` when the network changes (not for the first report).
    public init(_ changed: @escaping @MainActor () -> Void) {
        watching = Task {
            var last: NWPath?
            for await path in NWPathMonitor() {
                defer { last = path }
                guard let before = last, before.status != path.status || before.availableInterfaces.map(\.name) != path.availableInterfaces.map(\.name) else { continue }
                changed()
            }
        }
    }

    deinit { watching.cancel() }
}
#endif
