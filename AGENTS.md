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
  - All `<img>` elements for the whole archive are created upfront (placeholder height 200 px), but only pages near the viewport are fetched. Lazy loading is driven by a rAF-throttled container `scroll` handler calling `loadNearbyImages()` — there is no `IntersectionObserver` and no DOM virtualization (elements are never removed, only their `src` is set). Loads run in parallel batches of `LOAD_CONCURRENCY = 4`, guarded by a `loadGeneration` token so responses from a previously opened file are discarded.
  - `LAZY_LOAD_THRESHOLD = 5`: an image is loaded when it is within 5 viewport heights above or below the current scroll position.
- **Image Delivery**:
  - Images are fetched from the Rust backend via the `get_image_data` Tauri command, which returns **base64**. The frontend wraps it in a `data:<mime>;base64,...` URL using the `mime_type` from `ImageInfo` (do not re-derive MIME in JS). Do not switch to blob URLs or custom `tauri://` protocols without a compelling reason; if you ever do, call `URL.revokeObjectURL()` to avoid RAM hoarding.
  - Zoom sets the layout width of `#images-container` in percent (25–500 %, step 25), so page edges stay reachable via scrolling. Do not switch back to `transform: scale()` — it clips content beyond the viewport. Shortcuts: `+`/`=` in, `-` out, `0` reset, `Ctrl`+wheel, plus toolbar buttons.

### 2. Backend (`src/`)
- **Archive Reading** (no `tokio`/`rayon` in this crate; heavy commands use `#[tauri::command(async)]` so the UI thread is never blocked):
  - `zip` crate for `.cbz`, `tar` for `.cbt`, `sevenz_rust::SevenZReader` for `.cb7`. For solid 7z archives, non-matching entries must still be consumed (`std::io::copy` to sink) so the decoder can advance; 7z entries are decoded per request (solid archives cannot be seeked), while `.cbz`/`.cbt` keep an open handle in `AppState` (`OpenArchive` enum: `Zip(ZipArchive<File>)`, `Tar { file, index }` with a href → (offset, size) map built at open time).
  - Images are read strictly into memory buffers and returned as base64 — nothing is extracted to disk.
  - `__MACOSX` entries and directories are skipped. Pages are ordered by `natural_cmp` (natural number sort) of the entry path in `collect_images()` (`src/lib.rs`), so `page2` sorts before `page10`.
  - MIME type is inferred from the file extension (`mime_from_ext`) and sent to the frontend in `ImageInfo.mime_type`.
- **State**: `Mutex<AppState>` holds the currently opened file path, the open `OpenArchive` handle (if any), and a pending CLI file path.
- **Series Navigation**: `find_sibling_file` (behind `get_next_file` / `get_prev_file`) scans the parent directory for files matching the current extension (case-insensitively) and the regex `^(.*?)[-_ ](\d+(?:\.\d+)?)` on the file stem (e.g. `Berserk-001.cbz`), then returns the nearest higher/lower issue number.
- **Single Instance**: the `single-instance` plugin re-opens files passed to a second launch by storing the path in state and emitting the `file-opened` event (also used for macOS `RunEvent::Opened` and `tauri://drag-drop`).
- **App Menu**: desktop menu (`src/menu.rs`) with About and Quit; About opens the dedicated `about` window (`web/about.html`) which is populated by `get_app_info`.
- **Build Stamp**: `build.rs` sets `BUILD_DATE` via `cargo:rustc-env`; `src/commands.rs` embeds `LICENSE` with `include_str!("../LICENSE")` — the LICENSE file must remain at the repo root or compilation fails.

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
- **Tests**: `cargo test` runs unit tests plus an archive smoke test. Set `MANGA_TEST_DIR` to a directory with `.cbz`/`.cbt`/`.cb7` samples to exercise the real archive code paths (skipped when unset).

### Project Layout
- `src/main.rs` — binary entry point, calls `manga_viewer_lib::run()`.
- `src/lib.rs` — app setup (`run()`), CLI/OS file-argument handling.
- `src/commands.rs` — Tauri command handlers (IPC).
- `src/archive.rs` — archive opening and entry reading (zip/tar/7z).
- `src/images.rs` — entry filtering, MIME inference, natural sort, page listing.
- `src/series.rs` — series-aware neighbour issue lookup.
- `src/state.rs` — `AppState` / `OpenArchive` and the mutex lock helper.
- `src/model.rs` — serializable IPC types (`AppInfo`, `ImageInfo`, `ArchiveImages`).
- `src/menu.rs` — desktop app menu (About window, Quit).
- `web/` — `index.html`, `main.js`, `styles.css` (main window) and `about.html`, `about.js`, `about.css` (About window), all static, no build step.
- `tauri.conf.json` — window, bundle, file associations (`.cbz`, `.cbt`, `.cb7`).
- `capabilities/default.json` — Tauri v2 permissions (`core`, `opener`, `dialog`, `window-state`).
- `icons/`, `mime/`, `scripts/`, `build.rs`. Note: the crate root is the repo root (no `src-tauri/` directory).

### Tauri Commands (IPC, in `src/commands.rs`)
- `open_cbz_file(path: String)` / `open_cbt_file(path: String)` / `open_cb7_file(path: String)`: open an archive, return `ArchiveImages { images: Vec<ImageInfo> }` (sorted, with zero-padded ids), and store the file in state.
- `get_image_data(href: String)`: read one entry from the open archive as base64.
- `get_next_file(path: String)` / `get_prev_file(path: String)`: series-aware neighbour issue lookup (returns `Option<String>`).
- `get_cli_file()`: take the pending CLI / OS-open file path.
- `get_app_info()`: name, version, build date, author, license name/text, repository.
## Coding & Performance Guidelines
- **UI Chrome**: keep the interface minimal (single toolbar); About lives in the app menu and opens in a separate window. CSS transitions only for the drag-over highlight.
- **Threading**: heavy commands (`open_*`, `get_image_data`) use `#[tauri::command(async)]` so they run off the UI thread; if you add more heavy work (conversion, large-archive scans), do the same rather than blocking the UI thread.
- **No dependencies without need**: only add crates to `Cargo.toml` when required; the current dependency set is deliberately small (serde, zip, tar, sevenz-rust, base64, regex, percent-encoding + Tauri plugins).
- **Conventions**: lowercase, short git commit messages; Apache-2.0 headers on new source files; keep the frontend dependency-free. Versioning is X.Y.Z (X = major updates, Y = minor updates, Z = bug fixes), tagged as `vX.Y.Z`.
