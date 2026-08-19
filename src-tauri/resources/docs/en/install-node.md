# Install Node.js

Codex CLI and Claude Code are command-line tools published through npm, and npm ships with Node.js. So Node.js is the first building block: you need version **18 or newer**, and the current LTS (long-term support) release is recommended.

## How the tool helps

1. It measures which download source is faster from your network — the official site or a regional mirror — and picks one.
2. **Install now (needs administrator rights)**: downloads the newest LTS installer from that source (Windows `.msi` / macOS `.pkg`), verifies its SHA-256 against the `SHASUMS256.txt` of the same source, then runs the official installer — `msiexec /i … /passive /norestart` on Windows (UAC prompt), `installer -pkg` on macOS (password prompt). The installer is always the official Node.js one and the elevation prompt comes from it; the tool never elevates silently. Full details: "One-click actions explained".
3. Prefer to do it yourself? "Open download page" opens the download site; install manually, then click "Re-check".

## Windows

1. Download the **Windows Installer (.msi)** for your CPU (x64 or ARM64).
2. Double-click it and accept the defaults. "Add to PATH" is ticked by default — keep it ticked.
3. If the installer asks for administrator rights you do not have (the one-click install needs them too), use the **.zip (portable)** build instead: extract it into a folder you own (for example `C:\Users\<you>\nodejs`), then add that folder under **System Properties → Environment Variables → User variables → Path** (or click "Repair PATH" on the Node.js row of the Environment page).
4. **Close every open terminal window**, then click "Re-check" in the tool.

## macOS

1. Download the **macOS Installer (.pkg)** — ARM64 for Apple silicon, x64 for Intel (check via Apple menu → About This Mac if unsure).
2. Double-click and follow the prompts.
3. If you already use Homebrew, `brew install node` in a terminal works too.
4. Close all terminal windows, then click "Re-check".

## Node.js is installed but too old?

Just install the newer version; the installer replaces the old one. If you use a version manager such as nvm, fnm or volta, switch to a version ≥ 18 with it and make sure the _default_ version is switched as well (for example `nvm alias default 22`).

## How to confirm

Open a **new** terminal window and type:

```
node -v
npm -v
```

Both commands should print a version number (`v18` or higher).
