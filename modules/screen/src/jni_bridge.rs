use jni::objects::GlobalRef;
use jni::JNIEnv;
use std::sync::OnceLock;

static APPLICATION_CONTEXT: OnceLock<GlobalRef> = OnceLock::new();

pub fn set_application_context(env: &JNIEnv, obj: &jni::objects::JObject) {
    let global = env.new_global_ref(obj)
        .expect("jni_bridge: failed to create GlobalRef for Application Context");
    let _ = APPLICATION_CONTEXT.set(global);
}

pub fn get_application_context() -> Option<&'static GlobalRef> {
    APPLICATION_CONTEXT.get()
}

pub fn has_application_context() -> bool {
    APPLICATION_CONTEXT.get().is_some()
}
