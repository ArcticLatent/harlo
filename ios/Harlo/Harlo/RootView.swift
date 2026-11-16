import HarloKit
import SwiftUI

struct RootView: View {
    @Environment(HarloModel.self) private var model

    var body: some View {
        Group {
            if model.host == nil {
                NavigationStack { PairView() }
            } else {
                MainTabs()
            }
        }
        .alert("harlo", isPresented: Binding(get: { model.alert != nil }, set: { if !$0 { model.alert = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(model.alert ?? "")
        }
    }
}
