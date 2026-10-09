# Synapse-KB Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建 Synapse-KB —— 一个让用户专注于知识内容与关联本身、不引入新认知负担的个人知识库管理器。

**Architecture:** Tauri 2.0 桌面应用 + Rust 后端（文件扫描、解析、SQLite 索引、axum HTTP 服务）+ React 18 前端（React Flow 地图、详情面板、设置）。同一套前端代码既可在 Tauri WebView 中运行，也可作为静态 Web 部署（Web 端通过本地服务调用）。

**Tech Stack:** Tauri 2.0, Rust, React 18, Vite, TypeScript, React Flow (xyflow), Tailwind CSS, SQLite (rusqlite), axum, notify, Zustand

---

## File Structure

```
repository-manage/
├── shared/
│   └── types.ts                    # 前后端共享的类型定义
├── src-tauri/
│   ├── Cargo.toml                 # Rust 依赖
│   ├── tauri.conf.json            # Tauri 配置
│   ├── build.rs                   # 构建脚本
│   ├── src/
│   │   ├── main.rs                # Tauri 入口
│   │   ├── lib.rs                 # 应用启动编排
│   │   ├── error.rs               # 全局错误类型
│   │   ├── state.rs               # 应用状态
│   │   ├── parser/
│   │   │   ├── mod.rs
│   │   │   ├── comments.rs        # HTML 注释解析
│   │   │   ├── links.rs           # [[双向链接]] 提取
│   │   │   ├── attachments.rs     # ![[附件]] 提取
│   │   │   └── yaml_field.rs      # YAML 高级字段
│   │   ├── scanner.rs             # 目录扫描
│   │   ├── entry.rs               # 条目聚合（组合各解析器）
│   │   ├── layout_store.rs        # 节点位置读写
│   │   ├── index.rs               # SQLite 索引
│   │   ├── watcher.rs             # 文件变更监听
│   │   ├── local_server.rs        # axum HTTP 服务
│   │   ├── commands.rs            # Tauri IPC 命令
│   │   └── paths.rs               # 路径工具
│   └── tests/
│       ├── parser_test.rs
│       ├── scanner_test.rs
│       ├── layout_store_test.rs
│       └── integration_test.rs
├── src/
│   ├── main.tsx                   # React 入口
│   ├── App.tsx                    # 主应用
│   ├── views/
│   │   ├── MapView.tsx            # 画布视图
│   │   ├── DetailPanel.tsx        # 详情面板
│   │   └── Settings.tsx           # 设置视图
│   ├── components/
│   │   ├── NodeCard.tsx           # 自定义节点
│   │   ├── Sidebar.tsx            # 侧栏
│   │   ├── RelationDialog.tsx     # 创建关联
│   │   └── MetadataDialog.tsx     # 编辑元数据
│   ├── lib/
│   │   ├── api.ts                 # 后端 API 客户端
│   │   ├── markdown.ts            # Markdown 渲染
│   │   └── relationTypes.ts       # 关联类型定义
│   ├── store/
│   │   └── useAppStore.ts         # 全局状态
│   ├── styles.css                 # Tailwind 入口
│   └── env.d.ts
├── package.json
├── vite.config.ts
├── tsconfig.json
├── tsconfig.node.json
├── tailwind.config.js
├── postcss.config.js
├── index.html
├── .eslintrc.cjs
├── .prettierrc
└── README.md
```

---

## Phase 1: Project Scaffolding

### Task 1: Initialize Tauri 2.0 + Vite + React Project

**Files:**
- Create: `package.json`, `vite.config.ts`, `tsconfig.json`, `tsconfig.node.json`, `index.html`, `src/main.tsx`, `src/App.tsx`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/build.rs`, `src-tauri/src/main.rs`

- [ ] **Step 1: Create package.json**

```json
{
  "name": "synapse-kb",
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "tauri": "tauri",
    "tauri:dev": "tauri dev",
    "tauri:build": "tauri build"
  },
  "dependencies": {
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "reactflow": "^11.11.4",
    "@tauri-apps/api": "^2.0.0",
    "zustand": "^4.5.4"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.0.0",
    "@types/react": "^18.3.3",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.1",
    "typescript": "^5.5.3",
    "vite": "^5.3.4"
  }
}
```

- [ ] **Step 2: Create vite.config.ts**

```typescript
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: false,
    host: '127.0.0.1',
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    target: 'es2021',
    minify: 'esbuild',
    sourcemap: false,
  },
});
```

- [ ] **Step 3: Create tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2021",
    "lib": ["ES2021", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "allowImportingTsExtensions": false,
    "baseUrl": ".",
    "paths": {
      "@/*": ["src/*"],
      "@shared/*": ["shared/*"]
    }
  },
  "include": ["src", "shared"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

- [ ] **Step 4: Create tsconfig.node.json**

```json
{
  "compilerOptions": {
    "composite": true,
    "skipLibCheck": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true,
    "strict": true
  },
  "include": ["vite.config.ts"]
}
```

- [ ] **Step 5: Create index.html**

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Synapse KB</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 6: Create src/main.tsx**

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

- [ ] **Step 7: Create src/App.tsx**

```tsx
export default function App() {
  return (
    <div className="flex h-screen w-screen items-center justify-center bg-gray-50 text-gray-900">
      <h1 className="text-2xl font-semibold">Synapse KB</h1>
    </div>
  );
}
```

- [ ] **Step 8: Create src-tauri/Cargo.toml**

```toml
[package]
name = "synapse-kb"
version = "0.1.0"
edition = "2021"

[lib]
name = "synapse_kb_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
tokio = { version = "1", features = ["full"] }
rusqlite = { version = "0.31", features = ["bundled"] }
notify = "6"
notify-debouncer-mini = "0.4"
axum = "0.7"
walkdir = "2"
anyhow = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
dirs = "5"

[features]
default = ["custom-protocol"]
custom-protocol = ["tauri/custom-protocol"]
```

- [ ] **Step 9: Create src-tauri/build.rs**

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 10: Create src-tauri/tauri.conf.json**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Synapse KB",
  "version": "0.1.0",
  "identifier": "com.synapsekb.app",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "app": {
    "windows": [
      {
        "title": "Synapse KB",
        "width": 1200,
        "height": 800,
        "minWidth": 800,
        "minHeight": 600
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": ["icons/icon.png"]
  }
}
```

- [ ] **Step 11: Create src-tauri/src/main.rs**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    synapse_kb_lib::run();
}
```

- [ ] **Step 12: Install dependencies and verify**

Run: `npm install`
Expected: dependencies install without errors.

Run: `npm run build`
Expected: dist/ folder is created, no errors.

- [ ] **Step 13: Commit**

```bash
git add .
git commit -m "scaffold: initialize Tauri 2.0 + Vite + React project"
```

---

### Task 2: Define Shared Types

**Files:**
- Create: `shared/types.ts`

- [ ] **Step 1: Create shared/types.ts**

```typescript
export type RelationType =
  | 'reference'
  | 'derived'
  | 'contradicts'
  | 'supersedes'
  | 'extends'
  | 'custom';

export interface Relation {
  id: string;
  fromId: string;
  toId: string;
  type: RelationType;
  note?: string;
}

export interface Attachment {
  path: string;
  caption?: string;
  type: 'image' | 'document' | 'other';
}

export interface Entry {
  id: string;
  title: string;
  tags: string[];
  group?: string;
  path: string;
  contentPath: string;
  attachments: Attachment[];
  relations: Relation[];
  hasContentMd: boolean;
}

export interface Layout {
  entryId: string;
  x: number;
  y: number;
}

export interface AppConfig {
  kbRoot: string | null;
  relationColors: Record<RelationType, string>;
  theme: 'light' | 'dark';
}
```

- [ ] **Step 2: Verify build**

Run: `npm run build`
Expected: no errors, shared/types.ts is resolved.

- [ ] **Step 3: Commit**

```bash
git add shared/types.ts
git commit -m "feat: add shared TypeScript types"
```

---

### Task 3: Configure Tailwind CSS

**Files:**
- Create: `tailwind.config.js`, `postcss.config.js`
- Modify: `src/styles.css`, `src/main.tsx`

- [ ] **Step 1: Install Tailwind**

Run: `npm install -D tailwindcss postcss autoprefixer`

- [ ] **Step 2: Create tailwind.config.js**

```javascript
/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        synapse: {
          reference: '#3B82F6',
          derived: '#10B981',
          contradicts: '#EF4444',
          supersedes: '#F97316',
          extends: '#A855F7',
          custom: '#6B7280',
        },
      },
      fontFamily: {
        system: [
          '-apple-system',
          'BlinkMacSystemFont',
          'SF Pro Display',
          'Segoe UI',
          'sans-serif',
        ],
      },
    },
  },
  plugins: [],
};
```

- [ ] **Step 3: Create postcss.config.js**

```javascript
export default {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
  },
};
```

- [ ] **Step 4: Create src/styles.css**

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

@layer base {
  html {
    @apply font-system antialiased;
  }

  body {
    @apply bg-gray-50 text-gray-900;
  }

  ::-webkit-scrollbar {
    @apply w-2 h-2;
  }

  ::-webkit-scrollbar-thumb {
    @apply bg-gray-300 rounded-full;
  }

  ::-webkit-scrollbar-thumb:hover {
    @apply bg-gray-400;
  }
}
```

- [ ] **Step 5: Modify src/main.tsx to import styles**

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './styles.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

- [ ] **Step 6: Modify src/App.tsx to use Tailwind utilities**

```tsx
export default function App() {
  return (
    <div className="flex h-screen w-screen items-center justify-center bg-gradient-to-br from-gray-50 to-gray-100 text-gray-900">
      <div className="text-center">
        <h1 className="text-3xl font-semibold tracking-tight">Synapse KB</h1>
        <p className="mt-2 text-sm text-gray-500">Personal Knowledge Graph</p>
      </div>
    </div>
  );
}
```

- [ ] **Step 7: Verify build**

Run: `npm run build`
Expected: dist/ includes the Tailwind CSS, no errors.

- [ ] **Step 8: Commit**

```bash
git add .
git commit -m "feat: configure Tailwind CSS with synapse theme"
```

---

## Phase 2: Rust Core - Parser Layer (TDD)

### Task 4: HTML Comment Parser - Test

**Files:**
- Create: `src-tauri/src/parser/mod.rs`, `src-tauri/src/parser/comments.rs`, `src-tauri/src/parser/comments_test.rs` (in `#[cfg(test)] mod`)

- [ ] **Step 1: Create src-tauri/src/parser/mod.rs**

```rust
pub mod comments;
pub mod links;
pub mod attachments;
pub mod yaml_field;

pub use comments::*;
pub use links::*;
pub use attachments::*;
pub use yaml_field::*;
```

- [ ] **Step 2: Create src-tauri/src/parser/comments.rs with test skeleton**

```rust
use std::collections::HashMap;

/// 解析 content.md 文件开头的 HTML 注释行，提取 @synapse-* 元数据。
/// 返回值为 metadata 映射，未识别的注释会被忽略。
pub fn parse_metadata_comments(content: &str) -> HashMap<String, String> {
    let mut metadata = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with("<!--") || !line.ends_with("-->") {
            continue;
        }

        let inner = &line[4..line.len() - 3].trim();
        let Some((key, value)) = inner.split_once(':') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim();

        if key.starts_with("@synapse-") {
            let field = key.trim_start_matches("@synapse-");
            metadata.insert(field.to_string(), value.to_string());
        }
    }

    metadata
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_id_and_title() {
        let content = r#"<!-- @synapse-id: react-hooks -->
<!-- @synapse-title: React Hooks 深入理解 -->

# Content here
"#;

        let metadata = parse_metadata_comments(content);

        assert_eq!(metadata.get("id").map(|s| s.as_str()), Some("react-hooks"));
        assert_eq!(
            metadata.get("title").map(|s| s.as_str()),
            Some("React Hooks 深入理解")
        );
    }

    #[test]
    fn parses_tags_with_comma_separator() {
        let content = r#"<!-- @synapse-id: foo -->
<!-- @synapse-title: Foo -->
<!-- @synapse-tags: react, frontend, web -->
"#;

        let metadata = parse_metadata_comments(content);

        assert_eq!(
            metadata.get("tags").map(|s| s.as_str()),
            Some("react, frontend, web")
        );
    }

    #[test]
    fn ignores_non_synapse_comments() {
        let content = r#"<!-- regular HTML comment -->
<!-- @synapse-id: bar -->

body
"#;

        let metadata = parse_metadata_comments(content);

        assert_eq!(metadata.len(), 1);
        assert!(metadata.contains_key("id"));
    }

    #[test]
    fn handles_comments_with_no_value() {
        let content = "<!-- @synapse-id: -->";

        let metadata = parse_metadata_comments(content);

        assert_eq!(metadata.get("id").map(|s| s.as_str()), Some(""));
    }
}
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cd src-tauri && cargo test parser::comments`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/parser/
git commit -m "feat(parser): HTML comment metadata parser with tests"
```

---

### Task 5: Double Bracket Link Extractor - Test

**Files:**
- Create: `src-tauri/src/parser/links.rs`

- [ ] **Step 1: Create src-tauri/src/parser/links.rs with test skeleton**

```rust
/// 从 Markdown 正文提取 [[path]] 形式的双向链接。
/// 返回链接目标路径列表（不含路径前缀装饰）。
pub fn extract_double_bracket_links(content: &str) -> Vec<String> {
    let mut links = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            // 找到 ]] 结束位置
            let mut j = i + 2;
            while j + 1 < bytes.len() && !(bytes[j] == b']' && bytes[j + 1] == b']') {
                j += 1;
            }

            if j + 1 < bytes.len() {
                let inner = &content[i + 2..j];
                // 跳过 ! 开头的（那是附件引用）
                if !inner.starts_with('!') {
                    let target = strip_block_ref(inner);
                    if !target.is_empty() {
                        links.push(target.to_string());
                    }
                }
                i = j + 2;
                continue;
            }
        }
        i += 1;
    }

    links
}

/// 移除 #block-id 后缀
fn strip_block_ref(target: &str) -> &str {
    if let Some(idx) = target.find('#') {
        &target[..idx]
    } else {
        target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_single_link() {
        let content = "See [[other-entry]] for details.";
        let links = extract_double_bracket_links(content);
        assert_eq!(links, vec!["other-entry"]);
    }

    #[test]
    fn extracts_multiple_links() {
        let content = "Links: [[a]], [[b/c]], and [[d]].";
        let links = extract_double_bracket_links(content);
        assert_eq!(links, vec!["a", "b/c", "d"]);
    }

    #[test]
    fn strips_block_reference() {
        let content = "See [[entry#section-1]] and [[other#block]].";
        let links = extract_double_bracket_links(content);
        assert_eq!(links, vec!["entry", "other"]);
    }

    #[test]
    fn ignores_attachment_links() {
        let content = "Image: ![[image.png|caption]] and link [[real-link]].";
        let links = extract_double_bracket_links(content);
        assert_eq!(links, vec!["real-link"]);
    }

    #[test]
    fn handles_empty_content() {
        assert!(extract_double_bracket_links("").is_empty());
    }

    #[test]
    fn handles_unclosed_brackets() {
        let content = "Unclosed [[link here";
        let links = extract_double_bracket_links(content);
        assert!(links.is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cd src-tauri && cargo test parser::links`
Expected: 6 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/parser/links.rs
git commit -m "feat(parser): double bracket link extractor with tests"
```

---

### Task 6: Attachment Extractor - Test

**Files:**
- Create: `src-tauri/src/parser/attachments.rs`

- [ ] **Step 1: Create src-tauri/src/parser/attachments.rs with test skeleton**

```rust
/// 从 Markdown 提取 ![[path|caption]] 形式的附件引用。
/// 返回 (path, caption) 列表。
pub fn extract_attachments(content: &str) -> Vec<(String, Option<String>)> {
    let mut attachments = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if i + 2 < bytes.len()
            && bytes[i] == b'!'
            && bytes[i + 1] == b'['
            && bytes[i + 2] == b'['
        {
            let mut j = i + 3;
            while j + 1 < bytes.len() && !(bytes[j] == b']' && bytes[j + 1] == b']') {
                j += 1;
            }

            if j + 1 < bytes.len() {
                let inner = &content[i + 3..j];
                if let Some((path, caption)) = inner.split_once('|') {
                    attachments.push((path.trim().to_string(), Some(caption.trim().to_string())));
                } else {
                    attachments.push((inner.trim().to_string(), None));
                }
                i = j + 2;
                continue;
            }
        }
        i += 1;
    }

    attachments
}

/// 根据文件扩展名判断附件类型
pub fn classify_attachment(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();

    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" => "image",
        "pdf" | "doc" | "docx" | "txt" | "md" | "epub" => "document",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_attachment_with_caption() {
        let content = "Image: ![[./diagrams/flow.png|流程图]]";
        let atts = extract_attachments(content);
        assert_eq!(atts, vec![("./diagrams/flow.png".to_string(), Some("流程图".to_string()))]);
    }

    #[test]
    fn extracts_attachment_without_caption() {
        let content = "Plain: ![[image.png]]";
        let atts = extract_attachments(content);
        assert_eq!(atts, vec![("image.png".to_string(), None)]);
    }

    #[test]
    fn classifies_image_extensions() {
        assert_eq!(classify_attachment("photo.png"), "image");
        assert_eq!(classify_attachment("photo.JPG"), "image");
        assert_eq!(classify_attachment("photo.svg"), "image");
    }

    #[test]
    fn classifies_document_extensions() {
        assert_eq!(classify_attachment("doc.pdf"), "document");
        assert_eq!(classify_attachment("notes.md"), "document");
    }

    #[test]
    fn classifies_unknown_extensions() {
        assert_eq!(classify_attachment("file.xyz"), "other");
        assert_eq!(classify_attachment("noext"), "other");
    }

    #[test]
    fn ignores_non_attachment_double_brackets() {
        let content = "Regular link [[entry]] here.";
        let atts = extract_attachments(content);
        assert!(atts.is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cd src-tauri && cargo test parser::attachments`
Expected: 6 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/parser/attachments.rs
git commit -m "feat(parser): attachment extractor with tests"
```

---

### Task 7: YAML Field Parser - Test

**Files:**
- Create: `src-tauri/src/parser/yaml_field.rs`

- [ ] **Step 1: Create src-tauri/src/parser/yaml_field.rs with test skeleton**

```rust
use serde::{Deserialize, Serialize};

/// YAML frontmatter 中的高级配置字段
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct YamlFields {
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub order: Option<i32>,
}

/// 从 Markdown 文件开头提取 --- 包裹的 YAML 块，返回解析结果。
/// 没有 YAML 块时返回 None。
pub fn extract_yaml_frontmatter(content: &str) -> Option<&str> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        return None;
    }

    let end_idx = lines[1..]
        .iter()
        .position(|l| l.trim() == "---")
        .map(|i| i + 1)?;

    Some(&content[..lines[..=end_idx].join("\n").len()])
}

/// 解析 YAML 字符串为 YamlFields
pub fn parse_yaml_fields(yaml: &str) -> Result<YamlFields, serde_yaml::Error> {
    serde_yaml::from_str(yaml)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_yaml_frontmatter() {
        let content = r#"---
color: "#5B8DEF"
shape: rect
---

# Content
"#;
        let yaml = extract_yaml_frontmatter(content).unwrap();
        assert!(yaml.starts_with("---"));
        assert!(yaml.contains("color"));
    }

    #[test]
    fn returns_none_when_no_frontmatter() {
        let content = "# Just a heading\n\nSome content.";
        assert!(extract_yaml_frontmatter(content).is_none());
    }

    #[test]
    fn parses_simple_yaml() {
        let yaml = "color: \"#FF0000\"\nshape: rect\npinned: true\norder: 5\n";
        let parsed = parse_yaml_fields(yaml).unwrap();

        assert_eq!(parsed.color, Some("#FF0000".to_string()));
        assert_eq!(parsed.shape, Some("rect".to_string()));
        assert_eq!(parsed.pinned, Some(true));
        assert_eq!(parsed.order, Some(5));
    }

    #[test]
    fn parses_partial_yaml() {
        let yaml = "color: \"#FF0000\"\n";
        let parsed = parse_yaml_fields(yaml).unwrap();
        assert_eq!(parsed.color, Some("#FF0000".to_string()));
        assert_eq!(parsed.shape, None);
    }

    #[test]
    fn returns_error_on_invalid_yaml() {
        let yaml = "color: : invalid";
        assert!(parse_yaml_fields(yaml).is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cd src-tauri && cargo test parser::yaml_field`
Expected: 5 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/parser/yaml_field.rs
git commit -m "feat(parser): YAML frontmatter parser with tests"
```

---

## Phase 3: Rust Core - Scanner

### Task 8: Scanner - Test and Implementation

**Files:**
- Create: `src-tauri/src/scanner.rs`

- [ ] **Step 1: Create src-tauri/src/scanner.rs with test skeleton**

```rust
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// 扫描根目录，找出所有条目文件夹。
/// 条目文件夹 = 含 content.md 的子目录。
pub fn find_entry_dirs(root: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();

    if !root.exists() {
        return entries;
    }

    for entry in WalkDir::new(root)
        .max_depth(3)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_dir() && path.join("content.md").exists() {
            entries.push(path.to_path_buf());
        }
    }

    entries
}

/// 提取条目的 ID（相对于 root 的目录名）。
pub fn entry_id_from_path(root: &Path, entry_path: &Path) -> Option<String> {
    entry_path
        .strip_prefix(root)
        .ok()
        .and_then(|p| p.components().next())
        .map(|c| c.as_os_str().to_string_lossy().to_string())
}

/// 收集目录下所有文件路径（递归）。
pub fn collect_files(dir: &Path) -> HashSet<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn finds_direct_entry_folders() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::create_dir(root.join("topic-a")).unwrap();
        fs::write(root.join("topic-a").join("content.md"), "# A").unwrap();

        fs::create_dir(root.join("topic-b")).unwrap();
        fs::write(root.join("topic-b").join("content.md"), "# B").unwrap();

        let entries = find_entry_dirs(root);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn ignores_folders_without_content_md() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::create_dir(root.join("valid")).unwrap();
        fs::write(root.join("valid").join("content.md"), "# V").unwrap();

        fs::create_dir(root.join("invalid")).unwrap();
        fs::write(root.join("invalid").join("notes.md"), "# N").unwrap();

        let entries = find_entry_dirs(root);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].ends_with("valid"));
    }

    #[test]
    fn returns_empty_when_root_missing() {
        let entries = find_entry_dirs(Path::new("/nonexistent/path"));
        assert!(entries.is_empty());
    }

    #[test]
    fn extracts_entry_id() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let entry_path = root.join("react-hooks");
        fs::create_dir(&entry_path).unwrap();

        let id = entry_id_from_path(root, &entry_path).unwrap();
        assert_eq!(id, "react-hooks");
    }
}
```

- [ ] **Step 2: Add tempfile dev dependency**

Modify `src-tauri/Cargo.toml`, add to `[dev-dependencies]`:
```
tempfile = "3"
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cd src-tauri && cargo test scanner`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/scanner.rs
git commit -m "feat(scanner): directory scanner with tests"
```

---

### Task 9: Entry Aggregation - Test and Implementation

**Files:**
- Create: `src-tauri/src/entry.rs`

- [ ] **Step 1: Create src-tauri/src/entry.rs with test skeleton**

```rust
use std::path::{Path, PathBuf};

use crate::parser::{
    attachments::classify_attachment, attachments::extract_attachments, comments::parse_metadata_comments,
    links::extract_double_bracket_links, yaml_field::extract_yaml_frontmatter, yaml_field::parse_yaml_fields,
    yaml_field::YamlFields,
};

#[derive(Debug, Clone)]
pub struct RawEntry {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub group: Option<String>,
    pub path: PathBuf,
    pub content_path: PathBuf,
    pub attachments: Vec<RawAttachment>,
    pub linked_targets: Vec<String>,
    pub yaml: Option<YamlFields>,
    pub has_content_md: bool,
}

#[derive(Debug, Clone)]
pub struct RawAttachment {
    pub path: String,
    pub caption: Option<String>,
    pub attachment_type: String,
}

/// 从 entry 目录读取并解析完整 Entry 数据。
/// 如果目录没有 content.md，返回 has_content_md = false 的最小 Entry。
pub fn read_entry(entry_dir: &Path) -> RawEntry {
    let id = entry_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let content_path = entry_dir.join("content.md");
    let has_content_md = content_path.exists();

    if !has_content_md {
        return RawEntry {
            id: id.clone(),
            title: id.clone(),
            tags: vec![],
            group: None,
            path: entry_dir.to_path_buf(),
            content_path,
            attachments: vec![],
            linked_targets: vec![],
            yaml: None,
            has_content_md: false,
        };
    }

    let content = std::fs::read_to_string(&content_path).unwrap_or_default();

    let metadata = parse_metadata_comments(&content);
    let title = metadata
        .get("title")
        .cloned()
        .unwrap_or_else(|| id.clone());
    let tags = metadata
        .get("tags")
        .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
        .unwrap_or_default();
    let group = metadata.get("group").cloned();

    let attachments: Vec<RawAttachment> = extract_attachments(&content)
        .into_iter()
        .map(|(path, caption)| RawAttachment {
            attachment_type: classify_attachment(&path).to_string(),
            path,
            caption,
        })
        .collect();

    let linked_targets = extract_double_bracket_links(&content);

    let yaml = extract_yaml_frontmatter(&content)
        .and_then(|yaml| parse_yaml_fields(yaml).ok());

    RawEntry {
        id,
        title,
        tags,
        group,
        path: entry_dir.to_path_buf(),
        content_path,
        attachments,
        linked_targets,
        yaml,
        has_content_md: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn reads_complete_entry() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let entry_dir = root.join("react-hooks");
        fs::create_dir(&entry_dir).unwrap();

        let content = r#"<!-- @synapse-id: react-hooks -->
<!-- @synapse-title: React Hooks 深入理解 -->
<!-- @synapse-tags: react, frontend -->
<!-- @synapse-group: frontend -->

---
color: "#5B8DEF"
---

# 正文

参考 [[state-management]] 和 [[render-flow#block-1]]。
图片：![[./diagrams/flow.png|流程图]]
"#;
        fs::write(entry_dir.join("content.md"), content).unwrap();

        let entry = read_entry(&entry_dir);

        assert_eq!(entry.id, "react-hooks");
        assert_eq!(entry.title, "React Hooks 深入理解");
        assert_eq!(entry.tags, vec!["react", "frontend"]);
        assert_eq!(entry.group, Some("frontend".to_string()));
        assert_eq!(entry.linked_targets, vec!["state-management", "render-flow"]);
        assert_eq!(entry.attachments.len(), 1);
        assert_eq!(entry.attachments[0].path, "./diagrams/flow.png");
        assert_eq!(entry.attachments[0].caption, Some("流程图".to_string()));
        assert_eq!(entry.attachments[0].attachment_type, "image");
        assert!(entry.yaml.is_some());
        assert_eq!(entry.yaml.unwrap().color, Some("#5B8DEF".to_string()));
        assert!(entry.has_content_md);
    }

    #[test]
    fn handles_missing_content_md() {
        let dir = tempdir().unwrap();
        let entry_dir = dir.path().join("broken");
        fs::create_dir(&entry_dir).unwrap();

        let entry = read_entry(&entry_dir);

        assert_eq!(entry.id, "broken");
        assert_eq!(entry.title, "broken");
        assert!(!entry.has_content_md);
        assert!(entry.attachments.is_empty());
        assert!(entry.linked_targets.is_empty());
    }

    #[test]
    fn uses_id_when_title_missing() {
        let dir = tempdir().unwrap();
        let entry_dir = dir.path().join("no-title");
        fs::create_dir(&entry_dir).unwrap();
        fs::write(entry_dir.join("content.md"), "# Just content").unwrap();

        let entry = read_entry(&entry_dir);

        assert_eq!(entry.id, "no-title");
        assert_eq!(entry.title, "no-title");
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cd src-tauri && cargo test entry`
Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/entry.rs
git commit -m "feat(entry): entry aggregation from disk with tests"
```

---

## Phase 4: Rust Core - Layout Store

### Task 10: Layout Store - Test and Implementation

**Files:**
- Create: `src-tauri/src/layout_store.rs`

- [ ] **Step 1: Create src-tauri/src/layout_store.rs with test skeleton**

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const LAYOUT_FILENAME: &str = ".synapse-layout.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Layout {
    pub positions: HashMap<String, NodePosition>,
}

impl Layout {
    pub fn get(&self, id: &str) -> Option<&NodePosition> {
        self.positions.get(id)
    }

    pub fn set(&mut self, id: impl Into<String>, x: f64, y: f64) {
        self.positions.insert(id.into(), NodePosition { x, y });
    }
}

/// 从 kb 根目录加载布局；如果不存在则返回空布局。
pub fn load_layout(kb_root: &Path) -> Layout {
    let layout_path = kb_root.join(LAYOUT_FILENAME);
    if !layout_path.exists() {
        return Layout::default();
    }

    std::fs::read_to_string(&layout_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 保存布局到 kb 根目录的 .synapse-layout.json。
pub fn save_layout(kb_root: &Path, layout: &Layout) -> std::io::Result<()> {
    let layout_path = kb_root.join(LAYOUT_FILENAME);
    let json = serde_json::to_string_pretty(layout)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(layout_path, json)
}

/// 更新单个节点的位置并保存。
pub fn update_position(kb_root: &Path, entry_id: &str, x: f64, y: f64) -> std::io::Result<()> {
    let mut layout = load_layout(kb_root);
    layout.set(entry_id, x, y);
    save_layout(kb_root, &layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn returns_empty_when_no_file() {
        let dir = tempdir().unwrap();
        let layout = load_layout(dir.path());
        assert!(layout.positions.is_empty());
    }

    #[test]
    fn round_trips_positions() {
        let dir = tempdir().unwrap();

        let mut layout = Layout::default();
        layout.set("entry-a", 100.0, 200.0);
        layout.set("entry-b", -50.0, 300.0);

        save_layout(dir.path(), &layout).unwrap();

        let loaded = load_layout(dir.path());
        assert_eq!(loaded.get("entry-a").unwrap().x, 100.0);
        assert_eq!(loaded.get("entry-a").unwrap().y, 200.0);
        assert_eq!(loaded.get("entry-b").unwrap().x, -50.0);
        assert_eq!(loaded.get("entry-b").unwrap().y, 300.0);
    }

    #[test]
    fn updates_single_position() {
        let dir = tempdir().unwrap();

        update_position(dir.path(), "first", 1.0, 2.0).unwrap();
        update_position(dir.path(), "second", 3.0, 4.0).unwrap();

        let loaded = load_layout(dir.path());
        assert_eq!(loaded.get("first").unwrap().x, 1.0);
        assert_eq!(loaded.get("second").unwrap().y, 4.0);
    }

    #[test]
    fn falls_back_to_empty_on_corrupt_file() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(LAYOUT_FILENAME), "invalid json{").unwrap();

        let layout = load_layout(dir.path());
        assert!(layout.positions.is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cd src-tauri && cargo test layout_store`
Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/layout_store.rs
git commit -m "feat(layout): layout store with tests"
```

---

## Phase 5: Rust Core - SQLite Index

### Task 11: SQLite Index - Schema and CRUD Tests

**Files:**
- Create: `src-tauri/src/index.rs`, `src-tauri/src/paths.rs`

- [ ] **Step 1: Create src-tauri/src/paths.rs**

```rust
use std::path::PathBuf;

/// 获取应用数据目录（跨平台）。
/// macOS: ~/Library/Application Support/synapse-kb
/// Linux: ~/.local/share/synapse-kb
/// Windows: %APPDATA%/synapse-kb
pub fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .map(|p| p.join("synapse-kb"))
        .unwrap_or_else(|| PathBuf::from(".synapse-kb"))
}

/// 获取 SQLite 索引数据库文件路径。
pub fn index_db_path() -> PathBuf {
    app_data_dir().join("index.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_in_app_data() {
        let dir = app_data_dir();
        assert!(dir.to_string_lossy().contains("synapse-kb"));
    }
}
```

- [ ] **Step 2: Create src-tauri/src/index.rs with test skeleton**

```rust
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::entry::{RawAttachment, RawEntry};
use crate::paths::index_db_path;
use crate::scanner::find_entry_dirs;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS entries (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    path TEXT NOT NULL,
    group_name TEXT,
    has_content_md INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS tags (
    entry_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (entry_id, tag),
    FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS attachments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_id TEXT NOT NULL,
    path TEXT NOT NULL,
    caption TEXT,
    attachment_type TEXT NOT NULL,
    FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS relations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_id TEXT NOT NULL,
    to_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    note TEXT,
    UNIQUE(from_id, to_id, relation_type),
    FOREIGN KEY (from_id) REFERENCES entries(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_relations_from ON relations(from_id);
CREATE INDEX IF NOT EXISTS idx_relations_to ON relations(to_id);
CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags(tag);
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEntry {
    pub id: String,
    pub title: String,
    pub path: String,
    pub group: Option<String>,
    pub tags: Vec<String>,
    pub attachments: Vec<StoredAttachment>,
    pub relations: Vec<StoredRelation>,
    pub has_content_md: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAttachment {
    pub path: String,
    pub caption: Option<String>,
    pub attachment_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRelation {
    pub from_id: String,
    pub to_id: String,
    pub relation_type: String,
    pub note: Option<String>,
}

pub struct Index {
    conn: Connection,
}

impl Index {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn open_default() -> rusqlite::Result<Self> {
        Self::open(&index_db_path())
    }

    pub fn upsert_entry(&self, entry: &RawEntry) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO entries (id, title, path, group_name, has_content_md) VALUES (?, ?, ?, ?, ?)",
            params![entry.id, entry.title, entry.path.to_string_lossy(), entry.group, entry.has_content_md as i32],
        )?;

        self.conn.execute("DELETE FROM tags WHERE entry_id = ?", params![entry.id])?;
        for tag in &entry.tags {
            self.conn.execute(
                "INSERT INTO tags (entry_id, tag) VALUES (?, ?)",
                params![entry.id, tag],
            )?;
        }

        self.conn.execute("DELETE FROM attachments WHERE entry_id = ?", params![entry.id])?;
        for att in &entry.attachments {
            self.conn.execute(
                "INSERT INTO attachments (entry_id, path, caption, attachment_type) VALUES (?, ?, ?, ?)",
                params![entry.id, att.path, att.caption, att.attachment_type],
            )?;
        }

        Ok(())
    }

    pub fn delete_entry(&self, id: &str) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM entries WHERE id = ?", params![id])?;
        Ok(())
    }

    pub fn add_relation(
        &self,
        from_id: &str,
        to_id: &str,
        relation_type: &str,
        note: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO relations (from_id, to_id, relation_type, note) VALUES (?, ?, ?, ?)",
            params![from_id, to_id, relation_type, note],
        )?;
        Ok(())
    }

    pub fn remove_relation(
        &self,
        from_id: &str,
        to_id: &str,
        relation_type: &str,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM relations WHERE from_id = ? AND to_id = ? AND relation_type = ?",
            params![from_id, to_id, relation_type],
        )?;
        Ok(())
    }

    pub fn get_all_entries(&self) -> rusqlite::Result<Vec<StoredEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, path, group_name, has_content_md FROM entries ORDER BY id",
        )?;

        let entries_iter = stmt.query_map([], |row| {
            Ok(StoredEntry {
                id: row.get(0)?,
                title: row.get(1)?,
                path: row.get(2)?,
                group: row.get(3)?,
                tags: vec![],
                attachments: vec![],
                relations: vec![],
                has_content_md: row.get::<_, i32>(4)? != 0,
            })
        })?;

        let mut entries: Vec<StoredEntry> = entries_iter.collect::<rusqlite::Result<Vec<_>>>()?;

        for entry in &mut entries {
            entry.tags = self.get_tags(&entry.id)?;
            entry.attachments = self.get_attachments(&entry.id)?;
            entry.relations = self.get_relations(&entry.id)?;
        }

        Ok(entries)
    }

    fn get_tags(&self, entry_id: &str) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT tag FROM tags WHERE entry_id = ?")?;
        let rows = stmt.query_map(params![entry_id], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    fn get_attachments(&self, entry_id: &str) -> rusqlite::Result<Vec<StoredAttachment>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, caption, attachment_type FROM attachments WHERE entry_id = ?",
        )?;
        let rows = stmt.query_map(params![entry_id], |row| {
            Ok(StoredAttachment {
                path: row.get(0)?,
                caption: row.get(1)?,
                attachment_type: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    fn get_relations(&self, entry_id: &str) -> rusqlite::Result<Vec<StoredRelation>> {
        let mut stmt = self.conn.prepare(
            "SELECT from_id, to_id, relation_type, note FROM relations WHERE from_id = ? OR to_id = ?",
        )?;
        let rows = stmt.query_map(params![entry_id, entry_id], |row| {
            Ok(StoredRelation {
                from_id: row.get(0)?,
                to_id: row.get(1)?,
                relation_type: row.get(2)?,
                note: row.get(3)?,
            })
        })?;
        rows.collect()
    }

    /// 全量重建索引（从 kb 根目录扫描）。
    pub fn rebuild(&self, kb_root: &Path) -> rusqlite::Result<usize> {
        self.conn.execute("DELETE FROM entries", [])?;
        self.conn.execute("DELETE FROM relations", [])?;

        let mut count = 0;
        for entry_dir in find_entry_dirs(kb_root) {
            let entry = crate::entry::read_entry(&entry_dir);
            self.upsert_entry(&entry)?;

            for target in &entry.linked_targets {
                self.add_relation(&entry.id, target, "custom", None)?;
            }
            count += 1;
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::RawAttachment;
    use std::fs;
    use tempfile::tempdir;

    fn make_entry(id: &str, title: &str, path: &Path) -> RawEntry {
        RawEntry {
            id: id.to_string(),
            title: title.to_string(),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            group: Some("group-a".to_string()),
            path: path.to_path_buf(),
            content_path: path.join("content.md"),
            attachments: vec![RawAttachment {
                path: "./img.png".to_string(),
                caption: Some("cap".to_string()),
                attachment_type: "image".to_string(),
            }],
            linked_targets: vec![],
            yaml: None,
            has_content_md: true,
        }
    }

    #[test]
    fn opens_and_creates_schema() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let index = Index::open(&db_path).unwrap();

        // schema should be created
        assert!(db_path.exists());
        drop(index);
    }

    #[test]
    fn upserts_and_retrieves_entry() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Index::open(&db_path).unwrap();

        let entry = make_entry("foo", "Foo Title", dir.path());
        index.upsert_entry(&entry).unwrap();

        let entries = index.get_all_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "foo");
        assert_eq!(entries[0].title, "Foo Title");
        assert_eq!(entries[0].group, Some("group-a".to_string()));
        assert_eq!(entries[0].tags, vec!["tag1", "tag2"]);
        assert_eq!(entries[0].attachments.len(), 1);
        assert_eq!(entries[0].attachments[0].path, "./img.png");
    }

    #[test]
    fn upsert_replaces_tags_and_attachments() {
        let dir = tempdir().unwrap();
        let index = Index::open(&dir.path()).unwrap();

        let mut entry = make_entry("foo", "Foo", dir.path());
        entry.tags = vec!["a".to_string()];
        index.upsert_entry(&entry).unwrap();

        entry.tags = vec!["b".to_string(), "c".to_string()];
        index.upsert_entry(&entry).unwrap();

        let entries = index.get_all_entries().unwrap();
        assert_eq!(entries[0].tags, vec!["b", "c"]);
    }

    #[test]
    fn adds_and_removes_relations() {
        let dir = tempdir().unwrap();
        let index = Index::open(&dir.path()).unwrap();

        index
            .add_relation("a", "b", "reference", Some("see also"))
            .unwrap();
        index.add_relation("a", "d", "derived", None).unwrap();

        index.remove_relation("a", "b", "reference").unwrap();

        let entries = index.get_all_entries().unwrap();
        // entries table empty, relations persist only if entries exist
        // For this test, we just verify no panic and the relation can be added
        let _ = entries;
    }

    #[test]
    fn rebuild_scans_kb_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // create one entry on disk
        let entry_path = root.join("topic");
        fs::create_dir(&entry_path).unwrap();
        fs::write(
            entry_path.join("content.md"),
            "<!-- @synapse-id: topic -->\n<!-- @synapse-title: Topic -->\n",
        )
        .unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        let count = index.rebuild(root).unwrap();

        assert_eq!(count, 1);
        let entries = index.get_all_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "topic");
    }
}
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cd src-tauri && cargo test index`
Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/paths.rs src-tauri/src/index.rs
git commit -m "feat(index): SQLite index with schema and CRUD"
```

---

## Phase 6: Rust Core - Local Server

### Task 12: Local HTTP Server with axum

**Files:**
- Create: `src-tauri/src/local_server.rs`

- [ ] **Step 1: Create src-tauri/src/local_server.rs**

```rust
use axum::{extract::Path, extract::State, http::StatusCode, response::Json, routing::get, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::index::{Index, StoredEntry, StoredRelation};

pub const SERVER_PORT: u16 = 19181;

#[derive(Clone)]
pub struct AppState {
    pub index: Arc<RwLock<Index>>,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}

impl ApiError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            error: msg.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateRelationRequest {
    pub from_id: String,
    pub to_id: String,
    pub relation_type: String,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub data: T,
}

async fn list_entries(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<StoredEntry>>>, (StatusCode, Json<ApiError>)> {
    let index = state.index.read().await;
    let entries = index
        .get_all_entries()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(e.to_string()))))?;
    Ok(Json(ApiResponse { data: entries }))
}

async fn get_entry(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StoredEntry>>, (StatusCode, Json<ApiError>)> {
    let index = state.index.read().await;
    let entries = index
        .get_all_entries()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(e.to_string())))?;
    let entry = entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::new("Entry not found"))))?;
    Ok(Json(ApiResponse { data: entry }))
}

async fn create_relation(
    State(state): State<AppState>,
    Json(req): Json<CreateRelationRequest>,
) -> Result<Json<ApiResponse<StoredRelation>>, (StatusCode, Json<ApiError>)> {
    let index = state.index.write().await;
    index
        .add_relation(&req.from_id, &req.to_id, &req.relation_type, req.note.as_deref())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(e.to_string())))?;

    Ok(Json(ApiResponse {
        data: StoredRelation {
            from_id: req.from_id,
            to_id: req.to_id,
            relation_type: req.relation_type,
            note: req.note,
        },
    }))
}

async fn delete_relation(
    State(state): State<AppState>,
    Path((from, to, rel_type)): Path<(String, String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let index = state.index.write().await;
    index
        .remove_relation(&from, &to, &rel_type)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(e.to_string())))?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/entries", get(list_entries))
        .route("/api/entries/:id", get(get_entry))
        .route("/api/relations", axum::routing::post(create_relation))
        .route(
            "/api/relations/:from/:to/:rel_type",
            axum::routing::delete(delete_relation),
        )
        .with_state(state)
}

/// 启动 axum 服务，监听 127.0.0.1:SERVER_PORT。
pub async fn serve(state: AppState) -> anyhow::Result<()> {
    let app = router(state);
    let addr = SocketAddr::from(([127, 0, 0, 1], SERVER_PORT));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Local server listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::RawEntry;
    use crate::index::Index;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::fs;
    use tempfile::tempdir;
    use tower::ServiceExt;

    fn make_state() -> (AppState, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let index = Index::open(&dir.path().join("test.db")).unwrap();

        let entry = RawEntry {
            id: "topic-a".to_string(),
            title: "Topic A".to_string(),
            tags: vec!["tag1".to_string()],
            group: None,
            path: dir.path().join("topic-a"),
            content_path: dir.path().join("topic-a").join("content.md"),
            attachments: vec![],
            linked_targets: vec![],
            yaml: None,
            has_content_md: true,
        };
        index.upsert_entry(&entry).unwrap();

        let state = AppState {
            index: Arc::new(RwLock::new(index)),
        };
        (state, dir)
    }

    #[tokio::test]
    async fn list_entries_returns_ok() {
        let (state, _dir) = make_state();
        let app = router(state);

        let response = app
            .oneshot(Request::builder().uri("/api/entries").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn get_entry_returns_404_for_missing() {
        let (state, _dir) = make_state();
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/entries/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_entry_returns_200_for_existing() {
        let (state, _dir) = make_state();
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/entries/topic-a")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn create_relation_returns_ok() {
        let (state, _dir) = make_state();
        let app = router(state);

        let body = serde_json::to_string(&CreateRelationRequest {
            from_id: "topic-a".to_string(),
            to_id: "topic-b".to_string(),
            relation_type: "reference".to_string(),
            note: None,
        })
        .unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/relations")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
```

- [ ] **Step 2: Add tower dev dependency**

Modify `src-tauri/Cargo.toml`, add to `[dev-dependencies]`:
```
tower = { version = "0.5", features = ["util"] }
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cd src-tauri && cargo test local_server`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/local_server.rs
git commit -m "feat(server): axum local HTTP server with tests"
```

---

## Phase 7: Tauri Integration

### Task 13: Tauri Commands Bridge

**Files:**
- Create: `src-tauri/src/commands.rs`, `src-tauri/src/error.rs`, `src-tauri/src/state.rs`, `src-tauri/src/lib.rs`

- [ ] **Step 1: Create src-tauri/src/error.rs**

```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Other: {0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
```

- [ ] **Step 2: Create src-tauri/src/state.rs**

```rust
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::index::Index;

#[derive(Clone)]
pub struct AppState {
    pub kb_root: Arc<RwLock<Option<PathBuf>>>,
    pub index: Arc<RwLock<Index>>,
}
```

- [ ] **Step 3: Create src-tauri/src/commands.rs**

```rust
use crate::error::AppResult;
use crate::index::StoredEntry;
use crate::state::AppState;
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub async fn set_kb_root(state: State<'_, AppState>, path: String) -> AppResult<()> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(crate::error::AppError::Other(format!(
            "Path does not exist: {}",
            path.display()
        )));
    }

    {
        let index = state.index.read().await;
        index.rebuild(&path)?;
    }

    *state.kb_root.write().await = Some(path);
    Ok(())
}

#[tauri::command]
pub async fn get_entries(state: State<'_, AppState>) -> AppResult<Vec<StoredEntry>> {
    let index = state.index.read().await;
    let entries = index.get_all_entries()?;
    Ok(entries)
}

#[tauri::command]
pub async fn rebuild_index(state: State<'_, AppState>) -> AppResult<usize> {
    let root = state
        .kb_root
        .read()
        .await
        .clone()
        .ok_or_else(|| crate::error::AppError::Other("KB root not set".to_string()))?;
    let index = state.index.read().await;
    let count = index.rebuild(&root)?;
    Ok(count)
}
```

- [ ] **Step 4: Create src-tauri/src/lib.rs**

```rust
mod commands;
mod entry;
mod error;
mod index;
mod layout_store;
mod local_server;
mod parser;
mod paths;
mod scanner;
mod state;
mod watcher;

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::index::Index;
use crate::local_server::{serve as serve_local, AppState as ServerState};
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let index = Index::open_default().expect("Failed to open index");

    let kb_root: Arc<RwLock<Option<PathBuf>>> = Arc::new(RwLock::new(None));
    let index_arc = Arc::new(RwLock::new(index));

    let app_state = AppState {
        kb_root: kb_root.clone(),
        index: index_arc.clone(),
    };

    let server_state = ServerState {
        index: index_arc.clone(),
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::set_kb_root,
            commands::get_entries,
            commands::rebuild_index,
        ])
        .setup(move |_app| {
            tokio::spawn(async move {
                if let Err(e) = serve_local(server_state).await {
                    tracing::error!("Local server error: {}", e);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors. There may be unused warnings for `watcher` module.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/
git commit -m "feat(tauri): integrate backend with Tauri commands"
```

---

## Phase 8: Frontend - API Client and Store

### Task 14: API Client

**Files:**
- Create: `src/lib/api.ts`

- [ ] **Step 1: Create src/lib/api.ts**

```typescript
import type { Entry, Relation } from '@shared/types';

const BASE_URL = 'http://127.0.0.1:19181';

interface ApiResponse<T> {
  data: T;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${BASE_URL}${path}`, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...init?.headers,
    },
  });

  if (!response.ok) {
    const error = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(error.error || response.statusText);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  const body: ApiResponse<T> = await response.json();
  return body.data;
}

export const api = {
  listEntries: () => request<Entry[]>('/api/entries'),

  getEntry: (id: string) => request<Entry>(`/api/entries/${encodeURIComponent(id)}`),

  createRelation: (relation: Omit<Relation, 'id'>) =>
    request<Relation>('/api/relations', {
      method: 'POST',
      body: JSON.stringify(relation),
    }),

  deleteRelation: (fromId: string, toId: string, type: string) =>
    request<void>(
      `/api/relations/${encodeURIComponent(fromId)}/${encodeURIComponent(toId)}/${encodeURIComponent(type)}`,
      { method: 'DELETE' }
    ),
};

export const isTauriEnvironment = (): boolean => {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
};
```

- [ ] **Step 2: Verify build**

Run: `npm run build`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/lib/api.ts
git commit -m "feat(frontend): API client for backend communication"
```

---

### Task 15: App Store with Zustand

**Files:**
- Create: `src/store/useAppStore.ts`

- [ ] **Step 1: Create src/store/useAppStore.ts**

```typescript
import { create } from 'zustand';
import type { Entry, Relation, RelationType } from '@shared/types';
import { api } from '@/lib/api';

interface AppState {
  entries: Entry[];
  selectedEntryId: string | null;
  relationFilter: Set<RelationType>;
  isLoading: boolean;
  error: string | null;

  loadEntries: () => Promise<void>;
  selectEntry: (id: string | null) => void;
  toggleRelationType: (type: RelationType) => void;
  createRelation: (fromId: string, toId: string, type: RelationType, note?: string) => Promise<void>;
  deleteRelation: (fromId: string, toId: string, type: RelationType) => Promise<void>;
}

export const useAppStore = create<AppState>((set, get) => ({
  entries: [],
  selectedEntryId: null,
  relationFilter: new Set<RelationType>(['reference', 'derived', 'contradicts', 'supersedes', 'extends']),
  isLoading: false,
  error: null,

  loadEntries: async () => {
    set({ isLoading: true, error: null });
    try {
      const entries = await api.listEntries();
      set({ entries, isLoading: false });
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },

  selectEntry: (id) => set({ selectedEntryId: id }),

  toggleRelationType: (type) => {
    const filter = new Set(get().relationFilter);
    if (filter.has(type)) {
      filter.delete(type);
    } else {
      filter.add(type);
    }
    set({ relationFilter: filter });
  },

  createRelation: async (fromId, toId, type, note) => {
    try {
      await api.createRelation({ fromId, toId, type, note });
      await get().loadEntries();
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  deleteRelation: async (fromId, toId, type) => {
    try {
      await api.deleteRelation(fromId, toId, type);
      await get().loadEntries();
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },
}));
```

- [ ] **Step 2: Verify build**

Run: `npm run build`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/store/useAppStore.ts
git commit -m "feat(frontend): Zustand app store"
```

---

## Phase 9: Frontend - UI Components

### Task 16: App Shell with Sidebar

**Files:**
- Modify: `src/App.tsx`
- Create: `src/components/Sidebar.tsx`

- [ ] **Step 1: Create src/components/Sidebar.tsx**

```tsx
import { useAppStore } from '@/store/useAppStore';

export function Sidebar() {
  const entries = useAppStore((s) => s.entries);
  const selectedEntryId = useAppStore((s) => s.selectedEntryId);
  const selectEntry = useAppStore((s) => s.selectEntry);
  const loadEntries = useAppStore((s) => s.loadEntries);

  return (
    <aside className="flex w-64 flex-col border-r border-gray-200 bg-white">
      <div className="border-b border-gray-200 p-4">
        <button
          onClick={() => loadEntries()}
          className="w-full rounded-md bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600"
        >
          Reload
        </button>
      </div>
      <div className="flex-1 overflow-y-auto p-2">
        <ul className="space-y-1">
          {entries.map((entry) => (
            <li key={entry.id}>
              <button
                onClick={() => selectEntry(entry.id)}
                className={`w-full rounded-md px-3 py-2 text-left text-sm transition ${
                  selectedEntryId === entry.id
                    ? 'bg-blue-50 font-medium text-blue-700'
                    : 'text-gray-700 hover:bg-gray-100'
                }`}
              >
                {entry.title}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </aside>
  );
}
```

- [ ] **Step 2: Modify src/App.tsx**

```tsx
import { useEffect } from 'react';
import { Sidebar } from '@/components/Sidebar';
import { useAppStore } from '@/store/useAppStore';

export default function App() {
  const loadEntries = useAppStore((s) => s.loadEntries);
  const error = useAppStore((s) => s.error);
  const isLoading = useAppStore((s) => s.isLoading);

  useEffect(() => {
    loadEntries();
  }, [loadEntries]);

  return (
    <div className="flex h-screen w-screen bg-gray-50 text-gray-900">
      <Sidebar />
      <main className="flex-1">
        {error && (
          <div className="m-4 rounded-md bg-red-50 p-3 text-sm text-red-700">
            {error}
          </div>
        )}
        {isLoading && (
          <div className="m-4 text-sm text-gray-500">Loading...</div>
        )}
      </main>
    </div>
  );
}
```

- [ ] **Step 3: Verify build**

Run: `npm run build`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/App.tsx src/components/Sidebar.tsx
git commit -m "feat(frontend): app shell with sidebar"
```

---

### Task 17: MapView with React Flow

**Files:**
- Create: `src/views/MapView.tsx`, `src/components/NodeCard.tsx`

- [ ] **Step 1: Create src/components/NodeCard.tsx**

```tsx
import { Handle, NodeProps, Position } from 'reactflow';

export interface NodeCardData {
  title: string;
  group?: string;
  hasContentMd: boolean;
}

export function NodeCard({ data, selected }: NodeProps<NodeCardData>) {
  return (
    <div
      className={`min-w-[160px] rounded-lg border bg-white px-3 py-2 shadow-sm transition ${
        selected
          ? 'border-blue-500 ring-2 ring-blue-200'
          : 'border-gray-200 hover:border-gray-300'
      }`}
    >
      <Handle type="target" position={Position.Top} className="!bg-gray-300" />
      <div className="flex items-center gap-2">
        {!data.hasContentMd && (
          <span className="text-amber-500" title="Missing content.md">
            ⚠
          </span>
        )}
        <span className="text-sm font-medium text-gray-900">{data.title}</span>
      </div>
      {data.group && (
        <div className="mt-1 text-xs text-gray-500">{data.group}</div>
      )}
      <Handle type="source" position={Position.Bottom} className="!bg-gray-300" />
    </div>
  );
}
```

- [ ] **Step 2: Create src/views/MapView.tsx**

```tsx
import { useCallback, useEffect, useMemo } from 'react';
import ReactFlow, {
  Background,
  Controls,
  Edge,
  Node,
  NodeChange,
  applyNodeChanges,
} from 'reactflow';
import 'reactflow/dist/style.css';

import { useAppStore } from '@/store/useAppStore';
import { NodeCard, NodeCardData } from '@/components/NodeCard';
import type { RelationType } from '@shared/types';

const RELATION_COLORS: Record<RelationType, string> = {
  reference: '#3B82F6',
  derived: '#10B981',
  contradicts: '#EF4444',
  supersedes: '#F97316',
  extends: '#A855F7',
  custom: '#6B7280',
};

const nodeTypes = { card: NodeCard };

const DEFAULT_LAYOUT: Record<string, { x: number; y: number }> = {};

export function MapView() {
  const entries = useAppStore((s) => s.entries);
  const selectedEntryId = useAppStore((s) => s.selectedEntryId);
  const selectEntry = useAppStore((s) => s.selectEntry);
  const relationFilter = useAppStore((s) => s.relationFilter);

  const nodes: Node<NodeCardData>[] = useMemo(() => {
    return entries.map((entry, i) => {
      const fallback = DEFAULT_LAYOUT[entry.id] || {
        x: (i % 5) * 220,
        y: Math.floor(i / 5) * 140,
      };
      return {
        id: entry.id,
        type: 'card',
        position: fallback,
        data: {
          title: entry.title,
          group: entry.group,
          hasContentMd: entry.hasContentMd,
        },
      };
    });
  }, [entries]);

  const edges: Edge[] = useMemo(() => {
    const entryIds = new Set(entries.map((e) => e.id));
    const result: Edge[] = [];

    for (const entry of entries) {
      for (const rel of entry.relations) {
        if (!relationFilter.has(rel.type as RelationType)) continue;
        if (!entryIds.has(rel.toId)) continue;

        result.push({
          id: `${entry.id}-${rel.toId}-${rel.type}`,
          source: entry.id,
          target: rel.toId,
          label: rel.type,
          style: {
            stroke: RELATION_COLORS[rel.type as RelationType] || RELATION_COLORS.custom,
            strokeWidth: 1.5,
          },
          labelStyle: { fontSize: 10, fill: '#6B7280' },
          animated: rel.type === 'derived',
        });
      }
    }
    return result;
  }, [entries, relationFilter]);

  const onNodesChange = useCallback(
    (changes: NodeChange[]) => {
      // Note: position changes are intentionally not persisted in MVP
      // This can be extended to call backend update_position later
      return changes;
    },
    []
  );

  const onNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      selectEntry(node.id);
    },
    [selectEntry]
  );

  if (entries.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-gray-500">
        No entries yet. Configure a knowledge base root in Settings.
      </div>
    );
  }

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      nodeTypes={nodeTypes}
      onNodesChange={onNodesChange}
      onNodeClick={onNodeClick}
      fitView
      minZoom={0.1}
      maxZoom={2}
      proOptions={{ hideAttribution: true }}
    >
      <Background gap={16} size={1} color="#E5E7EB" />
      <Controls />
    </ReactFlow>
  );
}
```

- [ ] **Step 3: Modify src/App.tsx to include MapView**

```tsx
import { useEffect } from 'react';
import { Sidebar } from '@/components/Sidebar';
import { MapView } from '@/views/MapView';
import { useAppStore } from '@/store/useAppStore';

export default function App() {
  const loadEntries = useAppStore((s) => s.loadEntries);
  const error = useAppStore((s) => s.error);
  const isLoading = useAppStore((s) => s.isLoading);

  useEffect(() => {
    loadEntries();
  }, [loadEntries]);

  return (
    <div className="flex h-screen w-screen bg-gray-50 text-gray-900">
      <Sidebar />
      <main className="relative flex-1">
        {error && (
          <div className="absolute left-4 right-4 top-4 z-10 rounded-md bg-red-50 p-3 text-sm text-red-700 shadow-sm">
            {error}
          </div>
        )}
        {isLoading ? (
          <div className="flex h-full items-center justify-center text-sm text-gray-500">
            Loading...
          </div>
        ) : (
          <MapView />
        )}
      </main>
    </div>
  );
}
```

- [ ] **Step 4: Verify build**

Run: `npm run build`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/views/MapView.tsx src/components/NodeCard.tsx src/App.tsx
git commit -m "feat(frontend): map view with React Flow and custom nodes"
```

---

### Task 18: Detail Panel

**Files:**
- Create: `src/views/DetailPanel.tsx`

- [ ] **Step 1: Create src/views/DetailPanel.tsx**

```tsx
import { useAppStore } from '@/store/useAppStore';

export function DetailPanel() {
  const selectedEntryId = useAppStore((s) => s.selectedEntryId);
  const entry = useAppStore((s) =>
    s.entries.find((e) => e.id === selectedEntryId)
  );

  if (!entry) {
    return (
      <div className="flex h-full w-96 items-center justify-center border-l border-gray-200 bg-white text-sm text-gray-500">
        Select an entry to see details
      </div>
    );
  }

  return (
    <aside className="flex h-full w-96 flex-col overflow-y-auto border-l border-gray-200 bg-white">
      <div className="border-b border-gray-200 p-4">
        <h2 className="text-lg font-semibold text-gray-900">{entry.title}</h2>
        <div className="mt-2 flex flex-wrap gap-1">
          {entry.tags.map((tag) => (
            <span
              key={tag}
              className="rounded-full bg-gray-100 px-2 py-0.5 text-xs text-gray-700"
            >
              {tag}
            </span>
          ))}
        </div>
        {entry.group && (
          <div className="mt-2 text-xs text-gray-500">
            Group: <span className="font-medium">{entry.group}</span>
          </div>
        )}
      </div>

      {entry.attachments.length > 0 && (
        <div className="border-b border-gray-200 p-4">
          <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-gray-500">
            Attachments ({entry.attachments.length})
          </h3>
          <ul className="space-y-1">
            {entry.attachments.map((att, idx) => (
              <li
                key={idx}
                className="flex items-center gap-2 rounded-md px-2 py-1 text-sm hover:bg-gray-50"
              >
                <span className="text-gray-400">{att.attachment_type === 'image' ? '🖼' : '📎'}</span>
                <span className="text-gray-700">{att.caption || att.path}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="border-b border-gray-200 p-4">
        <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-gray-500">
          Relations ({entry.relations.length})
        </h3>
        {entry.relations.length === 0 ? (
          <p className="text-sm text-gray-500">No relations</p>
        ) : (
          <ul className="space-y-1">
            {entry.relations.map((rel, idx) => (
              <li
                key={idx}
                className="flex items-center justify-between rounded-md px-2 py-1 text-sm hover:bg-gray-50"
              >
                <span className="text-gray-700">
                  → {rel.toId}{' '}
                  <span className="ml-1 text-xs text-gray-500">({rel.type})</span>
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="p-4 text-xs text-gray-400">
        <div>ID: {entry.id}</div>
        <div className="mt-1 truncate">Path: {entry.path}</div>
      </div>
    </aside>
  );
}
```

- [ ] **Step 2: Modify src/App.tsx to include DetailPanel**

```tsx
import { useEffect } from 'react';
import { Sidebar } from '@/components/Sidebar';
import { MapView } from '@/views/MapView';
import { DetailPanel } from '@/views/DetailPanel';
import { useAppStore } from '@/store/useAppStore';

export default function App() {
  const loadEntries = useAppStore((s) => s.loadEntries);
  const error = useAppStore((s) => s.error);
  const isLoading = useAppStore((s) => s.isLoading);
  const selectedEntryId = useAppStore((s) => s.selectedEntryId);

  useEffect(() => {
    loadEntries();
  }, [loadEntries]);

  return (
    <div className="flex h-screen w-screen bg-gray-50 text-gray-900">
      <Sidebar />
      <main className="relative flex-1">
        {error && (
          <div className="absolute left-4 right-4 top-4 z-10 rounded-md bg-red-50 p-3 text-sm text-red-700 shadow-sm">
            {error}
          </div>
        )}
        {isLoading ? (
          <div className="flex h-full items-center justify-center text-sm text-gray-500">
            Loading...
          </div>
        ) : (
          <MapView />
        )}
      </main>
      {selectedEntryId && <DetailPanel />}
    </div>
  );
}
```

- [ ] **Step 3: Verify build**

Run: `npm run build`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/views/DetailPanel.tsx src/App.tsx
git commit -m "feat(frontend): detail panel for selected entry"
```

---

### Task 19: Settings View

**Files:**
- Create: `src/views/Settings.tsx`

- [ ] **Step 1: Create src/views/Settings.tsx**

```tsx
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '@/store/useAppStore';

interface SettingsProps {
  onClose: () => void;
}

export function Settings({ onClose }: SettingsProps) {
  const [path, setPath] = useState('');
  const [isSetting, setIsSetting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const loadEntries = useAppStore((s) => s.loadEntries);

  const handleSetRoot = async () => {
    setIsSetting(true);
    setError(null);
    try {
      await invoke('set_kb_root', { path });
      await loadEntries();
      onClose();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsSetting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
        <h2 className="text-lg font-semibold text-gray-900">Settings</h2>

        <div className="mt-4">
          <label className="block text-sm font-medium text-gray-700">
            Knowledge Base Root
          </label>
          <input
            type="text"
            value={path}
            onChange={(e) => setPath(e.target.value)}
            placeholder="/path/to/knowledge-base"
            className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none"
          />
          <p className="mt-1 text-xs text-gray-500">
            The folder containing your entries (each entry is a subfolder with content.md).
          </p>
        </div>

        {error && (
          <div className="mt-3 rounded-md bg-red-50 p-2 text-sm text-red-700">
            {error}
          </div>
        )}

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded-md px-3 py-2 text-sm text-gray-700 hover:bg-gray-100"
          >
            Cancel
          </button>
          <button
            onClick={handleSetRoot}
            disabled={!path || isSetting}
            className="rounded-md bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
          >
            {isSetting ? 'Setting...' : 'Set Root'}
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Modify src/App.tsx to add settings button**

```tsx
import { useEffect, useState } from 'react';
import { Sidebar } from '@/components/Sidebar';
import { MapView } from '@/views/MapView';
import { DetailPanel } from '@/views/DetailPanel';
import { Settings } from '@/views/Settings';
import { useAppStore } from '@/store/useAppStore';

export default function App() {
  const loadEntries = useAppStore((s) => s.loadEntries);
  const error = useAppStore((s) => s.error);
  const isLoading = useAppStore((s) => s.isLoading);
  const selectedEntryId = useAppStore((s) => s.selectedEntryId);

  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    loadEntries();
  }, [loadEntries]);

  return (
    <div className="flex h-screen w-screen bg-gray-50 text-gray-900">
      <Sidebar />
      <main className="relative flex-1">
        <button
          onClick={() => setSettingsOpen(true)}
          className="absolute right-4 top-4 z-10 rounded-md bg-white px-3 py-1.5 text-sm shadow-sm hover:bg-gray-50"
        >
          ⚙ Settings
        </button>
        {error && (
          <div className="absolute left-4 right-4 top-4 z-10 rounded-md bg-red-50 p-3 text-sm text-red-700 shadow-sm">
            {error}
          </div>
        )}
        {isLoading ? (
          <div className="flex h-full items-center justify-center text-sm text-gray-500">
            Loading...
          </div>
        ) : (
          <MapView />
        )}
      </main>
      {selectedEntryId && <DetailPanel />}
      {settingsOpen && <Settings onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
```

- [ ] **Step 3: Verify build**

Run: `npm run build`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/views/Settings.tsx src/App.tsx
git commit -m "feat(frontend): settings view for KB root configuration"
```

---

### Task 20: Relation Dialog

**Files:**
- Create: `src/components/RelationDialog.tsx`

- [ ] **Step 1: Create src/components/RelationDialog.tsx**

```tsx
import { useState } from 'react';
import { useAppStore } from '@/store/useAppStore';
import type { RelationType } from '@shared/types';

const RELATION_TYPES: RelationType[] = [
  'reference',
  'derived',
  'contradicts',
  'supersedes',
  'extends',
];

interface RelationDialogProps {
  fromId: string;
  onClose: () => void;
}

export function RelationDialog({ fromId, onClose }: RelationDialogProps) {
  const entries = useAppStore((s) => s.entries);
  const createRelation = useAppStore((s) => s.createRelation);

  const [toId, setToId] = useState('');
  const [type, setType] = useState<RelationType>('reference');
  const [note, setNote] = useState('');

  const otherEntries = entries.filter((e) => e.id !== fromId);

  const handleCreate = async () => {
    if (!toId) return;
    await createRelation(fromId, toId, type, note || undefined);
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
        <h2 className="text-lg font-semibold text-gray-900">Create Relation</h2>
        <p className="mt-1 text-sm text-gray-500">From: {fromId}</p>

        <div className="mt-4 space-y-3">
          <div>
            <label className="block text-sm font-medium text-gray-700">To Entry</label>
            <select
              value={toId}
              onChange={(e) => setToId(e.target.value)}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
            >
              <option value="">Select...</option>
              {otherEntries.map((e) => (
                <option key={e.id} value={e.id}>
                  {e.title}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Type</label>
            <select
              value={type}
              onChange={(e) => setType(e.target.value as RelationType)}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
            >
              {RELATION_TYPES.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Note (optional)</label>
            <input
              type="text"
              value={note}
              onChange={(e) => setNote(e.target.value)}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
            />
          </div>
        </div>

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded-md px-3 py-2 text-sm text-gray-700 hover:bg-gray-100"
          >
            Cancel
          </button>
          <button
            onClick={handleCreate}
            disabled={!toId}
            className="rounded-md bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Add create relation button to DetailPanel**

Modify `src/views/DetailPanel.tsx` - add to imports and component:

```tsx
import { useState } from 'react';
import { useAppStore } from '@/store/useAppStore';
import { RelationDialog } from '@/components/RelationDialog';

export function DetailPanel() {
  const selectedEntryId = useAppStore((s) => s.selectedEntryId);
  const entry = useAppStore((s) =>
    s.entries.find((e) => e.id === selectedEntryId)
  );
  const [showDialog, setShowDialog] = useState(false);

  if (!entry) {
    return (
      <div className="flex h-full w-96 items-center justify-center border-l border-gray-200 bg-white text-sm text-gray-500">
        Select an entry to see details
      </div>
    );
  }

  return (
    <>
      <aside className="flex h-full w-96 flex-col overflow-y-auto border-l border-gray-200 bg-white">
        <div className="border-b border-gray-200 p-4">
          <div className="flex items-start justify-between">
            <h2 className="text-lg font-semibold text-gray-900">{entry.title}</h2>
            <button
              onClick={() => setShowDialog(true)}
              className="rounded-md bg-blue-500 px-2 py-1 text-xs font-medium text-white hover:bg-blue-600"
              title="Create relation"
            >
              + Rel
            </button>
          </div>
          <div className="mt-2 flex flex-wrap gap-1">
            {entry.tags.map((tag) => (
              <span
                key={tag}
                className="rounded-full bg-gray-100 px-2 py-0.5 text-xs text-gray-700"
              >
                {tag}
              </span>
            ))}
          </div>
          {entry.group && (
            <div className="mt-2 text-xs text-gray-500">
              Group: <span className="font-medium">{entry.group}</span>
            </div>
          )}
        </div>

        {entry.attachments.length > 0 && (
          <div className="border-b border-gray-200 p-4">
            <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-gray-500">
              Attachments ({entry.attachments.length})
            </h3>
            <ul className="space-y-1">
              {entry.attachments.map((att, idx) => (
                <li
                  key={idx}
                  className="flex items-center gap-2 rounded-md px-2 py-1 text-sm hover:bg-gray-50"
                >
                  <span className="text-gray-400">
                    {att.attachment_type === 'image' ? '🖼' : '📎'}
                  </span>
                  <span className="text-gray-700">{att.caption || att.path}</span>
                </li>
              ))}
            </ul>
          </div>
        )}

        <div className="border-b border-gray-200 p-4">
          <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-gray-500">
            Relations ({entry.relations.length})
          </h3>
          {entry.relations.length === 0 ? (
            <p className="text-sm text-gray-500">No relations</p>
          ) : (
            <ul className="space-y-1">
              {entry.relations.map((rel, idx) => (
                <li
                  key={idx}
                  className="flex items-center justify-between rounded-md px-2 py-1 text-sm hover:bg-gray-50"
                >
                  <span className="text-gray-700">
                    → {rel.toId}{' '}
                    <span className="ml-1 text-xs text-gray-500">({rel.type})</span>
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="p-4 text-xs text-gray-400">
          <div>ID: {entry.id}</div>
          <div className="mt-1 truncate">Path: {entry.path}</div>
        </div>
      </aside>
      {showDialog && (
        <RelationDialog fromId={entry.id} onClose={() => setShowDialog(false)} />
      )}
    </>
  );
}
```

- [ ] **Step 3: Verify build**

Run: `npm run build`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/RelationDialog.tsx src/views/DetailPanel.tsx
git commit -m "feat(frontend): relation creation dialog"
```

---

## Phase 10: Integration & Polish

### Task 21: README and Example Standards

**Files:**
- Create: `README.md`
- Create: `docs/standards.md`

- [ ] **Step 1: Create README.md**

```markdown
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
```

- [ ] **Step 2: Create docs/standards.md**

````markdown
# Synapse KB File Format Standards

## 目录结构

```
kb-root/
├── topic-a/                      # 条目文件夹（文件夹名即条目 ID）
│   ├── content.md                # 必含
│   ├── attachments/              # 推荐：本地附件
│   └── notes/                    # 可选：块级补充文件
├── topic-b/
│   └── content.md
└── .synapse-layout.json          # 自动生成：节点位置
```

## content.md 基础元数据

在文件开头使用 HTML 注释行：

```
<!-- @synapse-id: react-hooks -->
<!-- @synapse-title: React Hooks 深入理解 -->
<!-- @synapse-tags: react, frontend -->
<!-- @synapse-group: frontend -->
```

## 双向链接与附件嵌入

```markdown
参考 [[state-management]] 了解更多。

![[./diagrams/flow.png|流程图]]
```

## 可选 YAML 高级字段

```markdown
---
color: "#5B8DEF"
shape: rect
pinned: true
order: 5
---

# Content
```

## 5 类关联（推荐）

- `reference`（蓝色实线）：参考引用
- `derived`（绿色带箭头）：派生/总结自
- `contradicts`（红色虚线）：与之矛盾
- `supersedes`（橙色双线）：取代旧版
- `extends`（紫色曲线）：补充/扩展
````

- [ ] **Step 3: Commit**

```bash
git add README.md docs/standards.md
git commit -m "docs: add README and file format standards"
```

---

### Task 22: Final Build Verification

**Files:** None (verification only)

- [ ] **Step 1: Verify Rust tests pass**

Run: `cd src-tauri && cargo test`
Expected: All tests pass (parser, scanner, entry, layout_store, index, local_server).

- [ ] **Step 2: Verify frontend builds**

Run: `npm run build`
Expected: dist/ folder created with no errors.

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: No type errors.

- [ ] **Step 4: Try Tauri build**

Run: `npm run tauri:build`
Expected: Tauri app bundles successfully (this may take a while on first run).

- [ ] **Step 5: Manual smoke test**

Run: `npm run tauri:dev`
Then in the app:
1. Click Settings, enter a test KB path
3. Verify entries appear on the map
4. Click an entry, verify it shows in detail panel
5. Click + Rel, create a relation
6. Verify the relation appears in map

- [ ] **Step 6: Commit any fixes**

If any issues were found and fixed:

```bash
git add .
git commit -m "fix: address issues found in smoke test"
```

---

## Summary

This plan delivers the MVP defined in the design document:

✅ Tauri 2.0 desktop app with React Flow map view
✅ Markdown + HTML comment + YAML file format parser
✅ SQLite local index
✅ Local axum HTTP server
✅ Map view with custom node cards
✅ Detail panel showing entry metadata
✅ Settings for KB root configuration
✅ Relation creation dialog

**Total tasks:** 22 bite-sized tasks with TDD discipline.
**Files created:** ~30 (Rust backend + React frontend + docs).
**Commits:** ~22 small, focused commits.