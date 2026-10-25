# Changelog

What's new in each harlo release, newest first. `packaging/release.sh`
releases the version at the top: each section starts with `## <version>`,
and its text becomes the release's notes.

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
