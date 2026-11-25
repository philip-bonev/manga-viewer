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

//! Entry filtering, MIME inference and page listing/sorting.

use crate::model::ImageInfo;

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "tiff", "tif",
];

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

pub(crate) fn ext_of(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_lowercase())
        .unwrap_or_default()
}

pub(crate) fn is_image_entry(name: &str) -> bool {
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

pub(crate) fn collect_images(names: impl Iterator<Item = String>) -> Vec<ImageInfo> {
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
}
