import HarloKit
import SwiftUI
import WidgetKit

@main
struct HarloWidgets: WidgetBundle {
    var body: some Widget {
        SessionLiveActivity()
        SessionsWidget()
        LimitsWidget()
    }
}
