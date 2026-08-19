# One-click actions explained

The one-click buttons on the "Environment check" and "Configure" pages **change things on your machine** — they install software, edit PATH, delete environment variables or write one configuration file. Every button works the same way:

1. It first builds a "plan" and shows you **exactly** the command that will run or the content that will change.
2. Nothing happens until you click "Confirm". Right before running, the tool rebuilds the plan and compares it with the one you saw; if they differ it stops.
3. Any step that needs administrator rights says so on the button — "(needs administrator rights)" — and the prompt you see (Windows UAC dialog / macOS password box) comes from the operating system or the installer itself, never from this tool elevating silently.
4. Every action that edits one of your files keeps a backup first, named `<file>.seedrouter-<timestamp>.bak`.

Below, one section per button: what it does, how, and how to undo it.

## Install Node.js with one click

**What it does**
Downloads the current Node.js LTS installer from the official site or a regional mirror, verifies it, and runs the official installer.

**Step by step**

1. Network probe: the tool probes `nodejs.org` and the npmmirror Node mirror at the same time and picks the reachable one with the lowest latency (the first entry if neither answers). The button and the plan name the source that was chosen.
2. It downloads `index.json` (Node's release list) from that source and selects the newest LTS release.
3. It picks the installer for your machine: `node-v<version>-x64.msi` or `node-v<version>-arm64.msi` on Windows, `node-v<version>.pkg` on macOS. The file is saved in the tool's cache directory, `downloads` folder.
4. It downloads `SHASUMS256.txt` from the **same** source and checks the installer's SHA-256 against it; on a mismatch the installer is not run.
5. It runs the installer:
   - **Windows**: `msiexec /i <file> /passive /norestart`. Windows Installer shows the UAC prompt — this MSI is a per-machine install and adds Node.js to the system PATH. You see a progress bar but do not have to click "Next". Exit code 3010 means "installed, reboot pending".
   - **macOS**: `osascript -e 'do shell script "installer -pkg <file> -target /" with administrator privileges'`. macOS asks for your login password.
6. Afterwards the tool re-checks Node.js automatically.

**Rights needed**: administrator (UAC / macOS password). The prompt is triggered by the Node.js installer.

**Undo**: Windows — uninstall Node.js under "Settings → Apps → Installed apps"; macOS — remove `/usr/local/bin/node`, `/usr/local/lib/node_modules` etc. as described in the Node.js docs (`sudo rm`). The downloaded installer in the cache folder can be deleted at any time.

**If it fails**: checksum mismatch → click again, the tool downloads a fresh copy; UAC / password cancelled → nothing was installed, try again later; "reboot pending" → reboot, then "Re-check"; company policy forbids installing → follow the portable-build route on the "Install Node.js" page.

## Repair PATH with one click

**What it does**
Adds one directory (the Node.js folder, or npm's global bin folder) to your **user-level** PATH so that new terminals can find `node`, `codex` and `claude`.

**The problems it fixes**

- "Codex CLI found but broken: `'"node"' is not recognized`" — the launcher script cannot find `node` because the Node folder is not on PATH.
- "Install succeeded but the re-check still cannot find the command" — npm put the command into its global folder, which is not on PATH.

**Step by step**

- **Windows**: the directory is appended to `HKCU\Environment\Path` (your user PATH; the registry type stays `REG_EXPAND_SZ`, existing entries are untouched), then a `WM_SETTINGCHANGE` broadcast is sent so that newly opened terminals pick the change up immediately. The plan shows the equivalent `reg add … /v Path …` command. The machine-wide (all users) PATH is **never** edited.
- **macOS**: one line is appended to your login shell's rc file — `export PATH="<dir>:$PATH"  # added by SeedRouter Onboarding` (zsh → `~/.zshrc`, bash → `~/.bash_profile`, fish → `~/.config/fish/config.fish` using `fish_add_path`). The file is copied to `<file>.seedrouter-<timestamp>.bak` first.
- If the directory is already on PATH the plan says "already present" and nothing is changed.

**Rights needed**: none.

**Undo**: Windows — remove the entry under "System Properties → Environment Variables → User variables → Path"; macOS — delete the line marked `# added by SeedRouter Onboarding`, or copy the backup back.

**If it fails**: the command is still not found → close **all** terminal windows and open a new one (see "Apply & verify"); the directory does not exist → the tool refuses the plan; finish the install first.

## Clean up environment variables with one click

**What it does**
Deletes the environment variables that override the CC Switch configuration (`OPENAI_API_KEY`, `OPENAI_BASE_URL`, `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN` and similar — see fault D). Proxy variables (`HTTP_PROXY` etc.) are never touched.

**Why does the preview show the full values?**
Because this is a delete. The confirmation dialog shows **on purpose** the complete value of every variable (addresses and tokens included) so you can see exactly what is removed and write it down if you need it. The values appear in that dialog only — they are never written to logs, diagnostic reports or telemetry.

**Step by step**
The tool first finds where each variable comes from and then handles every source separately:

| Source                                                    | Action                                                                                                                                                      | Rights              |
| --------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------- |
| Windows user variable (`HKCU\Environment`)                | delete the value (equivalent to `reg delete "HKCU\Environment" /v NAME /f`)                                                                                 | none                |
| Windows system variable (`HKLM\…\Environment`)            | run `reg delete …` as administrator — UAC prompt                                                                                                            | administrator (UAC) |
| a line in a shell rc file / PowerShell profile            | back the file up, then turn the line into `# removed by SeedRouter Onboarding: <original line>` (not deleted)                                               | none                |
| macOS `launchctl`                                         | run `launchctl unsetenv NAME`                                                                                                                               | none                |
| present only in the current process (inherited at launch) | **cannot be removed**. The terminal or program that started this tool passed it in; reopen the tool from the Start menu / Launchpad, or close that terminal | —                   |

When done, the environment-variable check runs again.

**Undo**: copy the rc-file backup back or remove the leading comment from the line; re-add a registry value via the path described under fault D (you saw the full value in the preview).

**If it fails**: UAC cancelled → the system variable stays, every other source is still handled and the result lists the item as "failed"; the value changed after the preview → the tool refuses to delete that item; click the button again to see the new value.

## Apply config.toml with one click

**What it does**
Writes the Codex configuration template generated on the "Configure" page to `~/.codex/config.toml`. The template routes Codex through the service gateway while keeping the official features (priority tier, large context window, auto-compaction). This is the **only** file this tool ever writes inside `~/.codex/`, and only after you click the button.

**Step by step**

1. The template shown on the page carries the placeholder `<API-KEY>`; the real key is substituted in memory on your machine when you click "Apply" — **the real key is never displayed**.
2. The tool checks that the content is valid TOML.
3. If `~/.codex/config.toml` already exists it is copied to `config.toml.seedrouter-<timestamp>.bak` first.
4. The new file is written. The result shows the path, the size and the backup name.

**Rights needed**: none (the file lives in your user profile).

**Undo**: click "Restore previous backup" — the newest backup is put back (the current file is backed up first, so this step can be undone as well).

**If it fails**

- **CC Switch may rewrite this file when you switch providers** — that is CC Switch's normal behaviour. After switching, go back to the "Configure" page and click "Apply" again; the page shows "differs from template" when that is needed.
- No key entered yet → the button is disabled; paste the key in the card above first.
- File in use / permission denied → close a running Codex and retry.

## Install the Codex client

**What it does**
Installs the Codex desktop client (now part of the **ChatGPT** desktop app). This step is **optional but recommended**: the command-line Codex CLI does not need it, but the desktop client is friendlier for non-developers.

**Step by step**

- **Windows, recommended**: "Open Microsoft Store" jumps to the ChatGPT app page in the Store (product `9PLM9XGG6VKS`); click "Get" there.
- **Windows, offline package**: the tool downloads the OpenAI-signed MSIX (about 745 MB) and runs `powershell Add-AppxPackage -Path <file>`. This is a per-user install and **needs no administrator rights**.
- **The Store says "not available in your region"**: click "Open Windows region settings" (`ms-settings:regionformatting`), change "Country or region" to one where the app is available, close and reopen the Store; you can change it back afterwards.
- **macOS**: the tool downloads the DMG (about 650 MB), mounts it, copies the `.app` into `/Applications` and ejects the image. Users in the "admin" group are not asked for a password. Manual alternative: open the DMG and drag the app into "Applications".
- Every step has a manual fallback: "Open download page" opens the official download site.

**Rights needed**: none for the Store and the MSIX; none for dragging into "Applications" on macOS (a standard user account may be asked for an administrator password by the system).

**Undo**: Windows — uninstall "ChatGPT" under "Settings → Apps"; macOS — move `/Applications/ChatGPT.app` to the Bin.

**If it fails**: `Add-AppxPackage` reports an untrusted signature → the download is incomplete or tampered with; delete it and download again, or use the Store; download is slow → it is a large file, the Store may be faster; the Store does not open → use the offline package.

## About permissions

- This tool **never elevates silently**. Only two kinds of action need administrator rights: running the official Node.js installer (MSI / pkg) and deleting a Windows **system-level** environment variable. In both cases the prompt (UAC, macOS password box) is shown by the operating system or the installer itself, and the button is labelled "(needs administrator rights)".
- Every other one-click action (user PATH repair, user-variable deletion, commenting out rc-file lines, writing `config.toml`, installing the Codex client) touches only your own user profile and user-level settings.
- The tool **never** writes to `~/.claude/` or `~/.cc-switch/` and never edits the machine-wide (all users) PATH.
- Every modified file has a `.seedrouter-<timestamp>.bak` backup; every executed command is recorded in the tool's log (keys and tokens redacted).
