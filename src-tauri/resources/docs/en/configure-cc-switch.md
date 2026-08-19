# Configure the service gateway in CC Switch

Providers are configured inside CC Switch's own window. The wizard's "Configure" page shows every value you need with a copy button — **copy the address and the name; only the API key is something you type yourself**. For Codex the tool additionally writes the recommended `config.toml` to your machine once you click the button (see "Apply config.toml" below). CC Switch updates often, so button labels may differ slightly; what the app shows always wins.

## Codex: pick your account situation first

CC Switch's Codex tab ships a built-in **"OpenAI Official"** preset provider. The top of the "Configure" page asks you to choose one of two paths; the steps below then show only what your path needs:

- **I have a ChatGPT account I can sign in with (recommended)** → use the "OpenAI Official" preset; **no** custom provider and no key:
  1. On the Codex tab click "Add provider", choose "OpenAI Official" from the preset list, save, and click "Use / Switch" on it so it becomes the current provider.
  2. Open a terminal, run `codex`, choose "Sign in with ChatGPT" and finish the login in the browser. Once signed in, the official login-gated features (speed / tier option and so on) are available.
  3. Then "Apply config.toml" (below) so that requests go through the service gateway.
  4. If you later switch to a third-party provider in CC Switch and want to keep the login, enable "Keep official login when switching to third-party" under CC Switch **Settings → General → Codex enhancements**.
- **No account** → add a custom provider with the service gateway address and your API key (the "Steps" below), then "Apply config.toml" as well.

Claude Code has a single path: the custom provider.

## Steps (custom provider)

1. **Open CC Switch.**
2. **Pick the tool tab.** There are separate Claude and Codex tabs at the top (or on the left). Choose the one you are configuring; if you use both, repeat the steps for each.
3. **Click "Add provider".**
4. **Fill in the form:**
   - **Name** — anything you like; the name suggested by the tool (for example "Service Gateway") is easiest to recognise later.
   - **API address / Base URL** — paste the service gateway address shown by the tool **exactly as displayed**. Do not add or remove a trailing slash or `/v1`; the address rules are explained under fault B.
   - **Protocol / request format (Codex only)** — choose **Responses**. This is the protocol the service gateway supports and recommends. If the form has no such option, the current version already defaults to Responses.
   - **API key** — paste the key that was issued to you. Make sure there is no space or line break at the beginning or end (the most common cause of a 401). The tool's Configure page can check the key format for you; the check runs in memory on your machine only and nothing is stored.
   - **Model** — enter the value suggested by the tool, or the model name assigned to you. This field may differ per person.
5. **Save.**
6. **Activate / switch to the new provider.** A newly added provider is not active automatically — click "Use" or "Switch" on it in the list so it becomes the current provider (the card usually changes colour or gets a tick).
7. Back in this tool, click "I did this"; the tool checks that the configuration is in place.

"Import into CC Switch" is the one-click alternative: the tool shows the masked import link first; after you confirm it opens CC Switch's `ccswitch://` import link, and CC Switch asks once more before writing its own data.

## Apply config.toml (Codex only)

The "Codex config.toml template" card on the "Configure" page generates the recommended configuration: the service gateway as a provider, while keeping the official features (priority tier, large context window, auto-compaction). The key inside the template is the placeholder `<API-KEY>`; every other value can be edited first. When you click "Apply to this machine":

1. the real key is substituted in memory on your machine and never displayed;
2. if `~/.codex/config.toml` already exists it is backed up as `config.toml.seedrouter-<timestamp>.bak`;
3. the new file is written.

"Restore previous backup" undoes it at any time. **CC Switch may rewrite this file when you switch providers** — after switching, come back and click "Apply" again; the card shows "differs from template" when needed. This is the only file the tool ever writes inside `~/.codex/`, and only after you click; details in "One-click actions explained".

## Codex vs Claude Code

- **Codex CLI** — CC Switch writes `~/.codex/config.toml` and `auth.json`; the protocol setting corresponds to `wire_api = "responses"`. This tool also writes `config.toml` (with backup) after you click "Apply".
- **Claude Code** — CC Switch writes `~/.claude/settings.json`. There is no protocol to choose. This tool never writes to `~/.claude/`.

Beyond that you do not need to edit these files by hand. If you edited them manually in the past, let CC Switch take over so the two do not overwrite each other.

## After configuring

The new configuration is **not** picked up by terminals that are already open. Continue to the "Verify" step, close all terminal windows first, and then test.
