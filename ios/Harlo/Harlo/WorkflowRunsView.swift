import HarloKit
import SwiftUI

/// Every workflow run harlo keeps, newest first: tap one for it in full.
struct WorkflowRunsView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @State private var openRun: WorkflowRun?

    var body: some View {
        List {
            ForEach(model.workflowRuns) { run in
                Button { openRun = run } label: { WorkflowRunRow(run: run) }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("workflowRun")
                    .plainRow()
            }
        }
        .listStyle(.plain)
        .screen()
        .navigationTitle("Workflow runs")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarVisibility(.visible, for: .navigationBar)
        .overlay {
            if model.workflowRuns.isEmpty {
                ContentUnavailableView("No runs", systemImage: "list.bullet.rectangle", description: Text("Start one in harlo on your computer: a project's menu → Run a workflow…"))
            }
        }
        .refreshable { await model.loadWorkflows() }
        .task { await model.loadWorkflows() }
        .onAppear { model.watchRuns(true) }
        .onDisappear { model.watchRuns(false) }
        .sheet(item: $openRun) { run in WorkflowSheet(id: run.id) }
    }
}
