import Foundation

/// The first of several tries to succeed (connecting to each of harlo's
/// endpoints at once).
enum Race {
    /// Runs `attempt` for each of `items` (the first at once, the others
    /// `headStart` seconds later) and returns the first success; later
    /// successes are closed. A refusal from harlo ends it at once; when
    /// everything fails, the first item's error is thrown.
    static func first(_ items: [URL], headStart: Double,
                      _ attempt: @escaping @Sendable (URL) async throws -> (HarloClient, Ready)) async throws -> (HarloClient, Ready)
    {
        guard !items.isEmpty else { throw HarloError.closed("harlo has no address to try.") }
        let box = Box(count: items.count)
        return try await withCheckedThrowingContinuation { cont in
            box.set(cont)
            for (i, url) in items.enumerated() {
                Task {
                    if i > 0 { try? await Task.sleep(nanoseconds: UInt64(headStart * 1_000_000_000)) }
                    if box.isDone { return box.failed(i, HarloError.closed("")) }
                    do {
                        let r = try await attempt(url)
                        if !box.won(r) { await r.0.close() }
                    } catch {
                        box.failed(i, error)
                    }
                }
            }
        }
    }

    final class Box: @unchecked Sendable {
        private let lock = NSLock()
        private var cont: CheckedContinuation<(HarloClient, Ready), Error>?
        private var left: Int
        private var errors: [Int: Error] = [:]
        private var done = false

        init(count: Int) { left = count }

        func set(_ c: CheckedContinuation<(HarloClient, Ready), Error>) {
            lock.lock()
            cont = c
            lock.unlock()
        }

        var isDone: Bool {
            lock.lock()
            defer { lock.unlock() }
            return done
        }

        /// The winner (true), or too late (false: close it).
        func won(_ r: (HarloClient, Ready)) -> Bool {
            lock.lock()
            guard !done, let c = cont else {
                lock.unlock()
                return false
            }
            done = true
            cont = nil
            lock.unlock()
            c.resume(returning: r)
            return true
        }

        func failed(_ i: Int, _ error: Error) {
            lock.lock()
            left -= 1
            errors[i] = error
            var outcome: Error?
            if !done, case HarloError.denied? = error as? HarloError {
                outcome = error
            } else if !done, left == 0 {
                outcome = errors[errors.keys.min() ?? 0]
            }
            guard let e = outcome, let c = cont else {
                lock.unlock()
                return
            }
            done = true
            cont = nil
            lock.unlock()
            c.resume(throwing: e)
        }
    }
}
