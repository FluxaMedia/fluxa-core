use crate::{app_state, core_contract, headless_engine};

#[uniffi::export]
pub fn fluxa_core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Funnel entry point — Swift calls this instead of binding each helper.
#[uniffi::export]
pub fn core_invoke(method: String, args_json: String) -> String {
    crate::ffi::core_invoke(&method, &args_json)
}

#[uniffi::export]
pub fn create_headless_engine_json(initial_json: String) -> i64 {
    headless_engine::create_headless_engine(&initial_json) as i64
}

#[uniffi::export]
pub fn destroy_headless_engine_json(handle: i64) -> bool {
    handle > 0 && headless_engine::destroy_headless_engine(handle as u64)
}

#[uniffi::export]
pub fn headless_engine_snapshot_json(handle: i64) -> String {
    if handle <= 0 {
        return String::new();
    }
    headless_engine::headless_engine_snapshot_json(handle as u64).unwrap_or_default()
}

#[uniffi::export]
pub fn headless_engine_dispatch_json(handle: i64, action_json: String) -> String {
    if handle <= 0 {
        return String::new();
    }
    headless_engine::headless_engine_dispatch_json(handle as u64, &action_json).unwrap_or_default()
}

#[uniffi::export]
pub fn headless_engine_complete_effect_json(handle: i64, result_json: String) -> String {
    if handle <= 0 {
        return String::new();
    }
    headless_engine::headless_engine_complete_effect_json(handle as u64, &result_json)
        .unwrap_or_default()
}

#[uniffi::export]
pub fn core_capabilities_json(portable: bool) -> String {
    core_contract::core_capabilities_json(portable)
}

#[uniffi::export]
pub fn create_app_core_state_json(initial_json: String) -> i64 {
    app_state::create_app_core_state(&initial_json) as i64
}

#[uniffi::export]
pub fn destroy_app_core_state_json(handle: i64) -> bool {
    handle > 0 && app_state::destroy_app_core_state(handle as u64)
}

#[uniffi::export]
pub fn app_core_state_json(handle: i64) -> String {
    if handle <= 0 {
        return String::new();
    }
    app_state::app_core_state_json(handle as u64).unwrap_or_default()
}

#[uniffi::export]
pub fn app_core_dispatch_json(handle: i64, action_json: String) -> String {
    if handle <= 0 {
        return String::new();
    }
    app_state::app_core_dispatch_json(handle as u64, &action_json).unwrap_or_default()
}
