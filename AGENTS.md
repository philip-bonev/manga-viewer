# AGENTS.md: Pure Manga Viewer (Tauri / Rust / Vanilla JS)

## Project Overview
- **Core Goal**: A desktop manga reader optimized for infinite vertical and horizontal scrolling.
- **Stack**: Tauri (v2), Rust (Backend), Vanilla JavaScript & HTML5 (Frontend).
- **Primary Formats**: Native support for `.cbz` and `.cbt` comic archives.
- **Conversion System**: Rust-powered background scripts to convert unsupported formats (PDF, EPUB, image folders) into `.cbz` or `.cbt`.

## Architecture & Technical Rules

### 1. Frontend (Vanilla JS & HTML5)
- **Continuous Scroll Engine**: 
  - Render images in a single vertical flexbox or grid layout without page-flip animations.
  - Implement an `IntersectionObserver` or a virtualization loop to handle long chapters without memory leaks.
  - Dynamically load images 3 viewports ahead and drop/unload images 3 viewports behind.
- **Image Delivery**: 
  - Fetch images from the Rust backend via custom Tauri protocol assets (`tauri://` or customized protocols).
  - Use `URL.revokeObjectURL()` immediately if blob URLs are passed to prevent RAM hoarding.

### 2. Backend (Rust)
- **Archive Extraction**: 
  - Use the `zip` crate for `.cbz` reading and the `tar` crate for `.cbt` reading.
  - Extract images strictly into memory buffers or a secure temporary directory (`AppCache`).
  - Sort file paths using natural alphanumeric sorting (`natord` crate) to prevent `page_10.jpg` from appearing before `page_2.jpg`.
- **Conversion Pipelines**: 
  - Implement Rust commands (`#[tauri::command]`) to handle bulk format conversion.
  - Convert directories of loose images into compressed `.cbz` files using synchronous or parallel zip streams.

## Command & Toolchain Reference

### Development & Build
- **Start Development App**: `cargo tauri dev`
- **Build Production App**: `cargo tauri build`

### System Commands (Tauri Invokes)
- `load_archive(path: String)`: Extracts metadata and returns the ordered list of image URLs.
- `convert_to_cbz(source_path: String, output_path: String)`: Packages a folder or external format into a standard `.cbz`.

## Coding & Performance Guidelines
- **UI Chrome**: Keep interface elements to an absolute minimum. Use CSS transitions only for overlay menus (e.g., settings, library).
- **Thread Safety**: Run archive extraction and file conversion on background worker threads using `tokio` or `rayon` so the UI never stutters during a scroll.

