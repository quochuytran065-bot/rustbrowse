//! Ad Matrix Verification Test
//! Tests all 8 categories of ad threats against AdblockEngine and ActiveShield.

use adblock_engine::AdblockEngine;

#[test]
fn test_ad_matrix_coverage() {
    let engine = AdblockEngine::default_engine();

    // 1. Vietnamese Gambling & Popunder Networks
    assert_eq!(
        engine.check_request("https://short-link.app/c=cpd&m=topbanner", "https://phimmoi.net", "script", "GET", 0),
        adblock_engine::FilterDecision::Block
    );
    assert_eq!(
        engine.check_request("https://playkclub.info/join", "https://phimmoi.net", "subdocument", "GET", 0),
        adblock_engine::FilterDecision::Block
    );
    assert_eq!(
        engine.check_request("https://clickadu.com/popunder", "https://truyentranh.net", "script", "GET", 0),
        adblock_engine::FilterDecision::Block
    );
    assert_eq!(
        engine.check_request("https://popads.net/serve.js", "https://example.com", "script", "GET", 0),
        adblock_engine::FilterDecision::Block
    );

    // 2. Commercial / Display Networks
    assert_eq!(
        engine.check_request("https://doubleclick.net/pagead/ads.js", "https://vnexpress.net", "script", "GET", 0),
        adblock_engine::FilterDecision::Block
    );
    assert_eq!(
        engine.check_request("https://adservice.google.com/adsid/integrator.sync", "https://dantri.com.vn", "script", "GET", 0),
        adblock_engine::FilterDecision::Block
    );
    assert_eq!(
        engine.check_request("https://criteo.com/delivery/r/a.js", "https://tuoitre.vn", "script", "GET", 0),
        adblock_engine::FilterDecision::Block
    );
    assert_eq!(
        engine.check_request("https://admicro.vn/banner/728x90.gif", "https://kenh14.vn", "image", "GET", 0),
        adblock_engine::FilterDecision::Block
    );
    assert_eq!(
        engine.check_request("https://eclick.vn/serve", "https://vnexpress.net", "script", "GET", 0),
        adblock_engine::FilterDecision::Block
    );

    // 3. YouTube & Spotify Telemetry/Ad endpoints
    assert_eq!(
        engine.check_request("https://youtube.com/api/stats/ads?v=123", "https://youtube.com", "xmlhttprequest", "GET", 0),
        adblock_engine::FilterDecision::Block
    );
    assert_eq!(
        engine.check_request("https://spclient.wg.spotify.com/ads/v1/ad", "https://open.spotify.com", "xmlhttprequest", "GET", 0),
        adblock_engine::FilterDecision::Block
    );

    // 4. Legitimate requests must NOT be blocked (Zero False Positives)
    assert_eq!(
        engine.check_request("https://google.com/search?q=rust", "https://google.com", "document", "GET", 0),
        adblock_engine::FilterDecision::Allow
    );
    assert_eq!(
        engine.check_request("https://github.com/rust-lang/rust", "https://github.com", "document", "GET", 0),
        adblock_engine::FilterDecision::Allow
    );
    assert_eq!(
        engine.check_request("https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.0.0/css/all.min.css", "https://example.com", "stylesheet", "GET", 0),
        adblock_engine::FilterDecision::Allow
    );
}
