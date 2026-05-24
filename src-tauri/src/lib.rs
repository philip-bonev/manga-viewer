use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ArchiveFormat {
    Zip,
    Tar,
    SevenZ,
}

pub(crate) struct EpubState {
    pub file_path: Option<PathBuf>,
    pub cli_file: Option<String>,
    pub archive_format: Option<ArchiveFormat>,
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
pub struct EpubImages {
    pub images: Vec<ImageInfo>,
}

fn resolve_base_path(opf_content: &str) -> String {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(opf_content);
    reader.config_mut().trim_text(true);

    let mut base_path = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"package" => {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"xml:base" {
                        base_path = String::from_utf8_lossy(&attr.value).to_string();
                        break;
                    }
                }
                break;
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => continue,
        }
    }

    base_path
}

fn parse_opf(opf_content: &str) -> (HashMap<String, (String, String)>, Vec<String>) {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(opf_content);
    reader.config_mut().trim_text(true);

    let mut manifest: HashMap<String, (String, String)> = HashMap::new();
    let mut spine: Vec<String> = Vec::new();
    let mut in_manifest = false;
    let mut in_spine = false;
    let mut current_item_id = String::new();
    let mut current_item_href = String::new();
    let mut current_item_media = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"manifest" => in_manifest = true,
                b"spine" => in_spine = true,
                b"item" if in_manifest => {
                    current_item_id.clear();
                    current_item_href.clear();
                    current_item_media.clear();
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"id" => {
                                current_item_id = String::from_utf8_lossy(&attr.value).to_string();
                            }
                            b"href" => {
                                current_item_href =
                                    String::from_utf8_lossy(&attr.value).to_string();
                            }
                            b"media-type" => {
                                current_item_media =
                                    String::from_utf8_lossy(&attr.value).to_string();
                            }
                            _ => {}
                        }
                    }
                }
                b"itemref" if in_spine => {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"idref" {
                            let idref = String::from_utf8_lossy(&attr.value).to_string();
                            spine.push(idref);
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"item" if in_manifest && !current_item_id.is_empty() => {
                    manifest.insert(
                        current_item_id.clone(),
                        (current_item_href.clone(), current_item_media.clone()),
                    );
                }
                b"manifest" => in_manifest = false,
                b"spine" => in_spine = false,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => continue,
        }
    }

    (manifest, spine)
}

fn extract_images_from_html(
    html_content: &str,
    base_path: &str,
    manifest: &HashMap<String, (String, String)>,
) -> Vec<String> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(html_content);
    reader.config_mut().trim_text(true);

    let mut images = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                if e.name().as_ref() == b"img" || e.name().as_ref() == b"image" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"src" || attr.key.as_ref() == b"xlink:href" {
                            let src = String::from_utf8_lossy(&attr.value).to_string();
                            let resolved = if base_path.is_empty() {
                                src.clone()
                            } else {
                                format!("{}/{}", base_path, src)
                            };
                            images.push(resolved);
                        }
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                if e.name().as_ref() == b"img" || e.name().as_ref() == b"image" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"src" || attr.key.as_ref() == b"xlink:href" {
                            let src = String::from_utf8_lossy(&attr.value).to_string();
                            let resolved = if base_path.is_empty() {
                                src.clone()
                            } else {
                                format!("{}/{}", base_path, src)
                            };
                            images.push(resolved);
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => continue,
        }
    }

    images
        .into_iter()
        .filter(|href| {
            manifest.values().any(|(_, media)| {
                media.starts_with("image/") && href.ends_with(&href.split('/').last().unwrap_or(""))
            })
        })
        .collect()
}

fn normalize_path(base: &str, relative: &str) -> String {
    let mut parts: Vec<&str> = base.split('/').collect();
    parts.pop();

    for part in relative.split('/') {
        match part {
            ".." => {
                if !parts.is_empty() {
                    parts.pop();
                }
            }
            "." => {}
            _ => parts.push(part),
        }
    }

    parts.join("/")
}

#[tauri::command]
fn open_epub_file(state: State<'_, Mutex<EpubState>>, path: String) -> Result<EpubImages, String> {
    let file_path = std::path::PathBuf::from(&path);

    let path = file_path;

    let mut epub_file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    epub_file
        .read_to_end(&mut buffer)
        .map_err(|e| e.to_string())?;

    let cursor = std::io::Cursor::new(buffer);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let mut container_xml = String::new();
    archive
        .by_name("META-INF/container.xml")
        .map_err(|e| e.to_string())?
        .read_to_string(&mut container_xml)
        .map_err(|e| e.to_string())?;

    let opf_path = {
        use quick_xml::events::Event;
        use quick_xml::reader::Reader;

        let mut reader = Reader::from_str(&container_xml);
        reader.config_mut().trim_text(true);
        let mut opf_path = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    if e.name().as_ref() == b"rootfile" {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"full-path" {
                                opf_path = String::from_utf8_lossy(&attr.value).to_string();
                                break;
                            }
                        }
                    }
                    if !opf_path.is_empty() {
                        break;
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => continue,
            }
        }

        if opf_path.is_empty() {
            return Err("Could not find OPF file in EPUB".to_string());
        }
        opf_path
    };

    let mut opf_content = String::new();
    archive
        .by_name(&opf_path)
        .map_err(|e| e.to_string())?
        .read_to_string(&mut opf_content)
        .map_err(|e| e.to_string())?;

    let opf_dir = std::path::Path::new(&opf_path)
        .parent()
        .and_then(|p| p.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let xml_base = resolve_base_path(&opf_content);
    let (manifest, spine) = parse_opf(&opf_content);

    let resolve = |href: &str| -> String {
        let href = if xml_base.is_empty() {
            href.to_string()
        } else {
            format!("{}/{}", xml_base, href)
        };
        if opf_dir.is_empty() {
            href
        } else {
            format!("{}/{}", opf_dir, href)
        }
    };

    let mut all_images: Vec<ImageInfo> = Vec::new();
    let mut seen_hrefs: HashMap<String, usize> = HashMap::new();

    for item_id in &spine {
        if let Some((href, media_type)) = manifest.get(item_id) {
            if media_type.starts_with("text/") || media_type.contains("html") {
                let full_href = resolve(href);

                if let Ok(mut file) = archive.by_name(&full_href) {
                    let mut html_content = String::new();
                    if file.read_to_string(&mut html_content).is_ok() {
                        let img_hrefs =
                            extract_images_from_html(&html_content, &xml_base, &manifest);
                        for img_href in img_hrefs {
                            let normalized = normalize_path(&full_href, &img_href);
                            if let Some((_, (_, mime_type))) =
                                manifest.iter().find(|(_, (h, _))| {
                                    resolve(h) == normalized || h == &img_href
                                })
                            {
                                let mime = mime_type.clone();
                                let id = item_id.clone();
                                let entry = seen_hrefs.entry(normalized.clone()).or_insert(0);
                                *entry += 1;
                                all_images.push(ImageInfo {
                                    id: format!("{}-{}", id, entry),
                                    href: normalized,
                                    mime_type: mime,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    if all_images.is_empty() {
        for (id, (href, media_type)) in &manifest {
            if media_type.starts_with("image/") {
                all_images.push(ImageInfo {
                    id: id.clone(),
                    href: resolve(href),
                    mime_type: media_type.clone(),
                });
            }
        }
    }

    {
        let mut state = state.lock().unwrap();
        state.file_path = Some(path);
        state.archive_format = Some(ArchiveFormat::Zip);
    }

    Ok(EpubImages { images: all_images })
}

fn read_zip_entry(path: &std::path::Path, href: &str) -> Result<Vec<u8>, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
    let cursor = std::io::Cursor::new(buffer);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    let mut entry = archive.by_name(href).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
    Ok(data)
}

fn read_tar_entry(path: &std::path::Path, href: &str) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = std::io::BufReader::new(file);
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let entry_path = entry.path().map_err(|e| e.to_string())?.to_string_lossy().to_string();
        if entry_path == href {
            let mut data = Vec::new();
            entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
            return Ok(data);
        }
    }
    Err(format!("Entry '{}' not found in archive", href))
}

fn read_7z_entry(path: &std::path::Path, href: &str) -> Result<Vec<u8>, String> {
    let mut archive = sevenz_rust::SevenZReader::open(
        path.to_string_lossy().as_ref(),
        "".into(),
    )
    .map_err(|e| e.to_string())?;

    let mut result: Option<Vec<u8>> = None;
    archive
        .for_each_entries(|entry, reader| {
            if entry.name() == href {
                let mut data = Vec::new();
                if std::io::Read::read_to_end(reader, &mut data).is_ok() {
                    result = Some(data);
                }
                Ok(false)
            } else {
                Ok(true)
            }
        })
        .map_err(|e| e.to_string())?;

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

fn collect_images(
    entries: impl Iterator<Item = (String, String)>,
) -> Vec<ImageInfo> {
    let mut images: Vec<ImageInfo> = entries
        .filter(|(name, _)| {
            let ext = name.split('.').last().unwrap_or("").to_lowercase();
            IMAGE_EXTENSIONS.contains(&ext.as_str())
        })
        .map(|(name, _)| {
            let ext = name.split('.').last().unwrap_or("").to_lowercase();
            ImageInfo {
                id: String::new(),
                href: name,
                mime_type: mime_from_ext(&ext).to_string(),
            }
        })
        .collect();

    images.sort_by(|a, b| a.href.cmp(&b.href));

    for (i, img) in images.iter_mut().enumerate() {
        img.id = format!("{:05}", i);
    }

    images
}

#[tauri::command]
fn get_image_data(state: State<'_, Mutex<EpubState>>, href: String) -> Result<String, String> {
    let state = state.lock().unwrap();
    let path = state
        .file_path
        .as_ref()
        .ok_or("No file opened")?
        .clone();
    let fmt = state.archive_format.unwrap_or(ArchiveFormat::Zip);
    drop(state);

    let image_data = match fmt {
        ArchiveFormat::Zip => read_zip_entry(&path, &href)?,
        ArchiveFormat::Tar => read_tar_entry(&path, &href)?,
        ArchiveFormat::SevenZ => read_7z_entry(&path, &href)?,
    };

    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image_data);
    Ok(encoded)
}

#[tauri::command]
fn close_epub(state: State<'_, Mutex<EpubState>>) -> Result<(), String> {
    let mut state = state.lock().unwrap();
    state.file_path = None;
    state.archive_format = None;
    Ok(())
}

fn extract_series_info(file_stem: &str) -> Option<(String, f64)> {
    let re = regex::Regex::new(r"^(.*?)[-_ ](\d+(?:\.\d+)?)").ok()?;
    let captures = re.captures(file_stem)?;
    let series = captures.get(1)?.as_str().trim().to_lowercase();
    let num: f64 = captures.get(2)?.as_str().parse().ok()?;
    if series.is_empty() {
        return None;
    }
    Some((series, num))
}

#[tauri::command]
fn get_next_file(path: String) -> Option<String> {
    let file_path = std::path::PathBuf::from(&path);
    let parent = file_path.parent()?;
    let file_name = file_path.file_stem()?.to_string_lossy().to_string();
    let extension = file_path.extension()?.to_string_lossy().to_string();

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
            let entry_stem = entry_path.file_stem()?.to_string_lossy().to_string();
            if let Some((series, num)) = extract_series_info(&entry_stem) {
                if series == current_series {
                    candidates.push((num, entry_path));
                }
            }
        }
    }

    candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    for (num, next_path) in candidates {
        if num > current_num {
            return next_path.to_str().map(|s| s.to_string());
        }
    }

    None
}

#[tauri::command]
fn get_prev_file(path: String) -> Option<String> {
    let file_path = std::path::PathBuf::from(&path);
    let parent = file_path.parent()?;
    let file_name = file_path.file_stem()?.to_string_lossy().to_string();
    let extension = file_path.extension()?.to_string_lossy().to_string();

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
            let entry_stem = entry_path.file_stem()?.to_string_lossy().to_string();
            if let Some((series, num)) = extract_series_info(&entry_stem) {
                if series == current_series {
                    candidates.push((num, entry_path));
                }
            }
        }
    }

    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    for (num, prev_path) in candidates {
        if num < current_num {
            return prev_path.to_str().map(|s| s.to_string());
        }
    }

    None
}

#[tauri::command]
fn open_cbz_file(state: State<'_, Mutex<EpubState>>, path: String) -> Result<EpubImages, String> {
    let file_path = std::path::PathBuf::from(&path);

    let mut file = std::fs::File::open(&file_path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
    let cursor = std::io::Cursor::new(buffer);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let entries = (0..archive.len()).filter_map(|i| {
        let file = archive.by_index(i).ok()?;
        let name = file.name().to_string();
        if name.starts_with("__MACOSX") || name.ends_with('/') {
            return None;
        }
        let ext = name.split('.').last().unwrap_or("").to_lowercase();
        if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            Some((name, ext))
        } else {
            None
        }
    });

    let images = collect_images(entries);

    {
        let mut state = state.lock().unwrap();
        state.file_path = Some(file_path);
        state.archive_format = Some(ArchiveFormat::Zip);
    }

    Ok(EpubImages { images })
}

#[tauri::command]
fn open_cbt_file(state: State<'_, Mutex<EpubState>>, path: String) -> Result<EpubImages, String> {
    let file_path = std::path::PathBuf::from(&path);

    let file = std::fs::File::open(&file_path).map_err(|e| e.to_string())?;
    let reader = std::io::BufReader::new(file);
    let mut archive = tar::Archive::new(reader);

    let entries = archive.entries().map_err(|e| e.to_string())?;
    let names: Vec<(String, String)> = entries
        .filter_map(|e| {
            let entry = e.ok()?;
            let path = entry.path().ok()?;
            let name = path.to_string_lossy().to_string();
            if name.ends_with('/') {
                return None;
            }
            let ext = name.split('.').last().unwrap_or("").to_lowercase();
            if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                Some((name, ext))
            } else {
                None
            }
        })
        .collect();

    let images = collect_images(names.into_iter());

    {
        let mut state = state.lock().unwrap();
        state.file_path = Some(file_path);
        state.archive_format = Some(ArchiveFormat::Tar);
    }

    Ok(EpubImages { images })
}

#[tauri::command]
fn open_cb7_file(state: State<'_, Mutex<EpubState>>, path: String) -> Result<EpubImages, String> {
    let file_path = std::path::PathBuf::from(&path);

    let reader = sevenz_rust::SevenZReader::open(
        file_path.to_string_lossy().as_ref(),
        "".into(),
    )
    .map_err(|e| e.to_string())?;

    let entries: Vec<(String, String)> = reader
        .archive()
        .files
        .iter()
        .filter_map(|entry| {
            let name = entry.name().to_string();
            if entry.is_directory() {
                return None;
            }
            let ext = name.split('.').last().unwrap_or("").to_lowercase();
            if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                Some((name, ext))
            } else {
                None
            }
        })
        .collect();

    let images = collect_images(entries.into_iter());

    {
        let mut state = state.lock().unwrap();
        state.file_path = Some(file_path);
        state.archive_format = Some(ArchiveFormat::SevenZ);
    }

    Ok(EpubImages { images })
}

#[tauri::command]
fn get_cli_file(state: State<'_, Mutex<EpubState>>) -> Option<String> {
    let mut state = state.lock().unwrap();
    state.cli_file.take()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cli_file = find_file_in_args(std::env::args().skip(1));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            use tauri::Emitter;
            use tauri::Manager;
            if let Some(path) = find_file_in_args(argv.iter().cloned()) {
                let state = app.state::<Mutex<EpubState>>();
                let mut state = state.inner().lock().unwrap();
                state.cli_file = Some(path.clone());
                let _ = app.emit("file-opened", path);
            }
        }))
        .manage(Mutex::new(EpubState {
            file_path: None,
            cli_file,
            archive_format: None,
        }))
        .invoke_handler(tauri::generate_handler![
            open_epub_file,
            open_cbz_file,
            open_cbt_file,
            open_cb7_file,
            get_image_data,
            close_epub,
            get_cli_file,
            get_next_file,
            get_prev_file
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    #[allow(unused_variables)]
    app.run(|app_handle, event| {
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
        if let tauri::RunEvent::Opened { urls } = event {
            use tauri::Emitter;
            use tauri::Manager;
            for url in urls {
                if url.scheme() == "file" {
                    if let Ok(path) = url.to_file_path() {
                        if let Some(ext) = path.extension() {
                            let ext = ext.to_string_lossy().to_lowercase();
                            if matches!(ext.as_ref(), "epub" | "cbz" | "cbt" | "cb7") {
                                let path_str = path.to_string_lossy().to_string();
                                let state = app_handle.state::<Mutex<EpubState>>();
                                let mut state = state.inner().lock().unwrap();
                                state.cli_file = Some(path_str.clone());
                                let _ = app_handle.emit("file-opened", path_str);
                            }
                        }
                    }
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
        let path = std::path::Path::new(&arg);
        if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if matches!(ext.as_ref(), "epub" | "cbz" | "cbt" | "cb7") {
                let decoded = percent_encoding::percent_decode_str(&arg).decode_utf8_lossy();
                return Some(
                    decoded
                        .strip_prefix("file://")
                        .unwrap_or(&decoded)
                        .to_string(),
                );
            }
        }
    }
    None
}
