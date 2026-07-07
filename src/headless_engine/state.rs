use super::addons::AddonsState;
use super::auth::AuthState;
use super::calendar::CalendarState;
use super::detail::{DetailState, LookupState};
use super::discover::DiscoverState;
use super::home::HomeState;
use super::library::LibraryState;
use super::navigation::NavigationState;
use super::offline::OfflineState;
use super::player::PlayerState;
use super::profile::ProfileState;
use super::search::SearchState;
use super::settings::SettingsState;
use super::sync::SyncState;
use crate::runtime::EffectEnvelope;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ops::{Deref, DerefMut};

// Wraps a domain's state so any mutable access (a direct field write, a whole-value
// replacement via DerefMut, a method call taking &mut) flips `dirty` automatically —
// StatePatch::from_dirty then clones only domains actually touched by a dispatch
// instead of cloning the whole EngineState before and after every action to diff it
// via PartialEq. Serializes/deserializes exactly like the wrapped value: the wire
// format is unaffected.
#[derive(Clone, Debug)]
pub(super) struct Tracked<T> {
    value: T,
    dirty: bool,
}

impl<T: Default> Default for Tracked<T> {
    fn default() -> Self {
        Tracked {
            value: T::default(),
            dirty: false,
        }
    }
}

impl<T> Tracked<T> {
    pub(super) fn take_if_dirty(&mut self) -> Option<T>
    where
        T: Clone,
    {
        if self.dirty {
            self.dirty = false;
            Some(self.value.clone())
        } else {
            None
        }
    }
}

impl<T> Deref for Tracked<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for Tracked<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.dirty = true;
        &mut self.value
    }
}

impl<T: Serialize> Serialize for Tracked<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Tracked<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Tracked {
            value: T::deserialize(deserializer)?,
            dirty: false,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GenerationKey {
    Detail,
    Player,
    Home,
    Library,
    Addon,
    Search,
    Discover,
    Sync,
    Auth,
    Settings,
    Calendar,
    Offline,
    DetailStreams,
    Lookup,
    PlaybackPrep,
    Intro,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct RuntimeGenerations {
    detail_generation: u64,
    player_generation: u64,
    home_generation: u64,
    library_generation: u64,
    addon_generation: u64,
    search_generation: u64,
    discover_generation: u64,
    sync_generation: u64,
    auth_generation: u64,
    settings_generation: u64,
    calendar_generation: u64,
    offline_generation: u64,
    detail_streams_generation: u64,
    lookup_generation: u64,
    playback_prep_generation: u64,
    intro_generation: u64,
}

impl RuntimeGenerations {
    pub(super) fn get(&self, key: GenerationKey) -> u64 {
        match key {
            GenerationKey::Detail => self.detail_generation,
            GenerationKey::Player => self.player_generation,
            GenerationKey::Home => self.home_generation,
            GenerationKey::Library => self.library_generation,
            GenerationKey::Addon => self.addon_generation,
            GenerationKey::Search => self.search_generation,
            GenerationKey::Discover => self.discover_generation,
            GenerationKey::Sync => self.sync_generation,
            GenerationKey::Auth => self.auth_generation,
            GenerationKey::Settings => self.settings_generation,
            GenerationKey::Calendar => self.calendar_generation,
            GenerationKey::Offline => self.offline_generation,
            GenerationKey::DetailStreams => self.detail_streams_generation,
            GenerationKey::Lookup => self.lookup_generation,
            GenerationKey::PlaybackPrep => self.playback_prep_generation,
            GenerationKey::Intro => self.intro_generation,
        }
    }

    pub(super) fn bump(&mut self, key: GenerationKey) -> u64 {
        let slot = match key {
            GenerationKey::Detail => &mut self.detail_generation,
            GenerationKey::Player => &mut self.player_generation,
            GenerationKey::Home => &mut self.home_generation,
            GenerationKey::Library => &mut self.library_generation,
            GenerationKey::Addon => &mut self.addon_generation,
            GenerationKey::Search => &mut self.search_generation,
            GenerationKey::Discover => &mut self.discover_generation,
            GenerationKey::Sync => &mut self.sync_generation,
            GenerationKey::Auth => &mut self.auth_generation,
            GenerationKey::Settings => &mut self.settings_generation,
            GenerationKey::Calendar => &mut self.calendar_generation,
            GenerationKey::Offline => &mut self.offline_generation,
            GenerationKey::DetailStreams => &mut self.detail_streams_generation,
            GenerationKey::Lookup => &mut self.lookup_generation,
            GenerationKey::PlaybackPrep => &mut self.playback_prep_generation,
            GenerationKey::Intro => &mut self.intro_generation,
        };
        *slot = slot.saturating_add(1);
        *slot
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct EngineState {
    pub(super) navigation: Tracked<NavigationState>,
    pub(super) home: Tracked<HomeState>,
    pub(super) search: Tracked<SearchState>,
    pub(super) discover: Tracked<DiscoverState>,
    pub(super) detail: Tracked<DetailState>,
    pub(super) player: Tracked<PlayerState>,
    pub(super) library: Tracked<LibraryState>,
    pub(super) profile: Tracked<ProfileState>,
    pub(super) settings: Tracked<SettingsState>,
    pub(super) calendar: Tracked<CalendarState>,
    pub(super) addons: Tracked<AddonsState>,
    pub(super) auth: Tracked<AuthState>,
    pub(super) sync: Tracked<SyncState>,
    pub(super) lookup: Tracked<LookupState>,
    pub(super) offline: Tracked<OfflineState>,
    pub(super) pending_effects: Tracked<Vec<EffectEnvelope>>,
    #[serde(rename = "_runtime")]
    pub(super) runtime: RuntimeGenerations,
}

impl EngineState {
    pub(super) fn diff_dirty(&mut self) -> super::contracts::StatePatch {
        super::contracts::StatePatch {
            navigation: self.navigation.take_if_dirty(),
            home: self.home.take_if_dirty(),
            search: self.search.take_if_dirty(),
            discover: self.discover.take_if_dirty(),
            detail: self.detail.take_if_dirty(),
            player: self.player.take_if_dirty(),
            library: self.library.take_if_dirty(),
            profile: self.profile.take_if_dirty(),
            settings: self.settings.take_if_dirty(),
            calendar: self.calendar.take_if_dirty(),
            addons: self.addons.take_if_dirty(),
            auth: self.auth.take_if_dirty(),
            sync: self.sync.take_if_dirty(),
            lookup: self.lookup.take_if_dirty(),
            offline: self.offline.take_if_dirty(),
            pending_effects: self.pending_effects.take_if_dirty(),
        }
    }
}
