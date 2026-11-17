//! Android TrustManager must be initialized before reqwest makes a TLS request.
use jni::{
    EnvUnowned,
    errors::ThrowRuntimeExAndDefault,
    objects::JObject,
    sys::{JNIEnv, jboolean, jclass, jobject},
};

/// Called once with the Application context; no Activity reference is retained.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_net_dmsg_android_core_NativeBootstrap_initialize(
    env: *mut JNIEnv,
    _class: jclass,
    context: jobject,
) -> jboolean {
    let mut env = unsafe { EnvUnowned::from_raw(env) };
    env.with_env(|env| {
        let context = unsafe { JObject::from_raw(env, context) };
        rustls_platform_verifier::android::init_with_env(env, context)?;
        Ok::<bool, jni::errors::Error>(true)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
