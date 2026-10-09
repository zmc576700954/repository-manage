# Synapse KB

A lightweight personal knowledge base manager with graph view.

## Quick Start

```bash
npm install
npm run tauri:dev
```

## Architecture

- **Tauri 2.0** desktop shell
- **Rust** backend for file scanning, parsing, indexing
- **React + React Flow** frontend
- **SQLite** local index
- **axum** local HTTP server (for Web build)

## Standards

See [docs/standards.md](docs/standards.md) for the file format specification.

## Development

```bash
npm run dev          # Vite dev server (frontend only)
npm run tauri:dev    # Full Tauri app with hot reload
npm run build        # Production frontend build
npm run tauri:build  # Full production bundle
```

## Tests

```bash
cd src-tauri && cargo test   # Backend tests
npm run build                # TypeScript build (catches type errors)
```

## License

MIT
