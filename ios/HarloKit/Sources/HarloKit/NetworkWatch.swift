#if canImport(Network)
import Network

/// Tells the model when the phone's network changes (Apple's path monitor).
public final class NetworkWatch: @unchecked Sendable {
    private let monitor = NWPathMonitor()
    private var last: NWPath?

    /// Calls `changed` on the main actor when the network changes (not for
    /// the first report).
    public init(_ changed: @escaping @MainActor () -> Void) {
        monitor.pathUpdateHandler = { [weak self] path in
            guard let self else { return }
            let before = self.last
            self.last = path
            guard let before, before.status != path.status || before.availableInterfaces.map(\.name) != path.availableInterfaces.map(\.name) else { return }
            Task { @MainActor in changed() }
        }
        monitor.start(queue: DispatchQueue(label: "harlo.network"))
    }

    deinit { monitor.cancel() }
}
#endif
