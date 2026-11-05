# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Synapse KB — a lightweight desktop personal knowledge base with graph/relations view. Users manage Markdown notes either as flat `*.md` files or as `content.md` inside per-entry folders, see them as nodes on a deterministic grouped grid (React Flow), and wire them together with typed relations (`reference`/`derived`/`contradicts`/`supersedes`/`extends`/`custom`). Node drag positions persist to `.synapse-layout.json` in the KB root.

The on-disk file format & metadata conventions are defined in [docs/standards.md](docs/standards.md). Read that before changing parser/scanner/entry code.

## Stack

- **Tauri 2** desktop shell (Rust backend + WebView frontend).
- **Rust backend** (`src-tauri/`): file scan + markdown parse → SQLite (`rusqlite`, bundled) → typed entries/relations.
- **React 18 + Vite + TypeScript** frontend (`src/`) — Zustand store, React Flow graph, Tailwind, **i18next with `zh` + `en` resources** (see below).
- Path aliases: `@/*` → `src/*`, `@shared/*` → `shared/*` (TS + Vite both configured in `tsconfig.json` / `vite.config.ts`).

## Build / dev / test

```bash
npm install              # once
npm run dev              # Vite-only frontend (port 1420)
npm run tauri:dev        # full app with HMR
npm run build            # tsc + vite build (strict TS — catches type errors)
npm run tauri:build      # production .app / .msi / .deb etc.

cd src-tauri && cargo test          # backend unit tests (parser, scanner, index, paths, layout_store, error)
```

There is no separate JS test runner — `npm run build` is the TS type-check + lint-equivalent. Rust tests live next to source as `#[cfg(test)] mod tests` blocks (see `src-tauri/src/parser/links.rs`, `src-tauri/src/parser/comments.rs`, `src-tauri/src/scanner.rs`, etc.).

## Architecture (big picture)

```
src/                          # React frontend
  App.tsx                     # top-level layout: Sidebar + (MapView | empty) + DetailPanel + Settings modal
  components/                 # one folder of small UI per file (each owns its own i18n JSON)
    Sidebar.tsx               # KB list, language switcher, reload, open-settings
    NodeCard.tsx              # per-entry node renderer on the map
    RelationDialog.tsx        # create/edit a relation
    EntryDialog.tsx           # create a new entry
  views/
    MapView.tsx               # React Flow canvas + node/edge renderers + legend + side panel
    DetailPanel.tsx           # markdown viewer for the selected entry (opened from a node)
    Settings.tsx              # manage knowledge bases (add/remove/switch active)
  store/
    useAppStore.ts            # Zustand: entries, selection, relation-filter, KB list, mutations -> invoke Tauri cmds
    langStore.ts              # persists `zh`/`en` choice via tauri-plugin-store
  i18n/
    index.ts                  # initI18n() (loads persisted lang, falls back to browser), changeLanguage()
    resources.ts              # NS list + zh/en resource bundles wired up here
    languages.ts              # Lang type + browser-lang detection

src-tauri/
  src/
    lib.rs                    # tauri::Builder, registers invoke handlers, wires shared state
    state.rs                  # AppState (Arc<RwLock<...>>: index, kb_registry, current_kb)
    commands.rs               # all #[tauri::command] entry points invoked from the frontend
    scanner.rs                # walkdir → entry paths (flat *.md or folders with content.md); reports KbLayout
    parser/
      mod.rs, comments.rs, yaml_field.rs, links.rs, attachments.rs
                             # extract frontmatter / `@synapse-*` comments / [[wikilinks]] / ![[embeds]]
    entry.rs                  # RawEntry → stored Entry model; unified parser for both layouts
    index.rs                  # SQLite schema + rebuild/get_entries
    kb_registry.rs            # multi-KB support: list/add/remove/switch active KB (persisted)
    layout_store.rs           # .synapse-layout.json read/write, exposed via get_layout / save_node_position
    paths.rs, error.rs
```

**State flow:** frontend calls `invoke('cmd', args)` → `commands.rs` reads `AppState` → updates `Index` / `KbRegistry` / `LayoutStore` → `loadEntries()` re-fetches everything and `useAppStore` updates. On startup the active KB is rebuilt asynchronously on the Tauri runtime (not on the main thread); `get_entries` waits behind a startup gate so the first paint never reads a half-built index. `Index::open` runs the schema migrations, so the rebuild is purely a data sync.

**Multi-KB:** since `kb_registry.rs` exists, the app supports several knowledge bases; the "active" one is what the UI shows. `refreshKbs()` and `setActiveKb()` in the store are how the UI reacts to changes.

## i18n (zh + en) — required when adding UI text

The project ships **both** Chinese and English UI. Any new or changed user-facing string **must** be added to both locales:

1. Decide which component owns the strings. The convention is **one i18n namespace per component/view** (see `src/i18n/resources.ts` for the current list: `App`, `Settings`, `Sidebar`, `NodeCard`, `RelationDialog`, `MapView`, `EntryDialog`, `DetailPanel`).
2. Create or edit both files:
   - `<Owner>.zh.json` — Chinese strings
   - `<Owner>.en.json` — English strings
3. Register the new namespace in `src/i18n/resources.ts` (add to `NS`, then to both `zh` and `en` resource objects).
4. In the component, call `useTranslation('NamespaceName')` and use `t('key')`. Interpolation uses `{{name}}` tokens, e.g. `"createInGroupTitle": "Create a new document under \"{{path}}\""`.

Files in `src/components/*.json` and `src/views/*.json` are managed as **namespace pairs** — never add a string to only one.

The `App` namespace at `src/App.{zh,en}.json` is for chrome-level strings only (loading/error/no-active-kb). Prefer adding a new namespace over growing `App` for component-specific copy.

## Notable conventions

- **Path aliases:** import shared types via `from '@shared/types'`, app code via `from '@/...'`.
- **Relation shapes/semantics:** defined by `docs/standards.md` — colors and arrow styles correspond to the 5 default types. Custom relations are allowed.
- **Markdown files on disk are the source of truth;** the SQLite index is a rebuildable cache. Read commands serve the index, but every mutation writes the `.md` file first and then re-indexes. Startup rebuilds the active KB from disk, and the file watcher keeps it in sync afterwards.
- **No Docker / no local HTTP server in current code.** The repo's `Dockerfile` and `src-tauri/src/local_server.rs` were removed (see git status); treat the Tauri desktop build as the only target.
