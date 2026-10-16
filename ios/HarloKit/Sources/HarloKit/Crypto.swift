import CTweetNaCl
import Foundation

public enum HarloError: Error, Equatable, CustomStringConvertible {
    case badKey
    case badBase64
    case badFrame(String)
    case badOffer(String)
    case denied(String)
    case notConnected
    case closed(String)
    /// The phone's network refused before reaching harlo (offline, or iOS
    /// still asking for Local Network access).
    case offline(String)

    public var description: String {
        switch self {
        case .badKey: return "a key is 32 bytes"
        case .badBase64: return "bad base64"
        case .badFrame(let why): return why
        case .badOffer(let why): return why
        case .denied(let reason): return reason
        case .notConnected: return "not connected"
        case .closed(let why): return why
        case .offline(let why): return why
        }
    }
}

/// Base64 as harlo writes it (standard), reading URL-safe and unpadded too.
public enum Base64 {
    public static func encode(_ data: Data) -> String { data.base64EncodedString() }

    public static func decode(_ s: String) throws -> Data {
        var t = s.trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        while t.count % 4 != 0 { t += "=" }
        guard let d = Data(base64Encoded: t) else { throw HarloError.badBase64 }
        return d
    }

    /// URL-safe, unpadded (pairing offers).
    public static func encodeURL(_ data: Data) -> String {
        encode(data).replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}

/// Random bytes from the system's secure generator.
public func randomBytes(_ n: Int) -> Data {
    var g = SystemRandomNumberGenerator()
    return Data((0..<n).map { _ in UInt8.random(in: .min ... .max, using: &g) })
}

/// This phone's key pair (X25519).
public struct KeyPair: Sendable, Equatable {
    public let secret: Data
    public let publicKey: Data

    public init(secret: Data) throws {
        guard secret.count == 32 else { throw HarloError.badKey }
        self.secret = secret
        var pk = [UInt8](repeating: 0, count: 32)
        let sk = [UInt8](secret)
        harlo_public_key(&pk, sk)
        publicKey = Data(pk)
    }

    public static func generate() -> KeyPair {
        // swiftlint:disable:next force_try
        try! KeyPair(secret: randomBytes(32))
    }
}

/// NaCl's box between two static keys, as harlo's `Channel`: a frame is the
/// base64 of `nonce (24) || MAC and text`.
public struct Box: Sendable {
    let shared: [UInt8]

    public init(theirPublic: Data, ours: KeyPair) throws {
        guard theirPublic.count == 32 else { throw HarloError.badKey }
        var k = [UInt8](repeating: 0, count: 32)
        guard harlo_box_shared(&k, [UInt8](theirPublic), [UInt8](ours.secret)) == 0 else { throw HarloError.badKey }
        shared = k
    }

    public func seal(_ plain: Data, nonce: Data = randomBytes(24)) -> String {
        let m = [UInt8](plain), n = [UInt8](nonce)
        var c = [UInt8](repeating: 0, count: m.count + 16)
        _ = harlo_box_seal(&c, m, m.count, n, shared)
        return Base64.encode(nonce + Data(c))
    }

    public func open(_ frame: String) throws -> Data {
        let raw = [UInt8](try Base64.decode(frame))
        guard raw.count >= 24 + 16 else { throw HarloError.badFrame("frame too short") }
        let nonce = Array(raw[0..<24]), c = Array(raw[24...])
        var m = [UInt8](repeating: 0, count: c.count - 16)
        guard harlo_box_open(&m, c, c.count, nonce, shared) == 0 else { throw HarloError.badFrame("a frame didn't decrypt") }
        return Data(m)
    }
}

/// One connection's channel, as harlo's `Session`: JSON messages tagged with
/// the connection id and increasing sequence numbers (each direction its own).
public struct Channel: Sendable {
    let box: Box
    public let id: String
    var sent: Int64 = 0
    var received: Int64 = 0

    public init(box: Box, id: String) {
        self.box = box
        self.id = id
    }

    public mutating func seal(_ msg: JSON) throws -> String {
        sent += 1
        let framed: JSON = ["c": .string(id), "s": .int(sent), "m": msg]
        return box.seal(try framed.data())
    }

    /// The message, if the frame is for this connection and newer than the
    /// last one.
    public mutating func open(_ frame: String) throws -> JSON {
        let v = try JSON.parse(try box.open(frame))
        guard v["c"].string == id else { throw HarloError.badFrame("a frame from another connection") }
        let seq = v["s"].int ?? 0
        guard seq > received else { throw HarloError.badFrame("a frame out of order (replayed?)") }
        received = seq
        return v["m"]
    }
}
