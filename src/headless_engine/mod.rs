mod addons;
mod auth;
mod calendar;
mod complete_effect;
mod contracts;
mod detail;
mod discover;
mod dispatch;
mod effect_bookkeeping;
mod helpers;
mod home;
mod library;
mod navigation;
mod offline;
mod player;
mod plugins;
mod profile;
mod search;
mod settings;
mod state;
mod sync;
mod trailer;
#[cfg(feature = "plugin-js-engine")]
mod youtube_cipher;

use crate::core_error::{CoreError, LogAndDiscard};
use crate::runtime::EffectEnvelope;
use contracts::{AppAction, DispatchResult, StatePatch};
use serde::{Deserialize, Serialize};
use state::EngineState;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use web_time::Instant;

pub(crate) use contracts::EffectResultInput;

// If the platform never calls complete_effect for an effect (a transient IPC failure on
// the completion call, a swallowed exception, etc.), it would otherwise sit in
// pending_effects/delivered_effect_ids forever for the life of the engine instance.
// Anything genuinely still in flight completes well within this window.
const EFFECT_EXPIRY: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct HeadlessEngine {
    #[serde(default)]
    state: EngineState,
    #[serde(default = "first_effect_id")]
    next_effect_id: u64,
    // Ids handed to the platform at least once, awaiting their complete_effect call.
    // Never serialized — purely tracks delivery so the "drain the queue" fallback in
    // resolve_visible_effects doesn't hand out an effect that's already in flight as
    // if it were fresh work (which used to make an unrelated dispatch while a slow
    // effect was still running re-trigger a full duplicate execution of it).
    #[serde(skip)]
    delivered_effect_ids: HashSet<String>,
    // When each pending effect was created, for expire_stale_pending_effects. Never
    // serialized — Instant isn't a portable wall-clock value, just an internal timer.
    #[serde(skip)]
    effect_created_at: HashMap<String, Instant>,
}

fn first_effect_id() -> u64 {
    1
}

static ENGINE_COUNTER: AtomicU64 = AtomicU64::new(1);
static ENGINES: OnceLock<Mutex<HashMap<u64, HeadlessEngine>>> = OnceLock::new();

pub fn create_headless_engine(initial_json: &str) -> u64 {
    let mut engine = HeadlessEngine {
        next_effect_id: 1,
        ..HeadlessEngine::default()
    };
    if let Ok(initial_state) = serde_json::from_str::<EngineState>(initial_json) {
        engine.state = initial_state;
    }
    let mut map = lock_engines();
    let handle = ENGINE_COUNTER.fetch_add(1, Ordering::Relaxed);
    map.insert(handle, engine);
    handle
}

pub fn destroy_headless_engine(handle: u64) -> bool {
    lock_engines().remove(&handle).is_some()
}

pub fn headless_engine_snapshot_json(handle: u64) -> Option<String> {
    let state = {
        let map = lock_engines();
        map.get(&handle)?.state.clone()
    };
    serde_json::to_string(&state).ok()
}

pub fn headless_engine_dispatch_json(handle: u64, action_json: &str) -> Option<String> {
    let action: AppAction = serde_json::from_str(action_json)
        .map_err(|e| CoreError::BadInput {
            context: "headless_engine_dispatch_json",
            detail: e.to_string(),
        })
        .log_discard()?;
    let (patch, visible_effects) = {
        let mut map = lock_engines();
        let engine = match map.get_mut(&handle) {
            Some(engine) => engine,
            None => {
                return CoreError::NotFound {
                    context: "headless_engine_dispatch_json",
                }
                .log_and_none();
            }
        };
        engine.expire_stale_pending_effects(Instant::now());
        let effects = engine.dispatch(action);
        let visible_effects = engine.resolve_visible_effects(effects);
        (engine.state.diff_dirty(), visible_effects)
    };
    result_patch_json(patch, visible_effects)
}

pub fn headless_engine_complete_effect_json(handle: u64, result_json: &str) -> Option<String> {
    let result: EffectResultInput = serde_json::from_str(result_json)
        .map_err(|e| CoreError::BadInput {
            context: "headless_engine_complete_effect_json",
            detail: e.to_string(),
        })
        .log_discard()?;
    let (patch, visible_effects) = {
        let mut map = lock_engines();
        let engine = match map.get_mut(&handle) {
            Some(engine) => engine,
            None => {
                return CoreError::NotFound {
                    context: "headless_engine_complete_effect_json",
                }
                .log_and_none();
            }
        };
        engine.expire_stale_pending_effects(Instant::now());
        let effects = engine.complete_effect(result);
        let visible_effects = engine.resolve_visible_effects(effects);
        (engine.state.diff_dirty(), visible_effects)
    };
    result_patch_json(patch, visible_effects)
}

// Deliberately takes owned before/after snapshots rather than a reference to the locked
// engine: diffing and serializing a large state (e.g. a big discover catalog) can take
// over a second, and every other Tauri command shares one global engine mutex — holding
// it for that long would stall unrelated IPC calls behind it. Callers clone what they
// need and drop the lock before calling this.
fn result_patch_json(state: StatePatch, effects: Vec<EffectEnvelope>) -> Option<String> {
    serde_json::to_string(&DispatchResult { state, effects }).ok()
}

fn engines() -> &'static Mutex<HashMap<u64, HeadlessEngine>> {
    ENGINES.get_or_init(|| Mutex::new(HashMap::new()))
}

// A panic while a request held this lock poisons it; with catch_unwind now
// guarding the FFI boundary, a single caught panic must not silently make
// every engine handle inaccessible for the rest of the process's life.
// Recovering the guard accepts that one engine's state might be left
// mid-update, which is still far better than every other handle going dark.
fn lock_engines() -> std::sync::MutexGuard<'static, HashMap<u64, HeadlessEngine>> {
    engines()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests;
