import AVFoundation
import Speech
import SwiftUI

/// Dictation, as on the desktop: tap the mic and speak, and the words go
/// into the message as you say them; tap again to stop. Recognized on the
/// iPhone (on the device itself when it has the language; else by Apple's
/// speech recognition). Nothing is kept.
@MainActor
@Observable
final class Dictation {
    enum State: Equatable {
        case idle, starting, listening
        case failed(String)
    }

    private(set) var state: State = .idle
    var listening: Bool { state == .listening || state == .starting }

    /// The message before dictating, and what's been said since.
    private var base = ""
    private var engine: AVAudioEngine?
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var text: Binding<String>?
    private var timeout: Task<Void, Never>?
    /// Which dictation this is (a late clean-up leaves a newer one alone).
    private var run = 0

    /// The language (Settings → Dictation): a code ("tr"), or "" for the
    /// phone's own.
    static var language: String { UserDefaults.standard.string(forKey: "dictationLanguage") ?? "" }

    /// The languages offered (as the desktop's Settings → Voice).
    static let languages: [(code: String, name: String)] = [
        ("", "The phone's language"), ("en", "English"), ("tr", "Turkish"), ("de", "German"), ("fr", "French"), ("es", "Spanish"),
        ("it", "Italian"), ("pt", "Portuguese"), ("nl", "Dutch"), ("pl", "Polish"), ("ru", "Russian"), ("uk", "Ukrainian"),
        ("ja", "Japanese"), ("zh", "Chinese"), ("ko", "Korean"),
    ]

    func toggle(into text: Binding<String>) {
        if listening { stop() } else { Task { await start(into: text) } }
    }

    func start(into text: Binding<String>) async {
        guard !listening else { return }
        release()
        run += 1
        state = .starting
        // Asked once each: speech recognition, then the microphone.
        let speech = await withCheckedContinuation { c in SFSpeechRecognizer.requestAuthorization { c.resume(returning: $0) } }
        guard speech == .authorized else { return fail("Allow Speech Recognition for harlo in the iPhone's Settings to dictate.") }
        guard await AVAudioApplication.requestRecordPermission() else { return fail("Allow the Microphone for harlo in the iPhone's Settings to dictate.") }
        let code = Self.language
        let locale = code.isEmpty ? Locale.current : Locale(identifier: code)
        guard let recognizer = SFSpeechRecognizer(locale: locale) ?? SFSpeechRecognizer(), recognizer.isAvailable else {
            return fail("Speech recognition isn't available for that language right now.")
        }
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.addsPunctuation = true
        // On the phone itself when it can (nothing leaves it).
        if recognizer.supportsOnDeviceRecognition { request.requiresOnDeviceRecognition = true }
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(.record, mode: .measurement, options: .duckOthers)
            try session.setActive(true, options: .notifyOthersOnDeactivation)
        } catch {
            return fail("The microphone isn't available: \(error.localizedDescription)")
        }
        let engine = AVAudioEngine()
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in request.append(buffer) }
        engine.prepare()
        do {
            try engine.start()
        } catch {
            input.removeTap(onBus: 0)
            return fail("The microphone didn't start: \(error.localizedDescription)")
        }
        self.engine = engine
        self.request = request
        self.text = text
        let current = text.wrappedValue
        base = current.isEmpty || current.hasSuffix(" ") || current.hasSuffix("\n") ? current : current + " "
        state = .listening
        task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            Task { @MainActor in
                guard let self, let text = self.text else { return }
                // Sent or cleared meanwhile: it stops (the words don't come back).
                if !text.wrappedValue.hasPrefix(self.base) {
                    self.stopAudio()
                    self.release()
                    return
                }
                if let result {
                    text.wrappedValue = self.base + result.bestTranscription.formattedString
                    if result.isFinal {
                        self.stopAudio()
                        self.release()
                    }
                } else if error != nil {
                    self.stopAudio()
                    self.release()
                }
            }
        }
        // (Apple's recognition stops after about a minute anyway.)
        timeout = Task { [weak self] in
            try? await Task.sleep(for: .seconds(120))
            guard !Task.isCancelled else { return }
            self?.stop()
        }
    }

    /// Stops listening; what was said so far stays in the message (the
    /// last words come a moment later: up to two seconds).
    func stop() {
        guard listening else { return }
        request?.endAudio()
        stopAudio()
        let mine = run
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(2))
            if let self, self.run == mine { self.release() }
        }
    }

    /// The microphone off (the words still on their way may come).
    private func stopAudio() {
        timeout?.cancel()
        if let engine {
            engine.stop()
            engine.inputNode.removeTap(onBus: 0)
        }
        engine = nil
        if listening {
            state = .idle
        }
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    /// Done with the recognition.
    private func release() {
        task?.finish()
        task = nil
        request = nil
        text = nil
    }

    private func fail(_ why: String) {
        stopAudio()
        release()
        state = .failed(why)
    }

    func clearFailure() {
        if case .failed = state { state = .idle }
    }
}
