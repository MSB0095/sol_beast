// Sol Beast Documentation - Custom JavaScript
// Adds enhanced functionality and cyber effects

(function() {
    'use strict';

    // Add scan line effect to page
    function addScanLineEffect() {
        const content = document.querySelector('.content');
        if (content && !document.querySelector('.cyber-scan-line')) {
            const scanLine = document.createElement('div');
            scanLine.className = 'cyber-scan-line';
            scanLine.style.cssText = `
                position: fixed;
                top: 0;
                left: 0;
                right: 0;
                height: 2px;
                background: linear-gradient(90deg, transparent, var(--sol-beast-accent), transparent);
                pointer-events: none;
                z-index: 9999;
                animation: scan-down 8s linear infinite;
            `;
            document.body.appendChild(scanLine);
        }
    }

    // Add keyboard navigation info
    function addKeyboardShortcuts() {
        const shortcuts = {
            'ArrowLeft': 'Previous page',
            'ArrowRight': 'Next page',
            's': 'Focus search',
            '?': 'Show shortcuts'
        };
        
        document.addEventListener('keydown', function(e) {
            if (e.key === '?' && !e.target.matches('input, textarea')) {
                e.preventDefault();
                showShortcutsModal();
            }
        });
    }

    // Show keyboard shortcuts modal
    function showShortcutsModal() {
        const existingModal = document.querySelector('.shortcuts-modal');
        if (existingModal) {
            existingModal.remove();
            return;
        }

        const modal = document.createElement('div');
        modal.className = 'shortcuts-modal';
        modal.style.cssText = `
            position: fixed;
            top: 50%;
            left: 50%;
            transform: translate(-50%, -50%);
            background: var(--sol-beast-bg-card);
            border: 2px solid var(--sol-beast-accent);
            border-radius: 8px;
            padding: 2rem;
            z-index: 10000;
            box-shadow: 0 0 30px var(--glow-color-strong), 0 0 60px var(--glow-color);
            min-width: 400px;
        `;

        modal.innerHTML = `
            <h2 style="color: var(--sol-beast-accent); margin-top: 0;">Keyboard Shortcuts</h2>
            <table style="width: 100%; border: none;">
                <tr><td><kbd>←/→</kbd></td><td>Navigate pages</td></tr>
                <tr><td><kbd>s</kbd></td><td>Focus search</td></tr>
                <tr><td><kbd>?</kbd></td><td>Show this help</td></tr>
                <tr><td><kbd>Esc</kbd></td><td>Close modals</td></tr>
            </table>
            <button onclick="this.parentElement.remove()" style="margin-top: 1rem;">Close</button>
        `;

        document.body.appendChild(modal);

        // Close on Escape
        document.addEventListener('keydown', function closeOnEsc(e) {
            if (e.key === 'Escape') {
                modal.remove();
                document.removeEventListener('keydown', closeOnEsc);
            }
        });

        // Close on click outside
        modal.addEventListener('click', function(e) {
            if (e.target === modal) {
                modal.remove();
            }
        });
    }

    // Add "back to top" button
    function addBackToTop() {
        const button = document.createElement('button');
        button.className = 'back-to-top';
        button.innerHTML = '↑';
        button.style.cssText = `
            position: fixed;
            bottom: 2rem;
            right: 2rem;
            width: 50px;
            height: 50px;
            border-radius: 50%;
            background: var(--sol-beast-bg-card) !important;
            color: var(--sol-beast-accent) !important;
            border: 2px solid var(--sol-beast-accent) !important;
            font-size: 1.5rem;
            cursor: pointer;
            opacity: 0;
            transition: opacity 0.3s, transform 0.3s;
            z-index: 1000;
            box-shadow: 0 0 20px var(--glow-color);
        `;

        button.addEventListener('click', function() {
            window.scrollTo({ top: 0, behavior: 'smooth' });
        });

        document.body.appendChild(button);

        // Show/hide based on scroll position
        window.addEventListener('scroll', function() {
            if (window.pageYOffset > 300) {
                button.style.opacity = '1';
            } else {
                button.style.opacity = '0';
            }
        });
    }

    // Enhance code blocks with copy button
    function enhanceCodeBlocks() {
        document.querySelectorAll('pre > code').forEach(function(codeBlock) {
            const pre = codeBlock.parentElement;
            if (pre.querySelector('.copy-button')) return;

            const button = document.createElement('button');
            button.className = 'copy-button';
            button.innerHTML = '📋 Copy';
            button.style.cssText = `
                position: absolute;
                top: 0.5rem;
                right: 0.5rem;
                padding: 0.25rem 0.75rem !important;
                font-size: 0.85rem !important;
            `;

            button.addEventListener('click', function() {
                const code = codeBlock.textContent;
                navigator.clipboard.writeText(code).then(function() {
                    button.innerHTML = '✓ Copied!';
                    setTimeout(function() {
                        button.innerHTML = '📋 Copy';
                    }, 2000);
                });
            });

            pre.style.position = 'relative';
            pre.appendChild(button);
        });
    }

    // Add warning icons to warning/caution blocks
    function enhanceWarnings() {
        const warningKeywords = ['warning', 'caution', 'important', 'critical'];
        
        document.querySelectorAll('blockquote').forEach(function(quote) {
            const text = quote.textContent.toLowerCase();
            const hasWarning = warningKeywords.some(kw => text.includes(kw));
            
            if (hasWarning && !quote.querySelector('.warning-icon')) {
                const icon = document.createElement('span');
                icon.className = 'warning-icon';
                icon.innerHTML = '⚠️ ';
                icon.style.cssText = `
                    font-size: 1.5rem;
                    margin-right: 0.5rem;
                    filter: drop-shadow(0 0 5px var(--glow-color));
                `;
                quote.insertBefore(icon, quote.firstChild);
                quote.style.borderLeft = '4px solid #ffbb00';
            }
        });
    }

    // Initialize all enhancements
    function init() {
        addScanLineEffect();
        addKeyboardShortcuts();
        addBackToTop();
        
        // Run these after content loads
        setTimeout(function() {
            enhanceCodeBlocks();
            enhanceWarnings();
        }, 100);
        
        // Re-run on page navigation
        if (window.playground_text) {
            const oldPushState = history.pushState;
            history.pushState = function() {
                oldPushState.apply(history, arguments);
                setTimeout(function() {
                    enhanceCodeBlocks();
                    enhanceWarnings();
                }, 100);
            };
        }
    }

    // Run on DOM ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
