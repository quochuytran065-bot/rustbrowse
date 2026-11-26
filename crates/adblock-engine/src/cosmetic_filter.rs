//! Cosmetic filtering and CSS selector generation for element hiding.

pub struct CosmeticFilter;

impl CosmeticFilter {
    /// Generates CSS rules to hide ad elements and placeholders for a given hostname.
    pub fn generate_css_for_host(hostname: &str) -> String {
        let mut selectors = vec![
            // Generic ad container selectors
            ".ad-banner",
            ".advertisement",
            ".ad-container",
            "div[id*='google_ads_']",
            "ins.adsbygoogle",
            "div[class*='sponsored-content']",
        ];

        let host = hostname.to_lowercase();

        // Platform-specific cosmetic selectors
        if host.contains("youtube.com") {
            selectors.extend(&[
                "ytd-promoted-sparkles-web-renderer",
                "ytd-promoted-video-renderer",
                "ytd-display-ad-renderer",
                "#player-ads",
                ".ytp-ad-module",
                ".ytp-ad-overlay-container",
                "ytd-statement-banner-renderer",
            ]);
        } else if host.contains("facebook.com") {
            selectors.extend(&[
                "div[data-pagelet*='FeedUnit:Sponsored']",
                "div[aria-label='Sponsored']",
            ]);
        } else if host.contains("tiktok.com") {
            selectors.extend(&[
                "div[class*='DivItemContainerV2'][data-e2e*='ad']",
                ".feed-card-ad",
            ]);
        }

        format!(
            "{} {{ display: none !important; visibility: hidden !important; height: 0 !important; }}",
            selectors.join(",\n")
        )
    }

    /// Wraps CSS string into an executable JavaScript snippet that injects a `<style>` element into DOM.
    pub fn build_injection_script(hostname: &str) -> String {
        let css = Self::generate_css_for_host(hostname);
        let escaped = css.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ");
        format!(
            r#"(function() {{
                if (document.getElementById('rustbrowse-adblock-styles')) return;
                const style = document.createElement('style');
                style.id = 'rustbrowse-adblock-styles';
                style.textContent = "{}";
                if (document.head) {{
                    document.head.appendChild(style);
                }} else {{
                    document.addEventListener('DOMContentLoaded', () => document.head.appendChild(style));
                }}
            }})();"#,
            escaped
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosmetic_css_generation() {
        let css = CosmeticFilter::generate_css_for_host("www.youtube.com");
        assert!(css.contains("ytd-promoted-sparkles-web-renderer"));
        assert!(css.contains("display: none !important;"));

        let script = CosmeticFilter::build_injection_script("www.youtube.com");
        assert!(script.contains("rustbrowse-adblock-styles"));
    }
}
