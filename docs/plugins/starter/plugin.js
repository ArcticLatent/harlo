// @ts-check
/// <reference path="./harlo.d.ts" />

// A tool: agents call it from a script, as tools.starter.count_words({path}).
// It reads the file through the session's own read tool, so the session's
// rules decide whether it may.
harlo.tool("count_words", async ({ path }) => {
  const text = await harlo.project.read({ path });
  return {
    lines: text === "" ? 0 : text.split("\n").length,
    words: text.split(/\s+/).filter(Boolean).length,
    characters: text.length,
  };
});

// A command: /standup in the composer. What it returns is sent as the prompt.
harlo.command("standup", async (text) => {
  const signature = await harlo.settings.get("signature");
  const about = text ? ` Focus on ${text}.` : "";
  const signed = signature ? ` Sign it "${signature}".` : "";
  return `Write a standup note from this session: what was done, what is next, what is in the way.${about}${signed}`;
});
