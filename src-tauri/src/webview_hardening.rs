// Adapted from AivoRelay (MaxITService/AIVORelay), MIT License.
// Source: src-tauri/src/webview_hardening.rs — Webview Hardening (2026-06-19).

/// Disable the WebView2 browser accelerator keys (F5, F6, Ctrl+F, F12, …) of
/// `window`, in every build profile. The main window calls this directly; the
/// overlays go through the release-only [`disable_browser_accelerator_keys`].
#[cfg(target_os = "windows")]
pub fn disable_accelerators_now(window: &tauri::WebviewWindow) {
    let label = window.label().to_string();
    if let Err(err) = window.with_webview(move |webview| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
        use windows_core::Interface;

        // In Tauri 3 the app runs under the type-erased `DynRuntime`, so
        // `with_webview` hands us a `PlatformWebview<DynRuntime>` whose inner
        // value is a `DynWebview`. Downcast to the concrete wry `Webview` to
        // reach `controller()` and the WebView2 COM interfaces.
        let Some(webview) = webview.downcast_ref::<tauri_runtime_wry::Webview>() else {
            log::warn!("Failed to downcast webview to wry runtime for '{}'", label);
            return;
        };

        let result = webview
            .controller()
            .CoreWebView2()
            .and_then(|core| core.Settings())
            .and_then(|settings| settings.cast::<ICoreWebView2Settings3>())
            .and_then(|settings| settings.SetAreBrowserAcceleratorKeysEnabled(false));

        if let Err(err) = result {
            log::warn!(
                "Failed to disable WebView2 browser accelerator keys for '{}': {}",
                label,
                err
            );
        }
    }) {
        log::warn!(
            "Failed to access WebView2 instance for '{}': {}",
            window.label(),
            err
        );
    }
}

#[cfg(all(target_os = "windows", not(debug_assertions)))]
pub fn disable_browser_accelerator_keys(window: &tauri::WebviewWindow) {
    disable_accelerators_now(window);
}

#[cfg(any(not(target_os = "windows"), debug_assertions))]
pub fn disable_browser_accelerator_keys(_window: &tauri::WebviewWindow) {}
