# AGENTS.md: Pure Manga Viewer (Tauri / Rust / Vanilla JS)

## Project Overview
- **Core Goal**: A lightweight desktop comic/manga reader optimized for continuous vertical scrolling.
- **Stack**: Tauri v2 (Rust backend), vanilla JavaScript & HTML5 frontend. No frameworks, no bundler, no Node.js — `tauri.conf.json` serves the static `web/` directory directly (`frontendDist: "web"`, `withGlobalTauri: true`).
- **Supported Formats**: `.cbz` (ZIP), `.cbt` (TAR), `.cb7` (7z, including solid archives).
- **Supported Image Types**: `jpg`, `jpeg`, `png`, `gif`, `webp`, `bmp`, `svg`, `tiff`, `tif`.

## Architecture & Technical Rules

### 1. Frontend (`web/`)
- **Continuous Scroll Engine**:
  - Render pages in a single vertical column (`#images-container`) inside the scrollable `#viewer-container`. No page-flip animations.
  - All `<img>` elements for the whole archive are created upfront (placeholder height 200 px), but only pages near the viewport are fetched. Lazy loading is driven by the container `scroll` event calling `loadNearbyImages()` — there is no `IntersectionObserver` and no DOM virtualization (elements are never removed, only their `src` is set).
  - `LAZY_LOAD_THRESHOLD = 5`: an image is loaded when it is within 5 viewport heights above or below the current scroll position.
- **Image Delivery**:
  - Images are fetched from the Rust backend via the `get_image_data` Tauri command, which returns **base64**. The frontend wraps it in a `data:<mime>;base64,...` URL. Do not switch to blob URLs or custom `tauri://` protocols without a compelling reason; if you ever do, call `URL.revokeObjectURL()` to avoid RAM hoarding.
  - Zoom is a CSS `transform: scale()` on `#images-container` (25–500 %, step 25). Shortcuts: `+`/`=` in, `-` out, `0` reset, `Ctrl`+wheel, plus toolbar buttons.

### 2. Backend (`src/`)
- **Archive Reading** (all synchronous, no `tokio`/`rayon` in this crate):
  - `zip` crate for `.cbz`, `tar` for `.cbt`, `sevenz_rust::SevenZReader` for `.cb7`. For solid 7z archives, non-matching entries must still be consumed (`std::io::copy` to sink) so the decoder can advance.
  - Images are read strictly into memory buffers and returned as base64 — nothing is extracted to disk.
  - `__MACOSX` entries and directories are skipped. Pages are ordered by plain lexicographic sort of the entry path in `collect_images()` (`src/lib.rs`); there is no natural-number sorting (`page_10` sorts before `page_2` if names differ that way — keep filenames zero-padded, or improve the sort if asked).
  - MIME type is inferred from the file extension (`mime_from_ext`).
- **State**: `Mutex<AppState>` holds the currently opened path, its `ArchiveFormat`, and a pending CLI/drag-drop file path.
- **Series Navigation**: `get_next_file` / `get_prev_file` scan the parent directory for files matching the current extension and the regex `^(.*?)[-_ ](\d+(?:\.\d+)?)` on the file stem (e.g. `Berserk-001.cbz`), then return the nearest higher/lower issue number.
- **Single Instance**: the `single-instance` plugin re-opens files passed to a second launch by storing the path in state and emitting the `file-opened` event (also used for macOS `RunEvent::Opened` and `tauri://drag-drop`).
- **Build Stamp**: `build.rs` sets `BUILD_DATE` via `cargo:rustc-env`; `src/lib.rs` embeds `LICENSE` with `include_str!("../LICENSE")` — the LICENSE file must remain at the repo root or compilation fails.

## Command & Toolchain Reference

### Mandatory Build Requirements
- Rust stable **≥ 1.85** (crate uses **edition 2024**).
- **Tauri CLI 2.x** (`cargo install tauri-cli --version "^2"`), then `cargo tauri …`.
- Platform deps: Linux → `libwebkit2gtk-4.1-dev`, `libjavascriptcoregtk-4.1-dev`, build tools, `libxdo-dev`, `libssl-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`; macOS → Xcode Command Line Tools; Windows → MSVC Build Tools + WebView2 Runtime.
- **Node.js is not required** (no `package.json`, no frontend build step).

### Shell PATH
Always use this PATH when running shell commands in this project:
```sh
export PATH="/opt/homebrew/bin:/opt/homebrew/sbin:$HOME/.cargo/bin:/usr/bin:/usr/sbin:/bin:/sbin:/usr/local/bin:/usr/local/sbin:$PATH"
```
- The `/opt/homebrew/...` entries are **macOS-specific** (Homebrew). On other platforms skip them and rely on the native prefixes (`/usr/local/bin`, `/usr/bin`, `~/.cargo/bin`, …).
- It is also appended to `~/.zshrc`, but the session shell may start with a stripped PATH — export it explicitly before commands like `git push` which need `/usr/bin/ssh`.

### Development & Build
- **Start Development App**: `cargo tauri dev` (DevTools open automatically in debug builds).
- **Build Production App**: `cargo tauri build` (bundles land in `target/release/bundle/`; `.deb` registers MIME types via `mime/comicbook-mime.xml` and `scripts/postinst` / `scripts/prerm`).

### Project Layout
- `src/main.rs` — binary entry point, calls `manga_viewer_lib::run()`.
- `src/lib.rs` — all Tauri commands and archive logic.
- `web/` — `index.html`, `main.js`, `styles.css` (static frontend).
- `tauri.conf.json` — window, bundle, file associations (`.cbz`, `.cbt`, `.cb7`).
- `capabilities/default.json` — Tauri v2 permissions (`core`, `opener`, `dialog`, `fs`, `window-state`).
- `icons/`, `mime/`, `scripts/`, `build.rs`. Note: the crate root is the repo root (no `src-tauri/` directory).

### Tauri Commands (IPC, in `src/lib.rs`)
- `open_cbz_file(path: String)` / `open_cbt_file(path: String)` / `open_cb7_file(path: String)`: open an archive, return `ArchiveImages { images: Vec<ImageInfo> }` (sorted, with zero-padded ids), and store the file in state.
- `get_image_data(href: String)`: read one entry from the open archive as base64.
- `get_next_file(path: String)` / `get_prev_file(path: String)`: series-aware neighbour issue lookup (returns `Option<String>`).
- `get_cli_file()`: take the pending CLI / OS-open file path.
- `close_file()`: clear the open archive from state.
- `get_app_info()`: name, version, build date, author, license name/text, repository.
- `exit_app()`: quit the app.

## Coding & Performance Guidelines
- **UI Chrome**: keep the interface minimal (single toolbar). CSS transitions only for overlays (e.g. the About dialog) and the drag-over highlight.
- **Threading**: commands are currently synchronous; if you add heavy work (conversion, large-archive scans), prefer making the command `async` or moving the work to a background thread rather than blocking the UI thread.
- **No dependencies without need**: only add crates to `Cargo.toml` when required; the current dependency set is deliberately small (serde, zip, tar, sevenz-rust, base64, regex, percent-encoding + Tauri plugins).
- **Conventions**: lowercase, short git commit messages; Apache-2.0 headers on new source files; keep the frontend dependency-free.
