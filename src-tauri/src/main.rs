// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use app_lib::CliArgs;
use clap::Parser;

fn main() {
    let cli_args = CliArgs::parse();

    #[cfg(target_os = "linux")]
    {
        // DMABUF renderer causes crashes on various GPU/display server configurations
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };

        // GTK's display backend, chosen by the user even inside the AppImage,
        // whose launch hook forces GDK_BACKEND=x11 (see requested_gdk_backend).
        // First, so GTK never sees the forced value.
        let gdk_backend = app_lib::app_env_var("GDK_BACKEND");
        if let Some(backend) = app_lib::requested_gdk_backend(gdk_backend.as_deref()) {
            unsafe { std::env::set_var("GDK_BACKEND", backend) };
        }

        // Gamescope's WSI layer aborts during Vulkan device creation on SteamOS
        // (upstream 310749b); this app uses Vulkan for compute only. A value
        // the user set, even an empty one, is kept.
        if std::env::var_os("DISABLE_GAMESCOPE_WSI").is_none() {
            unsafe { std::env::set_var("DISABLE_GAMESCOPE_WSI", "1") };
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Avoid overlay/capture layer crashes (#2049). Set before backend
        // initialization, preserving user overrides.
        let keep_implicit = app_lib::app_env_flag("KEEP_VULKAN_IMPLICIT_LAYERS");
        if std::env::var_os("VK_LOADER_LAYERS_DISABLE").is_none() && !keep_implicit {
            unsafe { std::env::set_var("VK_LOADER_LAYERS_DISABLE", "~implicit~") };
        }
    }

    app_lib::run(cli_args)
}
