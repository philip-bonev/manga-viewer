// Copyright 2026 Philip Bonev
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://apache.org
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::{Emitter, Manager, State};

const LICENSE_TEXT: &str = include_str!("../LICENSE");

#[derive(Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub build_date: String,
    pub author: String,
    pub license_name: String,
    pub license_text: String,
    pub repository: String,
}

pub(crate) enum OpenArchive {
    Zip(zip::ZipArchive<std::fs::File>),
    Tar {
        file: std::fs::File,
        /// Entry href -> (data offset in file, data size).
        index: HashMap<String, (u64, u64)>,
    },
    /// Solid 7z archives decode sequentially, so entries are decoded from the
    /// file per request instead of keeping an open reader.
    SevenZ,
}

pub(crate) struct AppState {
    pub file_path: Option<PathBuf>,
    pub archive: Option<OpenArchive>,
    pub cli_file: Option<String>,
}

fn lock_state(state: &Mutex<AppState>) -> MutexGuard<'_, AppState> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "tiff", "tif",
];

#[derive(Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub id: String,
    pub href: String,
    pub mime_type: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ArchiveImages {
    pub images: Vec<ImageInfo>,
}

fn read_zip_entry(
    archive: &mut zip::ZipArchive<std::fs::File>,
    href: &str,
) -> Result<Vec<u8>, String> {
    let mut entry = archive.by_name(href).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
    Ok(data)
}

fn read_tar_entry(
    file: &mut std::fs::File,
    index: &HashMap<String, (u64, u64)>,
    href: &str,
) -> Result<Vec<u8>, String> {
    let &(offset, size) = index
        .get(href)
        .ok_or_else(|| format!("Entry '{}' not found in archive", href))?;
    file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut data = vec![0u8; size as usize];
    file.read_exact(&mut data).map_err(|e| e.to_string())?;
    Ok(data)
}

fn read_7z_entry(path: &std::path::Path, href: &str) -> Result<Vec<u8>, String> {
    let mut archive = sevenz_rust::SevenZReader::open(path.to_string_lossy().as_ref(), "".into())
        .map_err(|e| e.to_string())?;

    let mut result: Option<Vec<u8>> = None;
    let mut io_error: Option<String> = None;
    archive
        .for_each_entries(|entry, reader| {
            if entry.name() == href {
                let mut data = Vec::new();
                match std::io::Read::read_to_end(reader, &mut data) {
                    Ok(_) => result = Some(data),
                    Err(e) => io_error = Some(e.to_string()),
                }
                Ok(false)
            } else {
                // Must consume entry data to advance the decoder
                // (required for solid 7z archives where data is interleaved)
                if let Err(e) = std::io::copy(reader, &mut std::io::sink()) {
                    io_error = Some(e.to_string());
                    return Ok(false);
                }
                Ok(true)
            }
        })
        .map_err(|e| e.to_string())?;

    if let Some(e) = io_error {
        return Err(e);
    }
    result.ok_or_else(|| format!("Entry '{}' not found in archive", href))
}

fn mime_from_ext(ext: &str) -> &str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "tiff" | "tif" => "image/tiff",
        _ => "image/jpeg",
    }
}

fn ext_of(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_lowercase())
        .unwrap_or_default()
}

fn is_image_entry(name: &str) -> bool {
    IMAGE_EXTENSIONS.contains(&ext_of(name).as_str())
}

/// Compare entry names with natural number ordering so that `page2` sorts
/// before `page10` (plain lexicographic sort would not).
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut a = a.bytes().peekable();
    let mut b = b.bytes().peekable();
    loop {
        match (a.next(), b.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                if x.is_ascii_digit() && y.is_ascii_digit() {
                    let mut xn = u128::from(x - b'0');
                    while let Some(&c) = a.peek() {
                        if !c.is_ascii_digit() {
                            break;
                        }
                        xn = xn
                            .saturating_mul(10)
                            .saturating_add(u128::from(c - b'0'));
                        a.next();
                    }
                    let mut yn = u128::from(y - b'0');
                    while let Some(&c) = b.peek() {
                        if !c.is_ascii_digit() {
                            break;
                        }
                        yn = yn
                            .saturating_mul(10)
                            .saturating_add(u128::from(c - b'0'));
                        b.next();
                    }
                    match xn.cmp(&yn) {
                        Ordering::Equal => continue,
                        ord => return ord,
                    }
                }
                match x.to_ascii_lowercase().cmp(&y.to_ascii_lowercase()) {
                    Ordering::Equal => continue,
                    ord => return ord,
                }
            }
        }
    }
}

fn collect_images(names: impl Iterator<Item = String>) -> Vec<ImageInfo> {
    let mut images: Vec<ImageInfo> = names
        .filter(|name| is_image_entry(name))
        .map(|name| {
            let mime_type = mime_from_ext(&ext_of(&name)).to_string();
            ImageInfo {
                id: String::new(),
                href: name,
                mime_type,
            }
        })
        .collect();

    images.sort_by(|a, b| natural_cmp(&a.href, &b.href));

    for (i, img) in images.iter_mut().enumerate() {
        img.id = format!("{:05}", i);
    }

    images
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        build_date: env!("BUILD_DATE").to_string(),
        author: env!("CARGO_PKG_AUTHORS").to_string(),
        license_name: "Apache License 2.0".to_string(),
        license_text: LICENSE_TEXT.to_string(),
        repository: env!("CARGO_PKG_REPOSITORY").to_string(),
    }
}

#[tauri::command(async)]
fn get_image_data(state: State<'_, Mutex<AppState>>, href: String) -> Result<String, String> {
    let mut state = lock_state(&state);
    let path = state
        .file_path
        .clone()
        .ok_or_else(|| "No file opened".to_string())?;

    let image_data = match state.archive.as_mut() {
        Some(archive) => read_entry(archive, &path, &href)?,
        None => return Err("No file opened".to_string()),
    };
    drop(state);

    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image_data);
    Ok(encoded)
}

fn extract_series_info(file_stem: &str) -> Option<(String, f64)> {
    static SERIES_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = SERIES_RE.get_or_init(|| {
        regex::Regex::new(r"^(.*?)[-_ ](\d+(?:\.\d+)?)").expect("static series regex is valid")
    });
    let captures = re.captures(file_stem)?;
    let series = captures.get(1)?.as_str().trim().to_lowercase();
    let num: f64 = captures.get(2)?.as_str().parse().ok()?;
    if series.is_empty() {
        return None;
    }
    Some((series, num))
}

fn find_sibling_file(path: &str, forward: bool) -> Option<String> {
    let file_path = std::path::PathBuf::from(path);
    let parent = file_path.parent()?;
    let file_name = file_path.file_stem()?.to_string_lossy().to_string();
    let extension = file_path.extension()?.to_string_lossy().to_lowercase();

    let (current_series, current_num) = extract_series_info(&file_name)?;

    let mut candidates: Vec<(f64, std::path::PathBuf)> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if !entry_path.is_file() {
                continue;
            }
            if entry_path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                != Some(extension.clone())
            {
                continue;
            }
            let Some(entry_stem) = entry_path.file_stem() else {
                continue;
            };
            let Some((series, num)) = extract_series_info(&entry_stem.to_string_lossy()) else {
                continue;
            };
            if series == current_series {
                candidates.push((num, entry_path));
            }
        }
    }

    candidates.sort_by(|a, b| {
        let ord = if forward {
            a.0.partial_cmp(&b.0)
        } else {
            b.0.partial_cmp(&a.0)
        };
        ord.unwrap_or(std::cmp::Ordering::Equal)
    });

    for (num, sibling) in candidates {
        if (forward && num > current_num) || (!forward && num < current_num) {
            return sibling.to_str().map(|s| s.to_string());
        }
    }

    None
}

#[tauri::command]
fn get_next_file(path: String) -> Option<String> {
    find_sibling_file(&path, true)
}

#[tauri::command]
fn get_prev_file(path: String) -> Option<String> {
    find_sibling_file(&path, false)
}

fn read_entry(
    archive: &mut OpenArchive,
    file_path: &std::path::Path,
    href: &str,
) -> Result<Vec<u8>, String> {
    match archive {
        OpenArchive::Zip(archive) => read_zip_entry(archive, href),
        OpenArchive::Tar { file, index } => read_tar_entry(file, index, href),
        OpenArchive::SevenZ => read_7z_entry(file_path, href),
    }
}

fn open_zip(path: &std::path::Path) -> Result<(Vec<ImageInfo>, OpenArchive), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let entries = (0..archive.len()).filter_map(|i| {
        let file = archive.by_index(i).ok()?;
        let name = file.name().to_string();
        if name.starts_with("__MACOSX") || name.ends_with('/') {
            return None;
        }
        Some(name)
    });

    Ok((collect_images(entries), OpenArchive::Zip(archive)))
}

fn open_tar(path: &std::path::Path) -> Result<(Vec<ImageInfo>, OpenArchive), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = tar::Archive::new(file);

    let mut index: HashMap<String, (u64, u64)> = HashMap::new();
    let mut names: Vec<String> = Vec::new();
    {
        let entries = archive.entries_with_seek().map_err(|e| e.to_string())?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry
                .path()
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .to_string();
            if name.ends_with('/') {
                continue;
            }
            if is_image_entry(&name) {
                index.insert(name.clone(), (entry.raw_file_position(), entry.size()));
            }
            names.push(name);
        }
    }
    let file = archive.into_inner();

    Ok((
        collect_images(names.into_iter()),
        OpenArchive::Tar { file, index },
    ))
}

fn open_sevenz(path: &std::path::Path) -> Result<(Vec<ImageInfo>, OpenArchive), String> {
    let reader = sevenz_rust::SevenZReader::open(path.to_string_lossy().as_ref(), "".into())
        .map_err(|e| e.to_string())?;

    let names: Vec<String> = reader
        .archive()
        .files
        .iter()
        .filter_map(|entry| {
            let name = entry.name().to_string();
            if entry.is_directory() {
                return None;
            }
            Some(name)
        })
        .collect();

    Ok((collect_images(names.into_iter()), OpenArchive::SevenZ))
}

#[tauri::command(async)]
fn open_cbz_file(state: State<'_, Mutex<AppState>>, path: String) -> Result<ArchiveImages, String> {
    let file_path = std::path::PathBuf::from(&path);
    let (images, archive) = open_zip(&file_path)?;

    {
        let mut state = lock_state(&state);
        state.file_path = Some(file_path);
        state.archive = Some(archive);
    }

    Ok(ArchiveImages { images })
}

#[tauri::command(async)]
fn open_cbt_file(state: State<'_, Mutex<AppState>>, path: String) -> Result<ArchiveImages, String> {
    let file_path = std::path::PathBuf::from(&path);
    let (images, archive) = open_tar(&file_path)?;

    {
        let mut state = lock_state(&state);
        state.file_path = Some(file_path);
        state.archive = Some(archive);
    }

    Ok(ArchiveImages { images })
}

#[tauri::command(async)]
fn open_cb7_file(state: State<'_, Mutex<AppState>>, path: String) -> Result<ArchiveImages, String> {
    let file_path = std::path::PathBuf::from(&path);
    let (images, archive) = open_sevenz(&file_path)?;

    {
        let mut state = lock_state(&state);
        state.file_path = Some(file_path);
        state.archive = Some(archive);
    }

    Ok(ArchiveImages { images })
}

#[tauri::command]
fn get_cli_file(state: State<'_, Mutex<AppState>>) -> Option<String> {
    let mut state = lock_state(&state);
    state.cli_file.take()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cli_file = find_file_in_args(std::env::args().skip(1));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(path) = find_file_in_args(argv.iter().skip(1).cloned()) {
                let state = app.state::<Mutex<AppState>>();
                let mut state = lock_state(state.inner());
                state.cli_file = Some(path.clone());
                let _ = app.emit("file-opened", path);
            }
        }))
        .manage(Mutex::new(AppState {
            file_path: None,
            archive: None,
            cli_file,
        }))
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            open_cbz_file,
            open_cbt_file,
            open_cb7_file,
            get_image_data,
            get_cli_file,
            get_next_file,
            get_prev_file
        ])
        .setup(|_app| {
            // Only automatically open DevTools during debug builds
            #[cfg(debug_assertions)]
            if let Some(window) = _app.get_webview_window("main") {
                window.open_devtools();
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    #[allow(unused_variables)]
    app.run(|app_handle, event| {
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
        if let tauri::RunEvent::Opened { urls } = event {
            for url in urls {
                if url.scheme() != "file" {
                    continue;
                }
                let Ok(path) = url.to_file_path() else {
                    continue;
                };
                let Some(ext) = path.extension() else {
                    continue;
                };
                let ext = ext.to_string_lossy().to_lowercase();
                if matches!(ext.as_str(), "cbz" | "cbt" | "cb7") {
                    let path_str = path.to_string_lossy().to_string();
                    let state = app_handle.state::<Mutex<AppState>>();
                    let mut state = lock_state(state.inner());
                    state.cli_file = Some(path_str.clone());
                    let _ = app_handle.emit("file-opened", path_str);
                }
            }
        }
    });
}

fn find_file_in_args(args: impl Iterator<Item = String>) -> Option<String> {
    for arg in args {
        #[cfg(target_os = "macos")]
        if arg.starts_with("-psn_") {
            continue;
        }
        if arg.starts_with("-") {
            continue;
        }
        let decoded = percent_encoding::percent_decode_str(&arg).decode_utf8_lossy().to_string();
        let mut path_str = decoded.strip_prefix("file://").unwrap_or(&decoded).to_string();
        if cfg!(windows) {
            // "file:///C:/dir/file.cbz" strips to "/C:/dir/file.cbz"; drop the leading slash
            let bytes = path_str.as_bytes();
            if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
                path_str = path_str[1..].to_string();
            }
        }
        if let Some(ext) = std::path::Path::new(&path_str).extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if matches!(ext.as_str(), "cbz" | "cbt" | "cb7") {
                return Some(path_str);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_cmp_orders_numerically() {
        use std::cmp::Ordering;
        assert_eq!(natural_cmp("page2.jpg", "page10.jpg"), Ordering::Less);
        assert_eq!(natural_cmp("page10.jpg", "page2.jpg"), Ordering::Greater);
        assert_eq!(natural_cmp("page2.jpg", "page2.jpg"), Ordering::Equal);
        assert_eq!(natural_cmp("a1b2", "a1b10"), Ordering::Less);
        assert_eq!(natural_cmp("Page2.JPG", "page10.jpg"), Ordering::Less);
        assert_eq!(natural_cmp("img", "img2"), Ordering::Less);
    }

    #[test]
    fn extract_series_info_parses_name_and_number() {
        assert_eq!(
            extract_series_info("Berserk-001"),
            Some(("berserk".to_string(), 1.0))
        );
        assert_eq!(
            extract_series_info("solo leveling 012"),
            Some(("solo leveling".to_string(), 12.0))
        );
        assert_eq!(
            extract_series_info("chapter-1.5"),
            Some(("chapter".to_string(), 1.5))
        );
        assert_eq!(extract_series_info("no-numbers-here"), None);
        assert_eq!(extract_series_info("123"), None);
    }

    #[test]
    fn ext_and_image_entry_detection() {
        assert_eq!(ext_of("dir/cover.JPG"), "jpg");
        assert_eq!(ext_of("noext"), "");
        assert!(is_image_entry("a/b/c.png"));
        assert!(is_image_entry("a/b/c.TIFF"));
        assert!(!is_image_entry("a/b/c.txt"));
        assert!(!is_image_entry("README"));
    }

    #[test]
    fn collect_images_sorts_naturally_and_numbers_ids() {
        let names = ["p10.jpg", "p2.jpg", "p1.jpg", "notes.txt"].map(String::from);
        let images = collect_images(names.into_iter());
        let hrefs: Vec<&str> = images.iter().map(|i| i.href.as_str()).collect();
        assert_eq!(hrefs, ["p1.jpg", "p2.jpg", "p10.jpg"]);
        assert_eq!(images[0].id, "00000");
        assert_eq!(images[2].id, "00002");
        assert_eq!(images[0].mime_type, "image/jpeg");
    }

    #[test]
    fn find_sibling_file_orders_by_issue_number() {
        let dir = std::env::temp_dir().join(format!("manga-viewer-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        for name in ["Berserk-001.cbz", "Berserk-002.cbz", "Berserk-010.cbz", "Other-001.cbz"] {
            std::fs::write(dir.join(name), b"").expect("write placeholder");
        }

        let current = dir.join("Berserk-002.cbz");
        let next = find_sibling_file(current.to_str().unwrap(), true).expect("next exists");
        assert!(next.ends_with("Berserk-010.cbz"));
        let prev = find_sibling_file(current.to_str().unwrap(), false).expect("prev exists");
        assert!(prev.ends_with("Berserk-001.cbz"));

        // Uppercase extensions match case-insensitively.
        let current_up = dir.join("Berserk-002.CBZ");
        std::fs::write(&current_up, b"").expect("write placeholder");
        assert!(find_sibling_file(current_up.to_str().unwrap(), true).is_some());

        std::fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }

    fn assert_archive_roundtrip(path: &std::path::Path) {
        let (images, mut archive) = match ext_of(&path.to_string_lossy()).as_str() {
            "cbz" => open_zip(path).expect("open zip"),
            "cbt" => open_tar(path).expect("open tar"),
            "cb7" => open_sevenz(path).expect("open 7z"),
            other => panic!("unexpected extension: {other}"),
        };
        assert!(
            !images.is_empty(),
            "{}: no images listed",
            path.display()
        );
        for img in [&images[0], &images[images.len() - 1]] {
            let data = read_entry(&mut archive, path, &img.href).expect("read entry");
            assert!(
                !data.is_empty(),
                "{}: empty entry {}",
                path.display(),
                img.href
            );
        }
    }

    /// Smoke test against real archives; set MANGA_TEST_DIR to a directory
    /// containing .cbz/.cbt/.cb7 files. Skipped when the variable is unset.
    #[test]
    fn smoke_test_sample_archives() {
        let Some(dir) = std::env::var_os("MANGA_TEST_DIR").map(std::path::PathBuf::from) else {
            eprintln!("MANGA_TEST_DIR not set; skipping sample archive smoke tests");
            return;
        };
        let mut tested = 0;
        for entry in std::fs::read_dir(&dir).expect("read test dir").flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if !matches!(
                ext_of(&path.to_string_lossy()).as_str(),
                "cbz" | "cbt" | "cb7"
            ) {
                continue;
            }
            assert_archive_roundtrip(&path);
            tested += 1;
        }
        assert!(tested > 0, "no archives found in {}", dir.display());
    }
}
