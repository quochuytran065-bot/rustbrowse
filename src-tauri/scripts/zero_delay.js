// RustBrowse Zero Artificial Delay Script
// Strips animation/transition delay from web page UI elements for snappy instant clicks
// Preserves requestAnimationFrame so video and smooth scrolling remain 60-144 FPS
(() => {
  'use strict';
  if (document.getElementById('__rb_zero_delay')) return;
  const style = document.createElement('style');
  style.id = '__rb_zero_delay';
  style.textContent = `
    *, *::before, *::after {
      transition-duration: 0.001ms !important;
      transition-delay: 0ms !important;
      animation-delay: 0ms !important;
    }
  `;
  (document.head || document.documentElement).appendChild(style);
})();
