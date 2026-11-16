import ActivityKit
import HarloKit
import SwiftUI
import WidgetKit

/// A session at work on the lock screen and in the Dynamic Island: what
/// it's doing (opened with this phone's keys: harlo seals it), its todos
/// and for how long; waiting for you; done.
struct SessionLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: SessionActivityAttributes.self) { context in
            LockScreenView(attributes: context.attributes, state: context.state, stale: context.isStale)
                .padding(16)
                .activityBackgroundTint(Color.black.opacity(0.55))
                .activitySystemActionForegroundColor(Live.accent)
                .widgetURL(Live.url(context.attributes.key))
        } dynamicIsland: { context in
            let state = context.state
            let stale = context.isStale
            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Live.mark(state.phase)
                        .padding(.leading, 4)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Live.clock(state)
                        .font(.system(size: 15, weight: .semibold).monospacedDigit())
                        .frame(maxWidth: 70, alignment: .trailing)
                        .padding(.trailing, 4)
                }
                DynamicIslandExpandedRegion(.center) {
                    Text(context.attributes.title)
                        .font(.system(size: 15, weight: .semibold))
                        .lineLimit(1)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(Live.words(state, stale: stale))
                            .font(.system(size: 13, design: .monospaced))
                            .foregroundStyle(stale ? .secondary : Live.color(state.phase))
                            .lineLimit(2)
                        if state.total > 0 {
                            ProgressView(value: Double(state.done), total: Double(state.total)) {
                                EmptyView()
                            } currentValueLabel: {
                                Text("\(state.done)/\(state.total) todos").font(.system(size: 11, design: .monospaced))
                            }
                            .tint(Live.accent)
                        }
                    }
                    .padding(.horizontal, 4)
                }
            } compactLeading: {
                Live.mark(state.phase)
            } compactTrailing: {
                if state.total > 0 && state.phase == "working" {
                    Text("\(state.done)/\(state.total)")
                        .font(.system(size: 13, weight: .semibold, design: .monospaced))
                        .foregroundStyle(Live.accent)
                } else {
                    Live.clock(state)
                        .font(.system(size: 13, weight: .semibold).monospacedDigit())
                        .frame(maxWidth: 52)
                        .foregroundStyle(Live.color(state.phase))
                }
            } minimal: {
                Live.mark(state.phase)
            }
            .widgetURL(Live.url(context.attributes.key))
            .keylineTint(Live.accent)
        }
    }
}
