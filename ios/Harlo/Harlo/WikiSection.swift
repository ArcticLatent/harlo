import HarloKit
import SwiftUI

/// The wiki sheet's top: how fresh the repository's wiki (OpenWiki's, in
/// openwiki/) is against the code, and a run that writes it, started on
/// the computer. Without one, what it is and Create.
struct WikiSection: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let wiki: Wiki
    @Binding var notice: String?
    /// Opens the session writing it.
    var showSession: (String) -> Void

    var body: some View {
        Section {
            if wiki.loading {
                HStack(spacing: 8) {
                    ProgressView()
                    Text("Reading the wiki…").font(.subheadline).foregroundStyle(Color(p.muted))
                }
            } else if !wiki.exists {
                VStack(alignment: .leading, spacing: 4) {
                    Text("No wiki yet").font(.body.weight(.medium))
                    Text("Your agent writes one from the code: how its parts work and why, each fact tied to the lines it came from. Agents search it instead of reading through the code, and you can read it here.")
                        .font(.footnote)
                        .foregroundStyle(Color(p.muted))
                }
                .padding(.vertical, 2)
                if wiki.running {
                    running
                } else if wiki.canWrite {
                    runButton("create", "Create wiki", "book")
                }
            } else {
                VStack(alignment: .leading, spacing: 6) {
                    Text(wiki.freshness())
                        .font(.subheadline)
                        .foregroundStyle(Color(wiki.run != nil && !wiki.running ? p.warning : p.muted))
                        .accessibilityIdentifier("wikiFreshness")
                    if let r = wiki.run, wiki.running, r.total > 0 {
                        ProgressView(value: Double(r.done), total: Double(max(r.total, 1)))
                            .tint(Color(p.primary))
                    }
                }
                .padding(.vertical, 2)
                if wiki.running {
                    running
                } else if wiki.canWrite && !wiki.checking {
                    runButton(wiki.run != nil ? "continue" : "update", wiki.run != nil ? "Continue the run" : "Update the wiki",
                              wiki.run != nil ? "play.circle" : "arrow.clockwise")
                }
            }
        } footer: {
            Text(!wiki.exists
                 ? "It's written in a session on your computer you can watch, and spends that agent's tokens. The pages land in openwiki/ for you to review in Changes."
                 : wiki.enabled ? "Agents search it in sessions started from now on." : "Agents don't use it: switched off in harlo's Wiki panel on the computer.")
        }
        // While a run writes it: how far along, now and then.
        .task(id: wiki.running || wiki.checking) {
            while (wiki.running || wiki.checking) && !Task.isCancelled {
                try? await Task.sleep(for: .seconds(3))
                await model.loadWiki()
            }
        }
    }

    @ViewBuilder var running: some View {
        if !wiki.session.isEmpty {
            Button {
                showSession(wiki.session)
            } label: {
                Label("Show the session writing it", systemImage: "arrow.up.forward.square")
            }
            .frame(minHeight: 44)
        }
    }

    func runButton(_ mode: String, _ title: String, _ icon: String) -> some View {
        Button {
            UISelectionFeedbackGenerator().selectionChanged()
            Task {
                if await model.runWiki(mode) {
                    notice = mode == "update" ? "harlo is checking what changed: a session on your computer updates the wiki only if a fact's code changed or files were added."
                                              : "A session on your computer is writing the wiki."
                }
            }
        } label: {
            Label(title, systemImage: icon)
                .font(.body.weight(wiki.wantsUpdate || !wiki.exists ? .semibold : .regular))
        }
        .frame(minHeight: 44)
        .accessibilityIdentifier("wikiRun")
    }
}
