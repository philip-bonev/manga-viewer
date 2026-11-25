/*
 * Copyright 2026 Philip Bonev
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://apache.org
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

const { invoke } = window.__TAURI__.core;

let images = [];
let currentFilePath = null;
let zoomLevel = 100;
let loadGeneration = 0;
const ZOOM_STEP = 25;
const ZOOM_MIN = 25;
const ZOOM_MAX = 500;
const LAZY_LOAD_THRESHOLD = 5;
const LOAD_CONCURRENCY = 4;

const openBtn = document.getElementById('open-btn');
const prevBtn = document.getElementById('prev-btn');
const nextBtn = document.getElementById('next-btn');
const zoomInBtn = document.getElementById('zoom-in-btn');
const zoomOutBtn = document.getElementById('zoom-out-btn');
const zoomFitBtn = document.getElementById('zoom-fit-btn');
const zoomLevelDisplay = document.getElementById('zoom-level');
const fileName = document.getElementById('file-name');
const viewerContainer = document.getElementById('viewer-container');
const imagesContainer = document.getElementById('images-container');
const placeholder = document.getElementById('placeholder');
const loadingIndicator = document.getElementById('loading-indicator');
const imageCounter = document.getElementById('image-counter');
const currentImageSpan = document.getElementById('current-image');
const totalImagesSpan = document.getElementById('total-images');

function showPlaceholder() {
  placeholder.style.display = 'flex';
  imagesContainer.innerHTML = '';
  imageCounter.classList.add('hidden');
  loadGeneration++;
  images = [];
  currentFilePath = null;
  zoomLevel = 100;
  applyZoom();
  prevBtn.disabled = true;
  nextBtn.disabled = true;
}

function hidePlaceholder() {
  placeholder.style.display = 'none';
}

function showLoading() {
  loadingIndicator.classList.remove('hidden');
}

function hideLoading() {
  loadingIndicator.classList.add('hidden');
}

function updateFileName(name) {
  fileName.textContent = name;
}

function updateCounter(current, total) {
  currentImageSpan.textContent = current;
  totalImagesSpan.textContent = total;
}

async function loadImageData(href, mimeType) {
  try {
    const base64Data = await invoke('get_image_data', { href });
    return `data:${mimeType || 'image/jpeg'};base64,${base64Data}`;
  } catch (error) {
    console.error('Failed to load image:', error);
    return null;
  }
}

function createImageElement(imageInfo, index) {
  const img = document.createElement('img');
  img.dataset.href = imageInfo.href;
  img.dataset.mime = imageInfo.mime_type;
  img.dataset.index = index;
  img.alt = `Page ${index + 1}`;
  img.style.minHeight = '200px';
  img.style.backgroundColor = '#2d2d2d';
  return img;
}

async function loadNearbyImages() {
  const generation = loadGeneration;
  const scrollTop = viewerContainer.scrollTop;
  const viewportHeight = viewerContainer.clientHeight;
  const viewerTop = viewerContainer.getBoundingClientRect().top;
  const margin = viewportHeight * LAZY_LOAD_THRESHOLD;

  const imgs = imagesContainer.querySelectorAll('img');
  const pending = [];

  for (const img of imgs) {
    if (img.src || img.dataset.loading) {
      continue;
    }
    const rect = img.getBoundingClientRect();
    const imgTop = rect.top - viewerTop + scrollTop;
    const imgBottom = rect.bottom - viewerTop + scrollTop;
    const isNearViewport = (
      imgBottom >= scrollTop - margin &&
      imgTop <= scrollTop + viewportHeight + margin
    );
    if (isNearViewport) {
      pending.push(img);
    }
  }

  for (let i = 0; i < pending.length; i += LOAD_CONCURRENCY) {
    if (generation !== loadGeneration) {
      return;
    }
    const batch = pending.slice(i, i + LOAD_CONCURRENCY);
    await Promise.all(batch.map(async (img) => {
      img.dataset.loading = 'true';
      const dataUrl = await loadImageData(img.dataset.href, img.dataset.mime);
      delete img.dataset.loading;
      if (generation !== loadGeneration) {
        return;
      }
      if (dataUrl) {
        img.src = dataUrl;
      }
    }));
  }
}

function updateCurrentPage() {
  const imgs = imagesContainer.querySelectorAll('img');
  if (imgs.length === 0) {
    return;
  }
  const viewerTop = viewerContainer.getBoundingClientRect().top;
  let current = imgs.length;
  for (let i = 0; i < imgs.length; i++) {
    if (imgs[i].getBoundingClientRect().bottom > viewerTop) {
      current = i + 1;
      break;
    }
  }
  updateCounter(current, imgs.length);
}

function applyZoom() {
  const prevWidth = imagesContainer.offsetWidth || 1;
  const prevScrollLeft = viewerContainer.scrollLeft;
  const prevScrollTop = viewerContainer.scrollTop;

  imagesContainer.style.width = `${zoomLevel}%`;
  zoomLevelDisplay.textContent = `${zoomLevel}%`;
  zoomOutBtn.disabled = zoomLevel <= ZOOM_MIN;
  zoomInBtn.disabled = zoomLevel >= ZOOM_MAX;

  // Keep the content point at the viewport center in view after resizing.
  const scale = imagesContainer.offsetWidth / prevWidth;
  viewerContainer.scrollLeft =
    (prevScrollLeft + viewerContainer.clientWidth / 2) * scale - viewerContainer.clientWidth / 2;
  viewerContainer.scrollTop =
    (prevScrollTop + viewerContainer.clientHeight / 2) * scale - viewerContainer.clientHeight / 2;
}

function zoomIn() {
  if (zoomLevel < ZOOM_MAX) {
    zoomLevel = Math.min(ZOOM_MAX, zoomLevel + ZOOM_STEP);
    applyZoom();
  }
}

function zoomOut() {
  if (zoomLevel > ZOOM_MIN) {
    zoomLevel = Math.max(ZOOM_MIN, zoomLevel - ZOOM_STEP);
    applyZoom();
  }
}

function zoomFit() {
  zoomLevel = 100;
  applyZoom();
}

function enableZoomControls() {
  zoomInBtn.disabled = false;
  zoomOutBtn.disabled = false;
  zoomFitBtn.disabled = false;
}

function disableZoomControls() {
  zoomInBtn.disabled = true;
  zoomOutBtn.disabled = true;
  zoomFitBtn.disabled = true;
}

async function displayImages(archiveImages) {
  images = archiveImages.images;
  loadGeneration++;

  if (images.length === 0) {
    showPlaceholder();
    hideLoading();
    return;
  }

  hidePlaceholder();
  imagesContainer.innerHTML = '';

  for (let i = 0; i < images.length; i++) {
    const img = createImageElement(images[i], i);
    imagesContainer.appendChild(img);
  }

  imageCounter.classList.remove('hidden');

  viewerContainer.scrollTop = 0;

  zoomLevel = 100;
  applyZoom();
  updateCurrentPage();

  await loadNearbyImages();
  hideLoading();
}

async function updateNavButtons() {
  if (!currentFilePath) {
    prevBtn.disabled = true;
    nextBtn.disabled = true;
    return;
  }

  const prevResult = await invoke('get_prev_file', { path: currentFilePath });
  const nextResult = await invoke('get_next_file', { path: currentFilePath });

  prevBtn.disabled = !prevResult;
  nextBtn.disabled = !nextResult;
}

async function openFileByPath(filePath) {
  showLoading();
  try {
    const ext = filePath.split('.').pop().toLowerCase();
    const command =
      ext === 'cbz' ? 'open_cbz_file' :
      ext === 'cbt' ? 'open_cbt_file' :
      ext === 'cb7' ? 'open_cb7_file' :
      null;
    if (!command) {
      throw new Error(`Unsupported file format: .${ext}`);
    }
    const result = await invoke(command, { path: filePath });
    if (result.images && result.images.length > 0) {
      currentFilePath = filePath;
      const displayName = filePath.split(/[\\/]/).pop() || 'Unknown';
      updateFileName(displayName);
      enableZoomControls();
      await displayImages(result);
      await updateNavButtons();
    } else {
      showPlaceholder();
      updateFileName('No images found');
      disableZoomControls();
      hideLoading();
    }
  } catch (error) {
    console.error('Failed to open file:', error);
    showPlaceholder();
    updateFileName(`Error: ${error?.message || error}`);
    disableZoomControls();
    hideLoading();
  }
}

async function openFile() {
  showLoading();
  try {
    const { open } = window.__TAURI__.dialog;
    const selected = await open({
      filters: [
        { name: 'Comic Files', extensions: ['cbz', 'cbt', 'cb7'] }
      ]
    });

    if (!selected) {
      hideLoading();
      return;
    }

    const filePath = typeof selected === 'string' ? selected : selected.path;
    await openFileByPath(filePath);
  } catch (error) {
    console.error('Failed to open file:', error);
    showPlaceholder();
    updateFileName(`Error: ${error?.message || error}`);
    disableZoomControls();
    hideLoading();
  }
}

async function openNextFile() {
  if (!currentFilePath) return;
  const nextPath = await invoke('get_next_file', { path: currentFilePath });
  if (nextPath) {
    await openFileByPath(nextPath);
  }
}

async function openPrevFile() {
  if (!currentFilePath) return;
  const prevPath = await invoke('get_prev_file', { path: currentFilePath });
  if (prevPath) {
    await openFileByPath(prevPath);
  }
}

openBtn.addEventListener('click', openFile);
prevBtn.addEventListener('click', openPrevFile);
nextBtn.addEventListener('click', openNextFile);
zoomInBtn.addEventListener('click', zoomIn);
zoomOutBtn.addEventListener('click', zoomOut);
zoomFitBtn.addEventListener('click', zoomFit);

let scrollRaf = 0;
viewerContainer.addEventListener('scroll', () => {
  if (scrollRaf) {
    return;
  }
  scrollRaf = requestAnimationFrame(() => {
    scrollRaf = 0;
    loadNearbyImages();
    updateCurrentPage();
  });
});

viewerContainer.addEventListener('wheel', (e) => {
  if (e.ctrlKey) {
    e.preventDefault();
    if (e.deltaY < 0) {
      zoomIn();
    } else {
      zoomOut();
    }
  }
}, { passive: false });

const aboutBtn = document.getElementById('about-btn');
const aboutDialog = document.getElementById('about-dialog');
const aboutCloseBtn = document.getElementById('about-close-btn');
const aboutName = document.getElementById('about-name');
const aboutVersion = document.getElementById('about-version');
const aboutBuildDate = document.getElementById('about-build-date');
const aboutAuthor = document.getElementById('about-author');
const aboutLicenseName = document.getElementById('about-license-name');
const aboutRepository = document.getElementById('about-repository');
const aboutLicenseText = document.getElementById('about-license-text');

async function showAboutDialog() {
  try {
    const info = await invoke('get_app_info');
    aboutName.textContent = info.name;
    aboutVersion.textContent = info.version;
    aboutBuildDate.textContent = info.build_date;
    aboutAuthor.textContent = info.author;
    aboutLicenseName.textContent = info.license_name;
    aboutRepository.textContent = info.repository;
    aboutRepository.href = info.repository;
    aboutLicenseText.textContent = info.license_text;
    aboutDialog.classList.remove('hidden');
  } catch (error) {
    console.error('Failed to load app info:', error);
  }
}

function hideAboutDialog() {
  aboutDialog.classList.add('hidden');
}

aboutBtn.addEventListener('click', showAboutDialog);
aboutCloseBtn.addEventListener('click', hideAboutDialog);
aboutDialog.addEventListener('click', (e) => {
  if (e.target === aboutDialog) {
    hideAboutDialog();
  }
});

window.addEventListener('keydown', (e) => {
  if (e.key === '+' || e.key === '=') {
    zoomIn();
  } else if (e.key === '-') {
    zoomOut();
  } else if (e.key === '0') {
    zoomFit();
  } else if (e.key === 'Escape') {
    hideAboutDialog();
  }
});

window.addEventListener('DOMContentLoaded', async () => {
  const body = document.body;
  let dragCounter = 0;

  body.addEventListener('dragenter', (e) => {
    e.preventDefault();
    dragCounter++;
    body.classList.add('drag-over');
  });

  body.addEventListener('dragover', (e) => {
    e.preventDefault();
    body.classList.add('drag-over');
  });

  body.addEventListener('dragleave', (e) => {
    e.preventDefault();
    dragCounter--;
    if (dragCounter === 0) {
      body.classList.remove('drag-over');
    }
  });

  body.addEventListener('drop', (e) => {
    e.preventDefault();
    dragCounter = 0;
    body.classList.remove('drag-over');
  });

  const { listen } = window.__TAURI__.event;
  await listen('tauri://drag-drop', async (event) => {
    body.classList.remove('drag-over');
    dragCounter = 0;
    const payload = event.payload;
    if (payload && payload.paths && payload.paths.length > 0) {
      const filePath = payload.paths[0];
      await openFileByPath(filePath);
    }
  });

  await listen('file-opened', async (event) => {
    if (event.payload) {
      const filePath = typeof event.payload === 'string' ? event.payload : event.payload.path;
      if (filePath) {
        await openFileByPath(filePath);
      }
    }
  });

  // Consume the CLI file last so no file-opened event can be missed while
  // the listeners above are still being registered.
  const cliFile = await invoke('get_cli_file');
  if (cliFile) {
    await openFileByPath(cliFile);
  } else {
    showPlaceholder();
  }
});
