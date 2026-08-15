# Welcome to Codex Onboarding

This small tool has exactly one job: take you from "nothing installed" to "Codex CLI or Claude Code works through the company gateway" — without looking things up, guessing commands, or needing administrator rights.

## The six steps you will go through

1. **Environment check** — automatically inspects your OS version, Node.js, the command-line tools, CC Switch, environment variables and network access.
2. **Install** — installs only what is missing. Every command is shown to you first and runs only after you confirm.
3. **Configure** — you open CC Switch and add the company gateway as a provider. The tool shows every value you need (address, protocol, name) with a copy button; only the API key is typed by you.
4. **Verify** — the tool runs the command-line tool in a brand-new session and, optionally, sends one minimal request to the gateway, then tells you plainly whether it worked.
5. **Diagnose** — on failure it matches the problem against faults A–G and gives you the fix, instead of leaving you with an error code.
6. **Done** — you can come back and re-check at any time.

## What this tool will **never** do

- It never modifies anything inside `~/.codex/`, `~/.claude/` or `~/.cc-switch/`. Configuration is always done by you inside CC Switch; the tool guides and verifies.
- It never edits your environment variables. When it finds a conflict it tells you where it is and how to fix it yourself.
- It never stores, logs or uploads your API key. During the connectivity test the key stays in memory for a single request only.
- It never runs a command without your confirmation and never silently asks for elevated rights.

## What you need

- A Windows 10 (build 2004 or later) or macOS 12+ machine on the office network.
- Your API key. If you do not have one yet, request it through the usual company process — the tool cannot do that for you.
- About ten minutes.

## If something goes wrong

Open the help panel for the current step first; when verification fails the diagnosis panel opens automatically. If you are still stuck, click "Export diagnostic report" and send the file to IT support — keys and other sensitive values are redacted automatically.
