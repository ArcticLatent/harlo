import HarloKit
import SwiftUI

/// The wiki sheet's top: what the repository's wiki (in .okf/) owes the
/// code, and Update, which starts a session on the computer when there's
/// something to do. Without one, what it is and Create.
struct WikiSection: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let wiki: Wiki
    @Binding var notice: String?
    /// Opens the session writing it.
    var showSession: (String) -> Void

    var body: some View {
        Section {
            if !wiki.error.isEmpty {
                Text("The wiki couldn't be read: \(wiki.error)")
                    .font(.subheadline)
                    .foregroundStyle(Color(p.muted))
            } else if wiki.loading {
                HStack(spacing: 8) {
                    ProgressView()
                    Text("Reading the wiki…").font(.subheadline).foregroundStyle(Color(p.muted))
                }
            } else if !wiki.exists || wiki.pages.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("No wiki yet").font(.body.weight(.medium))
                    Text("Your agent writes one from the code: how its parts fit and why, each claim footnoted with the code it came from. harlo tells you when that code changes, and agents read the pages instead of searching through the code.")
                        .font(.footnote)
                        .foregroundStyle(Color(p.muted))
                }
                .padding(.vertical, 2)
                if wiki.running {
                    running
                } else {
                    runButton("create", "Create wiki", "book")
                }
            } else {
                Text(wiki.freshness().joined(separator: " · "))
                    .font(.subheadline)
                    .foregroundStyle(Color(wiki.wantsUpdate && !wiki.running ? p.foreground : p.muted))
                    .padding(.vertical, 2)
                    .accessibilityIdentifier("wikiFreshness")
                if wiki.running {
                    running
                } else if !wiki.checking {
                    runButton("update", "Update the wiki", "arrow.clockwise")
                }
            }
        } footer: {
            if wiki.error.isEmpty {
                Text(!wiki.exists || wiki.pages.isEmpty
                     ? "It's written in a session on your computer you can watch, and spends that agent's tokens. The pages land in .okf/ for you to review in Changes."
                     : wiki.enabled ? "Agents are pointed to it, in sessions started from now on." : "Agents aren't told of it: switched off in harlo's Wiki panel on the computer.")
            }
        }
        // While it's read, checked or written: again now and then.
        .task(id: wiki.running || wiki.checking || wiki.loading) {
            while (wiki.running || wiki.checking || wiki.loading) && !Task.isCancelled {
                try? await Task.sleep(for: .seconds(wiki.loading ? 1 : 3))
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
                    notice = mode == "update" ? "harlo checks the code first: a session on your computer updates the wiki only when there's something to do."
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
