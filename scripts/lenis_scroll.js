// ============================================================================
// RustBrowse Lenis Smooth Scroll Engine (scripts/lenis_scroll.js)
// Based on darkroomengineering/lenis principles:
// - Frame-rate independent exponential interpolation
// - Zero CPU idle overhead (rAF runs strictly during active scroll)
// - Smart nested scroll container detection (overflow: auto / scroll)
// - Native touch & trackpad pass-through
// ============================================================================

(() => {
    'use strict';

    if (window.__RUSTBROWSE_LENIS_INJECTED__) return;
    window.__RUSTBROWSE_LENIS_INJECTED__ = true;

    // Global toggle switch (default: enabled)
    window.__RUSTBROWSE_SMOOTH_SCROLL_ENABLED__ = true;

    window.__RUSTBROWSE_TOGGLE_SMOOTH_SCROLL = function(enable) {
        window.__RUSTBROWSE_SMOOTH_SCROLL_ENABLED__ = Boolean(enable);
    };

    // Physics configuration
    const DAMPING = 0.12;      // Higher = faster settle, lower = silkier
    const EPSILON = 0.5;       // Pixel threshold to sleep rAF (0% CPU)
    const SPEED_MULTIPLIER = 1.0;

    let targetY = window.scrollY || window.pageYOffset || 0;
    let currentY = targetY;
    let isRunning = false;
    let rafId = null;
    let lastTime = performance.now();

    // Helper: find deepest scrollable parent if event occurred inside an overflow container
    function getScrollContainer(element) {
        let el = element;
        while (el && el !== document.body && el !== document.documentElement) {
            const style = window.getComputedStyle(el);
            const overflowY = style.overflowY;
            const isScrollable = (overflowY === 'auto' || overflowY === 'scroll') && el.scrollHeight > el.clientHeight;
            if (isScrollable) {
                return el;
            }
            el = el.parentElement;
        }
        return window;
    }

    function getMaxScrollY(container) {
        if (container === window) {
            const doc = document.documentElement;
            return Math.max(doc.scrollHeight - window.innerHeight, 0);
        }
        return Math.max(container.scrollHeight - container.clientHeight, 0);
    }

    function getContainerScrollY(container) {
        if (container === window) {
            return window.scrollY || window.pageYOffset || 0;
        }
        return container.scrollTop;
    }

    function setContainerScrollY(container, y) {
        if (container === window) {
            window.scrollTo(window.scrollX || window.pageXOffset || 0, y);
        } else {
            container.scrollTop = y;
        }
    }

    let activeContainer = window;

    function onRaf(now) {
        if (!isRunning) return;

        const dt = Math.min((now - lastTime) / 1000, 0.1);
        lastTime = now;

        // Exponential decay damping: 1 - Math.exp(-decay * dt)
        const factor = 1 - Math.exp(-25 * DAMPING * dt);
        currentY += (targetY - currentY) * factor;

        setContainerScrollY(activeContainer, currentY);

        if (Math.abs(targetY - currentY) <= EPSILON) {
            // Snap to final destination and stop loop -> zero CPU usage
            currentY = targetY;
            setContainerScrollY(activeContainer, currentY);
            isRunning = false;
            rafId = null;
            return;
        }

        rafId = requestAnimationFrame(onRaf);
    }

    function startLoop() {
        if (!isRunning) {
            isRunning = true;
            lastTime = performance.now();
            rafId = requestAnimationFrame(onRaf);
        }
    }

    // Wheel event listener (wheel delta normalization)
    window.addEventListener('wheel', (e) => {
        if (!window.__RUSTBROWSE_SMOOTH_SCROLL_ENABLED__) return;

        // Ignore if user is pinching / zooming (Ctrl + wheel)
        if (e.ctrlKey) return;

        const target = e.target;
        if (!target) return;

        // Find scrollable container
        const container = getScrollContainer(target);
        if (container !== activeContainer) {
            activeContainer = container;
            currentY = getContainerScrollY(container);
            targetY = currentY;
        }

        // Normalize deltaMode (0 = pixels, 1 = lines, 2 = pages)
        let deltaY = e.deltaY;
        if (e.deltaMode === 1) {
            deltaY *= 32; // Line height approx
        } else if (e.deltaMode === 2) {
            deltaY *= window.innerHeight;
        }

        deltaY *= SPEED_MULTIPLIER;

        const maxScroll = getMaxScrollY(activeContainer);
        const nextTarget = Math.max(0, Math.min(maxScroll, targetY + deltaY));

        // If at boundaries and cannot scroll further in this container, let event bubble
        if (nextTarget === targetY) {
            return;
        }

        e.preventDefault();
        targetY = nextTarget;
        startLoop();
    }, { passive: false });

    // Sync current position when window is scrolled by other means (keyboard, scrollbar, in-page links)
    window.addEventListener('scroll', () => {
        if (!isRunning && activeContainer === window) {
            currentY = window.scrollY || window.pageYOffset || 0;
            targetY = currentY;
        }
    }, { passive: true });

})();
