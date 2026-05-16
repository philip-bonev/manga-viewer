const { invoke } = window.__TAURI__.core;

let images = [];
let loadedImageCount = 0;
let currentFilePath = null;
let zoomLevel = 100;
const ZOOM_STEP = 25;
const ZOOM_MIN = 25;
const ZOOM_MAX = 500;
const LAZY_LOAD_THRESHOLD = 5;

const openBtn = document.getElementById('open-btn');
const prevBtn = document.getElementById('prev-btn');
const nextBtn = document.getElementById('next-btn');
const closeBtn = document.getElementById('close-btn');
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

function getImageMimeType(href) {
  const ext = href.split('.').pop().toLowerCase();
  const mimeTypes = {
    'jpg': 'image/jpeg',
    'jpeg': 'image/jpeg',
    'png': 'image/png',
    'gif': 'image/gif',
    'webp': 'image/webp',
    'svg': 'image/svg+xml',
    'bmp': 'image/bmp',
  };
  return mimeTypes[ext] || 'image/jpeg';
}

async function loadImageData(href) {
  try {
    const base64Data = await invoke('get_image_data', { href });
    const mimeType = getImageMimeType(href);
    return `data:${mimeType};base64,${base64Data}`;
  } catch (error) {
    console.error('Failed to load image:', error);
    return null;
  }
}

function createImageElement(imageInfo, index) {
  const img = document.createElement('img');
  img.dataset.href = imageInfo.href;
  img.dataset.index = index;
  img.alt = `Page ${index + 1}`;
  img.style.minHeight = '200px';
  img.style.backgroundColor = '#2d2d2d';
  return img;
}

async function loadNearbyImages() {
  const scrollTop = viewerContainer.scrollTop;
  const viewportHeight = viewerContainer.clientHeight;
  const viewportBottom = scrollTop + viewportHeight;

  const imgs = imagesContainer.querySelectorAll('img');

  for (let i = 0; i < imgs.length; i++) {
    const img = imgs[i];
    if (img.src && !img.dataset.loading) {
      continue;
    }

    const rect = img.getBoundingClientRect();
    const imgTop = rect.top + scrollTop;
    const imgBottom = rect.bottom + scrollTop;

    const isNearViewport = (
      imgBottom >= (scrollTop - viewportHeight * LAZY_LOAD_THRESHOLD) &&
      imgTop <= (viewportBottom + viewportHeight * LAZY_LOAD_THRESHOLD)
    );

    if (isNearViewport && !img.src && !img.dataset.loading) {
      img.dataset.loading = 'true';
      const dataUrl = await loadImageData(img.dataset.href);
      if (dataUrl) {
        img.src = dataUrl;
        loadedImageCount++;
        updateCounter(loadedImageCount, imgs.length);
      }
      delete img.dataset.loading;
    }
  }
}

function applyZoom() {
  imagesContainer.style.transform = `scale(${zoomLevel / 100})`;
  zoomLevelDisplay.textContent = `${zoomLevel}%`;
  zoomOutBtn.disabled = zoomLevel <= ZOOM_MIN;
  zoomInBtn.disabled = zoomLevel >= ZOOM_MAX;
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

async function displayImages(epubImages) {
  images = epubImages.images;
  loadedImageCount = 0;

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

  updateCounter(0, images.length);
  imageCounter.classList.remove('hidden');

  viewerContainer.scrollTop = 0;

  zoomLevel = 100;
  applyZoom();

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
    const command = ext === 'cbz' ? 'open_cbz_file' : 'open_epub_file';
    const result = await invoke(command, { path: filePath });
    if (result.images && result.images.length > 0) {
      currentFilePath = filePath;
      const displayName = filePath.split('/').pop() || filePath.split('\\').pop() || 'EPUB';
      updateFileName(displayName);
      closeBtn.disabled = false;
      enableZoomControls();
      await displayImages(result);
      await updateNavButtons();
    } else {
      showPlaceholder();
      updateFileName('No images found');
      closeBtn.disabled = true;
      disableZoomControls();
      hideLoading();
    }
  } catch (error) {
    console.error('Failed to open file:', error);
    showPlaceholder();
    updateFileName('Error opening file');
    closeBtn.disabled = true;
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
        { name: 'Comic Files', extensions: ['epub', 'cbz'] }
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
    updateFileName('Error opening file');
    closeBtn.disabled = true;
    disableZoomControls();
    hideLoading();
  }
}

async function closeFile() {
  try {
    await invoke('close_epub');
    currentFilePath = null;
    images = [];
    loadedImageCount = 0;
    zoomLevel = 100;
    applyZoom();
    showPlaceholder();
    updateFileName('No file opened');
    closeBtn.disabled = true;
    prevBtn.disabled = true;
    nextBtn.disabled = true;
    disableZoomControls();
  } catch (error) {
    console.error('Failed to close file:', error);
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
closeBtn.addEventListener('click', closeFile);
zoomInBtn.addEventListener('click', zoomIn);
zoomOutBtn.addEventListener('click', zoomOut);
zoomFitBtn.addEventListener('click', zoomFit);

viewerContainer.addEventListener('scroll', () => {
  loadNearbyImages();
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

window.addEventListener('keydown', (e) => {
  if (e.key === '+' || e.key === '=') {
    zoomIn();
  } else if (e.key === '-') {
    zoomOut();
  } else if (e.key === '0') {
    zoomFit();
  }
});

window.addEventListener('DOMContentLoaded', async () => {
  const cliFile = await invoke('get_cli_file');
  if (cliFile) {
    await openFileByPath(cliFile);
  } else {
    showPlaceholder();
  }
});
