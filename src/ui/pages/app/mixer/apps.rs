//! The Applications tab: default devices, every installed app and Steam game with the channel
//! it plays to, and where recording apps take their sound from.

use crate::ui::pages::app::mixer::catalog::{self, Catalog, Icon};
use crate::ui::pages::app::mixer::model::Application;
use crate::ui::pages::app::mixer::style::{self, ACCENT, MIX_A, MUTE, TEXT, TEXT_DIM};
use crate::ui::pages::app::mixer::view::{Choice, El, clip};
use crate::ui::pages::app::mixer::{MixerMessage, MixerPage};
use iced::widget::{
    Space, button, column, container, image, keyed_column, pick_list, row, scrollable, slider, svg,
    text, text_input,
};
use iced::{Alignment, Color, Length, Padding, Task};
use log::{debug, info};
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

const ICON_SIZE: u32 = 28;

#[derive(Debug, Clone)]
pub enum AppIcon {
    Svg(svg::Handle),
    Image(image::Handle),
}

/// The scanned catalog plus its icons, ready to draw
#[derive(Debug, Default)]
pub struct LoadedCatalog {
    pub catalog: Catalog,
    pub icons: HashMap<String, AppIcon>,
}

fn load_catalog() -> LoadedCatalog {
    let catalog = catalog::scan();
    let mut icons = HashMap::new();
    for app in &catalog.apps {
        let icon = match &app.icon {
            Some(Icon::Svg(path)) => Some(AppIcon::Svg(svg::Handle::from_path(path))),
            Some(Icon::Raster(path)) => catalog::load_raster(path, ICON_SIZE * 2)
                .map(|(w, h, rgba)| AppIcon::Image(image::Handle::from_rgba(w, h, rgba))),
            None => None,
        };
        if let Some(icon) = icon {
            icons.insert(app.key.clone(), icon);
        }
    }
    LoadedCatalog { catalog, icons }
}

type Pair = (String, String);

/// One line in the app list
struct Row<'a> {
    id: u64,
    name: String,
    detail: String,
    icon: Option<&'a AppIcon>,
    /// Catalogued app (routes through the saved choice), else a bare Pipeweaver pair
    key: Option<String>,
    pair: Option<Pair>,
    running: Vec<(&'a str, &'a str, &'a [Application])>,
    current: Option<String>,
    search: String,
}

impl MixerPage {
    pub(super) fn scan_apps(&mut self) -> Task<MixerMessage> {
        self.scanning = true;
        Task::perform(
            async {
                tokio::task::spawn_blocking(load_catalog)
                    .await
                    .unwrap_or_default()
            },
            |loaded| MixerMessage::CatalogLoaded(Arc::new(loaded)),
        )
    }

    /// Extra ids for a running app's streams (Flatpak id, SteamAppId), if we've read them
    fn extra_ids(&self, nodes: &[Application]) -> catalog::StreamIds {
        let mut ids = catalog::StreamIds::default();
        for node in nodes {
            if let Some(found) = self.stream_ids.get(&node.node_id) {
                for id in &found.ids {
                    if !ids.ids.contains(id) {
                        ids.ids.push(id.clone());
                    }
                }
            }
        }
        ids
    }

    /// Makes running apps follow the channel picked for them in the catalog.
    ///
    /// Pipeweaver routes by exact (process, stream name). When a catalogued app plays, its
    /// pair gets a Pipeweaver route to the chosen channel; Pipeweaver moves the stream (even
    /// off pipeweaver_game, where the Windows-game rule sends it) and remembers the pair, so
    /// it's right from the first moment next time, even without beacn-utility running.
    pub(super) fn enforce_app_routes(&mut self) -> Task<MixerMessage> {
        if !self.connected {
            return Task::none();
        }
        let Some(loaded) = self.catalog.clone() else {
            return Task::none();
        };
        let catalog = &loaded.catalog;
        let running = &self.model.status.audio.applications.source;
        let mapping = &self.model.status.audio.profile.application_mapping.source;

        let mut live_nodes = HashSet::new();
        let mut live_pairs = HashSet::new();
        let mut to_probe = vec![];
        let mut sends = vec![];
        for (process, apps) in running {
            for (name, nodes) in apps {
                live_pairs.insert((process.clone(), name.clone()));
                let mut waiting = false;
                for node in nodes {
                    live_nodes.insert(node.node_id);
                    if !self.stream_ids.contains_key(&node.node_id) {
                        waiting = true;
                        if !self.probing.contains(&node.node_id) {
                            to_probe.push(node.node_id);
                        }
                    }
                }
                // Ids read from the process beat name matching, so wait for them
                if waiting {
                    continue;
                }
                let extra = self.extra_ids(nodes);
                let Some(index) = catalog.find(process, name, Some(&extra)) else {
                    continue;
                };
                let app = &catalog.apps[index];
                let Some(saved) = self.routing.apps.get(&app.key) else {
                    continue;
                };
                if self.model.channel(&saved.channel).is_none() {
                    continue;
                }
                let pair = (process.clone(), name.clone());
                let current = mapping.get(process).and_then(|m| m.get(name));
                if current == Some(&saved.channel) {
                    continue;
                }
                if self.enforced.get(&pair) == Some(&saved.channel) {
                    continue;
                }
                info!(
                    "Applications: routing {process} / {name} ({}) to {}",
                    app.name, saved.channel_name
                );
                sends.push((pair, saved.channel.clone()));
            }
        }
        self.stream_ids.retain(|n, _| live_nodes.contains(n));
        self.enforced.retain(|p, _| live_pairs.contains(p));
        for ((process, name), target) in sends {
            self.pw(json!({ "SetApplicationRoute": [
                { "device_type": "Source", "process": process, "name": name },
                target
            ] }));
            self.enforced.insert((process, name), target);
        }

        if to_probe.is_empty() {
            return Task::none();
        }
        debug!("Applications: reading ids for nodes {to_probe:?}");
        self.probing.extend(to_probe.iter().copied());
        Task::perform(
            async move {
                let asked = to_probe.clone();
                tokio::task::spawn_blocking(move || catalog::probe(&to_probe))
                    .await
                    .unwrap_or_else(|_| {
                        asked
                            .into_iter()
                            .map(|n| (n, catalog::StreamIds::default()))
                            .collect()
                    })
            },
            MixerMessage::StreamsProbed,
        )
    }

    /// Saves the choice and applies it to every known (process, name) pair of the app
    pub(super) fn set_catalog_route(&mut self, key: String, target: Option<String>) {
        let Some(loaded) = self.catalog.clone() else {
            return;
        };
        let catalog = &loaded.catalog;
        let Some(index) = catalog.apps.iter().position(|a| a.key == key) else {
            return;
        };
        let app = &catalog.apps[index];
        match &target {
            Some(channel) => {
                let channel_name = self.model.channel_name(channel).unwrap_or("").to_string();
                self.routing.apps.insert(
                    key.clone(),
                    catalog::SavedRoute {
                        channel: channel.clone(),
                        channel_name,
                        app_name: app.name.clone(),
                    },
                );
            }
            None => {
                self.routing.apps.remove(&key);
            }
        }
        self.routing.save();

        // Every pair this app is known by: running now, or remembered by Pipeweaver
        let mut pairs: Vec<(Pair, bool)> = vec![];
        for (process, apps) in &self.model.status.audio.applications.source {
            for (name, nodes) in apps {
                let extra = self.extra_ids(nodes);
                if catalog.find(process, name, Some(&extra)) == Some(index) {
                    let mapped = self.mapping_of(process, name).is_some();
                    pairs.push(((process.clone(), name.clone()), mapped));
                }
            }
        }
        for (process, apps) in &self.model.status.audio.profile.application_mapping.source {
            for name in apps.keys() {
                let pair = (process.clone(), name.clone());
                if catalog.find(process, name, None) == Some(index)
                    && !pairs.iter().any(|(p, _)| *p == pair)
                {
                    pairs.push((pair, true));
                }
            }
        }
        for ((process, name), mapped) in pairs {
            let definition = json!({ "device_type": "Source", "process": process, "name": name });
            match &target {
                Some(channel) => {
                    self.pw(json!({ "SetApplicationRoute": [definition, channel] }));
                    self.enforced.insert((process, name), channel.clone());
                }
                None => {
                    if mapped {
                        self.pw(json!({ "ClearApplicationRoute": definition }));
                    }
                    self.enforced.remove(&(process, name));
                }
            }
        }
    }

    fn mapping_of(&self, process: &str, name: &str) -> Option<&String> {
        self.model
            .status
            .audio
            .profile
            .application_mapping
            .source
            .get(process)
            .and_then(|m| m.get(name))
    }

    pub(super) fn view_apps(&self) -> El<'_> {
        let defaults = container(
            row![
                text("Default output").color(TEXT),
                self.default_picker(false),
                Space::new().width(20),
                text("Default input").color(TEXT),
                self.default_picker(true),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding(10)
        .width(Length::Fill)
        .style(style::panel);

        let lists = row![
            self.catalog_list(),
            self.app_list(false, "Recording", "Apps that record sound"),
        ]
        .spacing(12)
        .height(Length::Fill);

        column![defaults, lists].spacing(12).into()
    }

    fn rows(&self) -> Vec<Row<'_>> {
        let running = &self.model.status.audio.applications.source;
        let mapping = &self.model.status.audio.profile.application_mapping.source;
        let hash = |s: &str| {
            let mut h = DefaultHasher::new();
            s.hash(&mut h);
            h.finish()
        };

        let mut rows: Vec<Row> = vec![];
        let catalog = self.catalog.as_deref();
        if let Some(loaded) = catalog {
            for app in &loaded.catalog.apps {
                let current = self
                    .routing
                    .apps
                    .get(&app.key)
                    .map(|s| s.channel.clone())
                    .filter(|c| self.model.channel(c).is_some());
                rows.push(Row {
                    id: hash(&app.key),
                    name: app.name.clone(),
                    detail: app.detail.clone(),
                    icon: loaded.icons.get(&app.key),
                    key: Some(app.key.clone()),
                    pair: None,
                    running: vec![],
                    current,
                    search: app.search.clone(),
                });
            }
        }
        let find = |process: &str, name: &str, extra: Option<&catalog::StreamIds>| {
            catalog.and_then(|l| l.catalog.find(process, name, extra))
        };

        for (process, apps) in running {
            for (name, nodes) in apps {
                let extra = self.extra_ids(nodes);
                match find(process, name, Some(&extra)) {
                    Some(i) => {
                        let row = &mut rows[i];
                        row.running.push((process, name, nodes));
                        if row.current.is_none()
                            && !self
                                .routing
                                .apps
                                .contains_key(&row.key.clone().unwrap_or_default())
                        {
                            row.current = mapping.get(process).and_then(|m| m.get(name)).cloned();
                        }
                    }
                    None => rows.push(Row {
                        id: hash(&format!("pair:{process}:{name}")),
                        name: name.clone(),
                        detail: process.clone(),
                        icon: None,
                        key: None,
                        pair: Some((process.clone(), name.clone())),
                        running: vec![(process, name, nodes)],
                        current: mapping.get(process).and_then(|m| m.get(name)).cloned(),
                        search: format!("{name} {process}").to_lowercase(),
                    }),
                }
            }
        }

        // Routes Pipeweaver remembers: shown on their app, or on their own if not catalogued
        for (process, apps) in mapping {
            for (name, target) in apps {
                if running.get(process).is_some_and(|a| a.contains_key(name)) {
                    continue;
                }
                match find(process, name, None) {
                    Some(i) => {
                        let row = &mut rows[i];
                        let saved = row
                            .key
                            .as_ref()
                            .is_some_and(|k| self.routing.apps.contains_key(k));
                        if row.current.is_none() && !saved {
                            row.current = Some(target.clone());
                        }
                    }
                    None => rows.push(Row {
                        id: hash(&format!("pair:{process}:{name}")),
                        name: name.clone(),
                        detail: format!("{process} · not installed as an app"),
                        icon: None,
                        key: None,
                        pair: Some((process.clone(), name.clone())),
                        running: vec![],
                        current: Some(target.clone()),
                        search: format!("{name} {process}").to_lowercase(),
                    }),
                }
            }
        }

        rows.retain(|r| {
            r.current
                .as_ref()
                .is_none_or(|c| self.model.channel(c).is_some())
        });
        let query = self.app_search.trim().to_lowercase();
        if !query.is_empty() {
            rows.retain(|r| query.split_whitespace().all(|w| r.search.contains(w)));
        }
        rows.sort_by_cached_key(|r| {
            (
                r.running.is_empty(),
                r.current.is_none(),
                r.name.to_lowercase(),
            )
        });
        rows
    }

    fn catalog_list(&self) -> El<'_> {
        let rows = self.rows();
        let count = rows.len();
        let mut cards = keyed_column([]).spacing(6);
        for r in rows {
            cards = cards.push(r.id, self.catalog_row(r));
        }
        let mut list = column![cards].spacing(6);
        if count == 0 {
            let message = if self.catalog.is_none() {
                "Looking for installed apps and games…"
            } else {
                "No apps match."
            };
            list =
                list.push(container(text(message).color(TEXT_DIM)).padding(Padding::from([6, 2])));
        }

        let rescan = button(
            row![
                style::icon("rotate", 11.0, TEXT),
                text(if self.scanning {
                    "Scanning…"
                } else {
                    "Rescan"
                })
                .size(11)
                .color(TEXT)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding(Padding::from([4, 8]))
        .style(style::control(false, ACCENT))
        .on_press_maybe((!self.scanning).then_some(MixerMessage::RescanApps));

        container(
            column![
                row![
                    style::title("Applications", MIX_A),
                    text("Pick the channel each app or game plays to. Default goes to the default output.")
                        .size(11)
                        .color(TEXT_DIM)
                        .width(Length::Fill),
                    rescan,
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                text_input("Search apps and games", &self.app_search)
                    .on_input(MixerMessage::AppSearch)
                    .padding(6),
                scrollable(container(list).padding(Padding {
                    top: 0.0,
                    right: 12.0,
                    bottom: 0.0,
                    left: 0.0
                }))
                .height(Length::Fill)
            ]
            .spacing(10),
        )
        .padding(10)
        .width(Length::FillPortion(3))
        .height(Length::Fill)
        .style(style::panel)
        .into()
    }

    fn default_label(&self) -> String {
        self.model
            .status
            .audio
            .defaults_id
            .get(false)
            .as_deref()
            .and_then(|id| self.model.channel_name(id))
            .map(|name| format!("Default ({name})"))
            .unwrap_or_else(|| "Default".into())
    }

    fn catalog_row<'a>(&'a self, r: Row<'a>) -> El<'a> {
        let icon: El = match r.icon {
            Some(AppIcon::Svg(h)) => svg(h.clone())
                .width(ICON_SIZE as f32)
                .height(ICON_SIZE as f32)
                .into(),
            Some(AppIcon::Image(h)) => image(h.clone())
                .width(ICON_SIZE as f32)
                .height(ICON_SIZE as f32)
                .into(),
            None => container(
                text(
                    r.name
                        .chars()
                        .next()
                        .unwrap_or('?')
                        .to_uppercase()
                        .to_string(),
                )
                .size(13)
                .color(TEXT)
                .font(style::BOLD),
            )
            .center_x(ICON_SIZE as f32)
            .center_y(ICON_SIZE as f32)
            .style(style::fill(style::CONTROL, 6.0))
            .into(),
        };

        let choices = self.route_choices(true, &self.default_label());
        let selected = choices.iter().find(|c| c.id == r.current).cloned();
        let (key, pair) = (r.key.clone(), r.pair.clone());
        let route = pick_list(choices, selected, move |c: Choice| match (&key, &pair) {
            (Some(key), _) => MixerMessage::SetCatalogRoute {
                key: key.clone(),
                target: c.id,
            },
            (None, Some((process, name))) => MixerMessage::SetAppRoute {
                playback: true,
                process: process.clone(),
                name: name.clone(),
                target: c.id,
            },
            (None, None) => MixerMessage::AppSearch(String::new()),
        })
        .width(170)
        .padding(Padding::from([3, 8]));

        let expand_key = format!("catalog:{}", r.id);
        let expanded = self.expanded_apps.contains(&expand_key);
        let streams: usize = r.running.iter().map(|(_, _, n)| n.len()).sum();
        let mut header = row![
            icon,
            column![
                text(clip(&r.name, 40))
                    .color(Color::WHITE)
                    .font(style::BOLD),
                text(clip(&r.detail, 60)).size(10).color(TEXT_DIM),
            ]
            .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if streams > 0 {
            let label = if streams == 1 {
                "Playing · 1 stream".to_string()
            } else {
                format!("Playing · {streams} streams")
            };
            header = header.push(
                button(
                    row![
                        text(label).size(11).color(MIX_A),
                        style::icon(
                            if expanded { "angle-up" } else { "angle-down" },
                            9.0,
                            TEXT_DIM
                        )
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .padding(Padding::from([2, 6]))
                .style(style::menu_item(expanded))
                .on_press(MixerMessage::ToggleAppExpanded(expand_key)),
            );
        }
        header = header.push(route);

        let mut card = column![header].spacing(6);
        if expanded {
            let streams = r
                .running
                .iter()
                .flat_map(|(_, _, nodes)| nodes.iter())
                .fold(keyed_column([]).spacing(6), |streams, node| {
                    streams.push(node.node_id, self.stream_row(true, node))
                });
            card = card.push(container(streams).padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: ICON_SIZE as f32 + 10.0,
            }));
        }

        container(card)
            .padding(Padding::from([6, 8]))
            .width(Length::Fill)
            .style(|_| iced::widget::container::Style {
                background: Some(style::PANEL_RAISED.into()),
                border: iced::Border {
                    radius: 5.0.into(),
                    width: 1.0,
                    color: style::BORDER,
                },
                ..Default::default()
            })
            .into()
    }

    fn default_picker(&self, source: bool) -> El<'_> {
        let choices: Vec<Choice> = self
            .model
            .default_choices(source)
            .into_iter()
            .map(|(id, label)| Choice {
                id: Some(id),
                label,
            })
            .collect();
        let current = self.model.status.audio.defaults_id.get(source).clone();
        let selected = choices.iter().find(|c| c.id == current).cloned();
        pick_list(choices, selected, move |c: Choice| {
            MixerMessage::SetDefault(source, c.id.unwrap_or_default())
        })
        .placeholder("Not set")
        .width(260)
        .padding(Padding::from([4, 8]))
        .into()
    }

    fn route_choices(&self, playback: bool, default_label: &str) -> Vec<Choice> {
        std::iter::once(Choice {
            id: None,
            label: default_label.into(),
        })
        .chain(
            self.model
                .app_route_choices(playback)
                .into_iter()
                .map(|c| Choice {
                    id: Some(c.id.clone()),
                    label: c.name.clone(),
                }),
        )
        .collect()
    }

    fn app_list(&self, playback: bool, title: &str, subtitle: &'static str) -> El<'_> {
        let running = self.model.status.audio.applications.get(playback);
        let mapping = self
            .model
            .status
            .audio
            .profile
            .application_mapping
            .get(playback);

        // Keyed, so an open route list stays with its app while others start and stop
        let mut cards = keyed_column([]).spacing(8);
        let mut any = false;
        for (process, apps) in running {
            for (name, nodes) in apps {
                any = true;
                let mut key = DefaultHasher::new();
                (process, name).hash(&mut key);
                cards = cards.push(key.finish(), self.app_card(playback, process, name, nodes));
            }
        }
        let mut list = column![cards].spacing(8);
        if !any {
            list = list.push(
                container(text("Nothing right now.").color(TEXT_DIM))
                    .padding(Padding::from([6, 2])),
            );
        }

        // Routes remembered for apps that aren't running
        let mut remembered = column![].spacing(4);
        let mut has_remembered = false;
        for (process, apps) in mapping {
            for (name, target) in apps {
                let is_running = running.get(process).is_some_and(|a| a.contains_key(name));
                if is_running {
                    continue;
                }
                has_remembered = true;
                let target = self.model.channel_name(target).unwrap_or("?").to_string();
                remembered = remembered.push(
                    row![
                        text(clip(name, 28)).color(TEXT).width(Length::Fill),
                        text(format!("→ {target}")).size(11).color(TEXT_DIM),
                        button(container(style::icon("xmark", 10.0, TEXT)).center(Length::Fill))
                            .width(22)
                            .height(22)
                            .padding(0)
                            .style(style::control(false, ACCENT))
                            .on_press(MixerMessage::SetAppRoute {
                                playback,
                                process: process.clone(),
                                name: name.clone(),
                                target: None,
                            }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
        }
        if has_remembered {
            list = list
                .push(Space::new().height(6))
                .push(text("REMEMBERED (NOT RUNNING)").size(10).color(TEXT_DIM))
                .push(remembered);
        }

        container(
            column![
                row![
                    style::title(title, if playback { MIX_A } else { style::MIX_B }),
                    text(subtitle).size(11).color(TEXT_DIM)
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                scrollable(container(list).padding(Padding {
                    top: 0.0,
                    right: 12.0,
                    bottom: 0.0,
                    left: 0.0
                }))
                .height(Length::Fill)
            ]
            .spacing(10),
        )
        .padding(10)
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .style(style::panel)
        .into()
    }

    fn app_card<'a>(
        &'a self,
        playback: bool,
        process: &'a str,
        name: &'a str,
        nodes: &'a [Application],
    ) -> El<'a> {
        let mapping = self
            .model
            .status
            .audio
            .profile
            .application_mapping
            .get(playback)
            .get(process)
            .and_then(|m| m.get(name))
            .cloned();
        let choices = self.route_choices(playback, "Default");
        let selected = choices.iter().find(|c| c.id == mapping).cloned();
        let (p, n) = (process.to_string(), name.to_string());
        let route = pick_list(choices, selected, move |c: Choice| {
            MixerMessage::SetAppRoute {
                playback,
                process: p.clone(),
                name: n.clone(),
                target: c.id,
            }
        })
        .width(150)
        .padding(Padding::from([3, 8]));

        let key = format!("{playback}:{process}:{name}");
        let expanded = self.expanded_apps.contains(&key);
        let streams = if nodes.len() == 1 {
            "1 stream".to_string()
        } else {
            format!("{} streams", nodes.len())
        };
        let expand = button(
            row![
                text(streams).size(11).color(TEXT_DIM),
                style::icon("angle-down", 9.0, TEXT_DIM)
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .padding(Padding::from([2, 6]))
        .style(style::menu_item(expanded))
        .on_press(MixerMessage::ToggleAppExpanded(key));

        let header = row![
            column![
                text(clip(name, 26)).color(Color::WHITE).font(style::BOLD),
                text(clip(process, 30)).size(10).color(TEXT_DIM),
            ]
            .width(Length::Fill),
            expand,
            route,
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut card = column![header].spacing(6);
        if expanded {
            let streams = nodes
                .iter()
                .fold(keyed_column([]).spacing(6), |streams, node| {
                    streams.push(node.node_id, self.stream_row(playback, node))
                });
            card = card.push(streams);
        }

        container(card)
            .padding(8)
            .width(Length::Fill)
            .style(|_| iced::widget::container::Style {
                background: Some(style::PANEL_RAISED.into()),
                border: iced::Border {
                    radius: 5.0.into(),
                    width: 1.0,
                    color: style::BORDER,
                },
                ..Default::default()
            })
            .into()
    }

    fn stream_row<'a>(&'a self, playback: bool, node: &'a Application) -> El<'a> {
        let node_id = node.node_id;
        let key = format!("app:{node_id}");
        let volume = self.value(&key, node.volume as f32).round() as u8;
        let title = node.title.clone().unwrap_or_else(|| node.name.clone());

        let choices = self.route_choices(playback, "Default");
        let current = node
            .target_id
            .clone()
            .filter(|t| self.model.channel(t).is_some());
        let selected = choices.iter().find(|c| c.id == current).cloned();

        row![
            text(clip(&title, 24))
                .size(11)
                .color(TEXT)
                .width(Length::Fill),
            slider(0..=100u8, volume, move |v| MixerMessage::AppVolume(
                node_id, v
            ))
            .on_release(MixerMessage::AppVolumeReleased(node_id))
            .width(110)
            .style(style::hslider(MIX_A)),
            text(format!("{volume}%"))
                .size(11)
                .color(TEXT_DIM)
                .width(34),
            button(
                container(style::icon(
                    if node.muted {
                        "volume-xmark"
                    } else {
                        "volume-high"
                    },
                    11.0,
                    if node.muted { Color::WHITE } else { TEXT }
                ))
                .center(Length::Fill)
            )
            .width(26)
            .height(22)
            .padding(0)
            .style(style::control(node.muted, MUTE))
            .on_press(MixerMessage::AppMute(node_id, !node.muted)),
            pick_list(choices, selected, move |c: Choice| {
                MixerMessage::SetStreamRoute(node_id, c.id)
            })
            .text_size(11)
            .width(130)
            .padding(Padding::from([2, 6])),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }
}
