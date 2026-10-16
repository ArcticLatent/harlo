import Foundation

/// Where the app keeps its secret key and pairing (the Keychain on iOS;
/// files or memory in tests).
public protocol SecretStore: AnyObject, Sendable {
    func load(_ name: String) -> Data?
    func save(_ name: String, _ data: Data?)
}

/// In memory (tests, previews).
public final class MemoryStore: SecretStore, @unchecked Sendable {
    private var items: [String: Data] = [:]
    private let lock = NSLock()

    public init() {}

    public func load(_ name: String) -> Data? {
        lock.lock()
        defer { lock.unlock() }
        return items[name]
    }

    public func save(_ name: String, _ data: Data?) {
        lock.lock()
        defer { lock.unlock() }
        items[name] = data
    }
}

#if canImport(Security)
import Security

/// The Keychain: this device only, readable after the first unlock (so a
/// reconnect in the background works), not in backups. The app and its
/// notification extension share the app's own group (`TEAM.com.arcticlatent.harlo`,
/// where the app's items have always been).
public final class KeychainStore: SecretStore, @unchecked Sendable {
    let service: String
    /// nil: the app's default group.
    let accessGroup: String?

    public init(service: String = "com.arcticlatent.harlo", accessGroup: String? = nil) {
        self.service = service
        self.accessGroup = accessGroup
    }

    private func query(_ name: String) -> [String: Any] {
        var q: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: name]
        if let accessGroup { q[kSecAttrAccessGroup as String] = accessGroup }
        return q
    }

    public func load(_ name: String) -> Data? {
        var q = query(name)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: AnyObject?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess else { return nil }
        return out as? Data
    }

    public func save(_ name: String, _ data: Data?) {
        SecItemDelete(query(name) as CFDictionary)
        guard let data else { return }
        var q = query(name)
        q[kSecValueData as String] = data
        q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        SecItemAdd(q as CFDictionary, nil)
    }
}
#endif
