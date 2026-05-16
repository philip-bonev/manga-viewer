const { invoke } = window.__TAURI__.core;

let images = [];
let loadedImageCount = 0;
const LAZY_LOAD_THRESHOLD = 5;

const openBtn = document.getElementById('open-btn');
const closeBtn = document.getElementById('close-btn');
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

  const images = imagesContainer.querySelectorAll('img');

  for (let i = 0; i < images.length; i++) {
    const img = images[i];
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
        updateCounter(loadedImageCount, images.length);
      }
      delete img.dataset.loading;
    }
  }
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

  await loadNearbyImages();
  hideLoading();
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
    const ext = filePath.split('.').pop().toLowerCase();
    const command = ext === 'cbz' ? 'open_cbz_file' : 'open_epub_file';
    const result = await invoke(command, { path: filePath });
    if (result.images && result.images.length > 0) {
      const displayName = filePath.split('/').pop() || filePath.split('\\').pop() || 'EPUB';
      updateFileName(displayName);
      closeBtn.disabled = false;
      await displayImages(result);
    } else {
      showPlaceholder();
      updateFileName('No file opened');
      closeBtn.disabled = true;
      hideLoading();
    }
  } catch (error) {
    console.error('Failed to open EPUB:', error);
    showPlaceholder();
    updateFileName('Error opening file');
    closeBtn.disabled = true;
    hideLoading();
  }
}

async function closeFile() {
  try {
    await invoke('close_epub');
    images = [];
    loadedImageCount = 0;
    showPlaceholder();
    updateFileName('No file opened');
    closeBtn.disabled = true;
  } catch (error) {
    console.error('Failed to close EPUB:', error);
  }
}

openBtn.addEventListener('click', openFile);
closeBtn.addEventListener('click', closeFile);

viewerContainer.addEventListener('scroll', () => {
  loadNearbyImages();
});

window.addEventListener('DOMContentLoaded', () => {
  showPlaceholder();
});
