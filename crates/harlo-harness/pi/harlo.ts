// harlo's extension for pi, loaded with `-e` into the pi sessions harlo
// starts. It gives pi:
//
// - harlo's tools (other sessions, reports to a parent),
//   from harlo's MCP bridge (HARLO_MCP_BRIDGE: its command line, as JSON),
//   declared to the model directly;
// - what those tools are for, as a section of the system prompt, asked of
//   harlo once a session (the bridge's socket, HARLO_MCP_SOCKET);
// - harlo's approval modes: `always-ask` asks before anything but reading,
//   `write` before commands and tools that may change things, `yolo` and
//   `auto` never. harlo sets them with `/harlo-modes {"approval": …}`;
// - harlo's sandbox and its check for commands that can't be undone, from
//   harlo-helper (HARLO_HELPER_BIN; the policy in HARLO_SANDBOX): such a
//   command is asked about in every mode, bash runs in the sandbox, and the
//   file tools keep out of hidden places and to the writable ones;
// - harlo's UI for other extensions (`hostUi` below): what pi keeps for
//   its terminal (an extension's own screens, overlays, widgets, footer
//   and header, and its drawing of tool calls and messages) is drawn by
//   the extension's components in harlo's colours and sent to harlo as
//   lines of styled text (with the Markdown and pictures in them, which
//   harlo draws itself), with harlo's keys, clicks and widths coming back.

import { AgentSession, type ExtensionAPI, Theme } from "@earendil-works/pi-coding-agent";
import { Box, Container, getKeybindings, HStack, Image, Markdown, Spacer, VStack } from "@earendil-works/pi-tui";
import { execFile, execFileSync } from "node:child_process";
import { connect } from "node:net";
import { homedir } from "node:os";
import { resolve } from "node:path";

/** Tools that only look: never asked about. */
const READS = new Set(["read", "grep", "find", "ls", "tool_search", "list_mcp_resources", "list_mcp_resource_templates", "read_mcp_resource"]);
/** Tools that change files: asked in `always-ask`, not in `write`. */
const EDITS = new Set(["write", "edit"]);

const DENIED = "The user denied this call. Don't try it another way: ask what they'd like instead, or carry on without it.";
/** Tools that take a path to look at or change. */
const PATHS = new Set(["read", "grep", "find", "ls", "write", "edit"]);

/** harlo-helper's sandbox for the session's folder, as `describe` gives it. */
interface Box {
  on: boolean;
  about?: string;
}

/** Asks harlo-helper's `sandbox` command; its JSON answer, or none. */
function guard(bin: string, args: string[]): Promise<Record<string, unknown> | undefined> {
  return new Promise((done) => {
    execFile(bin, ["sandbox", ...args], { timeout: 10000, maxBuffer: 1 << 20 }, (err, out) => {
      try {
        done(err ? undefined : JSON.parse(out));
      } catch {
        done(undefined);
      }
    });
  });
}

/** The wiki's note for a read of `file` when it's a `.okf/` page whose
 * code changed since it was checked (harlo-helper's `readingNote`); "" for
 * a current page or any other file. */
function wikiNote(bin: string, file: string): Promise<string> {
  const at = file.lastIndexOf("/.okf/");
  if (at < 0 || !file.endsWith(".md")) return Promise.resolve("");
  const request = JSON.stringify({ op: "readingNote", root: file.slice(0, at), path: file.slice(at + "/.okf/".length) });
  return new Promise((done) => {
    const child = execFile(bin, ["wiki", "--okf"], { timeout: 10000, maxBuffer: 1 << 20 }, (err, out) => {
      try {
        const text = err ? "" : JSON.parse(out).text;
        done(typeof text === "string" ? text : "");
      } catch {
        done("");
      }
    });
    child.stdin?.end(request);
  });
}

/** `text` quoted for bash. */
function quoted(text: string): string {
  return "'" + text.replace(/'/g, "'\\''") + "'";
}

/** A tool's path as pi resolves it: `@` and `~` handled, from `cwd`. */
function pathOf(cwd: string, raw: unknown): string {
  let p = typeof raw === "string" && raw.trim() ? raw.trim() : ".";
  if (p.startsWith("@")) p = p.slice(1);
  if (p === "~" || p.startsWith("~/")) p = homedir() + p.slice(1);
  return resolve(cwd, p);
}

/** The bridge's command line, or none (harlo's tools are off). */
function bridge(): string[] | undefined {
  try {
    const parsed = JSON.parse(process.env.HARLO_MCP_BRIDGE ?? "");
    return Array.isArray(parsed) && parsed.length > 0 && parsed.every((p) => typeof p === "string") ? parsed : undefined;
  } catch {
    return undefined;
  }
}

/** Asks harlo for what this session's tools are for ("" when it can't). */
function instructions(socket: string, session: string): Promise<string> {
  return new Promise((resolve) => {
    let answer = "";
    const done = (text: string) => {
      clearTimeout(timer);
      conn.destroy();
      resolve(text);
    };
    const conn = connect(socket, () => conn.write(JSON.stringify({ session, tool: "_instructions", args: {} }) + "\n"));
    const timer = setTimeout(() => done(""), 5000);
    conn.on("data", (chunk) => {
      answer += chunk.toString("utf8");
      const end = answer.indexOf("\n");
      if (end < 0) return;
      try {
        const reply = JSON.parse(answer.slice(0, end));
        done(reply.ok === true && typeof reply.result === "string" ? reply.result : "");
      } catch {
        done("");
      }
    });
    conn.on("error", () => done(""));
  });
}

/** One line of a call's arguments, for the approval card. */
function detail(name: string, input: Record<string, unknown>): string {
  const text = (v: unknown) => (typeof v === "string" ? v : "");
  if (name === "bash") return text(input.command);
  if (name === "write" || name === "edit") return text(input.path);
  const all = JSON.stringify(input);
  return all.length > 300 ? all.slice(0, 299) + "…" : all;
}

// ---------------------------------------------------------------------------
// harlo's UI for other extensions.
//
// pi's RPC mode leaves out what needs its terminal. When it binds its UI
// for the extensions, harlo's version takes the place of those parts:
// each piece of an extension's own UI is a view, the lines its component
// draws for the width harlo gives it.
//
// Out, on pi's stdout beside its RPC events, `{"type":"harlo_ui", op, …}`:
// - `hello`: `{shortcuts}`, the extensions' shortcut keys, once bound;
// - `view`: `{id, kind, lines, …}`, a view's lines, whole, each time they
//   change. `kind` is `screen` (custom(): at the message box, taking the
//   keys), `overlay` (over the chat; `options` as the extension gave them,
//   `hidden`), `widget` (`key`, `placement` aboveEditor|belowEditor),
//   `footer`, `header`, `tool` (`toolCallId`: `lines` of its call,
//   `result` of its result, `shell` "self" when it draws its own frame) or
//   `message` (`timestamp`, `customType` of the custom message it draws).
//   `focus`: the view takes the keys. `blocks` (and a tool's
//   `resultBlocks`), when the lines hold Markdown or a picture: the lines
//   in pieces, top to bottom, `{kind:"lines", row, rows}` (those rows of
//   `lines`), `{kind:"markdown", text, rows, bg?, pad?}` or
//   `{kind:"image", data, mime, cells, rows, bg?, pad?}` (`cells`: its
//   widest, in columns; `rows`: what the lines
//   give it; `bg` "#rrggbb" and `pad` columns: the Box it sits in);
// - `close`: `{id}`;
// - `working`: `{message}` or `{visible}`, the working line.
// In, as an `extension_ui_response` (pi's RPC ignores ids it doesn't know)
// with id `harlo:<view>` (`harlo:` alone: no view) and `harlo`:
// - `{op:"key", data}`: terminal key bytes;
// - `{op:"mouse", event, part}`: a TuiMouseEvent in the view's cells
//   (`part` "result" for a tool's result lines);
// - `{op:"size", width, rows}`: the view's columns (and the chat's rows);
//   with no view, the default for every view;
// - `{op:"close"}`: the user closed the view (Esc on it, as a key first);
// - `{op:"editor", text}`: the message box's text;
// - `{op:"expand", expanded}`: tool results open or folded (with a view:
//   that tool's or message's only);
// - `{op:"theme", mode, fg, bg}`: harlo's colours as pi's theme tokens
//   ("#rrggbb"), what every view is drawn with;
// - `{op:"shortcut", key}`: one of hello's shortcuts;
// - `{op:"draw", messages}`: custom messages from the session's history.

type Lines = { render(width: number): string[]; handleInput?(data: string): void; handleMouse?(event: Record<string, unknown>): { render?: boolean } | undefined; invalidate?(): void; dispose?(): void; focused?: boolean };
type Ui = Record<string, any>;
type Runner = Record<string, any>;

interface View {
  id: string;
  kind: string;
  /** What it draws for `width`: `lines`, and for a tool `result`. */
  draw(width: number): Record<string, unknown>;
  /** The component that takes the keys and the mouse (a tool's: by part). */
  target(part?: string): Lines | undefined;
  meta: Record<string, unknown>;
  width?: number;
  sent?: string;
  focus?: boolean;
  hidden?: boolean;
  dispose?(): void;
  inputs?: Set<(data: string) => { consume?: boolean; data?: string } | undefined>;
  /** Its own open or folded, once harlo set it. */
  expanded?: boolean;
}

// -- harlo's colours ---------------------------------------------------------

/** pi's theme lives here (for every copy of pi's modules). */
const THEME_KEY = Symbol.for("@earendil-works/pi-coding-agent:theme");
/** harlo's colours, as last sent (kept for a host bound later). */
let harloLook: { mode: string; fg: Record<string, string>; bg: Record<string, string> } | undefined;

/** Makes harlo's colours pi's theme: what extensions draw with, whether
 * they hold the theme they were given or ask pi for it. */
function useLook() {
  if (!harloLook) return;
  try {
    const theme = new Theme(harloLook.fg as any, harloLook.bg as any, "truecolor", { name: "harlo", appearance: harloLook.mode === "light" ? "light" : "dark" });
    (globalThis as Record<symbol, unknown>)[THEME_KEY] = theme;
  } catch {
    // (pi's own theme stays.)
  }
}

// -- blocks: the Markdown and pictures among a view's lines -----------------

type Block = Record<string, unknown> & { kind: string; rows: number };

/** A background function's colour ("#rrggbb"), when it sets one. */
function bgOf(fn: unknown): string | undefined {
  if (typeof fn !== "function") return undefined;
  try {
    const m = /\x1b\[48;2;(\d+);(\d+);(\d+)m/.exec(String(fn("x")));
    return m ? "#" + m.slice(1).map((v) => Number(v).toString(16).padStart(2, "0")).join("") : undefined;
  } catch {
    return undefined;
  }
}

/** `comp`'s lines at `width` in blocks, where its tree can be followed
 * (containers stacking their children; a Box's padding), else one block
 * of lines. */
function walk(comp: any, width: number, bg: string | undefined, pad: number, out: Block[]) {
  if (comp instanceof Markdown) {
    out.push({ kind: "markdown", text: String((comp as any).text ?? ""), rows: comp.render(width).length, bg, pad });
  } else if (comp instanceof Image) {
    const cells = Number((comp as any).options?.maxWidthCells) || 60;
    out.push({ kind: "image", data: String((comp as any).base64Data ?? ""), mime: String((comp as any).mimeType ?? ""), cells, rows: comp.render(width).length, bg, pad });
  } else if (comp instanceof Spacer) {
    out.push({ kind: "space", rows: comp.render(width).length, bg });
  } else if (comp instanceof Box) {
    const b = comp as any;
    const own = bgOf(b.bgFn) ?? bg;
    const inner = Math.max(1, width - b.paddingX * 2);
    const rows = comp.render(width).length;
    if (rows === 0) return;
    if (b.paddingY > 0) out.push({ kind: "space", rows: b.paddingY, bg: own });
    for (const child of b.children ?? []) walk(child, inner, own, pad + b.paddingX, out);
    if (b.paddingY > 0) out.push({ kind: "space", rows: b.paddingY, bg: own });
  } else if (comp instanceof Container && !(comp instanceof HStack) && !(comp instanceof VStack && (comp as any).gap > 0)) {
    for (const child of comp.children) walk(child, width, bg, pad, out);
  } else {
    out.push({ kind: "lines", rows: comp.render(width).length });
  }
}

/** The blocks of `lines` (`comp` drew them at `width`): none when they
 * hold no Markdown or picture, or the tree doesn't add up to them. */
function blocksOf(comp: unknown, width: number, lines: string[]): Block[] | undefined {
  if (!comp || typeof comp !== "object") return undefined;
  const found: Block[] = [];
  try {
    walk(comp, width, undefined, 0, found);
  } catch {
    return undefined;
  }
  if (!found.some((b) => b.kind === "markdown" || b.kind === "image")) return undefined;
  if (found.reduce((n, b) => n + b.rows, 0) !== lines.length) return undefined;
  // (Lines next to lines are one block; each says where its rows are.)
  const out: Block[] = [];
  let row = 0;
  for (const b of found) {
    if (b.rows === 0 && b.kind !== "markdown" && b.kind !== "image") continue;
    const last = out[out.length - 1];
    const plain = b.kind === "lines" || b.kind === "space";
    if (plain && last?.kind === "lines") last.rows += b.rows;
    else if (plain && b.kind === "lines") out.push({ kind: "lines", row, rows: b.rows });
    else out.push(b.kind === "space" ? { ...b, kind: "lines", row } : b);
    row += b.rows;
  }
  for (const b of out) {
    if (b.bg === undefined) delete b.bg;
    if (!b.pad) delete b.pad;
    if (b.kind === "lines") delete b.bg;
  }
  return out;
}

const writeOut = Object.getPrototypeOf(process.stdout).write as (this: NodeJS.WriteStream, chunk: string) => boolean;
function sendUi(frame: Record<string, unknown>) {
  // (pi sends its own output through process.stdout's own write, put aside
  // when RPC mode starts; a whole line per write keeps both intact.)
  writeOut.call(process.stdout, JSON.stringify({ type: "harlo_ui", ...frame }) + "\n");
}

/** Wraps `bindExtensions` once, so RPC mode's UI becomes harlo's (one
 * host at a time: a new or switched session binds again). */
function hostUi() {
  const proto = AgentSession.prototype as unknown as Record<string | symbol, any>;
  const mark = Symbol.for("harlo.ui");
  if (proto[mark]) return;
  proto[mark] = true;
  const bind = proto.bindExtensions;
  let current: UiHost | undefined;
  proto.bindExtensions = function (this: { extensionRunner: Runner; subscribe(l: (e: any) => void): () => void }, bindings: Record<string, any>) {
    if (bindings?.mode !== "rpc" || !bindings.uiContext) return bind.call(this, bindings);
    current?.dispose();
    const host = new UiHost(bindings.uiContext, () => this.extensionRunner);
    current = host;
    host.unsubscribe = this.subscribe((e) => host.event(e));
    return bind.call(this, { ...bindings, uiContext: host.ui, mode: "tui" }).then((r: unknown) => {
      host.hello();
      return r;
    });
  };
}

class UiHost {
  views = new Map<string, View>();
  dirty = new Set<View>();
  timer?: ReturnType<typeof setTimeout>;
  width = 80;
  rows = 24;
  serial = 0;
  editorText = "";
  expanded = false;
  statuses = new Map<string, string>();
  terminalInputs = new Set<(data: string) => { consume?: boolean; data?: string } | undefined>();
  ui: Ui;

  unsubscribe?: () => void;
  onData = (chunk: Buffer | string) => this.read(chunk);

  constructor(
    readonly base: Ui,
    readonly runner: () => Runner,
  ) {
    this.ui = this.makeUi();
    // (Only once pi reads stdin itself: a listener of ours alone would set
    // it flowing while pi still binds, and pi would never see what came
    // meanwhile, its first commands.)
    if (process.stdin.listenerCount("data") > 0) process.stdin.on("data", this.onData);
    else process.stdin.on("newListener", this.onListener);
  }

  onListener = (event: string | symbol) => {
    if (event !== "data" || this.disposed) return;
    process.stdin.off("newListener", this.onListener);
    // (After pi's own, which is added once this returns.)
    queueMicrotask(() => {
      if (!this.disposed) process.stdin.on("data", this.onData);
    });
  };
  disposed = false;

  dispose() {
    this.disposed = true;
    process.stdin.off("newListener", this.onListener);
    process.stdin.off("data", this.onData);
    this.unsubscribe?.();
    for (const v of [...this.views.values()]) this.close(v);
  }

  /** The session's events, as harlo gets them. */
  event(e: Record<string, any>) {
    switch (e?.type) {
      case "tool_execution_start":
        return this.tool(e.toolCallId, e.toolName, e.args, { started: true });
      case "tool_execution_update":
        return this.tool(e.toolCallId, e.toolName, e.args, { result: e.partialResult, partial: true });
      case "tool_execution_end":
        return this.tool(e.toolCallId, e.toolName, undefined, { result: e.result, isError: e.isError, durationMs: e.durationMs, partial: false });
      case "message_end":
        if (e.message?.role === "custom") this.message(e.message);
        return;
    }
  }

  hello() {
    // (A session bound again may have set pi's theme anew.)
    useLook();
    let shortcuts: string[] = [];
    try {
      shortcuts = [...this.runner().getShortcuts(getKeybindings().getResolvedBindings()).keys()].map(String);
    } catch {
      // (None.)
    }
    sendUi({ op: "hello", shortcuts });
  }

  // -- harlo's input ------------------------------------------------------

  pending = "";
  read(chunk: Buffer | string) {
    this.pending += chunk.toString();
    for (let end = this.pending.indexOf("\n"); end >= 0; end = this.pending.indexOf("\n")) {
      const line = this.pending.slice(0, end);
      this.pending = this.pending.slice(end + 1);
      if (!line.includes('"harlo:')) continue;
      try {
        const frame = JSON.parse(line);
        if (frame?.type === "extension_ui_response" && typeof frame.id === "string" && frame.id.startsWith("harlo:")) this.input(frame.id.slice(6), frame.harlo ?? {});
      } catch {
        // (Not one of harlo's.)
      }
    }
  }

  input(id: string, msg: Record<string, any>) {
    const view = id ? this.views.get(id) : undefined;
    switch (msg.op) {
      case "key": {
        let data = String(msg.data ?? "");
        const target = view ?? [...this.views.values()].reverse().find((v) => v.focus && !v.hidden);
        for (const listener of [...(target?.inputs ?? []), ...this.terminalInputs]) {
          const r = listener(data);
          if (r?.consume) return;
          if (typeof r?.data === "string") data = r.data;
        }
        const comp = target?.target();
        if (comp?.handleInput) {
          comp.handleInput(data);
          this.redraw(target!);
        }
        return;
      }
      case "mouse": {
        const comp = view?.target(msg.part);
        if (!view || typeof msg.event !== "object") return;
        const r = comp?.handleMouse?.(msg.event);
        // (A click a tool's or a message's drawing doesn't take opens or
        // folds it, as pi's expand key does.)
        if (r === undefined && msg.event.type === "click" && (view.kind === "tool" || view.kind === "message")) {
          view.expanded = !this.expandedOf(view);
          this.redraw(view);
          return;
        }
        if (comp?.handleMouse && r?.render !== false) this.redraw(view);
        return;
      }
      case "size": {
        const width = Math.max(10, Math.floor(Number(msg.width) || 0));
        if (Number(msg.rows) > 0) this.rows = Math.floor(Number(msg.rows));
        if (view) {
          view.width = width;
          this.redraw(view);
        } else if (Number(msg.width) > 0) {
          this.width = width;
          for (const v of this.views.values()) if (v.width === undefined) this.redraw(v);
        }
        return;
      }
      case "close":
        if (view) {
          // (As the Esc key first: most screens end themselves on it.)
          this.input(id, { op: "key", data: "\x1b" });
          if (this.views.has(id) && view.kind !== "screen" && view.kind !== "overlay") this.close(view);
        }
        return;
      case "editor":
        this.editorText = String(msg.text ?? "");
        return;
      case "expand":
        if (view) {
          view.expanded = msg.expanded === true;
          this.redraw(view);
          return;
        }
        this.expanded = msg.expanded === true;
        for (const v of this.views.values()) {
          if (v.kind !== "tool" && v.kind !== "message") continue;
          v.expanded = undefined;
          this.redraw(v);
        }
        return;
      case "theme":
        if (typeof msg.fg !== "object" || typeof msg.bg !== "object") return;
        harloLook = { mode: String(msg.mode ?? "dark"), fg: msg.fg, bg: msg.bg };
        useLook();
        // (Components keep what they drew: drawn again, in the new colours.)
        for (const v of this.views.values()) {
          for (const part of ["call", "result"]) {
            try {
              v.target(part)?.invalidate?.();
            } catch {
              // (Its own trouble.)
            }
          }
          this.redraw(v);
        }
        return;
      case "shortcut": {
        try {
          const runner = this.runner();
          const found = runner.getShortcuts(getKeybindings().getResolvedBindings()).get(msg.key);
          void found?.handler?.(runner.createContext());
        } catch {
          // (Gone since.)
        }
        return;
      }
      case "draw":
        for (const m of Array.isArray(msg.messages) ? msg.messages : []) this.message(m);
        return;
    }
  }

  // -- views ---------------------------------------------------------------

  newView(kind: string, id = `${kind}:${++this.serial}`): View {
    const view: View = { id, kind, meta: {}, draw: () => ({ lines: [] }), target: () => undefined };
    this.views.set(id, view);
    return view;
  }

  /** A view whose lines are one component's. */
  attach(view: View, comp: Lines) {
    view.dispose?.();
    view.draw = (w) => {
      const lines = comp.render(w);
      return { lines, blocks: blocksOf(comp, w, lines) };
    };
    view.target = () => comp;
    view.dispose = () => comp.dispose?.();
    this.redraw(view);
  }

  close(view: View) {
    if (!this.views.delete(view.id)) return;
    this.dirty.delete(view);
    try {
      view.dispose?.();
    } catch {
      // (Its own trouble.)
    }
    sendUi({ op: "close", id: view.id });
  }

  redraw(view: View) {
    this.dirty.add(view);
    this.timer ??= setTimeout(() => this.flush(), 16);
  }

  flush() {
    this.timer = undefined;
    const views = [...this.dirty];
    this.dirty.clear();
    for (const v of views) this.paint(v);
  }

  paint(view: View) {
    if (!this.views.has(view.id)) return;
    let drawn: Record<string, unknown>;
    try {
      drawn = view.draw(view.width ?? this.width);
    } catch (e) {
      drawn = { lines: [`(${view.kind} couldn't be drawn: ${e instanceof Error ? e.message : String(e)})`] };
    }
    const frame = { op: "view", id: view.id, kind: view.kind, ...view.meta, focus: view.focus === true, hidden: view.hidden === true, ...drawn };
    const text = JSON.stringify(frame);
    if (text === view.sent) return;
    view.sent = text;
    sendUi(frame);
  }

  /** What a component's factory gets as pi's TUI. */
  tuiFor(view: View): Record<string, any> {
    const host = this;
    const overlays: View[] = [];
    const noop = () => {};
    return {
      mode: "inline",
      children: [],
      fullRedraws: 0,
      terminal: {
        get columns() {
          return view.width ?? host.width;
        },
        get rows() {
          return host.rows;
        },
        write: noop,
        hideCursor: noop,
        showCursor: noop,
      },
      requestRender: () => host.redraw(view),
      renderNow: () => host.paint(view),
      setFocus: noop,
      addChild: noop,
      removeChild: noop,
      clear: noop,
      start: noop,
      stop: noop,
      invalidate: noop,
      setLayoutRoot: noop,
      getShowHardwareCursor: () => false,
      setShowHardwareCursor: noop,
      getClearOnShrink: () => false,
      setClearOnShrink: noop,
      onTerminalColorSchemeChange: () => noop,
      setTerminalColorSchemeNotifications: noop,
      queryTerminalColors: async () => undefined,
      addInputListener: (listener: (data: string) => { consume?: boolean; data?: string } | undefined) => {
        (view.inputs ??= new Set()).add(listener);
        return () => view.inputs?.delete(listener);
      },
      removeInputListener: (listener: (data: string) => { consume?: boolean; data?: string } | undefined) => view.inputs?.delete(listener),
      showOverlay: (comp: Lines, options?: unknown) => {
        const o = host.newView("overlay");
        o.meta = { options: options ?? {} };
        o.focus = true;
        host.attach(o, comp);
        overlays.push(o);
        return host.handleOf(o);
      },
      hideOverlay: () => {
        const o = overlays.pop();
        if (o) host.close(o);
      },
      hasOverlay: () => overlays.some((o) => host.views.has(o.id)),
    };
  }

  handleOf(view: View) {
    return {
      hide: () => this.close(view),
      setHidden: (hidden: boolean) => {
        view.hidden = hidden;
        this.redraw(view);
      },
      isHidden: () => view.hidden === true,
      focus: () => {
        view.focus = true;
        this.redraw(view);
      },
      unfocus: () => {
        view.focus = false;
        this.redraw(view);
      },
      isFocused: () => view.focus === true,
      getBounds: () => ({ row: 0, col: 0, width: view.width ?? this.width, height: 0 }),
    };
  }

  /** Whether a tool's or message's view is open: its own, else everyone's. */
  expandedOf(view: View) {
    return view.expanded ?? this.expanded;
  }

  /** pi's key hints for renderers, saying what works in harlo: a click
   * opens or folds a tool's result. */
  hints() {
    const kb = getKeybindings();
    const keyText = (id: string) => (id === "app.tools.expand" ? "click" : (kb.getKeys(id) ?? []).join("/"));
    return { keyText, keyHint: (id: string, description: string) => `${keyText(id)} ${description}` };
  }

  /** A view drawn by `factory(tui, theme)`, at `id` (replacing what was there). */
  placed(id: string, kind: string, meta: Record<string, unknown>, factory: ((tui: any, theme: any) => Lines) | string[] | undefined) {
    const old = this.views.get(id);
    if (factory === undefined) {
      if (old) this.close(old);
      return;
    }
    const view = old ?? this.newView(kind, id);
    view.meta = meta;
    if (Array.isArray(factory)) {
      this.attach(view, { render: () => factory.map(String) });
      return;
    }
    try {
      this.attach(view, factory(this.tuiFor(view), this.base.theme));
    } catch (e) {
      this.attach(view, { render: () => [`(${kind} couldn't start: ${e instanceof Error ? e.message : String(e)})`] });
    }
  }

  // -- tools and messages ---------------------------------------------------

  /** The renderers an extension gives a tool's calls (none for pi's own). */
  renderersOf(name: string): Record<string, any> | undefined {
    try {
      const runner = this.runner();
      const def = runner.getToolDefinition(name);
      const base = () => (def ? { renderShell: def.renderShell, renderCall: def.renderCall, renderResult: def.renderResult } : undefined);
      const r = runner.resolveToolRenderers(name, base);
      // (pi's own MCP extension draws every MCP tool alike: harlo's cards
      // do that better.)
      if (!r || (!def && name.startsWith("mcp__"))) return undefined;
      return r.renderCall || r.renderResult ? r : undefined;
    } catch {
      return undefined;
    }
  }

  toolState = new Map<string, Record<string, any>>();
  tool(toolCallId: string, name: string, args: unknown, change: Record<string, unknown>) {
    let t = this.toolState.get(toolCallId);
    if (!t) {
      const renderers = this.renderersOf(name);
      if (!renderers) return;
      t = { renderers, args: args ?? {}, state: {}, started: false, partial: false, isError: false, view: this.newView("tool", `tool:${toolCallId}`) };
      this.toolState.set(toolCallId, t);
      this.toolView(toolCallId, name, t);
    }
    if (args !== undefined) t.args = args;
    Object.assign(t, change);
    this.redraw(t.view);
  }

  toolView(toolCallId: string, name: string, t: Record<string, any>) {
    const view: View = t.view;
    view.meta = { toolCallId, toolName: name, shell: t.renderers.renderShell === "self" ? "self" : "default" };
    const hints = this.hints();
    const context = (slot: "call" | "result") => ({
      ...hints,
      args: t.args,
      toolCallId,
      invalidate: () => this.redraw(view),
      // (What this slot drew last: the result's is resultComp, not result.)
      lastComponent: slot === "call" ? t.call : t.resultComp,
      state: t.state,
      cwd: process.cwd(),
      executionStarted: t.started === true || t.result !== undefined,
      argsComplete: true,
      isPartial: t.partial === true,
      expanded: this.expandedOf(view),
      showImages: false,
      isError: t.isError === true,
      durationMs: t.durationMs,
      outputPad: 0,
    });
    const theme = this.base.theme;
    view.draw = (w) => {
      if (t.renderers.renderCall) t.call = t.renderers.renderCall(t.args, theme, context("call")) ?? t.call;
      if (t.renderers.renderResult && t.result !== undefined) t.resultComp = t.renderers.renderResult(t.result, { expanded: this.expandedOf(view), isPartial: t.partial === true }, theme, context("result")) ?? t.resultComp;
      const lines = t.call ? t.call.render(w) : [];
      const result = t.resultComp ? t.resultComp.render(w) : [];
      return { lines, result, blocks: blocksOf(t.call, w, lines), resultBlocks: blocksOf(t.resultComp, w, result) };
    };
    view.target = (part) => (part === "result" ? t.resultComp : t.call);
  }

  /** A custom message an extension draws itself. */
  message(m: Record<string, any>) {
    let renderer: any;
    try {
      renderer = this.runner().getMessageRenderer(String(m.customType ?? ""));
    } catch {
      return;
    }
    if (!renderer || m.display === false) return;
    const id = `message:${m.timestamp ?? ++this.serial}`;
    const view = this.views.get(id) ?? this.newView("message", id);
    view.meta = { timestamp: m.timestamp, customType: m.customType };
    let comp: Lines | undefined;
    view.draw = (w) => {
      comp = renderer(m, { expanded: this.expandedOf(view), outputPad: 0, ...this.hints() }, this.base.theme) ?? comp;
      const lines = comp ? comp.render(w) : [];
      return { lines, blocks: blocksOf(comp, w, lines) };
    };
    view.target = () => comp;
    this.redraw(view);
  }

  // -- the UI context -----------------------------------------------------

  makeUi(): Ui {
    const base = this.base;
    const host = this;
    // (pi copies the context's own properties when it wraps it: RPC's
    // methods are copied in, not inherited.)
    const ui: Ui = Object.defineProperties({}, Object.getOwnPropertyDescriptors(base));
    Object.assign(ui, {
      async custom(factory: (tui: any, theme: any, keybindings: any, done: (r: unknown) => void) => Lines | Promise<Lines>, options?: Record<string, any>) {
        const view = host.newView(options?.overlay ? "overlay" : "screen");
        view.focus = true;
        if (options?.overlay) {
          const o = options.overlayOptions;
          view.meta = { options: typeof o === "function" ? o() : (o ?? {}) };
        }
        return new Promise((resolve, reject) => {
          const done = (result: unknown) => {
            host.close(view);
            resolve(result);
          };
          Promise.resolve()
            .then(() => factory(host.tuiFor(view), base.theme, getKeybindings(), done))
            .then((comp) => {
              if (!host.views.has(view.id)) {
                comp?.dispose?.();
                return;
              }
              host.attach(view, comp);
              if (typeof options?.overlayOptions === "function") {
                const draw = view.draw;
                view.draw = (w) => {
                  view.meta = { options: options.overlayOptions() ?? {} };
                  return draw(w);
                };
              }
              options?.onHandle?.(host.handleOf(view));
            })
            .catch((e) => {
              host.close(view);
              reject(e);
            });
        });
      },
      setWidget(key: string, content: unknown, options?: { placement?: string }) {
        host.placed(`widget:${key}`, "widget", { key, placement: options?.placement ?? "aboveEditor" }, content as any);
      },
      setFooter(factory: ((tui: any, theme: any, data: unknown) => Lines) | undefined) {
        host.placed("footer", "footer", {}, factory && ((tui, theme) => factory(tui, theme, host.footerData())));
      },
      setHeader(factory: ((tui: any, theme: any) => Lines) | undefined) {
        host.placed("header", "header", {}, factory);
      },
      setStatus(key: string, text: string | undefined) {
        if (text === undefined) host.statuses.delete(key);
        else host.statuses.set(key, text);
        const footer = host.views.get("footer");
        if (footer) host.redraw(footer);
        base.setStatus(key, text);
      },
      setWorkingMessage(message?: string) {
        sendUi({ op: "working", message: message ?? null });
      },
      setWorkingVisible(visible: boolean) {
        sendUi({ op: "working", visible: visible !== false });
      },
      setWorkingIndicator() {
        // (Its frames are a terminal animation: harlo shows its own.)
      },
      setHiddenThinkingLabel() {
        // (harlo labels thinking itself.)
      },
      onTerminalInput(handler: (data: string) => { consume?: boolean; data?: string } | undefined) {
        host.terminalInputs.add(handler);
        return () => host.terminalInputs.delete(handler);
      },
      getEditorText: () => host.editorText,
      setEditorText(text: string) {
        host.editorText = text;
        base.setEditorText(text);
      },
      getToolsExpanded: () => host.expanded,
      setToolsExpanded(expanded: boolean) {
        host.input("", { op: "expand", expanded: expanded === true });
      },
    });
    return ui;
  }

  footerData() {
    const host = this;
    return {
      getGitBranch: () => {
        try {
          return execFileSync("git", ["rev-parse", "--abbrev-ref", "HEAD"], { cwd: process.cwd(), encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim() || null;
        } catch {
          return null;
        }
      },
      getExtensionStatuses: () => host.statuses,
      getAvailableProviderCount: () => 1,
      onBranchChange: () => () => {},
    };
  }
}

export default function harlo(pi: ExtensionAPI) {
  hostUi();
  const modes = { approval: "always-ask" };
  const always = new Set<string>();
  const command = bridge();
  const session = process.env.HARLO_SESSION ?? "";
  const agent = process.env.HARLO_HELPER_BIN ?? "";
  let said = "";
  let box: Box = { on: false };

  if (command) {
    pi.registerMcpServer("harlo", {
      command: command[0],
      args: command.slice(1),
      env: { HARLO_SESSION: session },
      exposure: "direct",
      description: "harlo's sessions: read past sessions' messages, and ask the user to decide what only they may.",
    });
  }

  pi.on("session_start", async (_event, ctx) => {
    // (The bridge's socket: here or forwarded to the host pi runs on.)
    const socket = process.env.HARLO_MCP_SOCKET ?? "";
    const [text, sandbox] = await Promise.all([command && socket ? instructions(socket, session) : Promise.resolve(""), agent ? guard(agent, ["describe", "--cwd", ctx.cwd]) : Promise.resolve(undefined)]);
    said = text;
    if (sandbox?.on === true) box = { on: true, about: typeof sandbox.about === "string" ? sandbox.about : undefined };
  });

  pi.on("before_agent_start", async (event) => {
    if (said) event.systemPromptOptions.sections["harlo"] = said;
    if (box.about) event.systemPromptOptions.sections["harlo-sandbox"] = box.about;
  });

  pi.registerCommand("harlo-modes", {
    description: "harlo's approval mode (set by harlo)",
    handler: async (args) => {
      try {
        const set = JSON.parse(args);
        if (typeof set.approval === "string") modes.approval = set.approval;
      } catch {
        // (Not harlo's: left as it was.)
      }
    },
  });

  pi.on("tool_call", async (event, ctx) => {
    const name = event.toolName;
    const input = (event.input ?? {}) as Record<string, unknown>;
    // harlo's own tools never ask (the sessions they start ask for what
    // they do).
    if (name.startsWith("mcp__harlo__")) return;
    if (box.on && PATHS.has(name)) {
      const use = EDITS.has(name) ? "write" : name === "grep" || name === "find" ? "search" : "read";
      const checked = await guard(agent, [use, "--cwd", ctx.cwd, "--", pathOf(ctx.cwd, input.path)]);
      if (typeof checked?.error === "string") return { block: true, reason: checked.error };
    }
    if (READS.has(name)) return;
    const refused = await approve(name, input, ctx);
    if (refused) return refused;
    if (box.on && name === "bash" && typeof input.command === "string") {
      // (exec: the command keeps pi's process, so Stop still ends it.)
      input.command = `exec ${quoted(agent)} sandbox run --cwd ${quoted(ctx.cwd)} -- ${quoted(input.command)}`;
    }
  });

  // Reading a wiki page whose code changed: which footnotes to check.
  pi.on("tool_result", async (event, ctx) => {
    if (!agent || event.isError || event.toolName !== "read") return;
    const note = await wikiNote(agent, pathOf(ctx.cwd, event.input.path));
    if (note) return { content: [...event.content, { type: "text", text: "\n\n" + note }] };
  });

  /** Asks the user when the modes say so; a refusal, or none to go ahead. */
  async function approve(name: string, input: Record<string, unknown>, ctx: { ui: { select(title: string, options: string[]): Promise<string | undefined> }; cwd: string }) {
    // A command that can't be undone is asked about in every mode, once.
    if (agent && name === "bash" && typeof input.command === "string") {
      const why = (await guard(agent, ["danger", "--cwd", ctx.cwd, "--", input.command]))?.why;
      if (typeof why === "string") {
        const choice = await ctx.ui.select(`Allow tool: bash\n${input.command}\nReason: this can't be undone: ${why}`, ["Approve", "Deny"]);
        return choice === "Approve" ? undefined : { block: true, reason: `The user said no: ${why}. Don't try it another way; ask what they'd like instead.` };
      }
    }
    const mcp = name.startsWith("mcp__");
    const readOnly = mcp && pi.getAllTools().some((t) => t.name === name && (t as { annotations?: { readOnlyHint?: boolean } }).annotations?.readOnlyHint === true);
    if (readOnly) return;
    if (modes.approval === "yolo" || modes.approval === "auto") return;
    if (modes.approval === "write" && EDITS.has(name)) return;
    if (always.has(name)) return;
    const choice = await ctx.ui.select(`Allow tool: ${name}\n${detail(name, input)}`, ["Approve", "Deny", "Always allow"]);
    if (choice === "Approve") return;
    if (choice === "Always allow") {
      always.add(name);
      return;
    }
    return { block: true, reason: DENIED };
  }
}
