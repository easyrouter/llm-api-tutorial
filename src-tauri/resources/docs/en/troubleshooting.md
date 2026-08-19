# Troubleshooting A–G

When verification fails, the diagnosis panel names one of the faults below. This page is the full reference. After fixing, go back to "Verify" and run it again.

## A. 401 / 403 — authentication failed

The gateway rejected your key. Check, in order:

1. **Whitespace.** Copy-and-paste easily drags a space or line break along at the start or end. In CC Switch delete the key and paste it again (paste into a plain text editor first to spot stray whitespace).
2. **Wrong provider active.** CC Switch may hold several providers, and a newly added one has to be switched to explicitly. Check which entry is highlighted or ticked.
3. **The key itself.** Expired, revoked, not yet enabled, or copied from another service. Check its status where it was issued; request a new one if needed.

## B. 404 — wrong address

The gateway is reachable but the path does not exist. Address rules:

- a trailing `/` is removed automatically;
- a bare domain (`https://gateway.example.com`) gets `/v1` appended;
- an address already ending in `/v1` is used as-is;
- a trailing `#` means "use literally, append nothing" — only when the gateway requires it.

Safest fix: copy the address from the tool's Configure page **exactly**; its URL preview shows the final effective address.

## C. Configured, but nothing changed

Almost always a terminal that was not closed. Close **every** terminal window and editor-integrated terminal (ideally quit VS Code / Cursor entirely), open a fresh terminal and try again. On Windows the tool lists terminal processes still running.

## D. Environment variable conflict

If `OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_API_BASE`, `ANTHROPIC_API_KEY`, `ANTHROPIC_BASE_URL` or `ANTHROPIC_AUTH_TOKEN` are set on your system, the command-line tools **prefer them** over the CC Switch configuration.

**One-click clean-up**: on the Environment page click "Clean up" on the environment-variable row. The confirmation dialog lists where each variable comes from and its **full value** (deliberately unmasked, so you can see what is deleted and note it down if needed). After you confirm, the tool deletes user variables, deletes system variables as administrator (UAC), comments the line out of rc files as `# removed by SeedRouter Onboarding: …` (backup first) and runs `launchctl unsetenv` on macOS. A variable that exists only in the current process cannot be removed — restart the tool. Details in "One-click actions explained".

To do it by hand instead:

- **Windows** — right-click "This PC" → "Properties" → "Advanced system settings" → "Environment Variables". Look for those names under both "User variables" and "System variables", select and "Delete" (note the value first if you might need it). Then close all terminals and reopen.
- **macOS** — they usually live in `~/.zshrc` or `~/.zprofile` (bash: `~/.bash_profile`, `~/.bashrc`). Open the file, delete lines such as `export OPENAI_API_KEY=...` or comment them out with a leading `#`, save, close all terminals and reopen.

The Environment page shows where each variable comes from (registry, or file and line). Proxy variables (`HTTP_PROXY` etc.) are not conflicts and are never cleaned up.

## E. "Command not found" / "not recognized as an internal or external command"

1. Just installed, terminal not restarted → close all terminals and reopen (as in C).
2. Node.js missing or too old → back to the Install step.
3. npm's global bin folder not on PATH → click "Repair PATH": the tool appends that folder to your **user** PATH (Windows: `HKCU\Environment\Path` plus a change broadcast; macOS: an `export PATH=…` line appended to `~/.zshrc` or similar, backup first), no administrator rights; then close all terminals and reopen. By hand: run `npm prefix -g` in a new terminal and add the printed folder (Windows: the folder itself; macOS: its `bin` sub-folder) to PATH.
4. `'"node"' is not recognized` (Codex CLI found but broken) → the Node.js folder is not on PATH; use the same "Repair PATH" button.

## F. Protocol mismatch (400 / 422, oddly shaped responses)

Codex uses the Responses protocol; the service gateway supports and recommends it, so this should not normally occur. If you connect to a **different** service that only speaks Chat Completions, enable CC Switch's local proxy for protocol conversion, or use an address that supports Responses.

## G. Installer blocked by the operating system

- **Windows SmartScreen** ("Windows protected your PC"): "More info" → "Run anyway".
- **macOS Gatekeeper** ("developer cannot be verified" / "is damaged"): "System Settings → Privacy & Security", scroll down, "Open Anyway"; or run `xattr -d com.apple.quarantine /Applications/<App>.app` in Terminal.

Officially distributed installers are signed and normally do not trigger these; if you see them often, download only from the intranet distribution point.

## Still stuck

Click "Export diagnostic report" in the diagnosis panel and send the Markdown file to IT support. Keys and other sensitive values are redacted automatically.
