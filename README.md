# URDF for Zed

URDF language support for [Zed](https://zed.dev) with:

- Syntax highlighting
- Outline / structure view
- Auto indentation
- Context-aware completions for common URDF tags, attributes, and values

This extension targets `.urdf` files and builds on top of the XML Tree-sitter grammar plus an external URDF language server.

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

## Installation

For normal users:

1. Install the extension in Zed
2. Open a `.urdf` file
3. The extension will look for a language server binary whose version exactly matches the extension version
4. If that exact-version binary is not already cached, the extension downloads the matching GitHub Release asset for the same version

This extension intentionally uses a **strict version lock** between the extension and the language server.

That means:

- extension version `0.1.0` only accepts language server release tag `v0.1.0`
- extension version `0.1.1` only accepts language server release tag `v0.1.1`
- the extension does **not** use the latest release automatically
- the extension does **not** silently fall back to a different installed server version

## Version Lock Policy

The repository follows these rules:

- `Cargo.toml` version
- `extension.toml` version
- `urdf-language-server/Cargo.toml` version
- Git tag name without the leading `v`

must all be identical.

Examples:

- extension version: `0.1.0`
- language server crate version: `0.1.0`
- Git tag: `v0.1.0`

At runtime, the extension downloads only the release with tag `v0.1.0` when the extension version is `0.1.0`.

## Release Assets

Expected release asset names:

- `urdf-language-server-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz`
- `urdf-language-server-vX.Y.Z-x86_64-apple-darwin.tar.gz`
- `urdf-language-server-vX.Y.Z-aarch64-apple-darwin.tar.gz`
- `urdf-language-server-vX.Y.Z-x86_64-pc-windows-msvc.zip`

Each archive must contain the executable at the archive root:

- `urdf-language-server`
- `urdf-language-server.exe` on Windows

## Development

### Prerequisites

Install Rust with `rustup`.

### Install as a Dev Extension in Zed

1. Open Zed
2. Open the command palette
3. Run `zed: install dev extension`
4. Select this repository folder

After changing the extension, open the Extensions page and click `Rebuild` for the dev extension.

## Local Testing Without GitHub Release

You do **not** need to publish a GitHub Release every time you modify the language server during development.

With the strict version-locked setup used by this extension, the runtime behavior is:

1. The extension first checks whether a language server binary for the **exact same version** already exists in the local versioned cache directory
2. Only if that exact-version binary is missing does it try to download the matching GitHub Release asset for the same version tag

That means local development can use a manually built binary, as long as:

- the extension version and language server version are the same
- the binary is placed at the expected versioned cache path

### Keep versions in sync

Before local testing, make sure these versions are identical:

- `Cargo.toml`
- `extension.toml`
- `urdf-language-server/Cargo.toml`

For example, if the version is `0.1.0`, then all three files should use `0.1.0`.

### Build the language server locally

From the repository root:

```sh
cargo build --manifest-path ./urdf-language-server/Cargo.toml --release
````

### Put the binary into the expected local cache directory

Create the versioned cache directory expected by the extension:

Linux:

```text
~/.local/share/zed/extensions/work/.zed-urdf/<version>/
```

macOS:

```text
~/Library/Application Support/zed/extensions/work/.zed-urdf/<version>/
```

Windows:

```text
%LOCALAPPDATA%\Zed\extensions\work\.zed-urdf\<version>\
```

Then copy the locally built binary into that directory.

#### Linux

If you build for the default host target:

```sh
ZED_EXT_DIR="$HOME/.local/share/zed/extensions/work/urdf/.zed-urdf/0.1.0"
mkdir -p "$ZED_EXT_DIR"
cp urdf-language-server/target/release/urdf-language-server "$ZED_EXT_DIR/urdf-language-server"
chmod +x "$ZED_EXT_DIR/urdf-language-server"
```

If you explicitly build a target triple, your output path may instead look like this:

```sh
ZED_EXT_DIR="$HOME/.local/share/zed/extensions/work/urdf/.zed-urdf/0.1.0"
mkdir -p "$ZED_EXT_DIR"
cp urdf-language-server/target/x86_64-unknown-linux-gnu/release/urdf-language-server "$ZED_EXT_DIR/urdf-language-server"
chmod +x "$ZED_EXT_DIR/urdf-language-server"
```

#### macOS

```sh
ZED_EXT_DIR="$HOME/Library/Application Support/zed/extensions/work/urdf/.zed-urdf/0.1.0"
mkdir -p "$ZED_EXT_DIR"
cp urdf-language-server/target/release/urdf-language-server "$ZED_EXT_DIR/urdf-language-server"
chmod +x "$ZED_EXT_DIR/urdf-language-server"
```

#### Windows (PowerShell)

```powershell
$ZedExtDir = "$env:LOCALAPPDATA\Zed\extensions\work\.zed-urdf\0.1.0"
New-Item -ItemType Directory -Force -Path $ZedExtDir | Out-Null
Copy-Item "urdf-language-server\target\release\urdf-language-server.exe" "$ZedExtDir\urdf-language-server.exe"
```

Replace `0.1.0` with your actual current extension version.

### Recommended local test workflow after changing the language server

When you modify `urdf-language-server/src/main.rs`:

1. Keep all versions unchanged if you are just testing local behavior
2. Run the local release build
3. Copy the new binary into `.zed-urdf/<version>/`
4. Rebuild the dev extension in Zed
5. Reopen a `.urdf` file and verify completions

### Testing a version bump locally

If you want to test a new version locally, such as moving from `0.1.0` to `0.1.1`:

1. Update these files to `0.1.1`:

   * `Cargo.toml`
   * `extension.toml`
   * `urdf-language-server/Cargo.toml`
2. Build the language server again
3. Copy the binary into `.zed-urdf/0.1.1/`
4. Rebuild the dev extension in Zed

The extension will then use only the `0.1.1` binary and will not reuse the old `0.1.0` cache.

### When is GitHub Release actually needed?

A GitHub Release is only needed when you want to test the **real published download path**:

* remove the local cached binary for the current version
* keep the same version
* create the matching Git tag such as `v0.1.0`
* publish the release assets
* let the extension download that exact-version asset automatically

## Publishing Notes

This repository keeps the language server source code, but the published Zed extension does not ship the server binary inside the extension package.

Instead:

* GitHub Actions builds versioned language server binaries
* those binaries are uploaded as GitHub Release assets
* the extension downloads only the asset whose tag exactly matches the extension version

## Troubleshooting

If the extension does not seem to work:

* open `Zed.log` using `zed: open log`
* or launch Zed from the command line with:

```sh
zed --foreground
```

This usually shows more detailed extension and language server logs during development.

You may also want to check:

* whether the three version fields are exactly the same
* whether the local binary exists under `.zed-urdf/<version>/`
* whether the binary name is correct for your platform
* whether the binary is executable on Unix-like systems

## Project Structure

```text
.
├── .github/
│   └── workflows/
│       ├── check-version-sync.yml
│       └── release-language-server.yml
├── README.md
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

* This extension currently uses the XML grammar for parsing URDF documents
* Completions are provided by the external custom URDF language server
* The completion engine is designed specifically for common URDF authoring workflows, rather than full XML Schema/XSD validation

## Acknowledgements

This project builds on and draws inspiration from:

* [tree-sitter-xml](https://github.com/tree-sitter-grammars/tree-sitter-xml)
* [sweetppro/zed-xml](https://github.com/sweetppro/zed-xml)

Many thanks to the maintainers and contributors of those projects.

## License

This project is licensed under the Apache License 2.0.

```
