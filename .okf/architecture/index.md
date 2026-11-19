# Architecture

* [Agent execution and model streams](agent.md) - How harlo-agent turns provider streams into a replayable, bounded tool execution loop.
* [Agent persistence and context management](agent-context.md) - How append-only session branches, recoverable outputs and compaction preserve continuity within a bounded model window.
* [Agent tools, sandbox and permissions](agent-tools.md) - The distinct safety boundaries around file changes, subprocesses, approvals and tool output.
* [Agents, model servers, and engine setup](core-engines.md) - The distinct agent and inference-engine lifecycles, shared local/remote setup logic, and configuration translation boundaries.
* [Code indexing and source-backed knowledge](knowledge.md) - How the code index supplies declarations and call evidence, and how the OKF wiki tracks ownership, validates citations, stamps pages, and detects changes.
* [Core application ownership and subsystem map](core.md) - The UI-independent coordination layer, its invariants, and the complete responsibility map for harlo-core.
* [Core persistence and derived views](core-persistence.md) - Ownership of application metadata, harness conversations, search indexes, remote copies, and disposable transcript pages.
* [Desktop application boundaries and lifecycle](desktop.md) - How the Qt desktop composes session services, native rendering, bundled resources, and background integrations.
* [Desktop code and terminal rendering](desktop-code.md) - The Qt-free document engine, incremental syntax pipeline, and native pixel/input bridge.
* [Desktop UI state and Qt model boundaries](desktop-ui.md) - Ownership, invalidation, and component lifetime rules connecting Workspace, ChatSession, and QML.
* [Harness protocols and session files](harness.md) - How Harlo and Claude Code become one event stream without pretending their storage and runtime protocols are identical.
* [Mobile notifications, widgets and Live Activities](mobile-notifications.md) - How the app and its extensions share encrypted push content and cached snapshots without a permanent background socket.
* [Mobile pairing, encryption and reconnect](mobile-protocol.md) - The phone's trust bootstrap, sealed message format, endpoint selection, and foreground recovery behavior.
* [Native mobile app and HarloKit](mobile-native.md) - How the iPhone UI, observable model, protocol client, and native rendering share responsibility.
