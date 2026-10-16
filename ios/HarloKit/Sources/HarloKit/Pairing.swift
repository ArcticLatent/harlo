import Foundation

/// What harlo's QR code carries: a link whose fragment is the offer
/// (`http://192.168.1.5:7676/#p=…`, Funnel's `https://…ts.net:8443/#p=…`, or
/// a relay's page `https://relay…/#p=…` with the relay in the offer).
public struct PairingOffer: Equatable, Sendable {
    /// harlo's public key (base64).
    public var key: String
    /// The one-time pairing code.
    public var code: String
    /// harlo's computer name.
    public var name: String
    /// `wss://relay.example` and harlo's id there, or empty.
    public var relay: String
    public var relayId: String
    /// Where the link pointed (its origin), when it came as a link.
    public var page: URL?
    /// harlo's other endpoints (its other addresses, Funnel, the relay).
    public var alternates: [URL]

    /// From the offer itself (base64url JSON, as `Offer::encode`).
    public init(encoded: String, page: URL? = nil) throws {
        guard let data = try? Base64.decode(encoded), let v = try? JSON.parse(data) else { throw HarloError.badOffer("That isn't a harlo pairing code.") }
        guard v["v"].int == 1 else { throw HarloError.badOffer("This pairing code is from a newer harlo. Update the app.") }
        guard let key = v["k"].string, (try? Base64.decode(key))?.count == 32 else { throw HarloError.badOffer("That isn't a harlo pairing code.") }
        self.key = key
        code = v["c"].text
        name = v["n"].text
        relay = v["r"].text
        relayId = v["i"].text
        alternates = (v["e"].array ?? []).compactMap { $0.string.flatMap(URL.init(string:)) }
        self.page = page
    }

    /// From what was scanned or pasted: the link, or the offer alone.
    public init(scanned: String) throws {
        let s = scanned.trimmingCharacters(in: .whitespacesAndNewlines)
        if let url = URL(string: s), let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https",
           let fragment = url.fragment
        {
            let p = fragment.split(separator: "&").first { $0.hasPrefix("p=") }
            guard let p else { throw HarloError.badOffer("That link isn't a harlo pairing link.") }
            var origin = URLComponents()
            origin.scheme = scheme
            origin.host = url.host
            origin.port = url.port
            try self.init(encoded: String(p.dropFirst(2)), page: origin.url)
        } else if s.contains("://") {
            throw HarloError.badOffer("That link isn't a harlo pairing link.")
        } else {
            try self.init(encoded: s)
        }
    }

    /// Where the phone reaches harlo for this offer: the relay when it has
    /// one, else the link's own server.
    public var endpoint: URL? {
        if !relay.isEmpty {
            return Endpoints.relay(relay, id: relayId)
        }
        return page.flatMap(Endpoints.direct)
    }

    /// Every endpoint, the scanned one first.
    public var endpoints: [URL] {
        var out: [URL] = endpoint.map { [$0] } ?? []
        for u in alternates where !out.contains(u) { out.append(u) }
        return out
    }
}

public enum Endpoints {
    /// harlo's own server (`http://host:port` → `ws://host:port/ws`).
    public static func direct(_ page: URL) -> URL? {
        guard var c = URLComponents(url: page, resolvingAgainstBaseURL: false) else { return nil }
        c.scheme = c.scheme == "https" ? "wss" : "ws"
        c.path = "/ws"
        c.query = nil
        c.fragment = nil
        return c.url
    }

    /// How good a way in is, best first: harlo on this network (0),
    /// over Tailscale or another direct address (1), Funnel (2), a relay (3).
    public static func rank(_ url: URL) -> Int {
        if url.query?.contains("id=") == true { return 3 }
        if url.scheme == "wss" { return 2 }
        return isLocal(url.host ?? "") ? 0 : 1
    }

    /// A home-network address: private IPv4 ranges, link-local, `.local`.
    static func isLocal(_ host: String) -> Bool {
        if host.hasSuffix(".local") || host == "localhost" { return true }
        let p = host.split(separator: ".").compactMap { Int($0) }
        guard p.count == 4 else { return false }
        return p[0] == 10 || p[0] == 127 || (p[0] == 192 && p[1] == 168) || (p[0] == 172 && (16...31).contains(p[1])) || (p[0] == 169 && p[1] == 254)
    }

    /// A harlo relay (`wss://relay.example` → `…/ws?id=<harlo's id>`).
    public static func relay(_ base: String, id: String) -> URL? {
        var b = base.trimmingCharacters(in: .whitespaces)
        while b.hasSuffix("/") { b.removeLast() }
        guard var c = URLComponents(string: b + "/ws") else { return nil }
        c.queryItems = [URLQueryItem(name: "id", value: id)]
        return c.url
    }
}

/// A harlo this phone is paired with.
public struct PairedHarlo: Codable, Equatable, Sendable, Identifiable {
    /// harlo's public key (base64): also its identity here.
    public var key: String
    public var name: String
    /// Where to reach it, tried in order (`ws://…/ws`, `wss://relay…/ws?id=…`).
    public var endpoints: [URL]
    /// The one that answered last.
    public var lastEndpoint: URL?
    public var pairedAt: Date

    public var id: String { key }

    public init(key: String, name: String, endpoints: [URL], pairedAt: Date = Date()) {
        self.key = key
        self.name = name
        self.endpoints = endpoints
        self.pairedAt = pairedAt
    }

    /// Endpoints in the order to try: the best ways in first (this network,
    /// Tailscale, Funnel, a relay), the one that worked last first among
    /// equals.
    public var tryOrder: [URL] {
        let last = lastEndpoint.flatMap { endpoints.contains($0) ? $0 : nil }
        let ordered = last.map { [$0] + endpoints.filter { $0 != last } } ?? endpoints
        return ordered.enumerated().sorted { a, b in
            let (ra, rb) = (Endpoints.rank(a.element), Endpoints.rank(b.element))
            return ra != rb ? ra < rb : a.offset < b.offset
        }.map(\.element)
    }

    /// Adds an endpoint (another link scanned for the same harlo).
    public mutating func add(_ url: URL) {
        if !endpoints.contains(url) { endpoints.append(url) }
    }

    /// Takes the endpoints harlo says it has now: its own addresses replace
    /// the ones it no longer has; others (a relay scanned by hand) stay.
    public mutating func learn(_ announced: [URL]) {
        guard !announced.isEmpty else { return }
        let kept = endpoints.filter { $0.scheme == "ws" ? announced.contains($0) || $0 == lastEndpoint : true }
        endpoints = kept
        for u in announced where !endpoints.contains(u) { endpoints.append(u) }
    }
}

/// A short, readable fingerprint of a public key, as harlo shows it.
public func fingerprint(_ publicKey: String) -> String {
    let bytes = (try? Base64.decode(publicKey)) ?? Data()
    let hex = bytes.prefix(8).map { String(format: "%02x", $0) }.joined()
    var groups: [String] = []
    var rest = Substring(hex)
    while !rest.isEmpty {
        groups.append(String(rest.prefix(4)))
        rest = rest.dropFirst(4)
    }
    return groups.joined(separator: " ")
}
