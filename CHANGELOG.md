# Changelog

What's new in each Harlo release, newest first. `packaging/release.sh`
releases the version at the top: each section starts with `## <version>`,
and its text becomes the release's notes.

## 2.0.0

Harlo is rebuilt from the ground up: a native Rust app drawn with Slint, on
Harlo's own agent server. Your old sessions stay on disk. If you used
OpenCode, its setup is copied to `~/.config/harlo` on the first start, and
Settings → Server imports its sessions.

- Harlo's server runs your agents in the background, so sessions, schedules
  and phone access keep going while the window is closed. It talks to
  Anthropic, OpenAI (an API key, or your ChatGPT plan), OpenRouter,
  OpenCode Go, Vercel's AI Gateway, Cohere and any OpenAI-compatible
  server, local ones included.
- Settings → Providers puts the popular providers up front, checks keys as
  you add them, and signs in to OpenRouter and ChatGPT in the browser.
- Settings → Local models installs Ollama or llama.cpp, runs their servers
  and gets models that fit your computer.
- The chat: your prompts, each agent's turn, real lists and tables, code
  coloured by language, pictures with a viewer, and jumps between earlier
  prompts. Long conversations are summarized as they grow.
- Agents read and edit files, run commands and background jobs, search the
  web, use MCP servers (OAuth sign-ins included), skills and commands, run
  subagents, keep a todo list, and carry notes from one session to the next.
- Approvals with permission modes, saved approvals, and questions the agent
  asks; undo a turn, one file at a time; fork, move, archive, export and
  import sessions; side questions with /btw, goals with /goal, and shell
  commands with !.
- The Changes panel shows only the lines that changed, takes comments on
  diff lines, and commits (✦ writes the message); Settings → Git signs in
  to GitHub, GitLab and Forgejo.
- A terminal, an editor with Ctrl+S, notes, schedules that send prompts or
  run commands, dictation, desktop notifications with a sound each, a tray
  icon, start at login, and a setup guide on the first start.
- Themes are Base16 colour schemes, light and dark, with contrast, fonts,
  text sizes, chat width and motion in Settings → Appearance.
- The iPhone app follows Harlo's server: it takes the desktop's theme and
  avatar, shows changes, diffs, files and branches, undoes turns, edits and
  resends messages, manages the queue, approval modes and pins, and follows
  a run in a Live Activity.
- Harlo follows a Harlo server on another computer over SSH.
- Harlo updates itself from a package or an AppImage, as before.

## 1.1.1

- For the iPhone app's next version: its plan limits (on this computer and
  each of your machines), Stop and continue at a used-up limit, and
  notifications when a limit is used up and when it resets. An older app
  gets nothing it doesn't know.
- A message sent from the iPhone starts a stopped model server first, as one
  sent here does; if the server doesn't start, the phone says why.
- A session started from the iPhone uses the project's own model (Settings →
  Projects) when you don't pick one.

## 1.1.0

- Run models yourself: Settings → Model servers sets up llama.cpp, Ollama,
  vLLM, Strata, Basalt or NInfer for your graphics card, downloads a model
  that fits (checked against its checksum), starts it and adds it to pi, Oh
  My Pi, OpenCode and Hermes. A stopped server starts before your message,
  and harlo updates the engines.
- On your other machines too: set up a model server on a machine you reach
  over SSH (a GPU box, a NAS), with its own engines, agents and updates. Its
  agents can use this computer's servers, and this computer's agents can
  use its, through the SSH connection. Works on x86-64 and ARM machines.
- Specs and a shared plan, on per project: every agent can find, read and
  write the project's SPEC.md files, and work through a plan whose steps
  are committed one by one and reviewed by another agent.
- Extensions for pi (Settings → Extensions): permissions, questions, a to-do
  list, subagents and web access, off until you turn them on, shown in
  harlo's own approval cards and docks.
- Plan limits as they change, a line above the composer from 90%,
  notifications when a limit is used up and when it resets (with harlo's own
  sounds), and Stop and continue at the reset. Each machine has its own
  meter, and Settings → Plan limits lists every machine.
- The context bar marks where each agent compacts, and says how many tokens
  are left near the end.
- Mermaid diagrams are drawn in the chat.
- Sessions on other machines work like local ones: sign in to any agent
  there from harlo, pick that machine's models, use pi's extensions, stats,
  specs and plans, worktrees, attachments, hand off and committees, and
  sessions outside a project. A remote session opens from a local copy that
  only fetches what's new. Quitting harlo closes its connections, and a
  machine stays disconnected until you connect it.
- Add a project with its folder, agent and model in one window; remove a
  project with or without its sessions. Finished worktree sessions go on a
  Settled shelf.
- A session keeps its permission mode and thinking level. Messages sent
  while pi, OpenCode or Hermes works are queued with Remove and Send now.
  Paste images or files to attach them. Long sessions open at once and
  scroll smoothly.
- Hermes signs in from harlo (a link and code, or an API key). The terminal
  opens links and copies with Ctrl+Shift+C; Copy buttons work again in the
  sign-in window. Mini chat stays on top on KDE Wayland.

## 1.0.3

- Hermes Agent: start Hermes sessions like the other agents, with
  approvals, models, edit and resend, search, remote hosts, its MCP
  servers in Settings → MCP, and update notices. harlo talks to it over
  the Agent Client Protocol (`hermes acp`).
- The phone button in the left rail pauses and resumes notifications to
  your phone once it's paired (right-click to pair another). The pairing
  stays.
- Closing the window keeps harlo running in the tray (Settings → General);
  Ctrl+Q quits.
- Settings → About Harlo: your version, how harlo was installed, and links
  to the changelog, website and source.
- The thinking menu says so when a model has no levels, instead of opening
  empty.

## 1.0.2

- Model lists put the models you run yourself (on this computer or your
  local network) last, under Local.
- OpenCode sessions can be deleted again: right-click → Delete…. Moving
  them to the trash failed, because OpenCode keeps sessions in its database.
- The plan meter follows OpenCode sessions too, and a provider's logo in it
  is the same size as Claude's.
- Settings → General → Check for updates has Check now, and harlo checks
  every 6 hours instead of 12.
- The update notice says what's new since your version, with Read more for
  the full changelog.

## 1.0.1

- OpenCode: denying a permission ends the run. Before, the session stayed
  on "Working…" on the desktop and the iPhone.
- Archiving a session you have open (on the desktop or from the iPhone)
  now takes it out of the list. A session still working or waiting on you
  stays until it's done.

## 1.0.0

The first release of harlo: a native Linux app for coding agents, written in
Rust and Qt.

### Agents

- Claude Code, OpenCode, pi and Oh My Pi in one chat, with your existing
  sessions in the sidebar and each agent's own login and settings.
- Approvals and questions, plan mode, the permissions shield, model choice,
  attachments, queued and steering messages.
- Oh My Pi's goals, todos and subagents.
- Sessions on your other machines over SSH, with their files, terminal and
  changes.

### Changes and git

- Every file an agent touched, side-by-side diffs, review comments sent back
  to the agent, and undo for any turn, across a folder of repositories too.
- Commit, push, branches and stashes; worktrees for sessions side by side,
  and one task run by several agents in parallel.
- Pull requests and CI checks on GitHub, GitLab and Forgejo (your own server
  too); failed checks go back to the agent.

### And

- An iPhone app: follow sessions, answer from notifications, Live Activities,
  start sessions on any of your machines, end-to-end encrypted.
- Dictation recognised on your computer, a terminal, files and an editor,
  templates, scheduled prompts, MCP servers, spending limits, an action log,
  quiet hours, export as Markdown, and 31 themes.
- Packages for Ubuntu and Debian, Fedora and openSUSE, Arch, and an AppImage;
  harlo updates itself, checking each update's signature.
