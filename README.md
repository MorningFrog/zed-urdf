# URDF for Zed

URDF language support for [Zed](https://zed.dev) with:

- Syntax highlighting
- Outline / structure view
- Auto indentation
- Context-aware completions for common URDF tags, attributes, and values

This extension targets `.urdf` files and builds on top of the XML Tree-sitter grammar plus a custom URDF language server.

## Features

- Recognizes `.urdf` files as the `URDF` language
- Reuses the XML Tree-sitter grammar for parsing
- Provides syntax highlighting through Tree-sitter queries
- Provides outline support for URDF/XML elements
- Shows `name` context in outline entries for `robot`, `link`, and `joint`
- Provides completions for common URDF tags such as:
  - `robot`
  - `link`
  - `joint`
  - `visual`
  - `collision`
  - `inertial`
  - `geometry`
  - `origin`
  - `parent`
  - `child`
  - `axis`
  - `limit`
  - `material`
  - `mesh`
- Provides completions for common URDF attributes and values
- Supports both block-style and self-closing completions for tags like `link` and `material`

## Project Structure

```text
.
├── extension.toml
├── Cargo.toml
├── src/
│   └── lib.rs
├── languages/
│   └── urdf/
│       ├── config.toml
│       ├── highlights.scm
│       ├── indents.scm
│       └── outline.scm
└── urdf-language-server/
    ├── Cargo.toml
    └── src/
        └── main.rs
```

## How It Works

This extension follows Zed’s language extension model:

* `languages/urdf/config.toml` defines the language metadata
* `extension.toml` registers the Tree-sitter grammar and language server
* `highlights.scm`, `indents.scm`, and `outline.scm` provide syntax-aware editor behavior
* `urdf-language-server` provides completion support

## Development

### Prerequisites

Install Rust with `rustup`:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Build the language server

The extension expects `urdf-language-server` to be available in `PATH`.

From the repository root:

```sh
cargo install --path ./urdf-language-server --force
```

### Install as a Dev Extension in Zed

1. Open Zed
2. Open the command palette
3. Run `zed: install dev extension`
4. Select this repository folder

After changing the extension, open the Extensions page and click `Rebuild` for the dev extension.

## Example

```xml
<?xml version="1.0"?>
<robot name="demo_robot">
  <link name="base_link" />

  <link name="arm_link">
    <visual>
      <geometry>
        <box size="1 1 1" />
      </geometry>
    </visual>
  </link>

  <joint name="base_to_arm" type="revolute">
    <parent link="base_link" />
    <child link="arm_link" />
    <origin xyz="0 0 0.1" rpy="0 0 0" />
    <axis xyz="0 0 1" />
    <limit lower="-1.57" upper="1.57" effort="100" velocity="1.0" />
  </joint>
</robot>
```

## Notes

* This extension currently uses the XML grammar for parsing URDF documents.
* Completions are provided by the bundled custom URDF language server.
* The completion engine is designed specifically for common URDF authoring workflows, rather than full XML Schema/XSD validation.

## Publishing

To publish this extension to the Zed extension registry, open a PR to:

* `zed-industries/extensions`

The high-level process is:

1. Push this extension to a public GitHub repository
2. Add it to the `zed-industries/extensions` repository as a Git submodule
3. Add an entry for it in the top-level `extensions.toml`
4. Run `pnpm sort-extensions`
5. Open a PR

Once merged, the extension will be packaged and published to the Zed extension registry.

## Acknowledgements

This project builds on and draws inspiration from:

* [tree-sitter-xml](https://github.com/tree-sitter-grammars/tree-sitter-xml)
* [sweetppro/zed-xml](https://github.com/sweetppro/zed-xml)

Many thanks to the maintainers and contributors of those projects.
