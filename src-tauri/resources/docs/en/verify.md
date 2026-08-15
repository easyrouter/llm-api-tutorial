# Apply & verify

## First: close every terminal window

This is the **single most common** source of trouble in the whole guide. Installing and configuring changes the PATH variable and configuration files, but every terminal window copies the environment at the moment it opens and keeps using that copy. As a result:

- a terminal that was already open does not know that `codex` / `claude` was just installed → "command not found";
- a terminal that was already open may still carry old environment variables → the new configuration "has no effect".

Close **all** of them: Windows Terminal, PowerShell, CMD, Git Bash, Terminal.app and iTerm on macOS, and the **built-in terminals inside editors such as VS Code or Cursor** (the safest option is to quit the editor entirely). On Windows the tool lists terminal processes that are still running.

## What "Verify" does

When you click "Start verification" the tool:

1. Runs `codex --version` or `claude --version` in a **fresh session** — exactly as if you had opened a new terminal — to make sure the command is found and runs.
2. Optionally sends one minimal test request to the service gateway to confirm that address, protocol and key are all correct. For this you enter your API key temporarily; it stays in memory for that single request only and is never stored, logged or uploaded.
3. Tells you plainly **success** or **failure**. On failure the diagnosis panel opens automatically and names the fault (A–G) together with the steps to fix it.

## Testing by hand

If you want to see it with your own eyes, open a **new** terminal window:

```
codex --version
codex "introduce yourself in one sentence"
```

or

```
claude --version
claude "introduce yourself in one sentence"
```

A version number followed by an actual reply means everything is wired up.

## Verification failed?

You do not need to match error codes yourself — read the conclusion in the diagnosis panel. The usual mapping:

- 401 / 403 → key problem (fault A)
- 404 → address problem (fault B)
- command not found → PATH not refreshed or not installed (fault E)
- configured but no effect → a terminal is still open (fault C) or an environment variable overrides the config (fault D)

After fixing, come back here and click "Start verification" again.
