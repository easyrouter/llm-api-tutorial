# Configure the service gateway in CC Switch

Configuration happens inside CC Switch's own window; this tool never writes a configuration file for you. The wizard's "Configure" page shows every value you need with a copy button — **copy the address and the name; only the API key is something you type yourself**. CC Switch updates often, so button labels may differ slightly; what the app shows always wins.

## Steps

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

## Codex vs Claude Code

- **Codex CLI** — CC Switch writes `~/.codex/config.toml` and `auth.json`; the protocol setting corresponds to `wire_api = "responses"`.
- **Claude Code** — CC Switch writes `~/.claude/settings.json`. There is no protocol to choose.

You do not need to edit these files by hand. If you edited them manually in the past, let CC Switch take over so the two do not overwrite each other.

## After configuring

The new configuration is **not** picked up by terminals that are already open. Continue to the "Verify" step, close all terminal windows first, and then test.
