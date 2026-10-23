//! JNI bridge to `MainActivity.kt`.
//!
//! Rust -> Kotlin: `showDialog(id, kind, exts, name)` and `openUrl(url)` on the Activity.
//! Kotlin -> Rust: `nativeOnResult(id, paths)` delivers the picker's answer. Imported files are
//! copied into app storage by Kotlin first, so the engine always sees ordinary file paths.
#![allow(unsafe_code)]

use filmcraft_ui_egui::native_dialogs::FileDialog;
use jni::objects::{JObject, JObjectArray, JString, JValue};
use jni::sys::jint;
use jni::{JNIEnv, JavaVM};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock, PoisonError};

static PENDING: Mutex<Option<HashMap<i32, Sender<Vec<String>>>>> = Mutex::new(None);
static NEXT_ID: AtomicI32 = AtomicI32::new(1);
static CTX: OnceLock<egui::Context> = OnceLock::new();

pub fn set_context(ctx: egui::Context) {
    let _ = CTX.set(ctx);
}

/// Run `f` with a JNI env attached to this thread and the Activity object.
fn with_activity<R>(f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<R>) -> Result<R, String> {
    let ctx = ndk_context::android_context();
    // SAFETY: android-activity fills ndk_context with the process's JavaVM and the Activity before
    // `android_main` runs; both stay valid for the life of the process.
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    // SAFETY: as above; a JObject made from a raw pointer is a borrow, it never frees the reference.
    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
    let result = f(&mut env, &activity);
    if result.is_err() {
        let _ = env.exception_clear();
    }
    result.map_err(|e| e.to_string())
}

/// What the picker should do: ("open" | "openMany" | "save" | "folder", extensions, suggested name).
fn describe(dialog: FileDialog) -> (&'static str, Vec<String>, String) {
    match dialog {
        FileDialog::OpenFiles { exts } => ("openMany", exts, String::new()),
        FileDialog::OpenFile { exts, .. } | FileDialog::Relink { exts, .. } => ("open", exts, String::new()),
        FileDialog::OpenProject => ("open", vec!["fcproj".into()], String::new()),
        FileDialog::SaveProject { name } => ("save", vec!["fcproj".into()], name),
        FileDialog::SaveAs { exts, name, .. } => ("save", exts, name),
        FileDialog::Folder { .. } => ("folder", Vec::new(), String::new()),
    }
}

/// `HostHooks::run_dialog`: ask the Activity to show the system picker; the answer arrives on the
/// returned channel. A dropped sender (the call failed) counts as cancelled.
pub fn run_dialog(dialog: FileDialog) -> Receiver<Vec<String>> {
    let (tx, rx) = channel();
    let (kind, exts, name) = describe(dialog);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    PENDING.lock().unwrap_or_else(PoisonError::into_inner).get_or_insert_with(HashMap::new).insert(id, tx);
    let shown = with_activity(|env, activity| {
        let kind = env.new_string(kind)?;
        let name = env.new_string(name)?;
        let string = env.find_class("java/lang/String")?;
        let arr = env.new_object_array(exts.len() as jint, string, JObject::null())?;
        for (i, e) in exts.iter().enumerate() {
            let s = env.new_string(e)?;
            env.set_object_array_element(&arr, i as jint, s)?;
        }
        env.call_method(
            activity,
            "showDialog",
            "(ILjava/lang/String;[Ljava/lang/String;Ljava/lang/String;)V",
            &[JValue::Int(id), JValue::Object(&kind), JValue::Object(&arr), JValue::Object(&name)],
        )?;
        Ok(())
    });
    if let Err(e) = shown {
        log::error!("could not show the file picker: {e}");
        deliver(id, None);
    }
    rx
}

/// `HostHooks::open_url`.
pub fn open_url(url: &str) -> Result<(), String> {
    with_activity(|env, activity| {
        let url = env.new_string(url)?;
        env.call_method(activity, "openUrl", "(Ljava/lang/String;)V", &[JValue::Object(&url)])?;
        Ok(())
    })
}

/// Answer request `id` (`None`: drop the sender, which the app reads as cancelled) and wake the frame loop.
fn deliver(id: i32, paths: Option<Vec<String>>) {
    let sender = PENDING.lock().unwrap_or_else(PoisonError::into_inner).as_mut().and_then(|m| m.remove(&id));
    if let (Some(tx), Some(paths)) = (sender, paths) {
        let _ = tx.send(paths);
    }
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

/// Called by `MainActivity.nativeOnResult` on a Java thread.
#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_storyteller_filmcraft_MainActivity_nativeOnResult(mut env: JNIEnv, _this: JObject, request_id: jint, paths: JObjectArray) {
    let mut out = Vec::new();
    let n = env.get_array_length(&paths).unwrap_or(0);
    for i in 0..n {
        let Ok(item) = env.get_object_array_element(&paths, i) else { continue };
        let item = JString::from(item);
        if let Ok(s) = env.get_string(&item) {
            out.push(String::from(s));
        }
    }
    deliver(request_id, Some(out));
}
