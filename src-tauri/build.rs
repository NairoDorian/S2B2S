fn main() {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    build_apple_intelligence_bridge();

    #[cfg(target_os = "windows")]
    embed_test_manifest();

    generate_tray_translations();

    // Linux ships transcribe-cpp as a shared libtranscribe + loadable ggml
    // backend modules (the `dynamic-backends` posture in Cargo.toml). Bake an
    // $ORIGIN-relative rpath into the `handy` binary so it finds libtranscribe
    // next to it in the package — AppImage `usr/bin/handy` -> `usr/lib`, and
    // deb/rpm `/usr/bin/handy` -> `/usr/lib`. transcribe's
    // init_backends_default() then loads the ggml modules co-located there.
    // (Windows resolves DLLs from the exe directory, so it needs no rpath;
    // macOS links transcribe-cpp statically via the `metal` feature.)
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib");
    }

    // Stage transcribe-cpp's shared runtime libraries (and the dlopen'd ggml
    // backend modules) for the installer. Self-gates on the shared /
    // dynamic-backends posture used by Linux and Windows; it's a no-op for the
    // static macOS `metal` build, where there is nothing to ship.
    stage_transcribe_runtime_libs();

    // When ORT is dynamically linked (Windows CI sets ORT_LIB_LOCATION +
    // ORT_PREFER_DYNAMIC_LINK to a baseline ONNX Runtime), ship its onnxruntime.dll
    // next to Handy.exe so the app loads our baseline build instead of statically
    // embedding pyke's /arch:AVX2 one (which crashes at startup on pre-Haswell CPUs).
    stage_onnxruntime_dll();

    // Must run after transcribe staging because that helper recreates transcribe-libs/.
    stage_vc_runtime_dlls();

    tauri_build::build()
}

/// Embed a Common-Controls v6 manifest into TEST executables on Windows.
///
/// tauri-build embeds an app manifest into the real binary, but plain test
/// exes get none — so the loader binds the legacy comctl32 5.82, which lacks
/// `TaskDialogIndirect` (imported via tauri-plugin-dialog/rfd), and every test
/// dies at load with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139).
#[cfg(target_os = "windows")]
fn embed_test_manifest() {
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let manifest_path = out_dir.join("test-manifest.xml");
    fs::write(&manifest_path, MANIFEST).expect("failed to write test manifest");

    // Only embed the test manifest when running `cargo test`. The main binary
    // already gets a manifest from tauri-build's resource compilation. Applying
    // /MANIFEST:EMBED globally causes CVTRES error CVT1100 (duplicate resource)
    // because both tauri-build's .rc and our /MANIFESTINPUT try to embed a
    // manifest with id 1. Checking PROFILE alone isn't enough (both use "debug"),
    // but the Tauri CLI sets TAURI_ENV when invoking cargo — its absence during
    // `cargo test` is the discriminator.
    let is_tauri_build = env::var("TAURI_ENV").is_ok()
        || env::var("TAURI_CONFIG").is_ok()
        || env::var("CARGO_CFG_TAURI").is_ok();
    if !is_tauri_build {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
            manifest_path.display()
        );
        // Disable manifest embedding on binary targets because Tauri's build
        // already embeds the application manifest in the binary. This prevents
        // CVTRES duplicate resource error CVT1100.
        println!("cargo:rustc-link-arg-bins=/MANIFEST:NO");
    }
}

/// Generate tray menu translations from frontend locale files.
///
/// Source of truth: src/i18n/locales/*/translation.json
/// The English "tray" section defines the struct fields.
fn generate_tray_translations() {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let locales_dir = Path::new("../src/i18n/locales");

    println!("cargo:rerun-if-changed=../src/i18n/locales");

    // Collect all locale translations
    let mut translations: BTreeMap<String, serde_json::Value> = BTreeMap::new();

    for entry in fs::read_dir(locales_dir).unwrap().flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let lang = path.file_name().unwrap().to_str().unwrap().to_string();
        let json_path = path.join("translation.json");

        println!("cargo:rerun-if-changed={}", json_path.display());

        let content = fs::read_to_string(&json_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        if let Some(tray) = parsed.get("tray").cloned() {
            translations.insert(lang, tray);
        }
    }

    // English defines the schema
    let english = translations.get("en").unwrap().as_object().unwrap();
    let fields: Vec<_> = english
        .keys()
        .map(|k| (camel_to_snake(k), k.clone()))
        .collect();

    // Generate code
    let mut out = String::from(
        "// Auto-generated from src/i18n/locales/*/translation.json - do not edit\n\n",
    );

    // Struct
    out.push_str("#[derive(Debug, Clone)]\npub struct TrayStrings {\n");
    for (rust_field, _) in &fields {
        out.push_str(&format!("    pub {rust_field}: String,\n"));
    }
    out.push_str("}\n\n");

    // Static map
    out.push_str(
        "pub static TRANSLATIONS: Lazy<HashMap<&'static str, TrayStrings>> = Lazy::new(|| {\n",
    );
    out.push_str("    let mut m = HashMap::new();\n");

    for (lang, tray) in &translations {
        out.push_str(&format!("    m.insert(\"{lang}\", TrayStrings {{\n"));
        for (rust_field, json_key) in &fields {
            let val = tray.get(json_key).and_then(|v| v.as_str()).unwrap_or("");
            out.push_str(&format!(
                "        {rust_field}: \"{}\".to_string(),\n",
                escape_string(val)
            ));
        }
        out.push_str("    });\n");
    }

    out.push_str("    m\n});\n");

    fs::write(Path::new(&out_dir).join("tray_translations.rs"), out).unwrap();

    println!(
        "cargo:warning=Generated tray translations: {} languages, {} fields",
        translations.len(),
        fields.len()
    );
}

fn camel_to_snake(s: &str) -> String {
    s.chars()
        .enumerate()
        .fold(String::new(), |mut acc, (i, c)| {
            if c.is_uppercase() && i > 0 {
                acc.push('_');
            }
            acc.push(c.to_lowercase().next().unwrap());
            acc
        })
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn build_apple_intelligence_bridge() {
    use std::env;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const REAL_SWIFT_FILE: &str = "swift/apple_intelligence.swift";
    const STUB_SWIFT_FILE: &str = "swift/apple_intelligence_stub.swift";
    const BRIDGE_HEADER: &str = "swift/apple_intelligence_bridge.h";

    println!("cargo:rerun-if-changed={REAL_SWIFT_FILE}");
    println!("cargo:rerun-if-changed={STUB_SWIFT_FILE}");
    println!("cargo:rerun-if-changed={BRIDGE_HEADER}");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let object_path = out_dir.join("apple_intelligence.o");
    let static_lib_path = out_dir.join("libapple_intelligence.a");

    // SDKROOT/SWIFTC env-var overrides let non-Xcode toolchains (e.g. nixpkgs
    // with apple-sdk_* + standalone swift) bypass xcrun, which is Xcode-only.
    let sdk_path = env::var("SDKROOT").unwrap_or_else(|_| {
        String::from_utf8(
            Command::new("xcrun")
                .args(["--sdk", "macosx", "--show-sdk-path"])
                .output()
                .expect("Failed to locate macOS SDK")
                .stdout,
        )
        .expect("SDK path is not valid UTF-8")
        .trim()
        .to_string()
    });

    // Check if the SDK supports FoundationModels (required for Apple Intelligence)
    let framework_path =
        Path::new(&sdk_path).join("System/Library/Frameworks/FoundationModels.framework");
    
    // HANDY_FORCE_AI_STUB=1 is an explicit escape hatch: force the stub even when
    // the active toolchain could build the real path.
    let force_stub = env::var("HANDY_FORCE_AI_STUB").as_deref() == Ok("1");

    // Auto-detect a Command-Line-Tools-only toolchain.
    let command_line_tools_only = env::var("SWIFTC").is_err() && is_command_line_tools_only();
    if command_line_tools_only && !force_stub {
        println!(
            "cargo:warning=Command Line Tools-only toolchain detected; Apple Intelligence \
             (FoundationModels) needs full Xcode. Falling back to stubs."
        );
    }

    let has_foundation_models = framework_path.exists() && !force_stub && !command_line_tools_only;

    let source_file = if has_foundation_models {
        println!("cargo:warning=Building with Apple Intelligence support.");
        REAL_SWIFT_FILE
    } else {
        if framework_path.exists() {
            println!("cargo:warning=Building Apple Intelligence with stubs.");
        } else {
            println!("cargo:warning=Apple Intelligence SDK not found. Building with stubs.");
        }
        STUB_SWIFT_FILE
    };

    if !Path::new(source_file).exists() {
        panic!("Source file {} is missing!", source_file);
    }

    // See SDKROOT note above — same env-override pattern for non-Xcode toolchains.
    let swiftc_path = env::var("SWIFTC").unwrap_or_else(|_| {
        String::from_utf8(
            Command::new("xcrun")
                .args(["--find", "swiftc"])
                .output()
                .expect("Failed to locate swiftc")
                .stdout,
        )
        .expect("swiftc path is not valid UTF-8")
        .trim()
        .to_string()
    });

    let toolchain_swift_lib = Path::new(&swiftc_path)
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("lib/swift/macosx"))
        .expect("Unable to determine Swift toolchain lib directory");
    let sdk_swift_lib = Path::new(&sdk_path).join("usr/lib/swift");

    // Use macOS 11.0 as deployment target for compatibility
    let status = Command::new(&swiftc_path)
        .args([
            "-parse-as-library",
            "-target",
            "arm64-apple-macosx11.0",
            "-sdk",
            &sdk_path,
            "-O",
            "-import-objc-header",
            BRIDGE_HEADER,
            "-c",
            source_file,
            "-o",
            object_path
                .to_str()
                .expect("Failed to convert object path to string"),
        ])
        .status()
        .expect("Failed to invoke swiftc for Apple Intelligence bridge");

    if !status.success() {
        panic!("swiftc failed to compile {source_file}");
    }

    let status = Command::new("libtool")
        .args([
            "-static",
            "-o",
            static_lib_path
                .to_str()
                .expect("Failed to convert static lib path to string"),
            object_path
                .to_str()
                .expect("Failed to convert object path to string"),
        ])
        .status()
        .expect("Failed to create static library for Apple Intelligence bridge");

    if !status.success() {
        panic!("libtool failed for Apple Intelligence bridge");
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=apple_intelligence");
    println!(
        "cargo:rustc-link-search=native={}",
        toolchain_swift_lib.display()
    );
    println!("cargo:rustc-link-search=native={}", sdk_swift_lib.display());
    println!("cargo:rustc-link-lib=framework=Foundation");

    if has_foundation_models {
        // Use weak linking so the app can launch on systems without FoundationModels
        println!("cargo:rustc-link-arg=-weak_framework");
        println!("cargo:rustc-link-arg=FoundationModels");
    }

    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
}

/// Returns true when the active developer directory is the standalone Command
/// Line Tools rather than a full Xcode install.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn is_command_line_tools_only() -> bool {
    use std::process::Command;

    Command::new("xcode-select")
        .arg("-p")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|path| path.trim().ends_with("CommandLineTools"))
        .unwrap_or(false)
}

/// Stage the MSVC runtime DLLs into `transcribe-libs/` for app-local deployment.
fn stage_vc_runtime_dlls() {
    use std::path::PathBuf;

    println!("cargo:rerun-if-env-changed=HANDY_VC_REDIST_DIRS");

    let Some(redist_dirs) = std::env::var_os("HANDY_VC_REDIST_DIRS") else {
        return;
    };

    let dest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("transcribe-libs");
    std::fs::create_dir_all(&dest).expect("create transcribe-libs staging dir");

    let mut copied: Vec<String> = Vec::new();
    for dir in std::env::split_paths(&redist_dirs) {
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("HANDY_VC_REDIST_DIRS: read {}: {e}", dir.display()))
            .flatten()
        {
            let src = entry.path();
            let name = src
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let lower = name.to_lowercase();
            let wanted = lower.ends_with(".dll")
                && (lower.starts_with("msvcp140")
                    || lower.starts_with("vcruntime140")
                    || lower.starts_with("vcomp140"));
            if wanted {
                std::fs::copy(&src, dest.join(&name))
                    .unwrap_or_else(|e| panic!("copy {}: {e}", src.display()));
                copied.push(lower);
            }
        }
    }

    for required in ["msvcp140.dll", "vcruntime140.dll"] {
        if !copied.iter().any(|n| n == required) {
            panic!(
                "HANDY_VC_REDIST_DIRS is set but {required} was not found in it; \
                 the app-local VC++ runtime would be incomplete and Handy would \
                 crash on machines without a current redist (issue #1527)"
            );
        }
    }
    println!(
        "cargo:warning=Staged {} VC++ runtime DLL(s) for app-local deployment",
        copied.len()
    );
}

/// Copy the dynamically-linked ONNX Runtime `onnxruntime.dll` into the
/// `transcribe-libs/` staging dir so `tauri.windows.conf.json` bundles it beside
/// `Handy.exe` (Windows resolves DLLs from the executable's directory).
fn stage_onnxruntime_dll() {
    use std::path::PathBuf;

    println!("cargo:rerun-if-env-changed=ORT_LIB_LOCATION");
    println!("cargo:rerun-if-env-changed=ORT_PREFER_DYNAMIC_LINK");

    if std::env::var_os("ORT_PREFER_DYNAMIC_LINK").is_none() {
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Some(lib_location) = std::env::var_os("ORT_LIB_LOCATION") else {
        return;
    };

    let src = PathBuf::from(&lib_location).join("onnxruntime.dll");
    if !src.exists() {
        panic!(
            "ORT_PREFER_DYNAMIC_LINK is set but {} does not exist; a dynamic ORT \
             build must supply onnxruntime.dll to bundle",
            src.display()
        );
    }

    let dest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("transcribe-libs");
    std::fs::create_dir_all(&dest_dir).expect("create transcribe-libs staging dir");
    std::fs::copy(&src, dest_dir.join("onnxruntime.dll"))
        .unwrap_or_else(|e| panic!("copy {}: {e}", src.display()));
    println!("cargo:warning=Staged onnxruntime.dll for Windows bundling");
}

/// Stage transcribe-cpp's shared runtime libraries into `transcribe-libs/` so the
/// installer can ship them next to the executable.
fn stage_transcribe_runtime_libs() {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    println!("cargo:rerun-if-env-changed=DEP_TRANSCRIBE_CPP_RUNTIME_DIR");
    println!("cargo:rerun-if-env-changed=DEP_TRANSCRIBE_CPP_MODULE_DIR");

    // Present only in a shared posture. A static build has nothing to ship.
    let Some(runtime_dir) = std::env::var_os("DEP_TRANSCRIBE_CPP_RUNTIME_DIR") else {
        return;
    };

    let mut dirs = BTreeSet::new();
    dirs.insert(PathBuf::from(runtime_dir));
    if let Some(module_dir) = std::env::var_os("DEP_TRANSCRIBE_CPP_MODULE_DIR") {
        dirs.insert(PathBuf::from(module_dir));
    }

    let dest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("transcribe-libs");
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest).expect("create transcribe-libs staging dir");

    let mut copied = 0usize;
    for dir in &dirs {
        println!("cargo:rerun-if-changed={}", dir.display());
        for entry in std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .flatten()
        {
            let src = entry.path();
            let name = src.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let is_lib = name.ends_with(".dll")
                || name.ends_with(".dylib")
                || name.ends_with(".so")
                || name.contains(".so.");
            if is_lib {
                std::fs::copy(&src, dest.join(name))
                    .unwrap_or_else(|e| panic!("copy {}: {e}", src.display()));
                copied += 1;
            }
        }
    }
    if copied == 0 {
        panic!(
            "no transcribe-cpp runtime libraries found under {dirs:?}; a shared / \
             dynamic-backends build must ship them or the app registers zero \
             compute devices"
        );
    }
    println!("cargo:warning=Staged {copied} transcribe-cpp runtime library file(s)");
}
