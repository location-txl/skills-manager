<p align="center">
  <img src="assets/icon.png" width="80" />
</p>

<h1 align="center">Skills Manager</h1>

<p align="center">
  A desktop front end for the open <a href="https://github.com/vercel-labs/skills">Vercel <code>skills</code> CLI</a>.
</p>

<p align="center">
  <a href="./README.zh-CN.md">中文说明</a>
</p>

---

Skills Manager 2.x is a **thin GUI** over the Vercel `skills` CLI:

- **No database of its own.** Your skills live where the CLI puts them —
  `~/.agents/skills` globally, `<project>/.agents/skills` + `skills-lock.json`
  per project — and stay fully usable by the CLI and every compatible agent
  without this app.
- **The CLI is the only writer.** Install, update and uninstall always run
  through `skills add` / `update` / `remove`. The GUI never copies skill files
  or touches the CLI's lock files.
- **Read-only when degraded.** If the CLI is missing or too old, the app still
  browses what's on disk and tells you how to fix the runtime.

> **Breaking change:** 2.0 is a destructive rewrite of the 1.x architecture
> (SQLite store, private central repo, presets, tags, Git backup, sync and the
> bundled `skills-manager-cli` are gone). Old data under `~/.skills-manager` is
> **not read, migrated or deleted** — remove it manually if you no longer need
> it. As a fork, the in-app updater has been removed; download releases from
> this repository.

## Requirements

- Node.js ≥ 22
- The Vercel skills CLI, write-capable version:

  ```bash
  npm install -g skills@^1.7.0
  ```

  With an older CLI (e.g. 1.5.x) the app still lists skills, but write
  operations are disabled until you upgrade. See
  [docs/CLI_CONTRACT.md](docs/CLI_CONTRACT.md) for the supported commands,
  output shapes and version policy.

## Features

- **Library** — every global skill (shared dir + agent dirs), deduplicated,
  with source metadata from the CLI's lock file, per-agent visibility and
  safe install/update/uninstall flows.
- **Agents** — which of the CLI's known agents are detected and what each
  can see (shared / symlink / copy / native).
- **Project** — open one folder per session; manage its `.agents/skills` and
  lock file without a project registry.
- **Marketplace** — skills.sh search; installation still goes through the CLI
  with explicit scope + agent selection.
- **Diagnostics** — Node/CLI paths and versions, lock-file location, last CLI
  error, panic log and log excerpt.

## Development

```bash
npm install
npm run tauri:dev    # desktop app in dev mode
npm test             # vitest
npm run lint         # eslint + UI spec + i18n checks
npm run build        # frontend build

cd src-tauri
cargo test           # unit + contract tests (isolated HOME, real skills CLI)
```

The contract tests install `skills@1.7.4` into a throwaway npm prefix and run
real install/remove/update flows against isolated HOMEs — your actual
`~/.agents` is never touched.

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the layering, data
ownership and safety rules, and [docs/CLI_CONTRACT.md](docs/CLI_CONTRACT.md)
for the CLI integration contract.

## Credits

Forked from [xingkongliang/skills-manager](https://github.com/xingkongliang/skills-manager)
(MIT), rebuilt around the Vercel skills CLI. Skill infrastructure by
[vercel-labs/skills](https://github.com/vercel-labs/skills).
