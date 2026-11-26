//! WebView2 WebResourceResponse constructors for adblocking and silent neutralization.

use windows_webview::WebResourceResponse;

pub struct ResponseHandler;

impl ResponseHandler {
    /// Produces an immediate HTTP 204 No Content response with empty body to cleanly and silently drop ad/tracker requests without timeouts.
    pub fn blocked_response() -> WebResourceResponse {
        WebResourceResponse::new(Vec::<u8>::new())
            .status(204, "No Content")
            .header("Content-Type", "text/plain")
            .header("X-RustBrowse-Shield", "Blocked")
    }

    /// Produces an immediate HTTP 200 OK empty response for script/css drops.
    #[allow(dead_code)]
    pub fn empty_ok_response(mime_type: &str) -> WebResourceResponse {
        WebResourceResponse::new(Vec::<u8>::new())
            .status(200, "OK")
            .header("Content-Type", mime_type)
            .header("X-RustBrowse-Shield", "Neutralized")
    }
}
