import HarloKit
import SwiftUI
import UIKit

struct Composer: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Bindable var session: OpenSession
    var openSheet: (SessionSheet) -> Void
    var onSend: () -> Void
    @State private var photos: [Photo] = []
    /// Photos sent and not yet with harlo (shown, dimmed, until they are).
    @State private var sendingPhotos: [Photo] = []
    @State private var levels: [String] = []
    /// Files matching the `@` being typed.
    @State private var fileMatches: [String] = []
    /// The folder's other sessions matching it (`@session:…`).
    @State private var sessionMatches: [SessionMention] = []
    @FocusState private var focused: Bool
    @Environment(\.dynamicTypeSize) private var typeSize
    /// At the largest text sizes the model (or Steer | Queue) gets a row of
    /// its own, above the buttons, so its name isn't cut.
    var big: Bool { typeSize.isAccessibilitySize }

    var text: String { session.draft }

    /// What follows the `@` being typed (nil: none).
    var mention: String? { Mention.query(in: text) }

    var canSend: Bool {
        (!text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !photos.isEmpty) && model.state == .connected
    }

    var commands: [SlashCommand] { SlashCommand.matching(session.commands, typed: text) }

    var placeholder: String {
        guard session.busy else { return "Message \(Style.harnessLabel(session.harness))…" }
        return session.steer ? "Steer the current run…" : "Queue a follow-up…"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !commands.isEmpty && focused {
                SlashList(commands: Array(commands.prefix(6))) { c in
                    session.draft = "/" + c.name + " "
                }
            } else if focused && mention != nil && !(fileMatches.isEmpty && sessionMatches.isEmpty) {
                let sessions = Array(sessionMatches.prefix(fileMatches.isEmpty ? 6 : 3))
                FileList(files: Array(fileMatches.prefix(6 - sessions.count)), sessions: sessions) { f in
                    session.draft = Mention.insert(f, into: session.draft)
                }
            }
            // How full the context is, in a slim bar over the box.
            ContextBar(info: session.info)
            // What to say, then one line: photos, the model (while it works:
            // steer or queue), the session's options in one menu,
            // dictation, and send or stop.
            VStack(alignment: .leading, spacing: 6) {
                if !sendingPhotos.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        PhotoStrip(photos: .constant(sendingPhotos), sending: true)
                        Text(sendingPhotos.count == 1 ? "Sending 1 photo…" : "Sending \(sendingPhotos.count) photos…")
                            .scaledFont(12.5)
                            .foregroundStyle(Color(p.muted))
                            .padding(.horizontal, 6)
                    }
                    .accessibilityElement(children: .combine)
                }
                if !photos.isEmpty { PhotoStrip(photos: $photos) }
                TextField(placeholder, text: $session.draft, axis: .vertical)
                    .lineLimit(1...6)
                    .scaledFont(16.5)
                    .focused($focused)
                    .padding(.horizontal, 6)
                    .padding(.top, 4)
                if big {
                    HStack(spacing: 0) {
                        if session.busy { SteerSwitch(session: session) } else { ModelChip(session: session, big: big) { openSheet(.models) } }
                        Spacer(minLength: 0)
                    }
                }
                HStack(spacing: 6) {
                    PhotoButton(photos: $photos, size: 34)
                    if !big { if session.busy { SteerSwitch(session: session) } else { ModelChip(session: session, big: big) { openSheet(.models) } } }
                    ComposerOptions(session: session, levels: levels)
                    Spacer(minLength: 0)
                    MicButton(text: $session.draft, size: 34)
                    sendOrStop
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .card(p, radius: 22)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        // `@`: the session folder's files, and its other sessions (the
        // agent gets what one said), as you type.
        .task(id: mention) {
            guard let q = mention else {
                fileMatches = []
                sessionMatches = []
                return
            }
            try? await Task.sleep(for: .milliseconds(150))
            guard !Task.isCancelled else { return }
            let f = await model.files(matching: q)
            let s = await model.sessionMentions(matching: q)
            if !Task.isCancelled {
                fileMatches = f
                sessionMatches = s
            }
        }
        // The thinking levels its model has (for the options menu).
        .task(id: session.liveKey + "|" + session.info.modelId) {
            guard !session.liveKey.isEmpty, !session.info.thinking.isEmpty else {
                levels = []
                return
            }
            levels = await model.thinkingLevels()
        }
        // Stopped before it was sent: its photos back in the strip.
        .onChange(of: session.returnedPhotos, initial: true) { takeBack() }
    }

    @ViewBuilder var sendOrStop: some View {
        if session.busy && text.isEmpty && photos.isEmpty {
            Button {
                UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                Task { await model.stop() }
            } label: {
                Image(systemName: "stop.fill")
                    .font(.system(size: 13, weight: .bold))
                    .frame(width: 36, height: 36)
                    .background { Circle().fill(Color(p.error)) }
                    .foregroundStyle(Color.white)
            }
            .accessibilityLabel("Stop")
        } else {
            Button(action: send) {
                Image(systemName: "arrow.up")
                    .font(.system(size: 16, weight: .bold))
                    .frame(width: 36, height: 36)
                    .background { Circle().fill(Color(canSend ? p.primary : p.subtle)) }
                    .foregroundStyle(Color(canSend ? p.onPrimary : p.muted))
            }
            .disabled(!canSend)
            .accessibilityLabel("Send")
        }
    }

    /// Photos of messages stopped before they were sent: back with the
    /// ones picked since.
    func takeBack() {
        guard !session.returnedPhotos.isEmpty else { return }
        photos = session.returnedPhotos + photos
        session.returnedPhotos = []
    }

    func send() {
        let (t, ph) = (text, photos)
        session.draft = ""
        photos = []
        sendingPhotos = ph
        onSend()
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        Task {
            let sent = await model.send(t, photos: ph)
            sendingPhotos = []
            if sent {
                // On the lock screen while it works.
                LiveActivities.start(for: session, model: model)
            } else {
                session.draft = t
                photos = ph
            }
        }
    }
}
