/**
 * RustBrowse — Element Zapper (1-Click Ad & Banner Remover)
 * Injected automatically via add_script_to_execute_on_document_created
 * Triggered via host message 'START_ELEMENT_ZAPPER'
 */
(function () {
  'use strict';

  if (window.__rustbrowse_element_zapper_injected) return;
  window.__rustbrowse_element_zapper_injected = true;

  let isZapperActive = false;
  let hoveredEl = null;

  function startZapper() {
    if (isZapperActive) return;
    isZapperActive = true;
    document.body.style.cursor = 'crosshair';

    document.addEventListener('mouseover', onMouseOver, true);
    document.addEventListener('mouseout', onMouseOut, true);
    document.addEventListener('click', onClick, true);
    document.addEventListener('keydown', onKeyDown, true);
  }

  function stopZapper() {
    if (!isZapperActive) return;
    isZapperActive = false;
    document.body.style.cursor = '';

    if (hoveredEl) {
      hoveredEl.style.outline = '';
      hoveredEl = null;
    }

    document.removeEventListener('mouseover', onMouseOver, true);
    document.removeEventListener('mouseout', onMouseOut, true);
    document.removeEventListener('click', onClick, true);
    document.removeEventListener('keydown', onKeyDown, true);
  }

  function onMouseOver(e) {
    if (!isZapperActive) return;
    if (hoveredEl) hoveredEl.style.outline = '';
    hoveredEl = e.target;
    if (hoveredEl) {
      hoveredEl.style.outline = '3px solid #ef4444';
    }
  }

  function onMouseOut(e) {
    if (e.target) {
      e.target.style.outline = '';
    }
  }

  function onClick(e) {
    if (!isZapperActive) return;
    e.preventDefault();
    e.stopPropagation();
    e.stopImmediatePropagation();

    const target = e.target;
    if (target && target !== document.body && target !== document.documentElement) {
      const selector = generateSelector(target);
      target.remove();
      stopZapper();

      console.info('[RustBrowse Zapper] Zapped element:', selector);
      if (window.chrome && window.chrome.webview) {
        window.chrome.webview.postMessage(JSON.stringify({
          action: 'ZAP_RULE',
          domain: window.location.hostname,
          selector: selector
        }));
      }
    }
    return false;
  }

  function onKeyDown(e) {
    if (e.key === 'Escape') {
      stopZapper();
    }
  }

  function generateSelector(el) {
    if (el.id) return '#' + el.id;
    if (el.className && typeof el.className === 'string') {
      const cls = el.className.trim().split(/\s+/).slice(0, 2).join('.');
      if (cls) return el.tagName.toLowerCase() + '.' + cls;
    }
    return el.tagName.toLowerCase();
  }

  window.addEventListener('message', (e) => {
    if (e.data === 'START_ELEMENT_ZAPPER' || (e.data && e.data.action === 'START_ELEMENT_ZAPPER')) {
      startZapper();
    }
  });

  if (window.chrome && window.chrome.webview) {
    window.chrome.webview.addEventListener('message', (e) => {
      if (e.data === 'START_ELEMENT_ZAPPER' || (e.data && e.data.action === 'START_ELEMENT_ZAPPER')) {
        startZapper();
      }
    });
  }
})();
