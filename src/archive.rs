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

//! Archive opening and entry reading for `.cbz` (zip), `.cbt` (tar) and
//! `.cb7` (7z).

use crate::images::{collect_images, is_image_entry};
use crate::model::ImageInfo;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

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

pub(crate) fn read_entry(
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

pub(crate) fn open_zip(path: &std::path::Path) -> Result<(Vec<ImageInfo>, OpenArchive), String> {
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

pub(crate) fn open_tar(path: &std::path::Path) -> Result<(Vec<ImageInfo>, OpenArchive), String> {
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

pub(crate) fn open_sevenz(path: &std::path::Path) -> Result<(Vec<ImageInfo>, OpenArchive), String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::images::ext_of;

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
