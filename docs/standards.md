# Synapse KB File Format Standards

## Directory Structure

```
kb-root/
├── topic-a/                      # Entry folder (folder name = entry ID)
│   ├── content.md                # Required
│   ├── attachments/              # Recommended: local attachments
│   └── notes/                    # Optional: block-level supplementary files
├── topic-b/
│   └── content.md
└── .synapse-layout.json          # Auto-generated: node positions
```

## content.md Basic Metadata

Use HTML comment lines at the top:

```
<!-- @synapse-id: react-hooks -->
<!-- @synapse-title: React Hooks Deep Dive -->
<!-- @synapse-tags: react, frontend -->
<!-- @synapse-group: frontend -->
```

## Bidirectional Links and Attachment Embeds

```markdown
See [[state-management]] for more.

![[./diagrams/flow.png|caption]]
```

## Optional YAML Advanced Fields

```markdown
---
color: "#5B8DEF"
shape: rect
pinned: true
order: 5
---

# Content
```

## 5 Relation Types (Recommended)

- `reference` (blue solid): reference citation
- `derived` (green with arrow): derived/summarized from
- `contradicts` (red dashed): contradicts
- `supersedes` (orange double): supersedes old version
- `extends` (purple curved): extends/supplements
