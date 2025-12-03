// WASM utility functions

use wasm_bindgen_futures::JsFuture;

/// Sleep for the specified number of milliseconds using browser's setTimeout
/// 
/// This is a platform-specific sleep implementation for WASM that uses
/// the browser's setTimeout API.
pub async fn sleep_ms(milliseconds: u64) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let window = web_sys::window().expect("no window");
        window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, milliseconds as i32)
            .expect("setTimeout failed");
    });
    let _ = JsFuture::from(promise).await;
}
