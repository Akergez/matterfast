//! Handing a downloaded file to another application, on Android.
//!
//! GPUI's prompt for a new path answers nothing there, so "Save" cannot ask
//! where to put a file. It is downloaded into the application's own cache
//! instead and offered to whatever opens its kind of file, which is also what
//! a phone's own apps do with an attachment. The Java half is `Files.java`;
//! the directory has to be the one its provider is allowed to hand out
//! (`res/xml/file_paths.xml`).

use std::path::{Path, PathBuf};

use gpui_mobile::android::jni as bridge;
use jni::objects::JValue;

const CLASS: &str = "app.akergez.matterfast.Files";

/// Where a downloaded file of this name is kept until something opens it.
pub fn place_for(name: &str) -> PathBuf {
    crate::paths::cache_dir()
        .join(crate::APP_ID)
        .join("downloads")
        .join(name)
}

/// Offers the file at `path` to the applications that open `mime`.
pub fn open(path: &Path, mime: &str) -> Result<(), String> {
    bridge::with_env(|env| {
        let activity = bridge::activity(env)?;
        let class = bridge::find_app_class(env, CLASS)?;
        let path = path.to_string_lossy().into_owned();
        let path = env.new_string(&path).map_err(|e| e.to_string())?;
        let mime = env.new_string(mime).map_err(|e| e.to_string())?;
        env.call_static_method(
            &class,
            jni::jni_str!("open"),
            jni::jni_sig!("(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;)V"),
            &[
                JValue::Object(&activity),
                JValue::Object(&path),
                JValue::Object(&mime),
            ],
        )
        .map_err(|e| {
            env.exception_clear();
            e.to_string()
        })?;
        Ok(())
    })
}
