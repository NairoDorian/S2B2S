# S2B2S Futuristic Vision — Transparent GPU Overlay & Screen Understanding

This document consolidates the conceptual architecture, UI/UX design specifications, and implementation roadmaps for the next generation of S2B2S, originally detailed in `futuristic_analysis/` (v0.1.3). It bridges today's Tauri-overlay experience with a platform-native transparent GPU overlay, cybernetic avatar, and screen-vision capabilities.

---

## 1. Transparent GPU Overlay Architecture

The overlay system is structured as a **two-track rendering architecture**:

```
+---------------------------------------------------------------+
|                       S2B2S Overlay                           |
+------------------------------------+--------------------------+
|  Track A: Tauri WebView Overlay    |  Track B: Native WGPU    |
|  - HTML/JS/CSS rendering           |  - WGPU render pipeline  |
|  - Rich UI, text rendering         |  - Physics-based trails  |
|  - Translucent windows             |  - Ultra-low latency     |
+------------------------------------+--------------------------+
```

### Track A: Tauri WebView Overlay (Current Default)

- **Implementation**: Transparent Webview windows (`always_on_top`, click-through using `set_ignore_cursor_events`).
- **Use Case**: Renders text bubbles, markdown formatting, menus, and simple interactive UI controls.
- **Platform Details**: Uses NSPanel styling on macOS, Win32 `HWND_TOPMOST` with `WS_EX_TRANSPARENT` on Windows, and GTK Layer Shell on Linux (Wayland/X11).

### Track B: Native WGPU Overlay (Next Gen Track)

- **Implementation**: Raw `wgpu` rendering directly onto a transparent OS window.
- **Use Case**: Renders ultra-smooth particle effects, cursor trails, click ripples, and high-frequency animations at native monitor refresh rates with minimal CPU overhead.
- **Vulkan/DX12 Fixes**: Incorporates fixes for GPU device selection (discrete over integrated) and guards against memory leaks on Windows (proactive memory limits and swapchain rebuilding).

---

## 2. 3D Cybernetic Avatar (Four Senses Spec)

The avatar serves as the visual representation of the LLM "Brain". Instead of static icons, it is a 3D cybernetic head/orb rendering state-based reactions.

### Core States & Visual Signifiers

1.  **Idle/Asleep**: Low-frequency wave motion, dim pulsing colors (slate/blue).
2.  **Listening**: Reacts dynamically to user microphone input level (RMS amplitude drives circle scale and ring speed).
3.  **Thinking**: Transition animations, swirling particle rings, gold/purple highlights indicating token computation.
4.  **Speaking**: Shape morphing and frequency modulation synced with the synthesized TTS audio stream.
5.  **Error/Interrupted**: Sudden contraction or wave disruption (barge-in effect).

---

## 3. Screen Vision & Multimodal Understanding

Extends the Brain's capabilities to capture, read, and understand the user's active screen context.

### Capture Methods

- **Full Screen Capture**: Takes a screenshot of the active monitor.
- **Region Capture**: Interactive bounding-box selector (similar to snipping tools) allowing users to select visual inputs.
- **Active Window Extraction**: Captures the foreground application window only.

### Multi-modal Integration (Gemma 4)

- Captured screen frames are converted to base64 PNG and sent as `image_url` parts to the LLM (OpenAI-compatible multi-modal API structure).
- Coordinates text-selection context with visual representation to resolve ambiguous queries (e.g., "Summarize this chart").

---

## 4. Conversation Mode 2.0 State Machine

Defines the interaction cycle of hands-free conversation with barge-in support.

```
       [Idle] <---------------------------------------------+
          | (Trigger Shortcut / Wake Word)                  |
          v                                                 |
     [Listening] --(Silence / Auto-stop)--> [Processing]     |
          ^                                     |           | (Barge-in / Cancel)
          | (Auto-Rearm / Hands-free)           v           |
      [Speaking] <------------------------- [Streaming] ----+
```

1.  **Idle**: System waiting. Mic and TTS inactive.
2.  **Listening**: Recording user audio, streaming Silero VAD state.
3.  **Processing**: STT transcription runs (Parakeet/Whisper).
4.  **Streaming**: Query sent to LLM, first sentence tokens returned and split.
5.  **Speaking**: TTS plays synthesized audio. Mic remains open for barge-in.
6.  **Barge-in / Interruption**: User speech detected during TTS playback immediately aborts the active stream, stops audio, and returns to **Listening** with the new input.

---

## 5. Implementation Roadmap Checklist

### Phase A: Hardening & Infrastructure

- [ ] Fix wgpu swapchain recreation on window resize (DPI scaling).
- [ ] Implement robust click-through setting for Windows/macOS.
- [ ] Package 3D Avatar assets cleanly in frontend build.

### Phase B: Feature Delivery

- [ ] Enable region-screenshot command in Tauri.
- [ ] Plumb screen vision images into `llm_client.rs` payload.
- [ ] Finalize hands-free VAD barge-in under microphone loop.
