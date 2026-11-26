// What a Harlo plugin's plugin.js can use. Reference it from plugin.js:
//   // @ts-check
//   /// <reference path="./harlo.d.ts" />

/** A value that travels as JSON. */
type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

/** What every function of a plugin is given beside its own argument. */
interface HarloContext {
  /** The plugin: its name and the version its plugin.json gives. */
  plugin: { name: string; version: string | null };
  /** The session the call is for, with its folder. */
  session: { id: string; directory: string } | null;
}

/** What a turn ended as, given to `harlo.on("turn.ended", …)`. */
interface HarloTurnEnded {
  /** How the turn ended, such as "succeeded" or "failed". */
  outcome: string;
  /** Why it failed, when it did. */
  error: string | null;
  session: { id: string; directory: string; title: string | null; parentID: string | null };
}

/** An answer to `harlo.fetch`. */
interface HarloResponse {
  status: number;
  /** Whether the status is 200 to 299. */
  ok: boolean;
  /** The address that answered, after redirects. */
  url: string;
  headers: Record<string, string>;
  /** The body as text, cut at 2 MiB. */
  body: string;
  /** Whether the body was cut. */
  truncated: boolean;
  text(): string;
  json(): any;
}

interface Harlo {
  /** Registers the code of a tool that plugin.json names under `tools`. */
  tool(name: string, run: (args: any, context: HarloContext) => Json | Promise<Json>): void;
  /** Registers the code of a command that plugin.json names under `commands`. It returns the prompt to send. */
  command(name: string, run: (text: string, context: HarloContext) => string | Promise<string>): void;
  /** Registers code for something plugin.json names under `events`. */
  on(event: "turn.ended", run: (event: HarloTurnEnded, context: HarloContext) => void | Promise<void>): void;

  /** Asks a host that `permissions.network` names. `https` only, or `http` to this computer. */
  fetch(
    url: string,
    init?: {
      method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD";
      headers?: Record<string, string>;
      body?: string;
    },
  ): Promise<HarloResponse>;

  /** What a person filled in for the settings plugin.json names. */
  settings: { get(key: string): Promise<string | null> };

  /** 256 KiB of the plugin's own, kept on the server between calls. */
  storage: {
    get(key: string): Promise<Json>;
    set(key: string, value: Json): Promise<null>;
    delete(key: string): Promise<null>;
  };

  /**
   * The session's own tools, for those `permissions.project` names. Only a
   * tool an agent's script called has them. The session's rules decide each
   * call, and a person is asked where they say to.
   */
  project: {
    read(args: { path: string; offset?: number; limit?: number }): Promise<string>;
    grep(args: { pattern: string; path?: string; include?: string; filesOnly?: boolean }): Promise<string>;
    glob(args: { pattern: string; path?: string }): Promise<string[]>;
    edit(args: { path: string; oldString: string; newString: string; replaceAll?: boolean }): Promise<string>;
    write(args: { path: string; content: string }): Promise<string>;
    shell(args: { command: string; timeout?: number; background?: boolean }): Promise<string>;
  };

  plugin: HarloContext["plugin"];
  session: HarloContext["session"];
}

declare const harlo: Harlo;
