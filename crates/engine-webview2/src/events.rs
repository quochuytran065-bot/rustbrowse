//! WebView2 COM event listeners dispatching into the EngineEvent channel.

use std::sync::Arc;
use common::messages::EngineEvent;
use common::types::TabId;
use tokio::sync::mpsc::UnboundedSender;
use windows_webview::{EventRegistration, PermissionKind, PermissionState, WebView};
use crate::shield::ActiveShield;

pub struct TabEventSubscriptions {
    _registrations: Vec<EventRegistration>,
}

impl TabEventSubscriptions {
    pub fn wire(
        id: TabId,
        webview: &WebView,
        event_sender: UnboundedSender<EngineEvent>,
        shield: Arc<ActiveShield>,
    ) -> Self {
        let mut registrations = Vec::new();

        // 1. Navigation Starting with ActiveShield Protection & Redirect Guard
        let tx = event_sender.clone();
        let s = shield.clone();
        if let Ok(reg) = webview.on_navigation_starting(move |args| {
            let uri = args.uri();
            if s.is_blocked(&uri) {
                s.log_blocked_redirect(&uri);
                let _ = args.set_cancel(true);
                let _ = tx.send(EngineEvent::BlockedPopup {
                    id,
                    url: uri.clone(),
                });
                return;
            }
            let _ = tx.send(EngineEvent::NavigationStarting { id, url: uri });
        }) {
            registrations.push(reg);
        }

        // 2. Navigation Completed
        let tx = event_sender.clone();
        if let Ok(reg) = webview.on_navigation_completed(move |args| {
            let is_success = args.is_success();
            let _ = tx.send(EngineEvent::NavigationCompleted {
                id,
                url: String::new(),
                is_success,
            });
        }) {
            registrations.push(reg);
        }

        // 3. Document Title Changed
        let tx = event_sender.clone();
        if let Ok(reg) = webview.on_document_title_changed(move |title| {
            let _ = tx.send(EngineEvent::TitleChanged { id, title });
        }) {
            registrations.push(reg);
        }

        // 4. Process Failed (Renderer / Browser crash handler)
        let tx = event_sender.clone();
        if let Ok(reg) = webview.on_process_failed(move |args| {
            let kind = args.kind();
            let _ = tx.send(EngineEvent::ProcessFailed {
                id,
                exit_code: 0,
                description: format!("{:?}", kind),
            });
        }) {
            registrations.push(reg);
        }

        // 5. Global New Tab & Popup/Popunder Guard (Zero-Tolerance: Handled=true ALWAYS)
        let tx = event_sender.clone();
        let s = shield.clone();
        let webview_clone = webview.clone();
        if let Ok(reg) = webview.on_new_window_requested(move |args| {
            let target_uri = args.uri();
            let is_user = args.is_user_initiated();

            // ALWAYS suppress default popup window creation — zero rogue Win32 windows
            let _ = args.set_handled(true);

            let source_url = webview_clone.source();
            let current_host = url::Url::parse(&source_url)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_string()))
                .unwrap_or_default();

            if s.is_untrusted_cross_origin_popup(&current_host, &target_uri) || !is_user {
                s.log_killed_popup(&target_uri);
                let _ = tx.send(EngineEvent::BlockedPopup {
                    id,
                    url: target_uri,
                });
            } else {
                tracing::info!(
                    target_uri = %target_uri,
                    "Legitimate user-initiated click routed to in-browser tab."
                );
                let _ = tx.send(EngineEvent::NewWindowRequested {
                    id,
                    target_url: target_uri,
                    is_user_initiated: true,
                });
            }
        }) {
            registrations.push(reg);
        }

        // 6. Permission Requested: Automatically DENY spam notifications
        if let Ok(reg) = webview.on_permission_requested(move |args| {
            if args.kind() == PermissionKind::Notifications {
                let _ = args.set_state(PermissionState::Deny);
                tracing::info!("Notification permission automatically DENIED by Notification Guard.");
            }
        }) {
            registrations.push(reg);
        }

        // 7. Web Message Received (IPC from injected scripts: ad_nuker, popup_guard, element_zapper)
        let tx = event_sender;
        let s = shield;
        if let Ok(reg) = webview.on_web_message_received(move |args| {
            let json_str = args.web_message_as_json();
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                if let Some(action) = val.get("action").and_then(|a| a.as_str()) {
                    match action {
                        "BlockedPopup" => {
                            let url = val.get("url").and_then(|u| u.as_str()).unwrap_or("Popup");
                            s.log_killed_popup(url);
                            let _ = tx.send(EngineEvent::BlockedPopup {
                                id,
                                url: url.to_string(),
                            });
                        }
                        "BlockedRequest" => {
                            let url = val.get("url").and_then(|u| u.as_str()).unwrap_or("Request");
                            s.log_blocked_request(url);
                        }
                        "ZAP_RULE" => {
                            let domain = val.get("domain").and_then(|d| d.as_str()).unwrap_or("");
                            let selector = val.get("selector").and_then(|s| s.as_str()).unwrap_or("");
                            if !domain.is_empty() && !selector.is_empty() {
                                let _ = tx.send(EngineEvent::ZapperRuleCreated {
                                    domain: domain.to_string(),
                                    selector: selector.to_string(),
                                });
                            }
                        }
                        "AUDIO_STATE" => {
                            let is_audible = val.get("isAudible").and_then(|a| a.as_bool()).unwrap_or(false);
                            let _ = tx.send(EngineEvent::AudioStateChanged { id, is_audible });
                        }
                        _ => {}
                    }
                }
            }
        }) {
            registrations.push(reg);
        }

        Self {
            _registrations: registrations,
        }
    }
}
