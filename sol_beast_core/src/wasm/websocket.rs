// WASM WebSocket implementation for browser
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{WebSocket, MessageEvent, ErrorEvent, CloseEvent};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

pub struct WasmWebSocket {
    ws: WebSocket,
    // These closures must be kept alive to maintain the event handlers
    #[allow(dead_code)]
    on_message: Closure<dyn FnMut(MessageEvent)>,
    #[allow(dead_code)]
    on_error: Closure<dyn FnMut(ErrorEvent)>,
    #[allow(dead_code)]
    on_close: Closure<dyn FnMut(CloseEvent)>,
    subscriptions: Arc<Mutex<HashMap<u64, String>>>,
}

impl WasmWebSocket {
    pub fn new(url: &str) -> Result<Self, JsValue> {
        let ws = WebSocket::new(url)?;
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let subscriptions = Arc::new(Mutex::new(HashMap::new()));

        // Message handler
        let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let message: String = txt.into();
                web_sys::console::log_1(&format!("WS Message: {}", message).into());
                
                // Parse and handle Solana subscription notifications
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&message) {
                    if let Some(method) = parsed.get("method").and_then(|m| m.as_str()) {
                        if method == "accountNotification" || method == "logsNotification" {
                            // Handle subscription update
                            web_sys::console::log_1(&format!("Notification: {}", method).into());
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);

        // Error handler
        let on_error = Closure::wrap(Box::new(move |e: ErrorEvent| {
            web_sys::console::error_1(&format!("WS Error: {:?}", e).into());
        }) as Box<dyn FnMut(_)>);

        // Close handler
        let on_close = Closure::wrap(Box::new(move |e: CloseEvent| {
            web_sys::console::log_1(&format!("WS Closed: code={} reason={}", e.code(), e.reason()).into());
        }) as Box<dyn FnMut(_)>);

        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        Ok(Self {
            ws,
            on_message,
            on_error,
            on_close,
            subscriptions,
        })
    }

    pub fn send(&self, data: &str) -> Result<(), JsValue> {
        self.ws.send_with_str(data)
    }

    pub async fn subscribe_account(&self, pubkey: &str) -> Result<u64, JsValue> {
        let sub_id = js_sys::Math::random() as u64;
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": sub_id,
            "method": "accountSubscribe",
            "params": [pubkey, {"encoding": "jsonParsed"}]
        });

        self.send(&request.to_string())?;
        
        self.subscriptions
            .lock()
            .map_err(|e| JsValue::from_str(&format!("Failed to lock subscriptions: {:?}", e)))?
            .insert(sub_id, pubkey.to_string());

        Ok(sub_id)
    }

    pub async fn subscribe_logs(&self, mentions: &[String]) -> Result<u64, JsValue> {
        let sub_id = js_sys::Math::random() as u64;
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": sub_id,
            "method": "logsSubscribe",
            "params": [{
                "mentions": mentions
            }]
        });

        self.send(&request.to_string())?;
        
        Ok(sub_id)
    }

    pub fn unsubscribe(&self, sub_id: u64) -> Result<(), JsValue> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": sub_id,
            "method": "accountUnsubscribe",
            "params": [sub_id]
        });

        self.send(&request.to_string())?;
        
        self.subscriptions
            .lock()
            .map_err(|e| JsValue::from_str(&format!("Failed to lock subscriptions: {:?}", e)))?
            .remove(&sub_id);
        
        Ok(())
    }

    pub fn close(&self) -> Result<(), JsValue> {
        self.ws.close()
    }
}

impl Drop for WasmWebSocket {
    fn drop(&mut self) {
        let _ = self.ws.close();
    }
}
