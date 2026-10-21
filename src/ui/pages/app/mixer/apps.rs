//! The Applications tab: default devices, and where each app's audio goes.

use crate::ui::pages::app::mixer::model::Application;
use crate::ui::pages::app::mixer::style::{self, ACCENT, MIX_A, MUTE, TEXT, TEXT_DIM};
use crate::ui::pages::app::mixer::view::{Choice, El, clip};
use crate::ui::pages::app::mixer::{MixerMessage, MixerPage};
use iced::widget::{
    Space, button, column, container, keyed_column, pick_list, row, scrollable, slider, text,
};
use iced::{Alignment, Color, Length, Padding};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

impl MixerPage {
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
            self.app_list(true, "Playback", "Apps that play sound"),
            self.app_list(false, "Recording", "Apps that record sound"),
        ]
        .spacing(12)
        .height(Length::Fill);

        column![defaults, lists].spacing(12).into()
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
