# FilmCraft for Android (native port)

Drop this folder into the repo as `apps/filmcraft-android` (the workspace globs `apps/*`, so no root
`Cargo.toml` change is needed). On desktop builds the crate compiles to nothing.

## Build

Prerequisites: Rust (rustup), Android Studio (SDK + NDK r26+; set `ANDROID_HOME`, `ANDROID_NDK_HOME`), JDK 17.

```bash
cd apps/filmcraft-android
# one-time: create the Gradle wrapper (or just open android/ in Android Studio once)
(cd android && gradle wrapper --gradle-version 8.9)
./build-apk.sh            # debug APK
./build-apk.sh release    # release APK (sign it with your own keystore)
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

## What is in it

| Piece | What it does |
|---|---|
| `src/lib.rs` | `android_main`: logging, crash log, GPU export renderer, eframe/wgpu (Vulkan) window, `HostHooks` |
| `src/bridge.rs` | JNI: system file picker, links, and the callback that wakes the frame loop |
| `src/touch.rs` | 40dp touch targets, larger text, and a landscape **phone workspace** (Program + panels over Timeline) |
| `src/audio*.rs` | The desktop cpal audio code, reused (cpal uses Oboe on Android) |
| `android/` | Gradle project, GameActivity (soft keyboard), keep-screen-on, landscape, mic permission |

File flow: **Import** opens the Android picker and copies the chosen files into app storage
(the engine works on real paths). **Export / Save** writes into app storage; finished videos, audio
and images are then published to `Movies/FilmCraft`, `Music/FilmCraft` or `Pictures/FilmCraft`.
Projects are saved in the app's `Projects` folder.

## Status: first draft, not yet compiled

This was written without an Android toolchain, so expect a round of compile fixes. Likely spots:
- `eframe` feature name / `NativeOptions::android_app` for your pinned eframe 0.36.
- `androidx.games:games-activity` version must match the `android-activity` crate (see its README).
- Crates pulled in by the engine that don't build for Android (check `cargo ndk build` output).
- `app.ui.dock` may be replaced by the restored workspace on first frame (`lib.rs` workspace restore).

## Not done yet (phase 2)

- Hardware decode/encode through MediaCodec (software decoders are used now; expect slow 4K).
- Gestures: pinch-zoom and two-finger pan on the Timeline, long-press context menus, bottom toolbar.
- Per-panel mobile layouts (Lumetri, Essential Graphics) instead of the shared desktop panels.
- Streaming imports without copying (large files are duplicated into app storage today).
- Android back button, share-sheet "open with FilmCraft", clipboard media, contextual mic prompt.
