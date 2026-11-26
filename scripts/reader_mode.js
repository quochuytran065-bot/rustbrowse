/**
 * RustBrowse — Smart Reader Mode (1-Touch Clean Reading)
 * Injected automatically via add_script_to_execute_on_document_created
 * Triggered via Alt+R or host message 'TOGGLE_READER_MODE'
 */
(function () {
  'use strict';

  if (window.__rustbrowse_reader_mode_injected) return;
  window.__rustbrowse_reader_mode_injected = true;

  let isReaderActive = false;

  function toggleReaderMode() {
    if (isReaderActive) {
      exitReaderMode();
    } else {
      enterReaderMode();
    }
  }

  function enterReaderMode() {
    const existing = document.getElementById('__rustbrowse_reader_container');
    if (existing) return;

    // 1. Find best content candidate
    let contentEl = document.querySelector('article') || document.querySelector('main') || document.querySelector('.post-content, .article-content, #article-body, #content');
    if (!contentEl) {
      // Fallback: search for div with highest text density
      const divs = document.querySelectorAll('div, section');
      let maxLen = 0;
      for (let i = 0; i < divs.length; i++) {
        const d = divs[i];
        const len = (d.innerText || '').length;
        if (len > maxLen && d.querySelectorAll('p').length >= 3) {
          maxLen = len;
          contentEl = d;
        }
      }
    }

    if (!contentEl) {
      alert('Không thể nhận diện nội dung bài viết chính trên trang này.');
      return;
    }

    const title = document.querySelector('h1')?.innerText || document.title;
    const bodyHtml = contentEl.cloneNode(true);

    // Clean scripts, forms, iframes, and ads inside extracted body
    const junk = bodyHtml.querySelectorAll('script, style, form, iframe, button, [class*="ad"], [id*="ad"]');
    for (let j = 0; j < junk.length; j++) {
      junk[j].remove();
    }

    // 2. Build Reader Overlay
    const container = document.createElement('div');
    container.id = '__rustbrowse_reader_container';
    container.innerHTML = `
      <style>
        #__rustbrowse_reader_container {
          position: fixed !important;
          top: 0 !important;
          left: 0 !important;
          width: 100vw !important;
          height: 100vh !important;
          background: #0d0f14 !important;
          color: #e5e7eb !important;
          z-index: 2147483647 !important;
          overflow-y: auto !important;
          padding: 40px 20px !important;
          box-sizing: border-box !important;
          font-family: -apple-system, BlinkMacSystemFont, "Segoe UI Variable Display", "Segoe UI", Roboto, Georgia, serif !important;
        }
        #__rustbrowse_reader_inner {
          max-width: 740px !important;
          margin: 0 auto !important;
          line-height: 1.8 !important;
          font-size: 18px !important;
        }
        #__rustbrowse_reader_inner h1 {
          font-size: 32px !important;
          font-weight: 800 !important;
          color: #60a5fa !important;
          margin-bottom: 24px !important;
          line-height: 1.3 !important;
        }
        #__rustbrowse_reader_inner img {
          max-width: 100% !important;
          height: auto !important;
          border-radius: 8px !important;
          margin: 20px 0 !important;
        }
        #__rustbrowse_reader_inner p {
          margin-bottom: 20px !important;
          color: #d1d5db !important;
        }
        .rb-reader-bar {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 30px;
          border-bottom: 1px solid #1f2937;
          padding-bottom: 12px;
        }
        .rb-reader-exit {
          background: #1f2937;
          color: #f3f4f6;
          border: 1px solid #374151;
          padding: 6px 14px;
          border-radius: 6px;
          cursor: pointer;
          font-size: 13px;
          font-weight: 600;
        }
        .rb-reader-exit:hover {
          background: #374151;
        }
      </style>
      <div id="__rustbrowse_reader_inner">
        <div class="rb-reader-bar">
          <span style="font-size:13px;color:#9ca3af;font-weight:600;">📖 RustBrowse Smart Reader</span>
          <button class="rb-reader-exit" id="rb-exit-btn">✕ Exit Reader Mode</button>
        </div>
        <h1>${title}</h1>
        <div id="__rustbrowse_reader_content"></div>
      </div>
    `;

    document.documentElement.appendChild(container);
    document.getElementById('__rustbrowse_reader_content').appendChild(bodyHtml);
    document.getElementById('rb-exit-btn').onclick = exitReaderMode;

    // Pause all background videos
    const videos = document.querySelectorAll('video');
    for (let v = 0; v < videos.length; v++) {
      try { videos[v].pause(); } catch (_) {}
    }

    isReaderActive = true;
  }

  function exitReaderMode() {
    const el = document.getElementById('__rustbrowse_reader_container');
    if (el) {
      el.remove();
    }
    isReaderActive = false;
  }

  // Keyboard shortcut: Alt+R
  window.addEventListener('keydown', (e) => {
    if (e.altKey && (e.key === 'r' || e.key === 'R')) {
      e.preventDefault();
      toggleReaderMode();
    }
    if (e.key === 'Escape' && isReaderActive) {
      exitReaderMode();
    }
  });

  // Listen for host postMessage
  window.addEventListener('message', (e) => {
    if (e.data === 'TOGGLE_READER_MODE' || (e.data && e.data.action === 'TOGGLE_READER_MODE')) {
      toggleReaderMode();
    }
  });

  if (window.chrome && window.chrome.webview) {
    window.chrome.webview.addEventListener('message', (e) => {
      if (e.data === 'TOGGLE_READER_MODE' || (e.data && e.data.action === 'TOGGLE_READER_MODE')) {
        toggleReaderMode();
      }
    });
  }
})();
