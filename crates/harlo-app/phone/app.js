// harlo on a phone: pairs by the QR code's link, then talks to harlo over an
// end-to-end encrypted WebSocket (NaCl box, tweetnacl), directly or through
// a harlo relay. See crates/harlo-core/src/phone.rs for the protocol.
"use strict";

const $ = id => document.getElementById(id);
const b64 = bytes => btoa(String.fromCharCode(...bytes));
const unb64 = s => Uint8Array.from(atob(s.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(s.length / 4) * 4, "=")), c => c.charCodeAt(0));
const enc = new TextEncoder(), dec = new TextDecoder();

// ---- stored state: this phone's key and the harlo it's paired with -------
const STORE = "harlo.phone.v1";
// harlo was called piqt: a phone paired then keeps its key and pairing.
function load() { try { return JSON.parse(localStorage.getItem(STORE) || localStorage.getItem("piqt.phone.v1")) || {}; } catch (e) { return {}; } }
function save(s) { try { localStorage.setItem(STORE, JSON.stringify(s)); } catch (e) { } }
let store = load();
if (!store.secret) {
  const kp = nacl.box.keyPair();
  store.secret = b64(kp.secretKey);
  save(store);
}
const mySecret = unb64(store.secret);
const myPublic = nacl.box.keyPair.fromSecretKey(mySecret).publicKey;

/// The QR code's offer: harlo's key, a one-time code, its name, its relay.
function readOffer() {
  const m = location.hash.match(/[#&]p=([A-Za-z0-9_-]+)/);
  if (!m) return null;
  try {
    const o = JSON.parse(dec.decode(unb64(m[1])));
    if (o.v !== 1 || !o.k) return null;
    return { key: o.k, code: o.c || "", name: o.n || "harlo", relay: o.r || "", relayId: o.i || "" };
  } catch (e) { return null; }
}
const offer = readOffer();
if (offer) {
  // The code is used once; the link shouldn't stay in history.
  history.replaceState(null, "", location.pathname);
  store.host = { key: offer.key, name: offer.name, relay: offer.relay, relayId: offer.relayId };
  save(store);
}
let pairingCode = offer ? offer.code : "";

function deviceName() {
  const ua = navigator.userAgent;
  if (/iPhone/.test(ua)) return "iPhone";
  if (/iPad/.test(ua)) return "iPad";
  if (/Android/.test(ua)) return (ua.match(/Android[^;)]*;\s*([^;)]+?)(?:\sBuild|\))/) || [, "Android phone"])[1].trim();
  return "Browser";
}

// ---- the encrypted connection -----------------------------------------
let ws = null, shared = null, conn = "", sent = 0, received = 0, nextId = 1;
const pending = new Map();
let retry = 1000, connected = false, everConnected = false;

function wsUrl() {
  const h = store.host;
  if (h && h.relay) return h.relay.replace(/\/$/, "") + "/ws?id=" + encodeURIComponent(h.relayId);
  return (location.protocol === "https:" ? "wss://" : "ws://") + location.host + "/ws";
}

function setDot(state) { $("dot").className = "dot " + state; }

function connect() {
  if (!store.host) { showUnpaired(); return; }
  setDot("wait");
  ws = new WebSocket(wsUrl());
  ws.onopen = () => {
    const hello = { type: "hello", key: b64(myPublic) };
    if (pairingCode) { hello.code = pairingCode; hello.name = deviceName(); }
    ws.send(JSON.stringify(hello));
  };
  ws.onmessage = ev => {
    if (!shared) {
      const m = JSON.parse(ev.data);
      if (m.type === "ready") {
        shared = nacl.box.before(unb64(store.host.key), mySecret);
        conn = m.conn; sent = 0; received = 0; retry = 1000; connected = true; everConnected = true;
        pairingCode = "";
        store.host.name = m.name || store.host.name; save(store);
        setDot("on");
        onConnected();
      } else if (m.type === "denied") {
        // Not paired (or removed): forget harlo unless a fresh code is on its way.
        if (!pairingCode) { delete store.host; save(store); }
        pairingCode = "";
        showMessage("Can't connect", m.reason || "harlo refused this phone.");
        ws.close();
      }
      return;
    }
    let m;
    try { m = open(ev.data); } catch (e) { console.warn(e); ws.close(); return; }
    if (m.id !== undefined && pending.has(m.id)) { const p = pending.get(m.id); pending.delete(m.id); p(m); return; }
    onEvent(m);
  };
  ws.onclose = ev => {
    const wasPaired = !!store.host;
    // The relay says harlo isn't there (harlo closed, or the computer asleep).
    if (ev.code === 4404 && wasPaired && !everConnected)
      showMessage("Waiting for harlo", "harlo on " + store.host.name + " isn't connected to the relay right now. This page connects when it is.");
    shared = null; connected = false;
    for (const p of pending.values()) p({ error: "disconnected" });
    pending.clear();
    setDot("off");
    if (wasPaired) { setTimeout(connect, retry); retry = Math.min(retry * 2, 15000); }
  };
}

function seal(msg) {
  sent += 1;
  const nonce = nacl.randomBytes(24);
  const box = nacl.box.after(enc.encode(JSON.stringify({ c: conn, s: sent, m: msg })), nonce, shared);
  const out = new Uint8Array(24 + box.length); out.set(nonce); out.set(box, 24);
  return b64(out);
}
function open(frame) {
  const raw = unb64(frame);
  const plain = nacl.box.open.after(raw.subarray(24), raw.subarray(0, 24), shared);
  if (!plain) throw new Error("a frame didn't decrypt");
  const v = JSON.parse(dec.decode(plain));
  if (v.c !== conn || !(v.s > received)) throw new Error("a frame out of order");
  received = v.s;
  return v.m;
}
function call(op, args) {
  return new Promise(resolve => {
    if (!connected) { resolve({ error: "not connected" }); return; }
    const id = nextId++;
    pending.set(id, resolve);
    ws.send(seal(Object.assign({ id, op }, args || {})));
  });
}

// ---- safe HTML (rows' markdown) -----------------------------------------
const ALLOWED = new Set(["P", "BR", "STRONG", "EM", "B", "I", "U", "S", "DEL", "CODE", "PRE", "SPAN", "A", "UL", "OL", "LI",
  "TABLE", "THEAD", "TBODY", "TR", "TH", "TD", "H1", "H2", "H3", "H4", "H5", "H6", "BLOCKQUOTE", "HR", "SUP", "SUB", "DIV", "FONT", "IMG"]);
function sanitize(html) {
  const doc = new DOMParser().parseFromString("<div>" + html + "</div>", "text/html");
  const walk = node => {
    for (const el of [...node.children]) {
      if (!ALLOWED.has(el.tagName)) { el.replaceWith(document.createTextNode(el.textContent)); continue; }
      for (const a of [...el.attributes]) {
        const n = a.name.toLowerCase(), v = a.value.trim();
        const okHref = n === "href" && /^(https?:|mailto:)/i.test(v);
        const okStyle = n === "style" && !/url\s*\(|expression|@import/i.test(v);
        const okOther = ["color", "align", "colspan", "rowspan"].includes(n);
        if (!okHref && !okStyle && !okOther) el.removeAttribute(a.name);
      }
      if (el.tagName === "IMG") { el.replaceWith(document.createTextNode("[image]")); continue; }
      if (el.tagName === "A") { el.setAttribute("target", "_blank"); el.setAttribute("rel", "noopener noreferrer"); }
      walk(el);
    }
  };
  const root = doc.body.firstChild;
  walk(root);
  return root.innerHTML;
}
const text = t => { const d = document.createElement("div"); d.textContent = t; return d; };

// ---- views -------------------------------------------------------------
let view = "list", sessions = [], current = null;
let rows = [], requests = [], status = "", earlier = 0;

function header(title, subtitle, back) {
  $("title").textContent = title;
  $("subtitle").textContent = subtitle || "";
  $("back").hidden = !back;
}
function showMessage(title, body, action) {
  view = "message";
  header("harlo", "");
  $("composer").hidden = true;
  const v = $("view"); v.replaceChildren();
  const c = document.createElement("div"); c.className = "center";
  const h = document.createElement("h2"); h.textContent = title; c.append(h, text(body));
  if (action) { const b = document.createElement("button"); b.className = "btn primary"; b.textContent = action[0]; b.onclick = action[1]; c.append(b); }
  v.append(c);
}
function showUnpaired() {
  showMessage("Pair this phone", "In harlo on your computer, open Settings → Devices → Pair a device, and scan the QR code with this phone's camera.");
}

function onConnected() {
  call("sessions").then(r => { if (r.sessions) { sessions = r.sessions; if (view !== "session") renderList(); } });
  if (view === "session" && current) openSession(current.key, true);
  else if (view !== "session") renderList();
}

function onEvent(m) {
  if (m.ev === "sessions") { sessions = m.sessions; if (view === "list") renderList(); else if (current) updateHeader(); return; }
  if (m.ev === "revoked") { delete store.host; save(store); ws.close(); showMessage("Removed", "This phone was removed in harlo. Pair it again to use it."); return; }
  if (!current || m.key !== current.key) return;
  if (m.ev === "reset") {
    rows = m.rows;
    earlier = m.earlier || 0;
    // (A ::before line, so the view's children stay one per row.)
    if (earlier) $("view").dataset.earlier = earlier + (earlier === 1 ? " earlier message is" : " earlier messages are") + " in harlo on your computer";
    else delete $("view").dataset.earlier;
    renderRows();
  }
  else if (m.ev === "rows") { applyChanges(m.changes); }
  else if (m.ev === "status") { status = m.status; updateComposer(); updateHeader(); }
  else if (m.ev === "meta") { current.model = m.model; if (m.title) current.title = m.title; updateHeader(); }
  else if (m.ev === "request") { requests.push(m.request); renderRequest(); }
  else if (m.ev === "cleared") { requests = requests.filter(r => r.id !== m.id); renderRequest(); }
}

function renderList() {
  view = "list"; current = null;
  header(store.host ? store.host.name : "harlo", connected ? sessions.length + (sessions.length === 1 ? " session" : " sessions") : "Connecting…");
  $("composer").hidden = true;
  const v = $("view"); v.replaceChildren();
  if (connected && sessions.length === 0) { v.append(Object.assign(text("No sessions yet."), { className: "center" })); return; }
  const groups = new Map();
  for (const s of sessions) { const g = s.project || "Other sessions"; if (!groups.has(g)) groups.set(g, []); groups.get(g).push(s); }
  for (const [g, list] of groups) {
    const h = document.createElement("div"); h.className = "group"; h.textContent = g; v.append(h);
    for (const s of list) {
      const b = document.createElement("button"); b.className = "session";
      const t = document.createElement("div"); t.className = "t";
      const n = document.createElement("div"); n.className = "name"; n.textContent = s.title || "New session";
      const meta = document.createElement("div"); meta.className = "meta";
      meta.textContent = [s.harness, s.branch ? "⎇ " + s.branch : "", s.where].filter(Boolean).join(" · ");
      t.append(n, meta); b.append(t);
      if (s.status === "busy" || s.status === "input") {
        const badge = document.createElement("span"); badge.className = "badge " + s.status;
        badge.textContent = s.status === "busy" ? "working" : "needs you"; b.append(badge);
      }
      b.onclick = () => openSession(s.key);
      v.append(b);
    }
  }
}

function updateHeader() {
  if (!current) return;
  const s = sessions.find(x => x.key === current.key);
  header((s && s.title) || current.title || "Session", [current.harness, current.model, status === "busy" ? "working…" : ""].filter(Boolean).join(" · "), true);
}

function openSession(key, again) {
  view = "session";
  if (!again) { rows = []; requests = []; status = ""; current = { key, title: "", harness: "", model: "" }; }
  $("view").replaceChildren(Object.assign(text("Loading…"), { className: "center" }));
  $("composer").hidden = false;
  updateHeader();
  call("open", { key }).then(r => {
    if (r.error) { showMessage("Can't open it", r.error, ["Back", renderList]); return; }
    current.key = r.key; current.harness = r.harness;
    updateHeader(); updateComposer();
  });
}

function rowEl(r) {
  const d = document.createElement("div"); d.className = "row " + r.kind;
  if (r.kind === "user") d.textContent = r.text;
  else if (r.kind === "markdown") d.innerHTML = sanitize(r.html);
  else if (r.kind === "code") { const pre = document.createElement("pre"); const c = document.createElement("code"); if (r.html) c.innerHTML = sanitize(r.html); else c.textContent = r.text; pre.append(c); d.append(pre); }
  else if (r.kind === "thinking") d.textContent = "💭 " + (r.text.length > 240 ? r.text.slice(0, 240) + "…" : r.text);
  else if (r.kind === "tool") {
    const det = document.createElement("details"); const sum = document.createElement("summary");
    const n = document.createElement("span"); n.className = "name"; n.textContent = r.title;
    const a = document.createElement("span"); a.className = "arg"; a.textContent = r.subtitle;
    const st = document.createElement("span"); st.className = "state " + r.state; st.textContent = r.state === "running" ? "…" : r.state === "error" ? "✕" : "✓";
    sum.append(n, a, st); det.append(sum);
    if (r.text) { const pre = document.createElement("pre"); pre.textContent = r.text.length > 6000 ? r.text.slice(0, 6000) + "\n…" : r.text; det.append(pre); }
    d.append(det);
  } else { d.classList.add(r.title); d.textContent = r.text; }
  return d;
}

function nearBottom() { const v = $("view"); return v.scrollHeight - v.scrollTop - v.clientHeight < 120; }
function toBottom() { const v = $("view"); v.scrollTop = v.scrollHeight; }

function renderRows() {
  const v = $("view"); v.replaceChildren(...rows.map(rowEl));
  toBottom();
}
function applyChanges(changes) {
  const v = $("view"), stick = nearBottom();
  if (v.firstChild && v.firstChild.className === "center") v.replaceChildren();
  for (const c of changes) {
    if (c.op === "update") { rows[c.at] = c.row; const old = v.children[c.at]; if (old) old.replaceWith(rowEl(c.row)); }
    else if (c.op === "remove") { rows.splice(c.at, c.count); for (let i = 0; i < c.count; i++) { const el = v.children[c.at]; if (el) el.remove(); } }
    else if (c.op === "insert") {
      rows.splice(c.at, 0, ...c.rows);
      const ref = v.children[c.at] || null;
      for (const r of c.rows) v.insertBefore(rowEl(r), ref);
    }
  }
  if (stick) toBottom();
}

function updateComposer() {
  const busy = status === "busy";
  $("stop").hidden = !busy;
  $("input").placeholder = busy ? "Queue a follow-up…" : "Message…";
}

// ---- questions and approvals ------------------------------------------
function answer(req, value) {
  call("answer", { key: current.key, requestId: req.id, answer: value });
  requests = requests.filter(r => r.id !== req.id);
  renderRequest();
}
function renderRequest() {
  const box = $("request"), req = requests[0];
  box.hidden = !req;
  box.replaceChildren();
  if (!req) return;
  const q = document.createElement("div"); q.className = "q"; q.textContent = req.approval ? "Approve this?" : (req.title || "The agent asks");
  box.append(q);
  if (req.message) { const m = document.createElement("div"); m.className = "msg"; m.textContent = req.message; box.append(m); }
  const actions = document.createElement("div"); actions.className = "actions";
  const button = (label, cls, fn) => { const b = document.createElement("button"); b.className = "btn " + (cls || ""); b.textContent = label; b.onclick = fn; return b; };
  if (req.approval || req.method === "confirm") {
    if (req.approval && req.options && req.options.length) {
      // Approve / Deny (and any others) as buttons, the safe one first.
      for (const o of req.options) {
        const cls = /approve|allow|yes/i.test(o) ? "primary" : /deny|reject|no/i.test(o) ? "danger" : "";
        actions.append(button(o, cls, () => answer(req, { value: o })));
      }
    } else {
      actions.append(button("Deny", "danger", () => answer(req, { confirmed: false })), button("Allow", "primary", () => answer(req, { confirmed: true })));
    }
  } else if (req.method === "select") {
    const opts = document.createElement("div"); opts.className = "opts";
    for (const o of req.options) opts.append(button(o, "opt", () => answer(req, { value: o })));
    box.append(opts);
  } else if (req.method === "input" || req.method === "editor") {
    const f = document.createElement("input"); f.type = "text"; f.value = req.prefill || ""; f.placeholder = req.placeholder || "";
    box.append(f);
    actions.append(button("Send", "primary", () => answer(req, { value: f.value })));
  } else if (req.method === "ask" && Array.isArray(req.questions)) {
    const picked = req.questions.map(() => new Set());
    const customs = [];
    req.questions.forEach((qq, i) => {
      const t = document.createElement("div"); t.className = "msg"; t.textContent = qq.question || qq.header || ""; box.append(t);
      const opts = document.createElement("div"); opts.className = "opts";
      for (const o of (qq.options || [])) {
        const label = typeof o === "string" ? o : o.label;
        const b = button(label, "opt", () => {
          if (!qq.multi) { picked[i].clear(); for (const s of opts.children) s.classList.remove("on"); }
          if (picked[i].has(label)) { picked[i].delete(label); b.classList.remove("on"); } else { picked[i].add(label); b.classList.add("on"); }
        });
        opts.append(b);
      }
      box.append(opts);
      const f = document.createElement("input"); f.type = "text"; f.placeholder = "Or type your answer"; customs.push(f); box.append(f);
    });
    actions.append(button("Answer", "primary", () => answer(req, { answers: req.questions.map((qq, i) => {
      const a = { id: qq.id, selectedOptions: [...picked[i]] };
      if (customs[i].value.trim()) a.customInput = customs[i].value.trim();
      return a;
    }) })));
  } else {
    actions.append(button("Dismiss", "", () => answer(req, { cancelled: true })));
  }
  if (actions.children.length) box.append(actions);
}

// ---- composer -----------------------------------------------------------
function send() {
  const t = $("input").value.trim();
  if (!t || !current) return;
  call("send", { key: current.key, text: t, mode: status === "busy" ? "followUp" : "" });
  $("input").value = "";
  grow();
}
function grow() { const i = $("input"); i.style.height = "auto"; i.style.height = Math.min(i.scrollHeight, window.innerHeight * 0.4) + "px"; }
$("send").onclick = send;
$("stop").onclick = () => current && call("abort", { key: current.key });
$("input").addEventListener("input", grow);
$("input").addEventListener("keydown", e => { if (e.key === "Enter" && !e.shiftKey && !/Android|iPhone|iPad/.test(navigator.userAgent)) { e.preventDefault(); send(); } });
$("back").onclick = () => { renderList(); call("sessions").then(r => { if (r.sessions) { sessions = r.sessions; renderList(); } }); };

// Test hook (harlo's self-tests drive the page).
window.harloPhone = { state: () => ({ view, connected, sessions, rows, earlier, requests, status, current, paired: !!store.host }), call, openSession, send: t => { $("input").value = t; send(); } };

// Something every 30 s, so relays and proxies don't close a quiet connection.
setInterval(() => { if (connected) call("ping"); }, 30000);
if (store.host) { showMessage("Connecting…", ""); connect(); } else showUnpaired();
