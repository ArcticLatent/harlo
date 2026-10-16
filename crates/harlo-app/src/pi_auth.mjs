// harlo's sign-in helper for pi: lists pi's providers and signs one in or
// out with pi's own login (its ModelRuntime, as pi's /login does), speaking
// JSON lines: this → harlo on stdout (events, prompts, the outcome), harlo →
// this on stdin (answers: { id, value } or { id, cancel: true }).
//
//   node pi-auth.mjs <pi's package folder> list
//   node pi-auth.mjs <pi's package folder> login <provider> <oauth|api_key>
//   node pi-auth.mjs <pi's package folder> logout <provider>
//
// pi keeps the credential (its auth.json); harlo never reads it.
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const [pkg, cmd, provider, type] = process.argv.slice(2);
const out = (v) => process.stdout.write(JSON.stringify(v) + "\n");
const finish = (code) => process.stdout.write("", () => process.exit(code));

try {
  const pi = await import(pathToFileURL(`${pkg}/dist/index.js`).href);
  const runtime = await pi.ModelRuntime.create();
  if (cmd === "list") {
    const credentials = await runtime.listCredentials({ signal: AbortSignal.timeout(15_000) });
    const providers = runtime.getProviders().map((p) => {
      const status = runtime.getProviderAuthStatus(p.id) ?? {};
      return {
        id: p.id,
        name: p.name,
        oauth: p.auth.oauth ? { label: p.auth.oauth.loginLabel ?? p.auth.oauth.name, subscription: p.auth.oauth.isSubscription === true } : null,
        apiKey: p.auth.apiKey?.login ? { label: p.auth.apiKey.name } : null,
        configured: status.configured === true,
        source: status.label ?? status.source ?? "",
      };
    });
    out({ providers, credentials: credentials.map((c) => ({ id: c.providerId, type: c.type })) });
    finish(0);
  } else if (cmd === "login") {
    const abort = new AbortController();
    const waiting = new Map();
    let next = 1;
    const input = createInterface({ input: process.stdin });
    input.on("line", (line) => {
      let a;
      try {
        a = JSON.parse(line);
      } catch {
        return;
      }
      const w = waiting.get(a.id);
      if (!w) return;
      waiting.delete(a.id);
      if (a.cancel) w.reject(new Error("Cancelled"));
      else w.resolve(String(a.value ?? ""));
    });
    // harlo closed the dialog.
    input.on("close", () => {
      for (const w of waiting.values()) w.reject(new Error("Cancelled"));
      abort.abort();
    });
    const settings = pi.SettingsManager.create(process.cwd());
    await runtime.login(
      provider,
      type,
      {
        signal: abort.signal,
        prompt: (p) =>
          new Promise((resolve, reject) => {
            const id = next++;
            waiting.set(id, { resolve, reject });
            // Answered another way (the browser's callback won): the box goes.
            p.signal?.addEventListener("abort", () => {
              if (waiting.delete(id)) {
                out({ dismiss: id });
                reject(new Error("Answered"));
              }
            });
            out({ prompt: { id, type: p.type, message: p.message, placeholder: p.placeholder ?? "", options: p.options ?? [] } });
          }),
        notify: (event) => out({ event }),
      },
      { getDeviceId: () => settings.getOrCreateDeviceId() },
    );
    out({ done: true });
    finish(0);
  } else if (cmd === "logout") {
    await runtime.logout(provider);
    out({ done: true });
    finish(0);
  } else {
    out({ error: `unknown command ${cmd}` });
    finish(2);
  }
} catch (e) {
  out({ error: String(e?.message ?? e) });
  finish(1);
}
