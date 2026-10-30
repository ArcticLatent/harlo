# Changelog

What's new in each harlo release, newest first. `packaging/release.sh`
releases the version at the top: each section starts with `## <version>`,
and its text becomes the release's notes.

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
