//! A typed, read-only view of Pipeweaver's `DaemonStatus` JSON.
//!
//! The page keeps the raw JSON (so RFC 6902 patches from the daemon apply cleanly) and
//! re-derives this model after every change. Everything is lenient: unknown fields are
//! ignored and missing ones fall back to defaults, so a newer daemon doesn't break the page.

use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mix {
    A,
    B,
}

impl Mix {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mix::A => "A",
            Mix::B => "B",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MuteTarget {
    A,
    B,
}

impl MuteTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            MuteTarget::A => "TargetA",
            MuteTarget::B => "TargetB",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    PhysicalSource,
    VirtualSource,
    PhysicalTarget,
    VirtualTarget,
}

impl NodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::PhysicalSource => "PhysicalSource",
            NodeKind::VirtualSource => "VirtualSource",
            NodeKind::PhysicalTarget => "PhysicalTarget",
            NodeKind::VirtualTarget => "VirtualTarget",
        }
    }
    pub fn is_source(&self) -> bool {
        matches!(self, NodeKind::PhysicalSource | NodeKind::VirtualSource)
    }
    pub fn is_physical(&self) -> bool {
        matches!(self, NodeKind::PhysicalSource | NodeKind::PhysicalTarget)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderGroup {
    Default,
    Pinned,
    Hidden,
}

impl OrderGroup {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderGroup::Default => "Default",
            OrderGroup::Pinned => "Pinned",
            OrderGroup::Hidden => "Hidden",
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////
// Raw serde shapes

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Status {
    pub config: Config,
    pub audio: Audio,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub auto_start: bool,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Audio {
    pub profile: Profile,
    pub filter_config: HashMap<String, FilterConfig>,
    pub devices: ByDirection<Vec<PhysicalDevice>>,
    pub defaults_id: ByDirection<Option<String>>,
    pub applications: ByDirection<BTreeMap<String, BTreeMap<String, Vec<Application>>>>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct ByDirection<T: Default> {
    #[serde(rename = "Source")]
    pub source: T,
    #[serde(rename = "Target")]
    pub target: T,
}

impl<T: Default> ByDirection<T> {
    pub fn get(&self, source: bool) -> &T {
        if source { &self.source } else { &self.target }
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub devices: Devices,
    pub routes: HashMap<String, Vec<String>>,
    pub audio_node_quantum: Option<String>,
    pub application_mapping: ByDirection<BTreeMap<String, BTreeMap<String, String>>>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Devices {
    pub sources: Group<SourceDevice>,
    pub targets: Group<TargetDevice>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Group<T> {
    pub physical_devices: Vec<T>,
    pub virtual_devices: Vec<T>,
    pub device_order: DeviceOrder,
}

impl<T> Default for Group<T> {
    fn default() -> Self {
        Self {
            physical_devices: vec![],
            virtual_devices: vec![],
            device_order: DeviceOrder::default(),
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct DeviceOrder {
    #[serde(rename = "Default")]
    pub default: Vec<String>,
    #[serde(rename = "Pinned")]
    pub pinned: Vec<String>,
    #[serde(rename = "Hidden")]
    pub hidden: Vec<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Description {
    pub id: String,
    pub name: String,
    pub colour: Colour,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Colour {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Default for Colour {
    fn default() -> Self {
        Self {
            red: 255,
            green: 255,
            blue: 0,
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct MuteStates {
    pub mute_state: Vec<String>,
    pub mute_targets: MuteTargets,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct MuteTargets {
    #[serde(rename = "TargetA")]
    pub a: Vec<String>,
    #[serde(rename = "TargetB")]
    pub b: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Volumes {
    pub volume: MixVolumes,
    pub volumes_linked: Option<f32>,
}

impl Default for Volumes {
    fn default() -> Self {
        Self {
            volume: MixVolumes { a: 100, b: 100 },
            volumes_linked: Some(1.0),
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct MixVolumes {
    #[serde(rename = "A")]
    pub a: u8,
    #[serde(rename = "B")]
    pub b: u8,
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Descriptor {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct FilterRef {
    pub id: String,
    pub filter: Value,
}

impl FilterRef {
    /// LV2 plugins also report their output ports (meters and the like) as parameters;
    /// Pipeweaver's own filters mark every parameter as an output, but all of them are settings.
    pub fn is_lv2(&self) -> bool {
        self.filter.get("LV2").is_some()
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct SourceDevice {
    pub description: Description,
    pub mute_states: MuteStates,
    pub volumes: Volumes,
    pub filters: Vec<FilterRef>,
    pub attached_devices: Vec<Descriptor>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TargetDevice {
    pub description: Description,
    pub mute_state: String,
    pub volume: u8,
    pub mix: String,
    pub filters: Vec<FilterRef>,
    pub attached_devices: Vec<Descriptor>,
}

impl Default for TargetDevice {
    fn default() -> Self {
        Self {
            description: Description::default(),
            mute_state: "Unmuted".into(),
            volume: 100,
            mix: "A".into(),
            filters: vec![],
            attached_devices: vec![],
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct PhysicalDevice {
    pub id: String,
    pub node_id: u32,
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_usable: bool,
    pub volume: u8,
    pub muted: bool,
}

impl PhysicalDevice {
    pub fn label(&self) -> String {
        self.description
            .clone()
            .or_else(|| self.name.clone())
            .unwrap_or_else(|| format!("Node {}", self.node_id))
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Application {
    pub node_id: u32,
    pub name: String,
    pub volume: u8,
    pub muted: bool,
    pub title: Option<String>,
    pub target_id: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct FilterConfig {
    pub name: String,
    pub identifier: String,
    pub state: Value,
    pub parameters: Vec<FilterProperty>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct FilterProperty {
    pub id: u32,
    pub name: String,
    pub symbol: String,
    pub value: Value,
    pub min: f32,
    pub max: f32,
    pub is_input: bool,
    pub enum_def: Option<HashMap<String, String>>,
}

////////////////////////////////////////////////////////////////////////////////////////////
// A flattened channel, which is what the UI works with

#[derive(Debug, Clone)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub colour: Colour,
    pub kind: NodeKind,
    pub group: OrderGroup,

    /// Sources: Mix A / Mix B. Targets: `a` is the volume, `b` is unused.
    pub volume_a: u8,
    pub volume_b: u8,
    pub linked: bool,

    /// Targets only: which mix they carry
    pub mix: Mix,

    /// Sources: Mute A / Mute B active. Targets: `mute_a` is the channel mute.
    pub mute_a: bool,
    pub mute_b: bool,
    pub mute_targets_a: Vec<String>,
    pub mute_targets_b: Vec<String>,

    pub attached: Vec<Descriptor>,
    pub filters: Vec<FilterRef>,
}

impl Channel {
    fn from_source(d: &SourceDevice, kind: NodeKind, group: OrderGroup) -> Self {
        Self {
            id: d.description.id.clone(),
            name: d.description.name.clone(),
            colour: d.description.colour,
            kind,
            group,
            volume_a: d.volumes.volume.a,
            volume_b: d.volumes.volume.b,
            linked: d.volumes.volumes_linked.is_some(),
            mix: Mix::A,
            mute_a: d.mute_states.mute_state.iter().any(|s| s == "TargetA"),
            mute_b: d.mute_states.mute_state.iter().any(|s| s == "TargetB"),
            mute_targets_a: d.mute_states.mute_targets.a.clone(),
            mute_targets_b: d.mute_states.mute_targets.b.clone(),
            attached: d.attached_devices.clone(),
            filters: d.filters.clone(),
        }
    }

    fn from_target(d: &TargetDevice, kind: NodeKind, group: OrderGroup) -> Self {
        Self {
            id: d.description.id.clone(),
            name: d.description.name.clone(),
            colour: d.description.colour,
            kind,
            group,
            volume_a: d.volume,
            volume_b: 0,
            linked: false,
            mix: if d.mix == "B" { Mix::B } else { Mix::A },
            mute_a: d.mute_state == "Muted",
            mute_b: false,
            mute_targets_a: vec![],
            mute_targets_b: vec![],
            attached: d.attached_devices.clone(),
            filters: d.filters.clone(),
        }
    }

    pub fn is_source(&self) -> bool {
        self.kind.is_source()
    }

    pub fn mute_targets(&self, target: MuteTarget) -> &Vec<String> {
        match target {
            MuteTarget::A => &self.mute_targets_a,
            MuteTarget::B => &self.mute_targets_b,
        }
    }

    pub fn is_muted(&self, target: MuteTarget) -> bool {
        match target {
            MuteTarget::A => self.mute_a,
            MuteTarget::B => self.mute_b,
        }
    }

    /// A physical channel that has nothing attached, so it carries no audio.
    pub fn needs_device(&self) -> bool {
        self.kind.is_physical() && self.attached.is_empty()
    }
}

/// Everything the page draws, derived from one status snapshot.
#[derive(Debug, Default, Clone)]
pub struct Model {
    pub status: Status,
    /// Sources in display order: pinned, default, then hidden
    pub sources: Vec<Channel>,
    /// Targets in display order: pinned, default, then hidden
    pub targets: Vec<Channel>,
}

impl Model {
    pub fn from_value(value: &Value) -> Result<Self, serde_json::Error> {
        let status: Status = serde_json::from_value(value.clone())?;
        let devs = &status.audio.profile.devices;

        let mut sources = vec![];
        for (group, list) in [
            (OrderGroup::Pinned, &devs.sources.device_order.pinned),
            (OrderGroup::Default, &devs.sources.device_order.default),
            (OrderGroup::Hidden, &devs.sources.device_order.hidden),
        ] {
            for id in list {
                if let Some(d) = devs
                    .sources
                    .physical_devices
                    .iter()
                    .find(|d| &d.description.id == id)
                {
                    sources.push(Channel::from_source(d, NodeKind::PhysicalSource, group));
                } else if let Some(d) = devs
                    .sources
                    .virtual_devices
                    .iter()
                    .find(|d| &d.description.id == id)
                {
                    sources.push(Channel::from_source(d, NodeKind::VirtualSource, group));
                }
            }
        }

        let mut targets = vec![];
        for (group, list) in [
            (OrderGroup::Pinned, &devs.targets.device_order.pinned),
            (OrderGroup::Default, &devs.targets.device_order.default),
            (OrderGroup::Hidden, &devs.targets.device_order.hidden),
        ] {
            for id in list {
                if let Some(d) = devs
                    .targets
                    .physical_devices
                    .iter()
                    .find(|d| &d.description.id == id)
                {
                    targets.push(Channel::from_target(d, NodeKind::PhysicalTarget, group));
                } else if let Some(d) = devs
                    .targets
                    .virtual_devices
                    .iter()
                    .find(|d| &d.description.id == id)
                {
                    targets.push(Channel::from_target(d, NodeKind::VirtualTarget, group));
                }
            }
        }

        Ok(Self {
            status,
            sources,
            targets,
        })
    }

    pub fn channel(&self, id: &str) -> Option<&Channel> {
        self.sources
            .iter()
            .chain(self.targets.iter())
            .find(|c| c.id == id)
    }

    pub fn channel_name(&self, id: &str) -> Option<&str> {
        self.channel(id).map(|c| c.name.as_str())
    }

    pub fn is_routed(&self, source: &str, target: &str) -> bool {
        self.status
            .audio
            .profile
            .routes
            .get(source)
            .is_some_and(|t| t.iter().any(|t| t == target))
    }

    /// Usable physical PipeWire devices for one direction
    pub fn physical_devices(&self, source: bool) -> Vec<&PhysicalDevice> {
        let mut list: Vec<&PhysicalDevice> = self
            .status
            .audio
            .devices
            .get(source)
            .iter()
            .filter(|d| d.is_usable)
            .collect();
        list.sort_by_key(|d| d.label().to_lowercase());
        list
    }

    /// The physical devices offered in a channel's device menu: what's attached (present or
    /// missing) first, then everything else that could be attached.
    pub fn attachable(&self, channel: &Channel) -> Vec<AttachEntry> {
        let devices = self.physical_devices(channel.is_source());
        let mut entries = vec![];
        let mut used = vec![];

        for (index, wanted) in channel.attached.iter().enumerate() {
            let found = devices.iter().find(|d| match &wanted.name {
                Some(name) => d.name.as_ref() == Some(name),
                None => d.description == wanted.description,
            });
            if let Some(d) = found {
                used.push(d.node_id);
            }
            let label = found.map(|d| d.label()).unwrap_or_else(|| {
                wanted
                    .description
                    .clone()
                    .or_else(|| wanted.name.clone())
                    .unwrap_or_else(|| "Unknown device".into())
            });
            entries.push(AttachEntry {
                label,
                node_id: found.map(|d| d.node_id),
                config_index: Some(index),
            });
        }

        for d in devices {
            if !used.contains(&d.node_id) {
                entries.push(AttachEntry {
                    label: d.label(),
                    node_id: Some(d.node_id),
                    config_index: None,
                });
            }
        }
        entries
    }

    /// Default device choices for one direction (physical devices plus Pipeweaver channels
    /// that can stand in for them), sorted by label.
    pub fn default_choices(&self, source: bool) -> Vec<(String, String)> {
        let mut list: Vec<(String, String)> = self
            .status
            .audio
            .devices
            .get(source)
            .iter()
            .map(|d| (d.id.clone(), d.label()))
            .collect();
        // An app's input comes from a virtual target, its output goes to a virtual source
        let virt = if source {
            self.targets
                .iter()
                .filter(|c| c.kind == NodeKind::VirtualTarget)
                .collect::<Vec<_>>()
        } else {
            self.sources
                .iter()
                .filter(|c| c.kind == NodeKind::VirtualSource)
                .collect::<Vec<_>>()
        };
        for c in virt {
            list.push((c.id.clone(), format!("Beacn {}", c.name)));
        }
        list.sort_by_key(|(_, l)| l.to_lowercase());
        list
    }

    /// Virtual channels an application can be routed to
    pub fn app_route_choices(&self, playback: bool) -> Vec<&Channel> {
        if playback {
            self.sources
                .iter()
                .filter(|c| c.kind == NodeKind::VirtualSource)
                .collect()
        } else {
            self.targets
                .iter()
                .filter(|c| c.kind == NodeKind::VirtualTarget)
                .collect()
        }
    }

    /// Running applications currently playing into (sources) or recording from (targets)
    /// a channel, by name
    pub fn apps_on(&self, channel: &Channel) -> Vec<String> {
        let mut names = vec![];
        for apps in self
            .status
            .audio
            .applications
            .get(channel.is_source())
            .values()
        {
            for (name, nodes) in apps {
                if nodes
                    .iter()
                    .any(|n| n.target_id.as_deref() == Some(channel.id.as_str()))
                    && !names.contains(name)
                {
                    names.push(name.clone());
                }
            }
        }
        names
    }

    /// Checks a new channel name the same way the web mixer does
    pub fn name_error(&self, name: &str, current: Option<&str>) -> Option<&'static str> {
        if name.is_empty() {
            return Some("Name cannot be empty.");
        }
        if name.chars().count() > 20 {
            return Some("Name cannot be longer than 20 characters.");
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '_' || c == '-')
        {
            return Some("Name can only contain letters, numbers, spaces, - and _.");
        }
        let lower = name.to_lowercase();
        let clash = self
            .sources
            .iter()
            .chain(self.targets.iter())
            .any(|c| c.name.to_lowercase() == lower && Some(c.name.as_str()) != current);
        if clash {
            return Some("Name cannot match an existing channel.");
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct AttachEntry {
    pub label: String,
    /// Present on the system right now
    pub node_id: Option<u32>,
    /// Attached in the profile (index into `attached_devices`)
    pub config_index: Option<usize>,
}

////////////////////////////////////////////////////////////////////////////////////////////
// Filters

pub const PWV_FILTERS: &[(&str, &str)] = &[
    ("Gain", "Gain"),
    ("Bleep", "Bleep"),
    ("RNNoise", "Noise Reduction"),
];

pub const LV2_FILTERS: &[(&str, &str)] = &[
    (
        "http://lsp-plug.in/plugins/lv2/comp_delay_x2_stereo",
        "Delay",
    ),
    (
        "http://lsp-plug.in/plugins/lv2/compressor_stereo",
        "Compressor",
    ),
    ("http://lsp-plug.in/plugins/lv2/gate_stereo", "Gate"),
    ("http://lsp-plug.in/plugins/lv2/expander_stereo", "Expander"),
    ("http://lsp-plug.in/plugins/lv2/limiter_stereo", "Limiter"),
    (
        "http://lsp-plug.in/plugins/lv2/para_equalizer_x32_lr",
        "Equaliser",
    ),
    (
        "http://lsp-plug.in/plugins/lv2/mb_compressor_stereo",
        "Multiband Compressor",
    ),
    (
        "http://lsp-plug.in/plugins/lv2/mb_gate_stereo",
        "Multiband Gate",
    ),
    ("http://lsp-plug.in/plugins/lv2/filter_stereo", "Filter"),
    (
        "http://lsp-plug.in/plugins/lv2/loud_comp_stereo",
        "Loudness",
    ),
    (
        "http://calf.sourceforge.net/plugins/BassEnhancer",
        "Bass Enhancer",
    ),
    ("http://calf.sourceforge.net/plugins/Crusher", "Crusher"),
    ("http://calf.sourceforge.net/plugins/Deesser", "Deesser"),
    ("http://calf.sourceforge.net/plugins/Exciter", "Exciter"),
    (
        "http://calf.sourceforge.net/plugins/StereoTools",
        "Stereo Tools",
    ),
    (
        "http://calf.sourceforge.net/plugins/Reverb",
        "Reverberation",
    ),
    ("urn:zamaudio:ZaMaximX2", "Maximizer"),
];

/// The enable switch for a filter: (parameter symbol, inverted)
pub fn filter_enable_symbol(identifier: &str) -> (&'static str, bool) {
    if identifier.starts_with("http://calf.sourceforge.net/plugins/") {
        if identifier.ends_with("/Reverb") {
            return ("on", false);
        }
        return ("bypass", true);
    }
    ("enabled", false)
}

pub fn filter_display_name(identifier: &str, fallback: &str) -> String {
    PWV_FILTERS
        .iter()
        .chain(LV2_FILTERS.iter())
        .find(|(id, _)| *id == identifier)
        .map(|(_, name)| name.to_string())
        .unwrap_or_else(|| {
            if fallback.is_empty() {
                identifier.to_string()
            } else {
                fallback.to_string()
            }
        })
}

/// A filter parameter value as a number, for sliders
pub fn filter_value_f32(value: &Value) -> Option<f32> {
    let (_, inner) = value.as_object()?.iter().next()?;
    match inner {
        Value::Number(n) => n.as_f64().map(|v| v as f32),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::Array(a) => a.get(1).and_then(|v| v.as_f64()).map(|v| v as f32),
        _ => None,
    }
}

pub fn filter_value_kind(value: &Value) -> &str {
    value
        .as_object()
        .and_then(|o| o.keys().next())
        .map(|s| s.as_str())
        .unwrap_or("")
}

pub const QUANTUMS: &[u32] = &[
    8, 16, 32, 64, 128, 256, 512, 768, 1024, 1280, 1536, 1792, 2048, 2304, 2560, 2816, 3072, 3328,
    3584, 3840, 4096,
];
