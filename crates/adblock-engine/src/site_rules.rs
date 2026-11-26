//! Target platform site-specific filtering policies (YouTube, Spotify, Facebook, TikTok).
//! Guarantees that critical media streams, playback APIs, and authentication endpoints are never blocked.

use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    YouTube,
    Spotify,
    Facebook,
    TikTok,
    Generic,
}

pub struct SiteRules;

impl SiteRules {
    /// Detects platform from source or target URL.
    pub fn identify_platform(url_str: &str) -> Platform {
        if let Ok(url) = Url::parse(url_str) {
            if let Some(host) = url.host_str() {
                let h = host.to_lowercase();
                if h.contains("youtube.com") || h.contains("youtu.be") || h.contains("googlevideo.com") {
                    return Platform::YouTube;
                }
                if h.contains("spotify.com") {
                    return Platform::Spotify;
                }
                if h.contains("facebook.com") || h.contains("fbcdn.net") || h.contains("messenger.com") {
                    return Platform::Facebook;
                }
                if h.contains("tiktok.com") || h.contains("byteoversea.com") || h.contains("ibytedtos.com") {
                    return Platform::TikTok;
                }
            }
        }
        Platform::Generic
    }

    /// Evaluates if a request on YouTube must be explicitly preserved (never blocked).
    pub fn is_youtube_protected_request(target_url: &str) -> bool {
        if let Ok(url) = Url::parse(target_url) {
            if let Some(host) = url.host_str() {
                let h = host.to_lowercase();
                let path = url.path().to_lowercase();

                // NEVER block media chunks from googlevideo
                if h.contains("googlevideo.com") && path.contains("/videoplayback") {
                    return true;
                }

                // NEVER block core player API, base.js, or initial navigation
                if h.contains("youtube.com") {
                    if path.starts_with("/youtubei/v1/player")
                        || path.starts_with("/s/player/")
                        || path.starts_with("/watch")
                        || path.starts_with("/results")
                        || path.starts_with("/live")
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Evaluates if a request on YouTube is an ad / tracking endpoint.
    pub fn is_youtube_ad_or_tracker(target_url: &str) -> bool {
        let lower = target_url.to_lowercase();
        lower.contains("/api/stats/ads")
            || lower.contains("video-ad-stats")
            || lower.contains("/youtubei/v1/player/ad_break")
            || lower.contains("googleads.g.doubleclick.net/pagead")
    }

    /// Evaluates if a request on Spotify must be preserved (audio playback, auth).
    pub fn is_spotify_protected_request(target_url: &str) -> bool {
        if let Ok(url) = Url::parse(target_url) {
            if let Some(host) = url.host_str() {
                let h = host.to_lowercase();
                let path = url.path().to_lowercase();

                // Core audio streaming CDN & auth
                if h.contains("audio-ak-spotify-com")
                    || h.contains("audio-sp-spotify-com")
                    || h.contains("audio-fa.spotifycdn.com")
                    || (h.contains("spotify.com") && (path.contains("/v1/tracks") || path.contains("/token") || path.contains("/login")))
                {
                    return true;
                }
            }
        }
        false
    }

    /// Evaluates if a request on Spotify is an ad endpoint.
    pub fn is_spotify_ad_or_tracker(target_url: &str) -> bool {
        let lower = target_url.to_lowercase();
        lower.contains("/ads/")
            || lower.contains("/ad-logic/")
            || lower.contains("spclient.wg.spotify.com/ads/")
            || lower.contains("dealer.spotify.com/ads")
    }

    /// Evaluates if a request on Facebook must be preserved.
    pub fn is_facebook_protected_request(target_url: &str) -> bool {
        if let Ok(url) = Url::parse(target_url) {
            if let Some(host) = url.host_str() {
                let h = host.to_lowercase();
                let path = url.path().to_lowercase();

                // Video/image CDN, messenger websocket
                if h.contains("fbcdn.net")
                    || h.contains("messenger.com")
                    || (h.contains("facebook.com") && (path.starts_with("/graphql") || path.starts_with("/api/graphql")))
                {
                    return true;
                }
            }
        }
        false
    }

    /// Evaluates if a request on Facebook is an ad/tracking beacon.
    pub fn is_facebook_ad_or_tracker(target_url: &str) -> bool {
        let lower = target_url.to_lowercase();
        lower.contains("connect.facebook.net/en_us/fbevents.js")
            || lower.contains("facebook.com/tr")
            || lower.contains("pixel.facebook.com")
            || lower.contains("facebook.com/adnw_logging")
    }

    /// Evaluates if a request on TikTok must be preserved.
    pub fn is_tiktok_protected_request(target_url: &str) -> bool {
        if let Ok(url) = Url::parse(target_url) {
            if let Some(host) = url.host_str() {
                let h = host.to_lowercase();
                // Video streaming CDNs
                if h.contains("byteoversea.com") || h.contains("ibytedtos.com") || h.contains("tiktokcdn.com") {
                    return true;
                }
            }
        }
        false
    }

    /// Evaluates if a request on TikTok is an ad or telemetry endpoint.
    pub fn is_tiktok_ad_or_tracker(target_url: &str) -> bool {
        let lower = target_url.to_lowercase();
        lower.contains("analytics.tiktok.com")
            || lower.contains("ads.tiktok.com")
            || lower.contains("log.tiktokv.com")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_youtube_rules() {
        let stream = "https://rr2---sn-4g5ednss.googlevideo.com/videoplayback?expire=123&sparams=ip";
        assert!(SiteRules::is_youtube_protected_request(stream));
        assert!(!SiteRules::is_youtube_ad_or_tracker(stream));

        let ad = "https://www.youtube.com/api/stats/ads?v=xyz";
        assert!(!SiteRules::is_youtube_protected_request(ad));
        assert!(SiteRules::is_youtube_ad_or_tracker(ad));
    }

    #[test]
    fn test_spotify_rules() {
        let ad = "https://spclient.wg.spotify.com/ads/v1/ad";
        assert!(SiteRules::is_spotify_ad_or_tracker(ad));
        assert!(!SiteRules::is_spotify_protected_request(ad));

        let audio = "https://audio-sp-spotify-com.akamaized.net/audio/xyz";
        assert!(SiteRules::is_spotify_protected_request(audio));
        assert!(!SiteRules::is_spotify_ad_or_tracker(audio));
    }
}
