# Welcome to SeedRouter Onboarding

This small tool has exactly one job: take you from "nothing installed" to "Codex CLI or Claude Code works through the service gateway" — without looking things up or guessing commands; only a few steps need administrator rights, and those buttons say so.

## The six steps you will go through

1. **Environment check** — automatically inspects your OS version, Node.js, the command-line tools, CC Switch, the Codex client, environment variables and network access. Whatever is missing or wrong has a one-click button (install Node.js, repair PATH, clean up environment variables, install the Codex client).
2. **Install** — installs only what is missing. Every command is shown to you first and runs only after you confirm.
3. **Configure** — you open CC Switch and add the service gateway as a provider (Codex users with a ChatGPT account simply use the "OpenAI Official" preset and sign in). The tool shows every value you need (address, protocol, name) with a copy button; only the API key is typed by you; the Codex `config.toml` template is applied with one click (backup first).
4. **Verify** — the tool runs the command-line tool in a brand-new session and, optionally, sends one minimal request to the gateway, then tells you plainly whether it worked.
5. **Diagnose** — on failure it matches the problem against faults A–G and gives you the fix, instead of leaving you with an error code.
6. **Done** — you can come back and re-check at any time.

## What this tool will **never** do

- It never modifies anything inside `~/.claude/` or `~/.cc-switch/`; inside `~/.codex/` it writes exactly one file, `config.toml`, and only after you click "Apply", with a backup. Provider configuration is always done by you inside CC Switch; the tool guides and verifies.
- It never edits your environment variables or PATH silently. "Clean up" and "Repair PATH" show the full change first, run only after you confirm, and back up every file they edit; the machine-wide (all users) PATH is never touched.
- It never stores, logs or uploads your API key. During the connectivity test the key stays in memory for a single request only.
- It never runs a command without your confirmation and never silently asks for elevated rights — buttons that need administrator rights say so, and the prompt comes from the system / installer. See "One-click actions explained".

## What you need

- A Windows 10 (build 2004 or later) or macOS 12+ machine on the office network.
- Your API key. If you do not have one yet, request it through the usual company process — the tool cannot do that for you.
- About ten minutes.

## If something goes wrong

Open the help panel for the current step first; when verification fails the diagnosis panel opens automatically. If you are still stuck, click "Export diagnostic report" and send the file to IT support — keys and other sensitive values are redacted automatically.
