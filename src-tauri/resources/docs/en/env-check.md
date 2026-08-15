# Environment check

The check starts automatically when you enter the wizard and usually finishes within a few seconds. Each item has one of three results:

- **Pass** — nothing to do.
- **Warning** — you can continue, but you may hit trouble later; follow the hint if you can.
- **Blocked** — must be fixed before you continue. Every blocked item has a "Fix" or "Show instructions" button next to it.

## What is checked

| Item                    | What it looks at                                                       | Typical outcome                                          |
| ----------------------- | ---------------------------------------------------------------------- | -------------------------------------------------------- |
| Operating system        | Windows / macOS version and CPU architecture                           | too old → blocked                                        |
| Node.js                 | installed and version ≥ 18                                             | missing or old → go to Install                           |
| npm                     | available together with Node.js                                        | normally passes with Node.js                             |
| Codex CLI / Claude Code | whether `codex --version` / `claude --version` runs                    | tells "not installed" apart from "installed, PATH stale" |
| CC Switch               | installed                                                              | missing → go to Install                                  |
| Environment variables   | `OPENAI_API_KEY`, `OPENAI_BASE_URL`, `ANTHROPIC_*` and similar present | warning (see fault D)                                    |
| Network                 | npm registry, service gateway and GitHub reachable                     | unreachable → mirror or proxy hint                       |

## Things that are easy to misread

**What does "installed, but PATH not refreshed" mean?**
Terminal windows that were already open do not know about a command that was just installed. This is not a failed installation — close every terminal window and open a new one (see "Apply & verify").

**Why are environment variables a warning?**
If `OPENAI_API_KEY`, `OPENAI_BASE_URL` or similar variables are set on your system, the command-line tools use them _instead of_ the CC Switch configuration. That shows up as "I configured it but nothing changed" or as a 401 error. The tool only tells you where the variable is defined; it never edits it for you (see fault D).

**Is a failed network check always a problem?**
Not necessarily. It only tests reachability, not authorisation. If your office network needs a proxy, make sure the proxy is configured. If the npm registry is unreachable the Install step switches to a mirror automatically.

## Next

Once every blocked item is resolved, click "Next". You can click "Re-run" at any time to refresh all results, or the small re-run button next to a single item.
