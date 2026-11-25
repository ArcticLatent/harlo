import Foundation

// A connection to harlo recorded, and played back without one: the app's
// UI tests and screenshots show what a real harlo sent (recorded by
// harlo-probe against harlo, tools/ios-record.sh), at named points of it
// (scenes), with no harlo to connect to.
//
// A recording is JSON lines, in the order they happened:
//   {"ready": {"name", "features"}}         the hello's answer
//   {"call": op, "args": {…}, "reply": {…}} a request and harlo's answer
//   {"ev": …}                               an event, as harlo sent it
//   {"scene": name}                         a point to play back to

/// Writes a recording as a connection goes (harlo-probe sets it).
public final class Recorder: @unchecked Sendable {
    private let lock = NSLock()
    private let handle: FileHandle

    public init?(path: String) {
        _ = FileManager.default.createFile(atPath: path, contents: nil)
        guard let h = FileHandle(forWritingAtPath: path) else { return nil }
        handle = h
    }

    /// The recorder connections write to (nil: none).
    nonisolated(unsafe) public static var shared: Recorder?

    public func write(_ line: JSON) {
        guard let data = try? line.data() else { return }
        lock.lock()
        defer { lock.unlock() }
        handle.write(data)
        handle.write(Data("\n".utf8))
    }

    /// A point to play back to.
    public func scene(_ name: String) {
        write(["scene": .string(name)])
    }
}

/// A recording up to one of its scenes, answering as harlo did.
public struct Replay: Sendable {
    public let harloName: String
    public let features: Set<String>
    /// Requests and their answers (all of them: one made after the scene
    /// still answers, if none before it did).
    let calls: [(op: String, args: JSON, reply: JSON, before: Bool)]
    /// Events up to the scene.
    let events: [JSON]

    public enum Failure: Error, CustomStringConvertible {
        case noScene(String, [String])
        public var description: String {
            switch self {
            case .noScene(let s, let all): "No scene \"\(s)\" in the recording (it has \(all.joined(separator: ", ")))"
            }
        }
    }

    /// The scenes in a recording, in order.
    public static func scenes(in text: String) -> [String] {
        lines(text).compactMap { $0["scene"].string }
    }

    static func lines(_ text: String) -> [JSON] {
        text.split(separator: "\n").compactMap { try? JSON.parse(String($0)) }
    }

    /// Times are milliseconds or seconds since 1970, between 2020 and 2040.
    static let msTimes: ClosedRange<Int64> = 1_577_836_800_000...2_208_988_800_000
    static let sTimes: ClosedRange<Int64> = 1_577_836_800...2_208_988_800

    /// How far (in ms) to move the recording's times for its last to be `now`.
    static func offset(_ text: String, now: Date) -> Int64 {
        var last: Int64 = 0
        func scan(_ j: JSON) {
            switch j {
            case .int(let v) where msTimes.contains(v): last = max(last, v)
            case .int(let v) where sTimes.contains(v): last = max(last, v * 1000)
            case .array(let a): a.forEach(scan)
            case .object(let o): o.values.forEach(scan)
            default: break
            }
        }
        lines(text).forEach(scan)
        return last == 0 ? 0 : Int64(now.timeIntervalSince1970 * 1000) - last
    }

    /// A line with its events' and answers' times moved by `ms` (what was
    /// asked stays as it was: answers are found by it).
    static func moved(by ms: Int64) -> (JSON) -> JSON {
        func move(_ j: JSON) -> JSON {
            switch j {
            case .int(let v) where msTimes.contains(v): .int(v + ms)
            case .int(let v) where sTimes.contains(v): .int(v + ms / 1000)
            case .array(let a): .array(a.map(move))
            case .object(let o): .object(o.mapValues(move))
            default: j
            }
        }
        return { line in
            guard ms != 0, case .object(var o) = line else { return line }
            for k in ["ev", "reply", "ready"] where o[k] != nil { o[k] = move(o[k]!) }
            return .object(o)
        }
    }

    /// `now`: when it's played; its times move with it, the recording's
    /// last moment becoming now ("2s ago", not the days since recording).
    public init(recording text: String, scene: String, now: Date = .now) throws {
        let all = Self.lines(text).map(Self.moved(by: Self.offset(text, now: now)))
        guard let end = all.firstIndex(where: { $0["scene"].string == scene }) else {
            throw Failure.noScene(scene, Self.scenes(in: text))
        }
        let ready = all.first { $0["ready"] != .null }?["ready"] ?? [:]
        harloName = ready["name"].text
        features = Set((ready["features"].array ?? []).compactMap(\.string))
        calls = all.enumerated().compactMap { i, l in
            guard let op = l["call"].string else { return nil }
            return (op, l["args"], l["reply"], i < end)
        }
        // (A `sessions` reply is the list at that moment, as an event is:
        // whichever came last is the list.)
        events = all[..<end].compactMap { l in
            if l["ev"] != .null { return l["ev"] }
            if l["call"].string == "sessions", let list = l["reply"]["sessions"].array { return ["ev": "sessions", "sessions": .array(list)] }
            return nil
        }
    }

    /// harlo's answer to `op` with `args`: the last one recorded before the
    /// scene with the same arguments, else with the same request, else
    /// (after the scene) the first; else an error.
    public func answer(_ op: String, _ args: JSON) -> JSON {
        let same = calls.filter { $0.op == op }
        let exact = same.filter { $0.args == args }
        // A session started rather than opened: it's live under its key.
        if op == "open", exact.isEmpty { return ["key": args["key"], "harness": ""] }
        if let r = exact.last(where: \.before) ?? same.last(where: \.before) ?? exact.first ?? same.first { return r.reply }
        return ["error": .string("Not in the recording: \(op)")]
    }

    /// Events for the session `key` up to the scene (its rows, status,
    /// requests…), in order: what opening it shows.
    public func events(of key: String) -> [JSON] {
        events.filter { $0["key"].string == key && $0["ev"].text != "sessions" }
    }

    /// Events about no one session (the list, limits), the latest of each.
    public func latestShared() -> [JSON] {
        var seen: Set<String> = []
        var out: [JSON] = []
        for e in events.reversed() where e["key"].text.isEmpty && !seen.contains(e["ev"].text) {
            seen.insert(e["ev"].text)
            out.insert(e, at: 0)
        }
        return out
    }
}

extension HarloModel {
    /// A model connected to a recording at `scene` instead of a harlo: what
    /// the screenshots and UI tests show.
    public static func replaying(_ replay: Replay, deviceName: String = "iPhone") -> HarloModel {
        let m = HarloModel(store: MemoryStore(), deviceName: deviceName)
        m.replaying = true
        m.host = PairedHarlo(key: Base64.encode(Data(repeating: 7, count: 32)), name: replay.harloName.isEmpty ? "harlo" : replay.harloName,
                             endpoints: [URL(string: "ws://192.168.1.20:7676/ws")!])
        m.features = replay.features
        m.active = true
        m.startLoop(with: HarloClient.replaying(replay))
        return m
    }
}
