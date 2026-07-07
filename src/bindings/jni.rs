#![allow(clippy::missing_safety_doc)]

use crate::addon_protocol::*;
use crate::addon_resource::*;
use crate::addon_store::*;
use crate::app_state::*;
use crate::calendar_plan::*;
use crate::content_identity::*;
use crate::core_contract::*;
use crate::data_policy::*;
use crate::discovery_plan::*;
use crate::dolby_vision_rpu::*;
use crate::external_sync::*;
use crate::headless_adapter_plan::*;
use crate::headless_engine::*;
use crate::home_ranking::*;
use crate::library_state::*;
use crate::offline_download::*;
use crate::player_flow::*;
use crate::player_policy::*;
use crate::player_scrobble::*;
use crate::profile_contract::*;
use crate::profile_prefs::*;
use crate::repository_flow::*;
use crate::search_plan::*;
use crate::stream_policy::*;
use crate::watchlist_plan::*;
use jni::objects::JClass;
pub(crate) use jni::objects::JString;
use jni::sys::{jboolean, jdouble, jfloat, jint, jlong, jstring};
pub(crate) use jni::JNIEnv;
use serde_json::json;
use std::ptr;

pub(crate) type JBoolean = jboolean;
pub(crate) type JDouble = jdouble;
pub(crate) type JFloat = jfloat;
pub(crate) type JInt = jint;
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

macro_rules! string_fn {
    ($name:ident, $body:expr) => {
        #[no_mangle]
        pub unsafe extern "system" fn $name(
            mut env: JNIEnv<'_>,
            _class: JObject<'_>,
            input: JString<'_>,
        ) -> JStringReturn {
            // A panic in the domain logic must not abort the host process —
            // catch it and hand back null, same as any other "couldn't compute
            // a result" outcome this function already returns.
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let output = read_jstring(&mut env, &input).map($body);
                write_jstring(&mut env, output)
            }))
            .unwrap_or(ptr::null_mut())
        }
    };
}

string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_normalizeManifestUrlNative,
    |value: String| normalize_manifest_url(&value)
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_identityNative,
    |value: String| identity(&value)
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_baseUrlNative,
    |value: String| base_url(&value)
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_preferHttpsAssetUrlNative,
    |value: String| prefer_https_asset_url(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_addonStoreInputTypeNative,
    |value: String| addon_store_input_type(&value).to_string()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_normalizeCloudstreamRepoUrlNative,
    |value: String| normalize_cloudstream_repo_url(&value)
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_normalizePluginRepositoryUrlNative,
    |value: String| normalize_plugin_repository_url(&value)
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_extractAddonManifestUrlNative,
    |value: String| extract_addon_manifest_url(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_offlineDownloadPlanJsonNative,
    |value: String| offline_download_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_profileSafePrefsJsonNative,
    |value: String| profile_safe_prefs_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_profileLocalAddonsKeyNative,
    |value: String| profile_local_addons_key_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_cacheEntryPolicyJsonNative,
    |value: String| cache_entry_policy_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_cacheTrimPolicyJsonNative,
    |value: String| cache_trim_policy_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_dataFailurePolicyJsonNative,
    |value: String| data_failure_policy_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_addonStoreSearchPolicyJsonNative,
    |value: String| addon_store_search_policy_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_playerTrackStateJsonNative,
    |value: String| player_track_state_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_repositoryMetaDetailPlanJsonNative,
    |value: String| repository_meta_detail_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_manifestFetchDecisionJsonNative,
    |value: String| manifest_fetch_decision_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_addonResourceRequestPlanJsonNative,
    |value: String| addon_resource_request_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_headlessProviderAvailabilityPlanJsonNative,
    |value: String| provider_availability_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_headlessDetailStreamResultPlanJsonNative,
    |value: String| detail_stream_result_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_headlessPrefetchDetailStreamsPlanJsonNative,
    |value: String| prefetch_detail_streams_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_dolbyVisionRpuInfoJsonNative,
    |value: String| dolby_vision_rpu_info_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_dolbyVisionConvertRpuJsonNative,
    |value: String| dolby_vision_convert_rpu_json(&value).unwrap_or_default()
);

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_repositorySeasonVideosJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    meta_detail_json: JString<'_>,
    season_number: JInt,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &meta_detail_json).map(|meta_detail_json| {
            repository_season_videos_json(&meta_detail_json, season_number)
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_sanitizeProfileJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    profile_json: JString<'_>,
    mirrored_addons_json: JString<'_>,
    merge_mirrored_addons: JBoolean,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &profile_json).and_then(|profile_json| {
            sanitize_profile_json(
                &profile_json,
                &read_jstring(&mut env, &mirrored_addons_json)?,
                merge_mirrored_addons != 0,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_pluginIsSecureRemoteUrlNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    url: JString<'_>,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if read_jstring(&mut env, &url)
            .map(|url| is_secure_remote_url(&url))
            .unwrap_or(false)
        {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_pluginSameRepositoryUrlNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    left: JString<'_>,
    right: JString<'_>,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if read_jstring(&mut env, &left)
            .and_then(|left| {
                Some(same_plugin_repository_url(
                    &left,
                    &read_jstring(&mut env, &right)?,
                ))
            })
            .unwrap_or(false)
        {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_addonStreamsWithProviderJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    streams_json: JString<'_>,
    addon_name: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &streams_json).and_then(|streams_json| {
            Some(addon_streams_with_provider_json(
                &streams_json,
                &read_jstring(&mut env, &addon_name)?,
            ))
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
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_coreCapabilitiesJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    portable: JBoolean,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        write_jstring(&mut env, Some(core_capabilities_json(portable != 0)))
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

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_headlessDirectPlaybackPolicyJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        write_jstring(&mut env, Some(direct_playback_policy_json()))
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_manifestCandidatesJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    input: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &input)
            .and_then(|value| serde_json::to_string(&manifest_candidates(&value)).ok());
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_manifestFetchPlanJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    input: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output =
            read_jstring(&mut env, &input).and_then(|value| manifest_fetch_plan_json(&value));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_buildResourceUrlNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    transport_url: JString<'_>,
    resource: JString<'_>,
    content_type: JString<'_>,
    id: JString<'_>,
    extra_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &transport_url).and_then(|transport_url| {
            Some(build_resource_url(
                &transport_url,
                &read_jstring(&mut env, &resource)?,
                &read_jstring(&mut env, &content_type)?,
                &read_jstring(&mut env, &id)?,
                read_jstring(&mut env, &extra_json).as_deref(),
            ))
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_parseManifestJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    body: JString<'_>,
    transport_url: JString<'_>,
    unknown_name: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &body).and_then(|body| {
            parse_manifest(
                &body,
                &read_jstring(&mut env, &transport_url)?,
                &read_jstring(&mut env, &unknown_name)?,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_resolveManifestAssetsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    descriptor_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &descriptor_json)
            .and_then(|descriptor_json| resolve_manifest_assets_json(&descriptor_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_mergeLiveManifestJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    descriptor_json: JString<'_>,
    live_json: JString<'_>,
    unknown_name: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &descriptor_json).and_then(|descriptor_json| {
            merge_live_manifest_json(
                &descriptor_json,
                read_jstring(&mut env, &live_json)
                    .filter(|value| !value.trim().is_empty())
                    .as_deref(),
                &read_jstring(&mut env, &unknown_name)?,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_supportsResourceNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    manifest_json: JString<'_>,
    resource_name: JString<'_>,
    content_type: JString<'_>,
    id: JString<'_>,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let result = read_jstring(&mut env, &manifest_json)
            .and_then(|manifest_json| {
                Some(supports_resource(
                    &manifest_json,
                    &read_jstring(&mut env, &resource_name)?,
                    read_jstring(&mut env, &content_type)
                        .filter(|value| !value.is_empty())
                        .as_deref(),
                    read_jstring(&mut env, &id)
                        .filter(|value| !value.is_empty())
                        .as_deref(),
                ))
            })
            .unwrap_or(false);
        if result {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_catalogSupportsExtraNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    catalog_json: JString<'_>,
    extra_name: JString<'_>,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let result = read_jstring(&mut env, &catalog_json)
            .and_then(|catalog_json| {
                Some(catalog_supports_extra(
                    &catalog_json,
                    &read_jstring(&mut env, &extra_name)?,
                ))
            })
            .unwrap_or(false);
        if result {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_catalogRequiresExtraNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    catalog_json: JString<'_>,
    extra_name: JString<'_>,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let result = read_jstring(&mut env, &catalog_json)
            .and_then(|catalog_json| {
                Some(catalog_requires_extra(
                    &catalog_json,
                    &read_jstring(&mut env, &extra_name)?,
                ))
            })
            .unwrap_or(false);
        if result {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_catalogHasRequiredExtraExceptNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    catalog_json: JString<'_>,
    allowed_names_json: JString<'_>,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let result = read_jstring(&mut env, &catalog_json)
            .and_then(|catalog_json| {
                Some(catalog_has_required_extra_except(
                    &catalog_json,
                    &read_jstring(&mut env, &allowed_names_json)?,
                ))
            })
            .unwrap_or(false);
        if result {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_parseEpisodeLocatorJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    input: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &input).and_then(|input| {
            let (base_id, season, episode) = parse_episode_locator(&input)?;
            serde_json::to_string(&json!({
                "baseId": base_id,
                "season": season,
                "episode": episode
            }))
            .ok()
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_streamRequestIdsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    content_type: JString<'_>,
    id: JString<'_>,
    detail_id: JString<'_>,
    current_series_lookup_id: JString<'_>,
    canonical_base_id: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &content_type).and_then(|content_type| {
            serde_json::to_string(&stream_request_ids(
                &content_type,
                &read_jstring(&mut env, &id)?,
                read_jstring(&mut env, &detail_id)
                    .filter(|value| !value.is_empty())
                    .as_deref(),
                read_jstring(&mut env, &current_series_lookup_id)
                    .filter(|value| !value.is_empty())
                    .as_deref(),
                read_jstring(&mut env, &canonical_base_id)
                    .filter(|value| !value.is_empty())
                    .as_deref(),
            ))
            .ok()
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_playbackStreamRequestIdsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    content_type: JString<'_>,
    id: JString<'_>,
    detail_id: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &content_type).and_then(|content_type| {
            playback_stream_request_ids_json(
                &content_type,
                &read_jstring(&mut env, &id)?,
                read_jstring(&mut env, &detail_id)
                    .filter(|value| !value.is_empty())
                    .as_deref(),
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_streamDiscoveryEpisodeContextJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    content_type: JString<'_>,
    request_id: JString<'_>,
    detail_json: JString<'_>,
    season_episodes_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &content_type).and_then(|content_type| {
            stream_discovery_episode_context_json(
                &content_type,
                &read_jstring(&mut env, &request_id)?,
                read_jstring(&mut env, &detail_json)
                    .filter(|value| !value.is_empty())
                    .as_deref(),
                &read_jstring(&mut env, &season_episodes_json)?,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_playbackIntroLookupContentIdNative,
    |value: String| playback_intro_lookup_content_id(&value)
);

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_directPlaybackPlanJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    meta_json: JString<'_>,
    detail_json: JString<'_>,
    today_iso: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &meta_json).and_then(|meta_json| {
            direct_playback_plan_json(
                &meta_json,
                read_jstring(&mut env, &detail_json)
                    .filter(|value| !value.is_empty())
                    .as_deref(),
                &read_jstring(&mut env, &today_iso)?,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_streamPlaybackInfoJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    stream_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &stream_json)
            .and_then(|stream_json| stream_playback_info_json(&stream_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_streamRequestHeadersJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    headers_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &headers_json)
            .and_then(|headers_json| stream_request_headers_json(&headers_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_streamRequestRefererNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    url: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &url)
            .map(|url| stream_request_referer(&url).unwrap_or_default());
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_episodeTextMatchesNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    text: JString<'_>,
    season: JInt,
    episode: JInt,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let result = read_jstring(&mut env, &text)
            .map(|text| text_matches_episode(&text, season, episode))
            .unwrap_or(false);
        if result {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_streamMatchesEpisodeNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    video_id: JString<'_>,
    title: JString<'_>,
    name: JString<'_>,
    description: JString<'_>,
    filename: JString<'_>,
    effective_filename: JString<'_>,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let result = read_jstring(&mut env, &video_id)
            .map(|video_id| {
                stream_matches_episode(
                    &video_id,
                    &[
                        read_jstring(&mut env, &title).unwrap_or_default(),
                        read_jstring(&mut env, &name).unwrap_or_default(),
                        read_jstring(&mut env, &description).unwrap_or_default(),
                        read_jstring(&mut env, &filename).unwrap_or_default(),
                        read_jstring(&mut env, &effective_filename).unwrap_or_default(),
                    ],
                )
            })
            .unwrap_or(false);
        if result {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_selectStreamIndexNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    streams_json: JString<'_>,
    current_video_id: JString<'_>,
    initial_stream_index: JInt,
    saved_url: JString<'_>,
    saved_title: JString<'_>,
    source_selection_mode: JString<'_>,
    regex_pattern: JString<'_>,
    preferred_binge_group: JString<'_>,
) -> JInt {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_jstring(&mut env, &streams_json)
            .and_then(|streams_json| {
                Some(select_stream_index(
                    &streams_json,
                    &read_jstring(&mut env, &current_video_id).unwrap_or_default(),
                    initial_stream_index,
                    read_jstring(&mut env, &saved_url)
                        .filter(|value| !value.is_empty())
                        .as_deref(),
                    read_jstring(&mut env, &saved_title)
                        .filter(|value| !value.is_empty())
                        .as_deref(),
                    &read_jstring(&mut env, &source_selection_mode)?,
                    read_jstring(&mut env, &regex_pattern)
                        .filter(|value| !value.is_empty())
                        .as_deref(),
                    read_jstring(&mut env, &preferred_binge_group)
                        .filter(|value| !value.is_empty())
                        .as_deref(),
                ))
            })
            .unwrap_or(-1)
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_mergeContinueWatchingDuplicatesJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    items_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &items_json)
            .and_then(|items_json| merge_continue_watching_duplicates_json(&items_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_filterDiscoverResultsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    items_json: JString<'_>,
    year: JString<'_>,
    rating: JFloat,
    has_rating: JBoolean,
    region: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &items_json).and_then(|items_json| {
            filter_discover_results_json(
                &items_json,
                read_jstring(&mut env, &year)
                    .filter(|value| !value.trim().is_empty())
                    .as_deref(),
                if has_rating != 0 { Some(rating) } else { None },
                read_jstring(&mut env, &region)
                    .filter(|value| !value.trim().is_empty())
                    .as_deref(),
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_resolvePreferredAudioLanguageNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    last_audio_language: JString<'_>,
    preferred_audio_language: JString<'_>,
    original_language: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = Some(resolve_preferred_audio_language(
            read_jstring(&mut env, &last_audio_language)
                .filter(|value| !value.is_empty())
                .as_deref(),
            read_jstring(&mut env, &preferred_audio_language)
                .filter(|value| !value.is_empty())
                .as_deref(),
            read_jstring(&mut env, &original_language)
                .filter(|value| !value.is_empty())
                .as_deref(),
        ));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_subtitleLanguageMatchesNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    label: JString<'_>,
    language: JString<'_>,
    preferred_language: JString<'_>,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let result = read_jstring(&mut env, &label)
            .and_then(|label| {
                Some(subtitle_language_matches(
                    &label,
                    read_jstring(&mut env, &language)
                        .filter(|value| !value.is_empty())
                        .as_deref(),
                    &read_jstring(&mut env, &preferred_language)?,
                ))
            })
            .unwrap_or(false);
        if result {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_findPreferredSubtitleIndexNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    tracks_json: JString<'_>,
    last_subtitle_language: JString<'_>,
    preferred_subtitle_language: JString<'_>,
    secondary_subtitle_language: JString<'_>,
) -> JInt {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_jstring(&mut env, &tracks_json)
            .map(|tracks_json| {
                find_preferred_subtitle_index(
                    &tracks_json,
                    read_jstring(&mut env, &last_subtitle_language)
                        .filter(|value| !value.is_empty())
                        .as_deref(),
                    read_jstring(&mut env, &preferred_subtitle_language)
                        .filter(|value| !value.is_empty())
                        .as_deref(),
                    read_jstring(&mut env, &secondary_subtitle_language)
                        .filter(|value| !value.is_empty())
                        .as_deref(),
                )
            })
            .unwrap_or(-1)
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_parseAddonResourceResultJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    resource: JString<'_>,
    url: JString<'_>,
    status_code: JInt,
    body: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &resource).and_then(|resource| {
            Some(parse_addon_resource_result_json(
                &resource,
                &read_jstring(&mut env, &url)?,
                status_code,
                read_jstring(&mut env, &body).as_deref(),
            ))
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_normalizeAddonSubtitlesJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    subtitles_json: JString<'_>,
    resource_url: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &subtitles_json).and_then(|subtitles_json| {
            Some(normalize_addon_subtitles_json(
                &subtitles_json,
                &read_jstring(&mut env, &resource_url)?,
            ))
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_torrentRuntimeInfoJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    request_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &request_json)
            .and_then(|request_json| torrent_runtime_info_json(&request_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_torrentStatusInfoJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    status_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &status_json)
            .and_then(|status_json| torrent_status_info_json(&status_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_stableFeedPartNative,
    |value: String| stable_feed_part(&value)
);

string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_normalizeContentTypeNative,
    |value: String| normalize_content_type(&value).unwrap_or("").to_string()
);

string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_traktBearerNative,
    |value: String| trakt_bearer(&value)
);

string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_traktScrobbleUrlNative,
    |value: String| trakt_scrobble_url(&value)
);

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_traktHasClientNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    api_key: JString<'_>,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let result = read_jstring(&mut env, &api_key)
            .map(|api_key| trakt_has_client(&api_key))
            .unwrap_or(false);
        if result {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_traktPlaybackUrlNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    content_type: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = Some(trakt_playback_url(
            read_jstring(&mut env, &content_type)
                .filter(|value| !value.trim().is_empty())
                .as_deref(),
        ));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_traktTokenExpiresAtNative(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    created_at_seconds: JLong,
    expires_in_seconds: JLong,
) -> JLong {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        trakt_token_expires_at(created_at_seconds, expires_in_seconds)
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_traktContentIdFromIdsNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    ids_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &ids_json)
            .and_then(|ids_json| trakt_content_id_from_ids_json(&ids_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_traktIdsFromContentIdJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    raw_id: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &raw_id)
            .and_then(|raw_id| trakt_ids_from_content_id_json(&raw_id));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_traktEpisodeLocatorJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    video_id: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &video_id)
            .and_then(|video_id| trakt_episode_locator_json(&video_id));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_traktShowIdFromEpisodeIdNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    video_id: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &video_id)
            .map(|video_id| trakt_show_id_from_episode_id(&video_id));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_traktScrobbleMediaIdNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    parent_id: JString<'_>,
    video_id: JString<'_>,
    media_type: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &parent_id).and_then(|parent_id| {
            Some(trakt_scrobble_media_id(
                &parent_id,
                read_jstring(&mut env, &video_id)
                    .filter(|value| !value.is_empty())
                    .as_deref(),
                &read_jstring(&mut env, &media_type)?,
            ))
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_traktOAuthErrorCodeNative,
    |value: String| trakt_oauth_error_code(&value).unwrap_or_default()
);

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_traktHistoryRequestJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    meta_json: JString<'_>,
    episodes_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &meta_json).and_then(|meta_json| {
            trakt_history_request_json(&meta_json, &read_jstring(&mut env, &episodes_json)?)
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_simklScrobbleBodyJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    ids_json: JString<'_>,
    is_episode: JBoolean,
    season: JLong,
    ep_number: JLong,
    time_pos_sec: JDouble,
    duration_sec: JDouble,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &ids_json).and_then(|ids_json| {
            simkl_scrobble_body_json(
                &ids_json,
                is_episode != 0,
                season,
                ep_number,
                time_pos_sec,
                duration_sec,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_simklMatchEpisodeJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    episodes_json: JString<'_>,
    target_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &episodes_json).and_then(|episodes_json| {
            simkl_match_episode_json(&episodes_json, &read_jstring(&mut env, &target_json)?)
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_simklWatchingToItemsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    shows_json: JString<'_>,
    movies_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &shows_json).and_then(|shows_json| {
            simkl_watching_to_items_json(&shows_json, &read_jstring(&mut env, &movies_json)?)
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_simklWatchlistToItemsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    shows_json: JString<'_>,
    movies_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &shows_json).and_then(|shows_json| {
            simkl_watchlist_to_items_json(&shows_json, &read_jstring(&mut env, &movies_json)?)
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_simklWatchedToIdsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    shows_json: JString<'_>,
    movies_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &shows_json).and_then(|shows_json| {
            simkl_watched_to_ids_json(&shows_json, &read_jstring(&mut env, &movies_json)?)
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_playbackProgressItemJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    meta_json: JString<'_>,
    time_offset: JLong,
    duration: JLong,
    now_utc: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &meta_json).and_then(|meta_json| {
            playback_progress_item_json(
                &meta_json,
                time_offset,
                duration,
                &read_jstring(&mut env, &now_utc)?,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_clearPlaybackProgressItemJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    meta_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &meta_json)
            .and_then(|meta_json| clear_playback_progress_item_json(&meta_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_watchedStateItemsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    meta_json: JString<'_>,
    episodes_json: JString<'_>,
    watched: JBoolean,
    watched_at: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &meta_json).and_then(|meta_json| {
            watched_state_items_json(
                &meta_json,
                &read_jstring(&mut env, &episodes_json)?,
                watched != 0,
                read_jstring(&mut env, &watched_at)
                    .filter(|value| !value.is_empty())
                    .as_deref(),
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_libraryContinueWatchingItemsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    items_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &items_json)
            .and_then(|items_json| library_continue_watching_items_json(&items_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_filterHomeContinueWatchingJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    items_json: JString<'_>,
    trakt_watched_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &items_json).and_then(|items_json| {
            filter_home_continue_watching_json(
                &items_json,
                &read_jstring(&mut env, &trakt_watched_json).unwrap_or_default(),
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_watchedVideoIdsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    items_json: JString<'_>,
    imdb_id: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &items_json).and_then(|items_json| {
            watched_video_ids_json(&items_json, &read_jstring(&mut env, &imdb_id)?)
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_parseExtraArgsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    extra: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &extra).and_then(|extra| parse_extra_args_json(&extra));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_providerSearchTermsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    provider: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &provider)
            .and_then(|provider| serde_json::to_string(&provider_search_terms(&provider)).ok());
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_effectiveMetadataFeedSelectionJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    selected_keys_json: JString<'_>,
    available_keys_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &selected_keys_json).and_then(|selected_keys_json| {
            effective_metadata_feed_selection_json(
                &selected_keys_json,
                &read_jstring(&mut env, &available_keys_json)?,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_toggleMetadataFeedJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    selected_keys_json: JString<'_>,
    available_keys_json: JString<'_>,
    key: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &selected_keys_json).and_then(|selected_keys_json| {
            toggle_metadata_feed_json(
                &selected_keys_json,
                &read_jstring(&mut env, &available_keys_json)?,
                &read_jstring(&mut env, &key)?,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_toggleMetadataFeedLimitedJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    selected_keys_json: JString<'_>,
    available_keys_json: JString<'_>,
    key: JString<'_>,
    max_enabled: JInt,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &selected_keys_json).and_then(|selected_keys_json| {
            toggle_metadata_feed_limited_json(
                &selected_keys_json,
                &read_jstring(&mut env, &available_keys_json)?,
                &read_jstring(&mut env, &key)?,
                max_enabled,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_setMetadataFeedGroupEnabledJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    selected_keys_json: JString<'_>,
    available_keys_json: JString<'_>,
    group_keys_json: JString<'_>,
    enabled: JBoolean,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &selected_keys_json).and_then(|selected_keys_json| {
            set_metadata_feed_group_enabled_json(
                &selected_keys_json,
                &read_jstring(&mut env, &available_keys_json)?,
                &read_jstring(&mut env, &group_keys_json)?,
                enabled != 0,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_orderedMetadataFeedKeysJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    option_keys_json: JString<'_>,
    order_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &option_keys_json).and_then(|option_keys_json| {
            ordered_metadata_feed_keys(&option_keys_json, &read_jstring(&mut env, &order_json)?)
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_moveMetadataFeedOrderJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    option_keys_json: JString<'_>,
    current_order_json: JString<'_>,
    key: JString<'_>,
    delta: JInt,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &option_keys_json).and_then(|option_keys_json| {
            move_metadata_feed_order_json(
                &option_keys_json,
                &read_jstring(&mut env, &current_order_json)?,
                &read_jstring(&mut env, &key)?,
                delta,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_contentTraktKeyNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    meta_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output =
            read_jstring(&mut env, &meta_json).and_then(|meta_json| content_trakt_key(&meta_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_contentTraktKeysBatchJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    metas_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &metas_json)
            .and_then(|metas_json| content_trakt_keys_batch(&metas_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_contentWatchedKeysBatchJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    metas_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &metas_json)
            .and_then(|metas_json| content_watched_keys_batch(&metas_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_contentMergeKeysJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    meta_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &meta_json)
            .and_then(|meta_json| content_keys_json(&meta_json, false));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_contentWatchedKeysJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    meta_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &meta_json)
            .and_then(|meta_json| content_keys_json(&meta_json, true));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_episodeFilenameCandidateNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    stream_json: JString<'_>,
    video_id: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &stream_json).and_then(|stream_json| {
            episode_filename_candidate(&stream_json, &read_jstring(&mut env, &video_id)?)
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_streamDiscoveryCacheKeyNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    request_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &request_json)
            .and_then(|request_json| stream_discovery_cache_key(&request_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_discoverCatalogCacheKeyNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    request_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &request_json)
            .and_then(|request_json| discover_catalog_cache_key(&request_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_streamDiscoveryPlanJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    request_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &request_json)
            .and_then(|request_json| stream_discovery_plan_json(&request_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_streamDiscoveryExecutionPolicyJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    request_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &request_json)
            .and_then(|request_json| stream_discovery_execution_policy_json(&request_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_streamDiscoveryCachePrefixNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    content_type: JString<'_>,
    id: JString<'_>,
    language: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &content_type).and_then(|content_type| {
            Some(stream_discovery_cache_prefix(
                &content_type,
                &read_jstring(&mut env, &id)?,
                &read_jstring(&mut env, &language)?,
            ))
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_curateHomeItemsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    category_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &category_json)
            .and_then(|category_json| curate_home_items_json(&category_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_homeOverlapRatioNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    first_json: JString<'_>,
    second_json: JString<'_>,
) -> JFloat {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_jstring(&mut env, &first_json)
            .and_then(|first_json| {
                home_overlap_ratio_json(&first_json, &read_jstring(&mut env, &second_json)?)
            })
            .unwrap_or(0.0)
    }))
    .unwrap_or(0.0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_homePersonalizationScoreNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    category_json: JString<'_>,
    preferred_genres_json: JString<'_>,
    preferred_types_json: JString<'_>,
    priority_labels_json: JString<'_>,
) -> JInt {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_jstring(&mut env, &category_json)
            .and_then(|category_json| {
                home_personalization_score_json(
                    &category_json,
                    &read_jstring(&mut env, &preferred_genres_json)?,
                    &read_jstring(&mut env, &preferred_types_json)?,
                    &read_jstring(&mut env, &priority_labels_json)?,
                )
            })
            .unwrap_or(0)
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_prioritizeHomeRowsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    categories_json: JString<'_>,
    preferred_order_labels_json: JString<'_>,
    preferred_genres_json: JString<'_>,
    preferred_types_json: JString<'_>,
    priority_labels_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &categories_json).and_then(|categories_json| {
            home_prioritize_rows_json(
                &categories_json,
                &read_jstring(&mut env, &preferred_order_labels_json)?,
                &read_jstring(&mut env, &preferred_genres_json)?,
                &read_jstring(&mut env, &preferred_types_json)?,
                &read_jstring(&mut env, &priority_labels_json)?,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_optimizeHomeRowsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    request_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &request_json)
            .and_then(|request_json| optimize_home_rows_json(&request_json));
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_playerProgressPercentNative(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    position_ms: JLong,
    duration_ms: JLong,
) -> f32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        progress_percent(position_ms, duration_ms)
    }))
    .unwrap_or(0.0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_playerShouldSendScrobbleStartNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    token: JString<'_>,
    is_playing: JBoolean,
    has_scrobbled_start: JBoolean,
    progress: f32,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let token = read_jstring(&mut env, &token);
        if should_send_start(
            token.as_deref(),
            is_playing != 0,
            has_scrobbled_start != 0,
            progress,
        ) {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_playerShouldMarkScrobbleStoppedNative(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    has_scrobbled_stop: JBoolean,
    progress: f32,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if should_mark_stopped(has_scrobbled_stop != 0, progress) {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_playerShouldQueueScrobblePauseNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    token: JString<'_>,
    was_play_when_ready: JBoolean,
    has_scrobbled_start: JBoolean,
    has_scrobbled_stop: JBoolean,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let token = read_jstring(&mut env, &token);
        if should_queue_pause(
            token.as_deref(),
            was_play_when_ready != 0,
            has_scrobbled_start != 0,
            has_scrobbled_stop != 0,
        ) {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_playerShouldEnqueueDurableScrobbleNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    action: JString<'_>,
    token: JString<'_>,
    progress: f32,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let action = read_jstring(&mut env, &action).unwrap_or_default();
        let token = read_jstring(&mut env, &token);
        if should_enqueue_durable(&action, token.as_deref(), progress) {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_playerShouldSavePeriodicProgressNative(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    is_playing: JBoolean,
    now_ms: JLong,
    last_saved_at_ms: JLong,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if should_save_periodic_progress(is_playing != 0, now_ms, last_saved_at_ms) {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_playerShouldSaveOnDisposeNative(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    position_ms: JLong,
) -> JBoolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if should_save_on_dispose(position_ms) {
            1
        } else {
            0
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_safePlayerBufferCacheMbNative(
    _env: JNIEnv<'_>,
    _class: JObject<'_>,
    value: JInt,
    has_value: JBoolean,
) -> JInt {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        safe_player_buffer_cache_mb(if has_value != 0 { Some(value) } else { None })
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_safeDolbyVisionFallbackModeNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    mode: JString<'_>,
    legacy_dv7_fallback: JBoolean,
    has_legacy_dv7_fallback: JBoolean,
    legacy_dv7_to_dv8_fallback: JBoolean,
    has_legacy_dv7_to_dv8_fallback: JBoolean,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mode = read_jstring(&mut env, &mode);
        let output = safe_dolby_vision_fallback_mode(
            mode.as_deref().filter(|value| !value.is_empty()),
            if has_legacy_dv7_fallback != 0 {
                Some(legacy_dv7_fallback != 0)
            } else {
                None
            },
            if has_legacy_dv7_to_dv8_fallback != 0 {
                Some(legacy_dv7_to_dv8_fallback != 0)
            } else {
                None
            },
        )
        .to_string();
        write_jstring(&mut env, Some(output))
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_safeStreamSourceSelectionModeNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    mode: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mode = read_jstring(&mut env, &mode);
        let output =
            safe_stream_source_selection_mode(mode.as_deref().filter(|value| !value.is_empty()))
                .to_string();
        write_jstring(&mut env, Some(output))
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_playerFlowDispatchJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    state_json: JString<'_>,
    action_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &state_json).and_then(|state_json| {
            player_flow_dispatch_json(&state_json, &read_jstring(&mut env, &action_json)?)
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_calendarContentPlanJsonNative,
    |value: String| calendar_content_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_calendarSeasonCandidatesJsonNative,
    |value: String| calendar_season_candidates_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_calendarWidgetRowsJsonNative,
    |value: String| calendar_widget_rows_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_calendarNotificationContentJsonNative,
    |value: String| calendar_notification_content_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_calendarReleaseDetectionJsonNative,
    |value: String| calendar_release_detection_json(&value).unwrap_or_default()
);

string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_activeProfilePlanJsonNative,
    |value: String| active_profile_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_tokenMergePlanJsonNative,
    |value: String| token_merge_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_profileDefaultSeedJsonNative,
    |value: String| profile_default_seed_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_profileSettingsMigrationPlanJsonNative,
    |value: String| profile_settings_migration_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_profileAvatarDefaultJsonNative,
    |value: String| profile_avatar_default_json(&value).unwrap_or_default()
);

string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_watchlistTogglePlanJsonNative,
    |value: String| watchlist_toggle_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_libraryExternalMergePlanJsonNative,
    |value: String| library_external_merge_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_libraryCollectionImportValidationJsonNative,
    |value: String| library_collection_import_validation_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_libraryOfflineGroupingJsonNative,
    |value: String| library_offline_grouping_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_playbackProgressMergePlanJsonNative,
    |value: String| playback_progress_merge_plan_json(&value).unwrap_or_default()
);

string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_playerBackendSelectionJsonNative,
    |value: String| player_backend_selection_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_dvProxyPlanJsonNative,
    |value: String| dv_proxy_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_torrentFallbackFilePolicyJsonNative,
    |value: String| torrent_fallback_file_policy_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_playerBufferTargetsJsonNative,
    |value: String| player_buffer_targets_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_playerRetryPolicyJsonNative,
    |value: String| player_retry_policy_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_playerSourceSidebarPlanJsonNative,
    |value: String| player_source_sidebar_plan_json(&value).unwrap_or_default()
);

string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_searchResultGroupingJsonNative,
    |value: String| search_result_grouping_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_discoverSortPlanJsonNative,
    |value: String| discover_sort_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_librarySortPlanJsonNative,
    |value: String| library_sort_plan_json(&value).unwrap_or_default()
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_detailSeriesLookupIdNative,
    |value: String| detail_series_lookup_id(&value)
);
string_fn!(
    Java_com_fluxa_app_core_rust_FluxaCoreNative_detailSeasonLoadPlanJsonNative,
    |value: String| detail_season_load_plan_json(&value).unwrap_or_default()
);

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_buildBillboardPoolJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    enriched_json: JString<'_>,
    candidates_json: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &enriched_json).and_then(|enriched_json| {
            build_billboard_pool_json(&enriched_json, &read_jstring(&mut env, &candidates_json)?)
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_fluxa_app_core_rust_FluxaCoreNative_normalizeHomeCatalogItemsJsonNative(
    mut env: JNIEnv<'_>,
    _class: JObject<'_>,
    items_json: JString<'_>,
    catalog_id: JString<'_>,
    genre: JString<'_>,
    today_iso: JString<'_>,
) -> JStringReturn {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let output = read_jstring(&mut env, &items_json).and_then(|items_json| {
            normalize_home_catalog_items_json(
                &items_json,
                &read_jstring(&mut env, &catalog_id)?,
                read_jstring(&mut env, &genre)
                    .filter(|v| !v.is_empty())
                    .as_deref(),
                &read_jstring(&mut env, &today_iso)?,
            )
        });
        write_jstring(&mut env, output)
    }))
    .unwrap_or(ptr::null_mut())
}
