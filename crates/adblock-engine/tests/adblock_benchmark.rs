//! End-to-end A/B Benchmarks for AdBlock and Tab Memory Scaling.

use adblock_engine::{AdblockEngine, FilterLists, RuleCache};
use std::time::Instant;

#[test]
fn benchmark_adblock_throughput_and_cache_latency() {
    println!("\n=======================================================");
    println!("      RUSTBROWSE ADBLOCK ENGINE A/B BENCHMARKS         ");
    println!("=======================================================");

    let rules = FilterLists::default_rules();

    // 1. Measure Cold Start: Text compilation
    let t0 = Instant::now();
    let engine = AdblockEngine::new(&rules);
    let cold_compile_us = t0.elapsed().as_micros();
    println!("Cold Compile (Text Rules): {} us ({:.2} ms)", cold_compile_us, cold_compile_us as f64 / 1000.0);

    // 2. Measure Cache Save & Load
    let temp_cache = std::env::temp_dir().join("rb_bench_cache.dat");
    let eng_inner = adblock::Engine::new_with_list_text(rules.join("\n"));
    RuleCache::save_to_file(&eng_inner, &temp_cache).expect("Save cache");

    let t1 = Instant::now();
    let _restored = RuleCache::load_from_file(&temp_cache).expect("Load cache");
    let cache_load_us = t1.elapsed().as_micros();
    println!("Cache Cold Load (Binary):  {} us ({:.2} ms) -> Instant Startup", cache_load_us, cache_load_us as f64 / 1000.0);
    let _ = std::fs::remove_file(temp_cache);

    // 3. Measure Network Filter Throughput over 10,000 requests
    let test_urls = [
        ("https://doubleclick.net/pagead/ads.js", "https://cnn.com", "script"),
        ("https://cnn.com/index.html", "https://cnn.com", "main_frame"),
        ("https://analytics.tiktok.com/i18n/pixel/events.js", "https://tiktok.com", "script"),
        ("https://rr2---sn-4g5ednss.googlevideo.com/videoplayback", "https://youtube.com", "media"),
        ("https://www.youtube.com/api/stats/ads?v=123", "https://youtube.com", "ping"),
        ("https://spclient.wg.spotify.com/ads/v1/ad", "https://spotify.com", "xmlhttprequest"),
        ("https://audio-sp-spotify-com.akamaized.net/audio/xyz", "https://spotify.com", "media"),
        ("https://connect.facebook.net/en_US/fbevents.js", "https://news.com", "script"),
        ("https://example.com/bundle.js", "https://example.com", "script"),
        ("https://adservice.google.com/adsid/google/ui", "https://example.com", "script"),
    ];

    let iterations = 10_000;
    let t2 = Instant::now();
    for i in 0..iterations {
        let (url, source, rtype) = test_urls[i % test_urls.len()];
        let _ = engine.check_request(url, source, rtype, "GET", 1_000_000 + i as u64);
    }
    let total_eval_time = t2.elapsed();
    let total_us = total_eval_time.as_micros();
    let avg_us_per_req = total_us as f64 / iterations as f64;
    let req_per_sec = (iterations as f64) / total_eval_time.as_secs_f64();

    println!("Throughput:                {:.0} requests/sec", req_per_sec);
    println!("Avg Latency per Request:   {:.2} us ({:.4} ms)", avg_us_per_req, avg_us_per_req / 1000.0);
    println!("Total Blocked in Bench:    {} ads/trackers", engine.stats().total_ads_blocked() + engine.stats().total_trackers_blocked());
    println!("=======================================================\n");

    assert!(avg_us_per_req < 500.0, "Latency per request must be < 500 us in debug mode");
}
