# Install Codex CLI / Claude Code

Both tools are installed globally through npm with a single command:

```
npm install -g @openai/codex
npm install -g @anthropic-ai/claude-code
```

Install only the one you will use — or both.

## What the tool does

1. Shows you the exact command it is about to run (including which npm registry it will use). Nothing runs until you click "Confirm".
2. If the official registry is slow or unreachable it switches to a mirror (npmmirror) and puts `--registry` explicitly into the command.
3. Streams the installer output live into the window; you can cancel at any time.
4. Re-checks the version afterwards.

None of this needs administrator rights: npm's global folder lives in your user profile by default. If the global folder turns out to be unwritable, the tool proposes switching npm to a user-level prefix instead of asking for `sudo` or "Run as administrator".

## What success looks like

After the install the tool runs `codex --version` or `claude --version` in a fresh session. A version number means success.

If the tool reports "installed, but not on the current PATH", nothing went wrong — terminals that were already open simply have not loaded the new PATH yet. Close **all** terminal windows (including the built-in terminal of your editor) and open a new one. See "Apply & verify" and fault E.

If the re-check **still** cannot find the command after that, npm's global folder really is not on your PATH. Click "Repair PATH": the tool appends the npm global folder to your user PATH (Windows: `HKCU\Environment\Path` plus a change broadcast; macOS: one `export PATH=…` line appended to `~/.zshrc` or similar, backup first) — no administrator rights. The same button handles "Codex CLI found but broken: `'"node"' is not recognized`", which means the Node.js folder is not on PATH. Details: "One-click actions explained".

## Common questions

**The install is very slow or seems stuck**
Click "Cancel", then "Install" again. The tool re-probes the network and may switch to the mirror. You can also look at the network row on the Environment page.

**EACCES / permission denied**
The npm global folder is inside a system directory. The tool offers a command that moves the npm prefix into your user folder; confirm it, then run the install again.

**My company laptop does not allow installing software**
A global npm install only copies files into your user profile, which is normally outside software-installation policies. If security software blocks `node.exe`, ask IT to whitelist it.

**Upgrading later**
Run the same install command again; npm replaces the previous version.
