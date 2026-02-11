// ===================================
// Versions
// ===================================
const STABLE_VERSION = "0.1.7";
const UNSTABLE_VERSION = "0.1.8-rc.6";

function detectOS() {
    const userAgent = window.navigator.userAgent;
    const platform = window.navigator.platform;
    const macosPlatforms = ['Macintosh', 'MacIntel', 'MacPPC', 'Mac68K'];
    const windowsPlatforms = ['Win32', 'Win64', 'Windows', 'WinCE'];
    const iosPlatforms = ['iPhone', 'iPad', 'iPod'];

    if (macosPlatforms.indexOf(platform) !== -1) {
        return 'macos';
    } else if (iosPlatforms.indexOf(platform) !== -1) {
        return 'ios';
    } else if (windowsPlatforms.indexOf(platform) !== -1) {
        return 'windows';
    } else if (/Android/.test(userAgent)) {
        return 'android';
    } else if (/Linux/.test(platform)) {
        return 'linux';
    }

    return 'unknown';
}

function updateDownloadButtons() {
    const os = detectOS();
    const primaryDownload = document.getElementById('primary-download');
    const unstableDownload = document.getElementById('unstable-download');
    const detectedOSSpan = document.getElementById('detected-os');
    const unstableContainer = document.getElementById('unstable-download-container');

    // Hide unstable button if it's the same as stable
    if (STABLE_VERSION === UNSTABLE_VERSION && unstableContainer) {
        unstableContainer.style.display = 'none';
    }

    const getUrl = (version, os, ext) => `https://github.com/fedro86/ferrispad/releases/download/${version}/FerrisPad-v${version}-${os}${ext}`;

    const osConfig = {
        'windows': {
            name: 'Windows',
            stable: getUrl(STABLE_VERSION, 'windows-x64', '.zip'),
            unstable: getUrl(UNSTABLE_VERSION, 'windows-x64', '.zip')
        },
        'macos': {
            name: 'macOS',
            stable: getUrl(STABLE_VERSION, 'macos', '.dmg'),
            unstable: getUrl(UNSTABLE_VERSION, 'macos', '.dmg')
        },
        'linux': {
            name: 'Ubuntu/Linux',
            stable: getUrl(STABLE_VERSION, 'ubuntu-amd64', '.deb'),
            unstable: getUrl(UNSTABLE_VERSION, 'ubuntu-amd64', '.deb')
        }
    };

    if (osConfig[os]) {
        detectedOSSpan.textContent = osConfig[os].name;
        if (primaryDownload) primaryDownload.href = osConfig[os].stable;
        if (unstableDownload) unstableDownload.href = osConfig[os].unstable;
    } else {
        detectedOSSpan.textContent = 'Your Platform';
        if (primaryDownload) primaryDownload.href = '#download';
        if (unstableDownload) unstableDownload.href = 'https://github.com/fedro86/ferrispad/releases';
    }
}

// ===================================
// Code Tab Switching
// ===================================

function initCodeTabs() {
    const tabs = document.querySelectorAll('.code-tab');

    tabs.forEach(tab => {
        tab.addEventListener('click', function () {
            const targetOS = this.getAttribute('data-os');
            const parent = this.closest('.step-content');

            // Update active tab
            parent.querySelectorAll('.code-tab').forEach(t => t.classList.remove('active'));
            this.classList.add('active');

            // Update active code block
            parent.querySelectorAll('.code-block').forEach(block => {
                if (block.getAttribute('data-os') === targetOS) {
                    block.classList.add('active');
                } else {
                    block.classList.remove('active');
                }
            });
        });
    });
}

// ===================================
// Copy to Clipboard
// ===================================

function initCopyButtons() {
    const copyButtons = document.querySelectorAll('.copy-btn');

    copyButtons.forEach(button => {
        button.addEventListener('click', async function () {
            const textToCopy = this.getAttribute('data-clipboard');

            try {
                await navigator.clipboard.writeText(textToCopy);

                // Visual feedback
                const originalText = this.textContent;
                this.textContent = 'Copied!';
                this.classList.add('copied');

                setTimeout(() => {
                    this.textContent = originalText;
                    this.classList.remove('copied');
                }, 2000);
            } catch (err) {
                console.error('Failed to copy text: ', err);

                // Fallback for older browsers
                const textarea = document.createElement('textarea');
                textarea.value = textToCopy;
                textarea.style.position = 'fixed';
                textarea.style.opacity = '0';
                document.body.appendChild(textarea);
                textarea.select();

                try {
                    document.execCommand('copy');
                    this.textContent = 'Copied!';
                    this.classList.add('copied');

                    setTimeout(() => {
                        this.textContent = 'Copy';
                        this.classList.remove('copied');
                    }, 2000);
                } catch (err2) {
                    console.error('Fallback: Failed to copy', err2);
                }

                document.body.removeChild(textarea);
            }
        });
    });
}

// ===================================
// Smooth Scroll Enhancement
// ===================================

function initSmoothScroll() {
    document.querySelectorAll('a[href^="#"]').forEach(anchor => {
        anchor.addEventListener('click', function (e) {
            const href = this.getAttribute('href');

            // Don't prevent default for just "#" (for non-links)
            if (href === '#') return;

            const target = document.querySelector(href);
            if (target) {
                e.preventDefault();
                const offsetTop = target.offsetTop - 80; // Account for sticky nav

                window.scrollTo({
                    top: offsetTop,
                    behavior: 'smooth'
                });
            }
        });
    });
}

// ===================================
// Scroll Animations
// ===================================

function initScrollAnimations() {
    const observerOptions = {
        threshold: 0.1,
        rootMargin: '0px 0px -100px 0px'
    };

    const observer = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                entry.target.style.opacity = '1';
                entry.target.style.transform = 'translateY(0)';
            }
        });
    }, observerOptions);

    // Animate feature cards
    document.querySelectorAll('.feature-card, .download-card, .screenshot-card').forEach(card => {
        card.style.opacity = '0';
        card.style.transform = 'translateY(20px)';
        card.style.transition = 'opacity 0.6s ease, transform 0.6s ease';
        observer.observe(card);
    });
}

// ===================================
// Mobile Navigation Toggle
// ===================================

function initMobileNav() {
    // Add mobile menu button if screen is small
    const nav = document.querySelector('.navbar .container');

    if (window.innerWidth <= 768) {
        const menu = document.querySelector('.nav-menu');

        // Create hamburger button if it doesn't exist
        if (!document.querySelector('.nav-toggle')) {
            const toggle = document.createElement('button');
            toggle.className = 'nav-toggle';
            toggle.innerHTML = '☰';
            toggle.setAttribute('aria-label', 'Toggle navigation');

            toggle.addEventListener('click', () => {
                menu.classList.toggle('show');
            });

            nav.appendChild(toggle);
        }
    }
}

// ===================================
// Download Link Validation
// ===================================

function initDownloadValidation() {
    const downloadLinks = document.querySelectorAll('a[href^="assets/binaries"]');

    downloadLinks.forEach(link => {
        link.addEventListener('click', function (e) {
            const href = this.getAttribute('href');

            // Check if binary exists (this would require server-side validation in production)
            // For now, we'll just show a message for missing binaries
            if (href.includes('binaries')) {
                // Show note about binaries coming soon
                const note = document.querySelector('.download-note');
                if (note) {
                    note.style.display = 'block';
                    note.scrollIntoView({ behavior: 'smooth', block: 'center' });
                }
            }
        });
    });
}

// ===================================
// Set Active OS Tab
// ===================================

function setActiveOSTab() {
    const os = detectOS();
    const osMap = {
        'windows': 'windows',
        'macos': 'macos',
        'linux': 'linux'
    };

    const activeOS = osMap[os] || 'linux';

    // Find and click the appropriate tab
    const tab = document.querySelector(`.code-tab[data-os="${activeOS}"]`);
    if (tab) {
        tab.click();
    }
}

// ===================================
// Keyboard Navigation
// ===================================

function initKeyboardNav() {
    document.addEventListener('keydown', (e) => {
        // Press 'Escape' to close any open details/modals
        if (e.key === 'Escape') {
            document.querySelectorAll('details[open]').forEach(details => {
                details.removeAttribute('open');
            });
        }
    });
}

// ===================================
// Performance: Lazy Load Images
// ===================================

function initLazyLoading() {
    const images = document.querySelectorAll('img[data-src]');

    const imageObserver = new IntersectionObserver((entries, observer) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                const img = entry.target;
                img.src = img.dataset.src;
                img.removeAttribute('data-src');
                observer.unobserve(img);
            }
        });
    });

    images.forEach(img => imageObserver.observe(img));
}

// ===================================
// Analytics (Placeholder)
// ===================================

function trackDownload(platform) {
    // Placeholder for analytics tracking
    console.log(`Download initiated: ${platform}`);

    // In production, you might use:
    // gtag('event', 'download', { 'platform': platform });
}

// ===================================
// Initialize on DOM Ready
// ===================================

document.addEventListener('DOMContentLoaded', () => {
    updateDownloadButtons();
    initCodeTabs();
    initCopyButtons();
    initSmoothScroll();
    initScrollAnimations();
    initMobileNav();
    initDownloadValidation();
    setActiveOSTab();
    initKeyboardNav();
    initLazyLoading();

    console.log('🦀 FerrisPad website loaded successfully!');
});

// ===================================
// Handle Window Resize
// ===================================

let resizeTimer;
window.addEventListener('resize', () => {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => {
        initMobileNav();
    }, 250);
});