# Help documentation layout (M6)

This folder is the **bundled fallback** for the in-app help panel and, at the same time, the
reference layout that the designated documentation site (PRD #18, `docs.baseUrl` in
`app-config.json`) must publish. The app resolves every document through the same cascade:

```
remote  {baseUrl}/{lang}/{path}   →   cache  <app-cache-dir>/docs/{lang}/{path}   →   bundled  resources/docs/{lang}/{path}
```

- Within `docs.cacheTtlSeconds` the cache is served without touching the network.
- After the TTL the remote is tried (10 s timeout); on failure the stale cache is served.
- Without any cache the bundled copy is used (the files here are also compiled into the binary
  with `include_str!`, so dev builds work without a resource directory).

## Layout the docs site must publish

```
{baseUrl}/
  zh-CN/
    index.json          ← section list for this language
    overview.md         ← one Markdown file per section (path relative to the language folder)
    …
  en/
    index.json
    overview.md
    …
```

`lang` is one of `zh-CN` or `en` (anything else falls back to `en`). Sub-directories are allowed
in `path` (e.g. `guides/verify.md`).

## `index.json`

```json
{
  "sections": [
    {
      "id": "overview",
      "title": "欢迎与总览",
      "path": "overview.md",
      "lang": "zh-CN",
      "wizardStep": "welcome",
      "children": []
    }
  ]
}
```

| field        | required | rules                                                                                                |
| ------------ | -------- | ---------------------------------------------------------------------------------------------------- |
| `id`         | yes      | stable slug used by the wizard ("Learn more" links); `[A-Za-z0-9._-]`, max 64 chars, no `..`         |
| `title`      | yes      | display title in the section language                                                                |
| `path`       | yes      | Markdown file relative to `{baseUrl}/{lang}/`; `/` separators; no `..`, `.`, empty or absolute parts |
| `lang`       | yes      | `zh-CN` or `en`                                                                                      |
| `wizardStep` | no       | `welcome`, `env_check`, `install`, `configure`, `verify`, `diagnose`, `done` or `null`               |
| `children`   | no       | nested sections with the same shape                                                                  |

Sections with an unsafe `id` or `path` are dropped when the index is loaded.

## Markdown pages

- Served with `Content-Type: text/*` (`text/markdown; charset=utf-8` preferred) or
  `application/octet-stream`; anything else is rejected and the app falls back.
- Maximum size 2 MiB per page, 1 MiB for `index.json`; UTF-8 without BOM.
- Start with a single `#` heading. Rendered with react-markdown (no raw HTML, no scripts).
- Plain language for non-developers; keep zh-CN and en pages equivalent.

## Section ids used by the wizard

| id                    | wizard step | content                                                  |
| --------------------- | ----------- | -------------------------------------------------------- |
| `overview`            | welcome     | what the tool does, the six steps, what it never touches |
| `env-check`           | env_check   | what is checked and what pass / warning / blocked mean   |
| `install-node`        | install     | Node.js LTS installation                                 |
| `install-cli`         | install     | `npm install -g` for Codex CLI / Claude Code             |
| `install-cc-switch`   | install     | downloading and installing CC Switch                     |
| `configure-cc-switch` | configure   | adding the company gateway as a provider in CC Switch    |
| `verify`              | verify      | closing all terminals, running the check                 |
| `troubleshooting`     | diagnose    | faults A–G                                               |
| `faq`                 | —           | frequently asked questions                               |

Publishing a new version of the site does not require an app release; the app picks the new
content up after the cache TTL expires.
