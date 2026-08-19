# Environment check

The check starts automatically when you enter the wizard and usually finishes within a few seconds. Each item has one of three results:

- **Pass** — nothing to do.
- **Warning** — you can continue, but you may hit trouble later; follow the hint if you can.
- **Blocked** — must be fixed before you continue. Every blocked item has a "Fix" or "Show instructions" button next to it.

Many fix buttons are now **one-click actions**: install Node.js, repair PATH, clean up environment variables, install the Codex client. Each one shows what it is about to do before it runs, and says on the button when it needs administrator rights. What every button does behind the scenes is explained in "One-click actions explained" (`one-click`).

## What is checked

| Item                    | What it looks at                                                       | Typical outcome                                          |
| ----------------------- | ---------------------------------------------------------------------- | -------------------------------------------------------- |
| Operating system        | Windows / macOS version and CPU architecture                           | too old → blocked                                        |
| Node.js                 | installed and version ≥ 18                                             | missing or old → go to Install                           |
| npm                     | available together with Node.js                                        | normally passes with Node.js                             |
| Codex CLI / Claude Code | whether `codex --version` / `claude --version` runs                    | tells "not installed" apart from "installed, PATH stale" |
| CC Switch               | installed                                                              | missing → go to Install                                  |
| Codex client            | ChatGPT / Codex desktop app installed (optional)                       | missing → warning, one-click install                     |
| Environment variables   | `OPENAI_API_KEY`, `OPENAI_BASE_URL`, `ANTHROPIC_*` and similar present | warning, one-click clean-up (fault D)                    |
| Network                 | npm registry, service gateway and GitHub reachable                     | unreachable → mirror or proxy hint                       |

## Things that are easy to misread

**What does "installed, but PATH not refreshed" mean?**
Terminal windows that were already open do not know about a command that was just installed. This is not a failed installation — close every terminal window and open a new one (see "Apply & verify"). If the command is still missing after that, its folder really is not on PATH: click "Repair PATH" and the tool adds the Node.js folder or npm's global folder to your user PATH (no administrator rights). The same button appears for "Codex CLI found but broken: `'"node"' is not recognized`".

**Why are environment variables a warning?**
If `OPENAI_API_KEY`, `OPENAI_BASE_URL` or similar variables are set on your system, the command-line tools use them _instead of_ the CC Switch configuration. That shows up as "I configured it but nothing changed" or as a 401 error. The tool shows where each variable is defined (registry, or file and line) and offers a one-click "Clean up": the confirmation dialog shows the **full value** of every variable, nothing is deleted until you confirm, and every edited file gets a backup (see fault D and "One-click actions explained").

**Is a failed network check always a problem?**
Not necessarily. It only tests reachability, not authorisation. If your office network needs a proxy, make sure the proxy is configured. If the npm registry is unreachable the Install step switches to a mirror automatically.

**Is the Codex client required?**
No. Codex CLI works in the terminal and does not need the desktop client. The desktop client (now part of the ChatGPT desktop app) is friendlier for non-developers, so the row is a recommendation: on Windows install it from the Microsoft Store or with the offline package — when the Store says "not available in your region" a button opens the Windows region settings; on macOS the tool downloads the DMG.

## Next

Once every blocked item is resolved, click "Next". You can click "Re-run" at any time to refresh all results, or the small re-run button next to a single item.
