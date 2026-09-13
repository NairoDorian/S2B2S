// Phase 0(b): the Tauri IPC harness the settings window needs to boot in a
// plain browser. Injected with `addInitScript({ path })`, so it is plain script
// — no imports — and it must run before any page module.
//
// Why this is required at all: the app dies on its FIRST synchronous line
// outside Tauri. `src/main.tsx:21` calls `platform()` from
// `@tauri-apps/plugin-os`, which reads `window.__TAURI_OS_PLUGIN_INTERNALS__`
// — a global Tauri's build step injects into the webview. It is not an
// `invoke`, so `@tauri-apps/api/mocks` does not provide it and no amount of
// `mockIPC` will help. Without the global the module throws before
// `createRoot(...).render(...)` is ever reached and `#root` stays empty.
//
// The `__TAURI_INTERNALS__` plumbing below mirrors the real
// `@tauri-apps/api/mocks` `mockIPC` so behaviour matches what the app would
// see under Tauri, rather than a guess at it.
(function () {
  // ---- synchronous globals a Tauri build injects -------------------------
  window.__TAURI_OS_PLUGIN_INTERNALS__ = {
    platform: "windows",
    version: "10.0.26200",
    family: "windows",
    arch: "x86_64",
    type: "Windows_NT",
    locale: "en-US",
    exe_extension: "exe",
    eol: "\r\n",
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: function () {},
  };
  window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};

  // ---- callback registry (mirrors mocks.js) ------------------------------
  var callbacks = new Map();
  var nextId = 1;

  function registerCallback(callback, once) {
    var identifier = nextId++;
    callbacks.set(identifier, function (data) {
      if (once) callbacks.delete(identifier);
      return callback && callback(data);
    });
    return identifier;
  }
  function runCallback(id, data) {
    var cb = callbacks.get(id);
    if (cb) cb(data);
  }
  function unregisterCallback(id) {
    callbacks.delete(id);
  }

  window.__TAURI_INTERNALS__.transformCallback = registerCallback;
  window.__TAURI_INTERNALS__.unregisterCallback = unregisterCallback;
  window.__TAURI_INTERNALS__.runCallback = runCallback;
  window.__TAURI_INTERNALS__.callbacks = callbacks;
  window.__TAURI_INTERNALS__.convertFileSrc = function (p) {
    return p;
  };
  window.__TAURI_INTERNALS__.metadata = {
    currentWindow: { label: "main" },
    currentWebview: { label: "main" },
    windows: [{ label: "main" }],
    webviews: [{ label: "main" }],
  };

  // ---- event plugin ------------------------------------------------------
  var listeners = new Map();

  function handleEventPlugin(cmd, args) {
    switch (cmd) {
      case "plugin:event|listen":
        if (!listeners.has(args.event)) listeners.set(args.event, []);
        listeners.get(args.event).push(args.handler);
        return args.handler;
      case "plugin:event|emit":
        (listeners.get(args.event) || []).forEach(function (h) {
          runCallback(h, args);
        });
        return null;
      case "plugin:event|unlisten": {
        // `eventId`, not `id`: `@tauri-apps/api`'s `_unlisten` passes the id it
        // was handed back by `plugin:event|listen`, which here is the callback
        // id that `listen` pushed. Reading the wrong key used to make this a
        // silent no-op, so a re-registered listener accumulated instead of
        // replacing the one it superseded.
        var ls = listeners.get(args.event);
        if (ls) {
          var i = ls.indexOf(args.eventId);
          if (i !== -1) ls.splice(i, 1);
        }
        return null;
      }
      default:
        return null;
    }
  }

  // ---- command handler ---------------------------------------------------
  // The default fixtures are deliberately empty-but-well-typed so the store
  // hydrates and the shell paints. Every unhandled command is recorded on
  // `window.__TAURI_UNHANDLED__` and warned about, so a new call site shows up
  // as a failing assertion rather than a silently blank page.
  window.__TAURI_UNHANDLED__ = [];
  // Every `invoke` is recorded here, in order, so a test can assert what the
  // app actually asked the backend for — the persistence half of a setting
  // round-trip. Cleared and read per test, never asserted on wholesale.
  window.__TAURI_CALLS__ = [];

  // Commands are called BARE: `src/bindings.ts` wraps every one in `typedError`,
  // which turns a resolved value into `{status:"ok", data}` itself. Returning an
  // envelope here would nest it and the store would read `undefined` forever.
  //
  // `get_app_settings` and `get_default_settings` come from the generated
  // fixture, which `tests/harness/fixtures.ts` puts on `window` before this
  // script runs.
  var settings = window.__TAURI_MOCK_SETTINGS__ || {};

  var RESPONSES = {
    get_app_settings: settings,
    get_default_settings: settings,

    // The debug panel's log console polls the log file; the mock has no file,
    // so the tail is empty and the console shows only what its live stream
    // receives (which is nothing, without the plugin's webview target).
    get_recent_logs: "",
    clear_logs: null,

    // Persistence: the app also writes settings through tauri-plugin-store.
    "plugin:store|load": 1,
    "plugin:store|get": [null, false],
    "plugin:store|set": null,
    "plugin:store|save": null,
    "plugin:store|delete": null,
    "plugin:store|clear": null,
    "plugin:store|reload": null,
    "plugin:store|keys": [],
    "plugin:store|values": [],

    // Plugins.
    "plugin:window|is_maximized": false,
    "plugin:os|platform": "windows",
    "plugin:os|locale": "en-US",
    "plugin:app|version": "0.0.0-test",
    "plugin:app|name": "zer0",
    // Resolved from the environment by the fixture at inject time (see
    // `fixtures.ts`) — this file carries no machine-specific path.
    "plugin:path|resolve_directory":
      (window.__TAURI_MOCK_PATHS__ && window.__TAURI_MOCK_PATHS__.appDataDir) ||
      "/mock/appdata/roaming",

    // ---- scalars and legal nulls ------------------------------------------
    // Everything the app reads as an object or an array comes from the
    // generated fixture instead (see `IPC_PAYLOADS` in generate-fixtures.ts);
    // those are the ones where a wrong value is a crash rather than a blank
    // corner of the UI. What is left here divides in two:
    //
    //   NULL IS AN ANSWER. `bindings.ts` declares these `| null` or as
    //   `Option<T>`, so "there is no llama install" / "no sample yet" is a
    //   real state the UI already renders. Returning null exercises it.
    detect_llama_install: null,
    detect_cuda_toolkit: null,
    get_system_stats: null,
    get_transcription_model_status: null,
    get_current_model: null,
    get_current_audio_device: null,
    get_latest_recording_info: null,

    //   VOID. Fire-and-forget commands: the app awaits them and ignores the
    //   result, so null is the only correct reply. Listed rather than left to
    //   fall through, because `__TAURI_UNHANDLED__` is an ASSERTION in the
    //   suite and it only stays useful if a non-empty list means "a command
    //   nobody has seen before" rather than "a command this fixture has not
    //   got round to".
    //
    //   `bindings.ts` declares these `typedError<null, string>`, and each is
    //   driven on purpose by exactly one test — they are the only setting writes
    //   the net performs, so they are declared to keep `__TAURI_UNHANDLED__`
    //   empty there. Every other `change_*_setting` is deliberately absent: no
    //   page reaches for one unprompted, and a new call site failing this
    //   assertion is the point of it.
    change_start_hidden_setting: null,
    //   The language selector. Same shape of proof as the toggle above, from
    //   the other end of the window: the write is what says the switch reached
    //   the backend at all, since the mock keeps answering `get_app_settings`
    //   with the fixture's `app_language`.
    change_app_language_setting: null,
    //   The post-processing model combobox. Nothing persists it here — the mock
    //   answers `get_app_settings` with the same fixture object every time — so
    //   the test asserts on the write leaving the webview rather than on the
    //   value coming back.
    change_post_process_model_setting: null,
    cancel_operation: null,
    cancel_file_transcription: null,
    live_fft_reset: null,
    play_test_sound: null,
    set_model_unload_timeout: null,
    initialize_enigo: null,
    initialize_shortcuts: null,

    //   PLAIN VALUES. Empty-but-typed: enough for the shell to paint, and
    //   nothing a test should assert on.
    get_available_models: [],
    get_available_microphones: [],
    get_available_output_devices: [],
    get_audio_devices: { input_devices: [], output_devices: [] },
    get_microphone_channels: [],
    list_devices: [],
    list_models: [],
    is_recording: false,
    is_portable: false,
    is_laptop: false,
    is_update_checks_locked: false,
    check_apple_intelligence_available: false,
    detect_llama_backend: "",
    get_keyboard_implementation: "",
    get_app_dir_path: "",
    get_log_dir_path: "",
    live_mode_default_output_dir: "",
    overlay_stream_text_height: 0,
  };

  // Structured results (paginated history, statistics summary, arch plugins)
  // come from the generated fixture rather than from literals here, because
  // getting one wrong is invisible at the point of the mistake: the page gets
  // `null`, reads a property off it, and the crash is reported against the
  // page. Generated, they track `src/bindings.ts`.
  var generated = window.__TAURI_MOCK_IPC__ || {};
  Object.keys(generated).forEach(function (cmd) {
    RESPONSES[cmd] = generated[cmd];
  });

  window.__TAURI_INTERNALS__.invoke = async function (cmd, args) {
    window.__TAURI_CALLS__.push({ cmd: cmd, args: args || {} });
    if (cmd.startsWith("plugin:event|")) return handleEventPlugin(cmd, args);
    if (Object.prototype.hasOwnProperty.call(RESPONSES, cmd)) {
      return RESPONSES[cmd];
    }
    window.__TAURI_UNHANDLED__.push(cmd);
    console.warn(
      "[tauri-mock] unhandled command:",
      cmd,
      JSON.stringify(args || {}).slice(0, 160),
    );
    return null;
  };
})();
