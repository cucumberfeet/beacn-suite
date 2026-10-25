//! The native Mixer page: Pipeweaver's mixer, routing, applications, devices and settings,
//! drawn by beacn-utility itself so everything lives in one window.
//!
//! See docs/native-mixer-plan.md for the feature inventory and the daemon API it uses.

mod apps;
mod catalog;
mod client;
mod devices;
mod filters;
mod model;
mod routing;
mod settings;
mod strip;
mod style;
mod view;

use crate::ui::pages::app::mixer::client::{Event, Outgoing};
use crate::ui::pages::app::mixer::model::{Colour, Mix, Model, MuteTarget, NodeKind, OrderGroup};
use iced::futures::channel::mpsc;
use iced::{Subscription, Task};
use log::{debug, warn};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use web_time::{Duration, Instant};

/// How long a fader keeps showing the local value after it's let go, so the daemon's echo can
/// arrive before we switch back to the daemon's state (avoids the handle snapping back).
/// The name field in the rename and create dialogs, focused when they open
pub(super) const MODAL_INPUT: &str = "mixer-modal-input";

const RELEASE_GRACE: Duration = Duration::from_millis(400);
const ERROR_TIMEOUT: Duration = Duration::from_secs(8);

/// The fallback switch: use the old Beacn Mixer window instead of this page.
/// `BEACN_LEGACY_MIXER=1`, or `~/.config/beacn-utility/legacy-mixer` exists.
pub fn legacy_mixer() -> bool {
    if let Ok(v) = std::env::var("BEACN_LEGACY_MIXER") {
        return !matches!(v.trim(), "" | "0" | "false" | "no");
    }
    directories::ProjectDirs::from("", "", "beacn-utility")
        .is_some_and(|dirs| dirs.config_dir().join("legacy-mixer").exists())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Mixer,
    Routing,
    Applications,
    Devices,
    Settings,
}

impl Tab {
    const ALL: [(Tab, &'static str); 5] = [
        (Tab::Mixer, "mixer"),
        (Tab::Routing, "routing"),
        (Tab::Applications, "applications"),
        (Tab::Devices, "devices"),
        (Tab::Settings, "settings"),
    ];

    fn path() -> Option<std::path::PathBuf> {
        directories::ProjectDirs::from("", "", "beacn-utility")
            .map(|dirs| dirs.config_dir().join("mixer-tab"))
    }

    /// The tab that was open last time (`~/.config/beacn-utility/mixer-tab`)
    fn load() -> Self {
        let saved = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        Self::ALL
            .iter()
            .find(|(_, name)| *name == saved.trim())
            .map(|(tab, _)| *tab)
            .unwrap_or_default()
    }

    fn save(self) {
        let Some(path) = Self::path() else { return };
        if let Some((_, name)) = Self::ALL.iter().find(|(tab, _)| *tab == self)
            && let Err(e) = std::fs::write(&path, name)
        {
            debug!("Mixer: couldn't save the open tab: {e}");
        }
    }
}

/// Small menus anchored to the button that opened them
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Popup {
    Channel(String),
    Mute(String, MuteTarget),
    Add(bool),
    Hidden(bool),
}

/// Centered dialogs
#[derive(Debug, Clone)]
pub enum Modal {
    Rename { id: String, value: String },
    Create { kind: NodeKind, value: String },
    Remove(String),
    Colour { id: String, colour: Colour },
    Filters { id: String },
    ResetAudio,
}

#[derive(Debug, Clone)]
pub enum MixerMessage {
    Client(Event),
    Meters(Vec<(String, u8)>),
    Tick,

    Tab(Tab),
    OpenPopup(Popup),
    ClosePopup,
    OpenModal(Modal),
    CloseModal,
    /// Escape: closes the open menu, or else the open dialog
    Escape,
    DismissError,

    // Channel strips
    Volume(String, Option<Mix>, u8),
    VolumeReleased(String, Option<Mix>),
    ToggleLink(String),
    SetTargetMix(String, Mix),
    ToggleMute(String, MuteTarget),
    MuteToAll(String, MuteTarget),
    ToggleMuteTargetNode(String, MuteTarget, String),

    // Channel menu
    Attach(String, u32),
    Detach(String, usize),
    SetOrderGroup(String, OrderGroup),
    Move(String, i32),
    ModalInput(String),
    ModalSubmit,
    Remove(String),
    ColourChanged(Colour),

    // Routing
    ToggleRoute(String, String),

    // Applications
    SetDefault(bool, String),
    SetAppRoute {
        playback: bool,
        process: String,
        name: String,
        target: Option<String>,
    },
    SetStreamRoute(u32, Option<String>),
    AppVolume(u32, u8),
    AppVolumeReleased(u32),
    AppMute(u32, bool),
    ToggleAppExpanded(String),
    AppSearch(String),
    RescanApps,
    CatalogLoaded(Arc<apps::LoadedCatalog>),
    StreamsProbed(Vec<(u32, catalog::StreamIds)>),
    /// Pick a channel for a catalogued app (`None`: Default, i.e. the default output)
    SetCatalogRoute {
        key: String,
        target: Option<String>,
    },

    // Devices
    PhysicalVolume(String, u8),
    PhysicalVolumeReleased(String),
    PhysicalMute(String, bool),

    // Settings
    SetQuantum(Option<u32>),
    ResetAudio,
    OpenWebMixer,

    // Filters
    SelectFilter(String),
    AddFilter(String, bool, String),
    RemoveFilter(String),
    MoveFilter(String, usize),
    FilterValue(String, u32, Value),
    FilterValueReleased(String, u32),
    FilterSearch(String),
}

pub struct MixerPage {
    raw: Option<Value>,
    model: Model,
    connected: bool,
    sender: Option<mpsc::Sender<Outgoing>>,
    next_id: u64,

    // Keyed commands that are coalesced while one is in flight (faders, colours)
    inflight: HashMap<String, u64>,
    inflight_by_id: HashMap<u64, String>,
    queued: HashMap<String, Value>,

    // Values shown instead of the daemon's while the user is dragging
    overrides: HashMap<String, (f32, Option<Instant>)>,

    meters: HashMap<String, u8>,

    tab: Tab,
    popup: Option<Popup>,
    modal: Option<Modal>,
    error: Option<(String, Instant)>,
    expanded_apps: HashSet<String>,
    selected_filter: Option<String>,
    filter_search: String,

    // Applications tab: installed apps / Steam games and the channel picked for each
    catalog: Option<Arc<apps::LoadedCatalog>>,
    scanning: bool,
    routing: catalog::AppRouting,
    app_search: String,
    /// Extra ids (Flatpak id, SteamAppId) read for running streams, by node id
    stream_ids: HashMap<u32, catalog::StreamIds>,
    probing: HashSet<u32>,
    /// (process, name) -> channel we last asked Pipeweaver to route it to
    enforced: HashMap<(String, String), String>,
}

impl MixerPage {
    pub fn new() -> Self {
        Self {
            raw: None,
            model: Model::default(),
            connected: false,
            sender: None,
            next_id: 1,
            inflight: HashMap::new(),
            inflight_by_id: HashMap::new(),
            queued: HashMap::new(),
            overrides: HashMap::new(),
            meters: HashMap::new(),
            tab: Tab::load(),
            popup: None,
            modal: None,
            error: None,
            expanded_apps: HashSet::new(),
            selected_filter: None,
            filter_search: String::new(),
            catalog: None,
            scanning: false,
            routing: catalog::AppRouting::load(),
            app_search: String::new(),
            stream_ids: HashMap::new(),
            probing: HashSet::new(),
            enforced: HashMap::new(),
        }
    }

    /// `visible`: the window is open and this page is showing (meters only run then)
    pub fn subscription(&self, visible: bool) -> Subscription<MixerMessage> {
        let mut subs = vec![Subscription::run(client::connection).map(MixerMessage::Client)];
        if visible && self.connected {
            subs.push(Subscription::run(client::meters).map(MixerMessage::Meters));
        }
        let fading = self.overrides.values().any(|(_, r)| r.is_some());
        if visible && (self.error.is_some() || fading) {
            subs.push(iced::time::every(Duration::from_millis(100)).map(|_| MixerMessage::Tick));
        }
        if visible && (self.popup.is_some() || self.modal.is_some()) {
            subs.push(iced::keyboard::listen().filter_map(|event| match event {
                iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                } => Some(MixerMessage::Escape),
                _ => None,
            }));
        }
        Subscription::batch(subs)
    }

    /// Called when the page goes off screen
    pub fn on_hide(&mut self) {
        self.popup = None;
        self.meters.clear();
    }

    ////////////////////////////////////////////////////////////////////////////////////////
    // Sending

    fn send(&mut self, data: Value) -> Option<u64> {
        let Some(sender) = self.sender.as_mut() else {
            self.show_error("The audio engine isn't connected yet.".into());
            return None;
        };
        let id = self.next_id;
        self.next_id += 1;
        if let Err(e) = sender.try_send(Outgoing { id, data }) {
            warn!("Mixer: failed to queue command: {e}");
            self.show_error("Couldn't reach the audio engine.".into());
            return None;
        }
        Some(id)
    }

    fn pw(&mut self, command: Value) {
        self.send(json!({ "Pipewire": command }));
    }

    fn daemon(&mut self, command: Value) {
        self.send(json!({ "Daemon": command }));
    }

    /// Sends now, or (if the same key is still waiting for a reply) replaces what's queued
    fn pw_keyed(&mut self, key: String, command: Value) {
        let data = json!({ "Pipewire": command });
        if self.inflight.contains_key(&key) {
            self.queued.insert(key, data);
            return;
        }
        if let Some(id) = self.send(data) {
            self.inflight.insert(key.clone(), id);
            self.inflight_by_id.insert(id, key);
        }
    }

    fn on_response(&mut self, id: u64, data: Value) {
        let error = data
            .get("Err")
            .or_else(|| data.get("Pipewire").and_then(|p| p.get("Err")))
            .and_then(|e| e.as_str().map(|s| s.to_string()));
        if let Some(error) = error {
            self.show_error(error);
        }

        if let Some(key) = self.inflight_by_id.remove(&id) {
            self.inflight.remove(&key);
            if let Some(next) = self.queued.remove(&key)
                && let Some(new_id) = self.send(next)
            {
                self.inflight.insert(key.clone(), new_id);
                self.inflight_by_id.insert(new_id, key);
            }
        }
    }

    fn set_override(&mut self, key: &str, value: f32) {
        self.overrides.insert(key.to_string(), (value, None));
    }

    fn release_override(&mut self, key: &str) {
        if let Some((_, released)) = self.overrides.get_mut(key) {
            *released = Some(Instant::now());
        }
    }

    fn prune_overrides(&mut self) {
        let now = Instant::now();
        let busy: HashSet<&String> = self.inflight.keys().chain(self.queued.keys()).collect();
        let mut done = vec![];
        for (key, (_, released)) in &self.overrides {
            if let Some(at) = released
                && now.duration_since(*at) > RELEASE_GRACE
                && !busy.contains(key)
            {
                done.push(key.clone());
            }
        }
        for key in done {
            self.overrides.remove(&key);
        }
    }

    pub(crate) fn value(&self, key: &str, actual: f32) -> f32 {
        self.overrides.get(key).map(|(v, _)| *v).unwrap_or(actual)
    }

    fn show_error(&mut self, message: String) {
        debug!("Mixer error: {message}");
        self.error = Some((message, Instant::now()));
    }

    fn reset_connection_state(&mut self) {
        self.connected = false;
        self.sender = None;
        self.inflight.clear();
        self.inflight_by_id.clear();
        self.queued.clear();
        self.overrides.clear();
        self.meters.clear();
    }

    fn rebuild(&mut self) {
        let Some(raw) = &self.raw else { return };
        match Model::from_value(raw) {
            Ok(model) => self.model = model,
            Err(e) => warn!("Mixer: couldn't read the daemon status: {e}"),
        }

        // Drop UI state that points at things which no longer exist
        let exists = |m: &Model, id: &str| m.channel(id).is_some();
        if let Some(Popup::Channel(id) | Popup::Mute(id, _)) = &self.popup
            && !exists(&self.model, id)
        {
            self.popup = None;
        }
        if let Some(
            Modal::Rename { id, .. }
            | Modal::Remove(id)
            | Modal::Colour { id, .. }
            | Modal::Filters { id },
        ) = &self.modal
            && !exists(&self.model, id)
        {
            self.modal = None;
        }
    }

    ////////////////////////////////////////////////////////////////////////////////////////

    pub fn update(&mut self, message: MixerMessage) -> Task<MixerMessage> {
        // Anything that can change which apps are running, or where they should go
        let check_routes = matches!(
            message,
            MixerMessage::Client(_)
                | MixerMessage::CatalogLoaded(_)
                | MixerMessage::StreamsProbed(_)
                | MixerMessage::SetCatalogRoute { .. }
        );
        let task = self.update_inner(message);
        if !check_routes {
            return task;
        }
        let mut tasks = vec![task, self.enforce_app_routes()];
        if self.catalog.is_none() && !self.scanning && self.connected {
            tasks.push(self.scan_apps());
        }
        Task::batch(tasks)
    }

    fn update_inner(&mut self, message: MixerMessage) -> Task<MixerMessage> {
        match message {
            MixerMessage::Client(event) => match event {
                Event::Ready(sender) => {
                    self.reset_connection_state();
                    self.sender = Some(sender);
                }
                Event::Status(status) => {
                    self.connected = true;
                    self.raw = Some(status);
                    self.rebuild();
                }
                Event::Patch(patch) => {
                    if let Some(raw) = self.raw.as_mut() {
                        match serde_json::from_value::<json_patch::Patch>(patch) {
                            Ok(patch) => {
                                if let Err(e) = json_patch::patch(raw, &patch) {
                                    warn!("Mixer: patch failed ({e}), refetching status");
                                    self.send(json!("GetStatus"));
                                }
                            }
                            Err(e) => warn!("Mixer: unreadable patch: {e}"),
                        }
                        self.rebuild();
                        self.prune_overrides();
                    }
                }
                Event::Response(id, data) => {
                    // A fresh status (after a failed patch) arrives as a plain response
                    if let Some(status) = data.get("Status") {
                        self.raw = Some(status.clone());
                        self.rebuild();
                    }
                    self.on_response(id, data);
                }
                Event::Disconnected => {
                    self.reset_connection_state();
                    self.popup = None;
                    self.modal = None;
                }
            },
            MixerMessage::Meters(batch) => {
                for (id, level) in batch {
                    self.meters.insert(id, level);
                }
                self.prune_overrides();
            }
            MixerMessage::Tick => {
                self.prune_overrides();
                if let Some((_, at)) = &self.error
                    && at.elapsed() > ERROR_TIMEOUT
                {
                    self.error = None;
                }
            }

            MixerMessage::Tab(tab) => {
                self.tab = tab;
                self.popup = None;
                tab.save();
            }
            MixerMessage::OpenPopup(popup) => {
                self.popup = if self.popup.as_ref() == Some(&popup) {
                    None
                } else {
                    Some(popup)
                };
            }
            MixerMessage::ClosePopup => self.popup = None,
            MixerMessage::OpenModal(modal) => {
                self.popup = None;
                if let Modal::Filters { id } = &modal {
                    self.selected_filter = self
                        .model
                        .channel(id)
                        .and_then(|c| c.filters.first())
                        .map(|f| f.id.clone());
                    self.filter_search.clear();
                }
                let typing = matches!(modal, Modal::Rename { .. } | Modal::Create { .. });
                self.modal = Some(modal);
                if typing {
                    return iced::widget::operation::focus(MODAL_INPUT);
                }
            }
            MixerMessage::CloseModal => self.modal = None,
            MixerMessage::Escape => {
                // Close the innermost thing first: a menu, then a dialog
                if self.popup.is_some() {
                    self.popup = None;
                } else {
                    self.modal = None;
                }
            }
            MixerMessage::DismissError => self.error = None,

            MixerMessage::Volume(id, mix, value) => {
                let key = volume_key(&id, mix);
                self.set_override(&key, value as f32);
                let is_source = self.model.channel(&id).is_some_and(|c| c.is_source());
                let command = match (is_source, mix) {
                    (true, Some(mix)) => json!({ "SetSourceVolume": [id, mix.as_str(), value] }),
                    _ => json!({ "SetTargetVolume": [id, value] }),
                };
                // A linked partner fader moves too, so keep its override out of the way
                if is_source && self.model.channel(&id).is_some_and(|c| c.linked) {
                    let other = volume_key(&id, Some(other_mix(mix.unwrap_or(Mix::A))));
                    self.overrides.remove(&other);
                }
                self.pw_keyed(key, command);
            }
            MixerMessage::VolumeReleased(id, mix) => self.release_override(&volume_key(&id, mix)),
            MixerMessage::ToggleLink(id) => {
                if let Some(c) = self.model.channel(&id) {
                    let linked = !c.linked;
                    self.pw(json!({ "SetSourceVolumeLinked": [id, linked] }));
                }
            }
            MixerMessage::SetTargetMix(id, mix) => {
                self.pw(json!({ "SetTargetMix": [id, mix.as_str()] }));
            }
            MixerMessage::ToggleMute(id, target) => {
                let Some(c) = self.model.channel(&id) else {
                    return Task::none();
                };
                if c.is_source() {
                    let command = if c.is_muted(target) {
                        "DelSourceMuteTarget"
                    } else {
                        "AddSourceMuteTarget"
                    };
                    self.pw(json!({ command: [id, target.as_str()] }));
                } else {
                    let state = if c.mute_a { "Unmuted" } else { "Muted" };
                    self.pw(json!({ "SetTargetMuteState": [id, state] }));
                }
            }
            MixerMessage::MuteToAll(id, target) => {
                self.pw(json!({ "ClearMuteTargetNodes": [id, target.as_str()] }));
            }
            MixerMessage::ToggleMuteTargetNode(id, target, node) => {
                if let Some(c) = self.model.channel(&id) {
                    let command = if c.mute_targets(target).contains(&node) {
                        "DelMuteTargetNode"
                    } else {
                        "AddMuteTargetNode"
                    };
                    self.pw(json!({ command: [id, target.as_str(), node] }));
                }
            }

            MixerMessage::Attach(id, node) => {
                self.pw(json!({ "AttachPhysicalNode": [id, node] }));
            }
            MixerMessage::Detach(id, index) => {
                self.pw(json!({ "RemovePhysicalNode": [id, index] }));
            }
            MixerMessage::SetOrderGroup(id, group) => {
                self.popup = None;
                self.pw(json!({ "SetOrderGroup": [id, group.as_str()] }));
            }
            MixerMessage::Move(id, delta) => {
                if let Some(c) = self.model.channel(&id) {
                    let list = if c.is_source() {
                        &self.model.sources
                    } else {
                        &self.model.targets
                    };
                    let group: Vec<&String> = list
                        .iter()
                        .filter(|o| o.group == c.group)
                        .map(|o| &o.id)
                        .collect();
                    if let Some(pos) = group.iter().position(|o| **o == id) {
                        let new = pos as i32 + delta;
                        if new >= 0 && (new as usize) < group.len() {
                            self.pw(json!({ "SetOrder": [id, new as u8] }));
                        }
                    }
                }
            }
            MixerMessage::ModalInput(value) => {
                if let Some(Modal::Rename { value: v, .. } | Modal::Create { value: v, .. }) =
                    &mut self.modal
                {
                    *v = value.chars().take(20).collect();
                }
            }
            MixerMessage::ModalSubmit => match self.modal.clone() {
                Some(Modal::Rename { id, value }) => {
                    let current = self.model.channel_name(&id).map(|s| s.to_string());
                    if self.model.name_error(&value, current.as_deref()).is_none() {
                        self.modal = None;
                        if current.as_deref() != Some(value.as_str()) {
                            self.pw(json!({ "RenameNode": [id, value] }));
                        }
                    }
                }
                Some(Modal::Create { kind, value }) => {
                    if self.model.name_error(&value, None).is_none() {
                        self.modal = None;
                        self.pw(json!({ "CreateNode": [kind.as_str(), value] }));
                    }
                }
                Some(Modal::Remove(id)) => {
                    self.modal = None;
                    self.pw(json!({ "RemoveNode": id }));
                }
                Some(Modal::ResetAudio) => {
                    self.modal = None;
                    self.daemon(json!("ResetAudio"));
                }
                _ => self.modal = None,
            },
            MixerMessage::Remove(id) => {
                self.modal = Some(Modal::Remove(id));
            }
            MixerMessage::ColourChanged(colour) => {
                if let Some(Modal::Colour { id, colour: c }) = &mut self.modal {
                    *c = colour;
                    let id = id.clone();
                    let rgb =
                        json!({ "red": colour.red, "green": colour.green, "blue": colour.blue });
                    self.pw_keyed(
                        format!("colour:{id}"),
                        json!({ "SetNodeColour": [id, rgb] }),
                    );
                }
            }

            MixerMessage::ToggleRoute(source, target) => {
                let enabled = !self.model.is_routed(&source, &target);
                self.pw(json!({ "SetRoute": [source, target, enabled] }));
            }

            MixerMessage::SetDefault(source, id) => {
                self.popup = None;
                let command = if source {
                    "SetDefaultInput"
                } else {
                    "SetDefaultOutput"
                };
                self.pw(json!({ command: id }));
            }
            MixerMessage::SetAppRoute {
                playback,
                process,
                name,
                target,
            } => {
                let definition = json!({
                    "device_type": if playback { "Source" } else { "Target" },
                    "process": process,
                    "name": name,
                });
                match target {
                    Some(target) => self.pw(json!({ "SetApplicationRoute": [definition, target] })),
                    None => self.pw(json!({ "ClearApplicationRoute": definition })),
                }
            }
            MixerMessage::SetStreamRoute(node, target) => match target {
                Some(target) => self.pw(json!({ "SetTransientApplicationRoute": [node, target] })),
                None => self.pw(json!({ "ClearTransientApplicationRoute": node })),
            },
            MixerMessage::AppVolume(node, value) => {
                let key = format!("app:{node}");
                self.set_override(&key, value as f32);
                self.pw_keyed(key, json!({ "SetApplicationVolume": [node, value] }));
            }
            MixerMessage::AppVolumeReleased(node) => self.release_override(&format!("app:{node}")),
            MixerMessage::AppMute(node, muted) => {
                self.pw(json!({ "SetApplicationMute": [node, muted] }));
            }
            MixerMessage::ToggleAppExpanded(key) => {
                if !self.expanded_apps.remove(&key) {
                    self.expanded_apps.insert(key);
                }
            }
            MixerMessage::AppSearch(search) => self.app_search = search,
            MixerMessage::RescanApps => {
                if !self.scanning {
                    return self.scan_apps();
                }
            }
            MixerMessage::CatalogLoaded(loaded) => {
                self.scanning = false;
                self.catalog = Some(loaded);
            }
            MixerMessage::StreamsProbed(found) => {
                for (node, ids) in found {
                    self.probing.remove(&node);
                    self.stream_ids.insert(node, ids);
                }
            }
            MixerMessage::SetCatalogRoute { key, target } => self.set_catalog_route(key, target),

            MixerMessage::PhysicalVolume(id, value) => {
                let key = format!("phys:{id}");
                self.set_override(&key, value as f32);
                self.pw_keyed(key, json!({ "SetPhysicalDeviceVolume": [id, value] }));
            }
            MixerMessage::PhysicalVolumeReleased(id) => {
                self.release_override(&format!("phys:{id}"))
            }
            MixerMessage::PhysicalMute(id, muted) => {
                self.pw(json!({ "SetPhysicalDeviceMute": [id, muted] }));
            }

            MixerMessage::SetQuantum(quantum) => {
                self.popup = None;
                let value = quantum.map(|q| format!("Quantum{q}"));
                self.daemon(json!({ "SetAudioQuantum": value }));
            }
            MixerMessage::ResetAudio => self.modal = Some(Modal::ResetAudio),
            MixerMessage::OpenWebMixer => {
                if !crate::integrations::pipeweaver::launch_pipeweaver_ui() {
                    self.show_error("Couldn't open the web mixer.".into());
                }
            }

            MixerMessage::SelectFilter(id) => self.selected_filter = Some(id),
            MixerMessage::AddFilter(node, lv2, key) => {
                let filter = if lv2 {
                    json!({ "LV2": { "plugin_uri": key, "values": {} } })
                } else {
                    json!({ "PWV": { "plugin_type": key, "values": {} } })
                };
                self.pw(json!({ "AddFilterToNode": [node, { "filter": filter }] }));
            }
            MixerMessage::RemoveFilter(id) => {
                if self.selected_filter.as_ref() == Some(&id) {
                    self.selected_filter = None;
                }
                self.pw(json!({ "RemoveFilter": id }));
            }
            MixerMessage::MoveFilter(id, index) => {
                self.pw(json!({ "MoveFilter": [id, index] }));
            }
            MixerMessage::FilterValue(filter, param, value) => {
                let key = format!("filter:{filter}:{param}");
                if let Some(v) = model::filter_value_f32(&value) {
                    self.set_override(&key, v);
                }
                self.pw_keyed(key, json!({ "SetFilterValue": [filter, param, value] }));
            }
            MixerMessage::FilterValueReleased(filter, param) => {
                self.release_override(&format!("filter:{filter}:{param}"))
            }
            MixerMessage::FilterSearch(search) => self.filter_search = search,
        }
        Task::none()
    }
}

pub(crate) fn volume_key(id: &str, mix: Option<Mix>) -> String {
    match mix {
        Some(mix) => format!("vol:{id}:{}", mix.as_str()),
        None => format!("vol:{id}:T"),
    }
}

fn other_mix(mix: Mix) -> Mix {
    match mix {
        Mix::A => Mix::B,
        Mix::B => Mix::A,
    }
}
