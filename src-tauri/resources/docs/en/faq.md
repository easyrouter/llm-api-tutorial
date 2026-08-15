# FAQ

**How does this tool relate to CC Switch?**
CC Switch does the _configuring_; this tool ties the _whole journey_ together — checking the environment, installing what is missing, telling you what to enter in CC Switch, verifying the result and diagnosing failures. It does not replace CC Switch and never touches its data.

**I only use Codex, not Claude Code (or the other way round).**
Tick only the tool you need at the start of the wizard. Install, configure and verify then apply to that tool only.

**Does the tool store my API key?**
No. The key exists in memory only while the connectivity test in "Verify" is running and is discarded once the request finishes. It is never written to disk, logs, diagnostic reports or telemetry. CC Switch writes the key into each tool's own configuration file — that is its normal job.

**Why do I have to close all terminal windows?**
A terminal copies the environment (PATH, variables) when it opens and never refreshes it. If you do not reopen terminals after installing or configuring, you get "command not found" or "configuration has no effect". This is the most frequent problem, which is why the tool keeps reminding you.

**Can I finish without administrator rights?**
Yes. Node.js has a portable build or a per-user install; global npm installs go into your user profile by default; the CC Switch Windows installer supports a current-user install; this tool itself installs per user. It never silently requests elevated rights.

**The office network cannot reach the official npm registry.**
The tool measures latency and switches to a mirror (npmmirror) automatically. The command it shows always states which registry it uses.

**What do I put in the model field?**
Only the gateway address is company-wide; model name and reasoning effort vary per person. Enter the model assigned to you, or start with the value the tool suggests — you can change it in CC Switch at any time.

**How do I upgrade Codex / Claude Code later?**
Run `npm install -g @openai/codex` (or `@anthropic-ai/claude-code`) again in a new terminal. CC Switch shows its own update prompt inside the app.

**Does the tool update itself?**
No. Download new versions from the company intranet or the official site.

**What does telemetry collect?**
It is off by default and only sends anything when IT has configured an intranet endpoint and you have not switched it off. The payload contains anonymous step success/failure, durations, error classes and diagnosis rule ids — no host names, user names, addresses, paths, keys or free text. You can turn it off in Settings at any time.

**Is the diagnostic report safe to share?**
Yes. Keys, tokens and addresses containing credentials are replaced with `[REDACTED]` when the report is generated.
