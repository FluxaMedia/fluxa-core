#![allow(clippy::missing_safety_doc)]

use crate::app_state::*;
use crate::headless_engine::*;
use jni::objects::JClass;
pub(crate) use jni::objects::JString;
use jni::sys::{jboolean, jlong, jstring};
pub(crate) use jni::JNIEnv;
use std::ptr;

pub(crate) type JBoolean = jboolean;
pub(crate) type JLong = jlong;
pub(crate) type JObject<'local> = JClass<'local>;
pub(crate) type JStringReturn = jstring;

pub(crate) fn read_jstring(env: &mut JNIEnv<'_>, value: &JString<'_>) -> Option<String> {
    env.get_string(value)
        .ok()
        .map(|value| value.to_string_lossy().into_owned())
}

pub(crate) fn write_jstring(env: &mut JNIEnv<'_>, value: Option<String>) -> JStringReturn {
    let Some(value) = value else {
        return ptr::null_mut();
    };
    env.new_string(value)
        .map(JString::into_raw)
        .unwrap_or_else(|_| ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_coreInvokeNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    method: JString<'_>,
    args_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &method).and_then(|method| {
            let args_json = read_jstring(&mut env, &args_json)?;
            Some(crate::ffi::core_invoke(&method, &args_json))
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_createAppCoreStateNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    initial_json: JString<'_>,
) -> JLong {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_jstring(&mut env, &initial_json)
            .map(|initial_json| create_app_core_state(&initial_json) as JLong)
            .unwrap_or(0)
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_destroyAppCoreStateNative(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    handle: JLong,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if handle > 0 && destroy_app_core_state(handle as u64) {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_appCoreStateJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    handle: JLong,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = if handle > 0 {
            app_core_state_json(handle as u64)
        } else {
            None
        };
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_appCoreDispatchJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    handle: JLong,
    action_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = if handle > 0 {
            read_jstring(&mut env, &action_json)
                .and_then(|action_json| app_core_dispatch_json(handle as u64, &action_json))
        } else {
            None
        };
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_createHeadlessEngineNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    initial_json: JString<'_>,
) -> JLong {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_jstring(&mut env, &initial_json)
            .map(|initial_json| create_headless_engine(&initial_json) as JLong)
            .unwrap_or(0)
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_destroyHeadlessEngineNative(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    handle: JLong,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if handle > 0 && destroy_headless_engine(handle as u64) {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_headlessEngineSnapshotJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    handle: JLong,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = if handle > 0 {
            headless_engine_snapshot_json(handle as u64)
        } else {
            None
        };
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_headlessEngineDispatchJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    handle: JLong,
    action_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = if handle > 0 {
            read_jstring(&mut env, &action_json)
                .and_then(|action_json| headless_engine_dispatch_json(handle as u64, &action_json))
        } else {
            None
        };
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_headlessEngineCompleteEffectJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    handle: JLong,
    result_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = if handle > 0 {
            read_jstring(&mut env, &result_json).and_then(|result_json| {
                headless_engine_complete_effect_json(handle as u64, &result_json)
            })
        } else {
            None
        };
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_drainCoreErrorLogJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        write_jstring(&mut env, Some(crate::log_sink::drain_core_log_json()))
    }))
    .unwrap_or(ptr::null_mut())
}
