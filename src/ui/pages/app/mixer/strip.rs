//! The Mixer tab: Sources and Targets sections made of channel strips.

use crate::ui::pages::app::mixer::model::{Channel, Mix, MuteTarget, NodeKind, OrderGroup};
use crate::ui::pages::app::mixer::style::{self, ACCENT, MIX_A, MIX_B, MUTE, TEXT, TEXT_DIM, WARN};
use crate::ui::pages::app::mixer::view::{El, clip, menu, menu_entry, menu_heading, menu_rule};
use crate::ui::pages::app::mixer::{MixerMessage, MixerPage, Modal, Popup, volume_key};
use crate::ui::widgets::render::pop_over::Popover;
use iced::widget::{
    Space, button, column, container, row, rule, scrollable, text, tooltip, vertical_slider,
};
use iced::{Alignment, Color, Element, Length, Padding};

const STRIP_WIDTH: f32 = 148.0;
const STRIP_SPACING: f32 = 8.0;
const DIVIDER_WIDTH: f32 = 5.0;

impl MixerPage {
    pub(super) fn view_mixer(&self) -> El<'_> {
        let sections = row![self.section(true), self.section(false)]
            .spacing(12)
            .height(Length::Fill);
        scrollable(sections)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(8).scroller_width(8),
            ))
            .spacing(6)
            .height(Length::Fill)
            .into()
    }

    fn section(&self, sources: bool) -> El<'_> {
        let list = if sources {
            &self.model.sources
        } else {
            &self.model.targets
        };
        let label = if sources { "Sources" } else { "Targets" };
        let colour = if sources { MIX_A } else { MIX_B };

        // Header: title, hidden channels, add
        let hidden: Vec<&Channel> = list
            .iter()
            .filter(|c| c.group == OrderGroup::Hidden)
            .collect();
        let mut header = row![
            style::title(label, colour),
            Space::new().width(Length::Fill)
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if !hidden.is_empty() {
            let items = std::iter::once(menu_heading("Show hidden channel"))
                .chain(hidden.iter().map(|c| {
                    menu_entry(
                        None,
                        c.name.clone(),
                        TEXT,
                        Some(MixerMessage::SetOrderGroup(
                            c.id.clone(),
                            OrderGroup::Default,
                        )),
                    )
                }))
                .collect();
            let trigger = tooltip(
                style::icon_button("eye-slash", TEXT)
                    .on_press(MixerMessage::OpenPopup(Popup::Hidden(sources))),
                text(format!("{} hidden", hidden.len())).size(11),
                tooltip::Position::Bottom,
            )
            .style(style::sheet);
            header = header.push(
                Popover::new(
                    trigger,
                    menu(items, 200.0),
                    self.popup == Some(Popup::Hidden(sources)),
                )
                .on_close(MixerMessage::ClosePopup),
            );
        }
        let (virt, phys, what) = if sources {
            (NodeKind::VirtualSource, NodeKind::PhysicalSource, "source")
        } else {
            (NodeKind::VirtualTarget, NodeKind::PhysicalTarget, "target")
        };
        let add_items = vec![
            menu_entry(
                None,
                format!("Add virtual {what}"),
                TEXT,
                Some(MixerMessage::OpenModal(Modal::Create {
                    kind: virt,
                    value: String::new(),
                })),
            ),
            menu_entry(
                None,
                format!("Add physical {what}"),
                TEXT,
                Some(MixerMessage::OpenModal(Modal::Create {
                    kind: phys,
                    value: String::new(),
                })),
            ),
        ];
        header = header.push(
            Popover::new(
                style::icon_button("plus", TEXT)
                    .on_press(MixerMessage::OpenPopup(Popup::Add(sources))),
                menu(add_items, 200.0),
                self.popup == Some(Popup::Add(sources)),
            )
            .on_close(MixerMessage::ClosePopup),
        );

        // Strips: pinned, a divider, then the rest
        let pinned: Vec<&Channel> = list
            .iter()
            .filter(|c| c.group == OrderGroup::Pinned)
            .collect();
        let normal: Vec<&Channel> = list
            .iter()
            .filter(|c| c.group == OrderGroup::Default)
            .collect();
        let mut strips = row![].spacing(STRIP_SPACING).height(Length::Fill);
        for c in &pinned {
            strips = strips.push(self.strip(c));
        }
        if !pinned.is_empty() && !normal.is_empty() {
            strips = strips.push(
                container(rule::vertical(2).style(|_| rule::Style {
                    color: style::with_alpha(TEXT_DIM, 0.45),
                    radius: 1.0.into(),
                    fill_mode: rule::FillMode::Full,
                    snap: true,
                }))
                .width(DIVIDER_WIDTH)
                .center_x(DIVIDER_WIDTH)
                .padding(Padding::from([10, 0]))
                .height(Length::Fill),
            );
        }
        for c in &normal {
            strips = strips.push(self.strip(c));
        }
        if pinned.is_empty() && normal.is_empty() {
            strips = strips.push(
                container(
                    text(format!("No {label} yet. Use + to add one.").to_lowercase())
                        .color(TEXT_DIM),
                )
                .center_y(Length::Fill)
                .width(STRIP_WIDTH * 1.5),
            );
        }

        // The horizontal scrollable offers unbounded width, so a filler in the
        // header would never resolve. Size the header to the strips instead.
        let count = pinned.len() + normal.len();
        let mut width = if count == 0 {
            STRIP_WIDTH * 1.5
        } else {
            count as f32 * STRIP_WIDTH + (count - 1) as f32 * STRIP_SPACING
        };
        if !pinned.is_empty() && !normal.is_empty() {
            width += DIVIDER_WIDTH + STRIP_SPACING;
        }
        let header = header.width(width);

        container(column![header, strips].spacing(10))
            .padding(10)
            .height(Length::Fill)
            .style(style::panel)
            .into()
    }

    fn strip<'a>(&'a self, c: &'a Channel) -> El<'a> {
        let colour = style::colour(c.colour);

        // Header: name and menu
        let menu_button = {
            let b = button(
                container(style::icon("ellipsis-vertical", 12.0, TEXT))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill),
            )
            .width(22)
            .height(22)
            .padding(0)
            .on_press(MixerMessage::OpenPopup(Popup::Channel(c.id.clone())));
            if c.needs_device() {
                b.style(style::outlined(WARN))
            } else {
                b.style(style::control(false, ACCENT))
            }
        };
        let menu_popup = Popover::new(
            menu_button,
            self.channel_menu(c),
            self.popup == Some(Popup::Channel(c.id.clone())),
        )
        .on_close(MixerMessage::ClosePopup);

        let header = row![
            text(clip(&c.name, 15))
                .size(14)
                .color(Color::WHITE)
                .font(style::BOLD)
                .width(Length::Fill),
            menu_popup
        ]
        .align_y(Alignment::Center);

        // What feeds this channel
        let (sub, sub_colour) = if c.needs_device() {
            ("No device attached".to_string(), WARN)
        } else if c.kind.is_physical() {
            let entries = self.model.attachable(c);
            let names: Vec<String> = entries
                .iter()
                .filter(|e| e.config_index.is_some())
                .map(|e| e.label.clone())
                .collect();
            let missing = entries
                .iter()
                .any(|e| e.config_index.is_some() && e.node_id.is_none());
            (names.join(", "), if missing { WARN } else { TEXT_DIM })
        } else {
            let apps = self.model.apps_on(c);
            if apps.is_empty() {
                ("Virtual".to_string(), TEXT_DIM)
            } else {
                (apps.join(", "), TEXT_DIM)
            }
        };

        let meter = self.meters.get(&c.id).copied().unwrap_or(0);
        let faders: El<'a> = if c.is_source() {
            row![
                self.fader(c, Some(Mix::A), MIX_A, meter),
                self.link_button(c),
                self.fader(c, Some(Mix::B), MIX_B, meter),
            ]
            .spacing(4)
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .into()
        } else {
            let mix_colour = style::mix_colour(c.mix);
            row![
                Space::new().width(Length::Fill),
                self.fader(c, None, mix_colour, meter),
                Space::new().width(Length::Fill),
            ]
            .height(Length::Fill)
            .into()
        };

        let mut body = column![
            container(Space::new().width(Length::Fill).height(3)).style(style::colour_bar(colour)),
            header,
            text(clip(&sub, 22)).size(11).color(sub_colour),
            faders,
        ]
        .spacing(6)
        .height(Length::Fill);

        if c.is_source() {
            body = body
                .push(self.mute_row(c, MuteTarget::A))
                .push(self.mute_row(c, MuteTarget::B));
        } else {
            body = body.push(self.mix_toggle(c)).push(self.target_mute(c));
        }

        container(body)
            .width(STRIP_WIDTH)
            .height(Length::Fill)
            .padding(Padding {
                top: 6.0,
                right: 8.0,
                bottom: 8.0,
                left: 8.0,
            })
            .style(style::strip(colour))
            .into()
    }

    fn fader<'a>(&'a self, c: &'a Channel, mix: Option<Mix>, colour: Color, meter: u8) -> El<'a> {
        let actual = match mix {
            Some(Mix::B) => c.volume_b,
            _ => c.volume_a,
        };
        let value = self.value(&volume_key(&c.id, mix), actual as f32).round() as u8;
        let id = c.id.clone();
        let id_release = c.id.clone();
        let slider = vertical_slider(0..=100u8, value, move |v| {
            MixerMessage::Volume(id.clone(), mix, v)
        })
        .on_release(MixerMessage::VolumeReleased(id_release, mix))
        .width(22)
        .height(Length::Fill)
        .style(style::fader(colour));

        let caption = match mix {
            Some(Mix::A) => "A",
            Some(Mix::B) => "B",
            None => match c.mix {
                Mix::A => "Mix A",
                Mix::B => "Mix B",
            },
        };

        column![
            text(caption).size(10).color(colour),
            row![slider, meter_bar(meter, colour)]
                .spacing(4)
                .height(Length::Fill),
            text(format!("{value}%")).size(12).color(TEXT),
        ]
        .spacing(4)
        .align_x(Alignment::Center)
        .height(Length::Fill)
        .into()
    }

    fn link_button<'a>(&'a self, c: &'a Channel) -> El<'a> {
        let (icon, tint, tip) = if c.linked {
            ("link", Color::WHITE, "Mix A and Mix B move together")
        } else {
            ("link-slash", TEXT_DIM, "Mix A and Mix B are independent")
        };
        tooltip(
            button(
                container(style::icon(icon, 12.0, tint))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill),
            )
            .width(24)
            .height(24)
            .padding(0)
            .style(style::control(false, ACCENT))
            .on_press(MixerMessage::ToggleLink(c.id.clone())),
            container(text(tip).size(11)).padding(6).style(style::sheet),
            tooltip::Position::Top,
        )
        .into()
    }

    fn mute_label(&self, c: &Channel, target: MuteTarget) -> String {
        let targets = c.mute_targets(target);
        match targets.len() {
            0 => "Mute All".to_string(),
            1 => format!(
                "Mute to {}",
                self.model.channel_name(&targets[0]).unwrap_or("?")
            ),
            n => format!("Mute to {n} outputs"),
        }
    }

    fn mute_row<'a>(&'a self, c: &'a Channel, target: MuteTarget) -> El<'a> {
        let active = c.is_muted(target);
        let label = self.mute_label(c, target);
        let icon = if active {
            "volume-xmark"
        } else {
            "volume-high"
        };
        let tip = match target {
            MuteTarget::A => "Mute A: pressing this channel's dial on the Mix Create",
            MuteTarget::B => "Mute B: the button under this channel on the Mix Create",
        };

        let main = tooltip(
            button(
                container(
                    row![
                        style::icon(icon, 11.0, if active { Color::WHITE } else { TEXT }),
                        text(clip(&label, 14))
                            .size(11)
                            .wrapping(text::Wrapping::None)
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .center_y(Length::Fill),
            )
            .width(Length::Fill)
            .height(26)
            .padding(Padding::from([0, 8]))
            .style(style::control(active, MUTE))
            .on_press(MixerMessage::ToggleMute(c.id.clone(), target)),
            container(text(tip).size(11)).padding(6).style(style::sheet),
            tooltip::Position::Top,
        );

        let chooser = Popover::new(
            button(
                container(style::icon("angle-down", 10.0, TEXT))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill),
            )
            .width(22)
            .height(26)
            .padding(0)
            .style(style::control(false, ACCENT))
            .on_press(MixerMessage::OpenPopup(Popup::Mute(c.id.clone(), target))),
            self.mute_menu(c, target),
            self.popup == Some(Popup::Mute(c.id.clone(), target)),
        )
        .on_close(MixerMessage::ClosePopup);

        row![main, chooser].spacing(3).into()
    }

    fn mute_menu<'a>(&'a self, c: &'a Channel, target: MuteTarget) -> El<'a> {
        let targets = c.mute_targets(target);
        let mut items = vec![
            menu_heading(match target {
                MuteTarget::A => "Mute A mutes this channel in",
                MuteTarget::B => "Mute B mutes this channel in",
            }),
            menu_entry(
                Some(targets.is_empty()),
                "All outputs (Mute All)".into(),
                TEXT,
                Some(MixerMessage::MuteToAll(c.id.clone(), target)),
            ),
            menu_rule(),
        ];
        for t in &self.model.targets {
            items.push(menu_entry(
                Some(targets.contains(&t.id)),
                t.name.clone(),
                TEXT,
                Some(MixerMessage::ToggleMuteTargetNode(
                    c.id.clone(),
                    target,
                    t.id.clone(),
                )),
            ));
        }
        menu(items, 230.0)
    }

    fn mix_toggle<'a>(&'a self, c: &'a Channel) -> El<'a> {
        let side = |mix: Mix, label: &'static str| {
            let active = c.mix == mix;
            button(text(label).size(11).center().width(Length::Fill))
                .width(Length::Fill)
                .height(26)
                .style(style::control(active, style::mix_colour(mix)))
                .on_press_maybe((!active).then(|| MixerMessage::SetTargetMix(c.id.clone(), mix)))
        };
        tooltip(
            row![side(Mix::A, "Mix A"), side(Mix::B, "Mix B")].spacing(3),
            container(text("Which mix this output carries").size(11))
                .padding(6)
                .style(style::sheet),
            tooltip::Position::Top,
        )
        .into()
    }

    fn target_mute<'a>(&'a self, c: &'a Channel) -> El<'a> {
        let active = c.mute_a;
        button(
            container(
                row![
                    style::icon(
                        if active {
                            "volume-xmark"
                        } else {
                            "volume-high"
                        },
                        11.0,
                        if active { Color::WHITE } else { TEXT }
                    ),
                    text(if active { "Muted" } else { "Mute" }).size(11)
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .center_y(Length::Fill),
        )
        .width(Length::Fill)
        .height(26)
        .padding(Padding::from([0, 8]))
        .style(style::control(active, MUTE))
        .on_press(MixerMessage::ToggleMute(c.id.clone(), MuteTarget::A))
        .into()
    }

    fn channel_menu<'a>(&'a self, c: &'a Channel) -> El<'a> {
        let id = c.id.clone();
        let mut items: Vec<El<'a>> = vec![];

        if c.kind.is_physical() {
            items.push(menu_heading(if c.is_source() {
                "Input devices"
            } else {
                "Output devices"
            }));
            let entries = self.model.attachable(c);
            if entries.is_empty() {
                items.push(menu_entry(None, "No devices found".into(), TEXT_DIM, None));
            }
            for e in entries {
                let label = if e.node_id.is_none() {
                    format!("{} (not connected)", clip(&e.label, 26))
                } else {
                    clip(&e.label, 34)
                };
                let message = match (e.config_index, e.node_id) {
                    (Some(index), _) => Some(MixerMessage::Detach(id.clone(), index)),
                    (None, Some(node)) => Some(MixerMessage::Attach(id.clone(), node)),
                    _ => None,
                };
                let colour = if e.node_id.is_none() { WARN } else { TEXT };
                items.push(menu_entry(
                    Some(e.config_index.is_some()),
                    label,
                    colour,
                    message,
                ));
            }
            items.push(menu_rule());
        }

        let effects = if c.filters.is_empty() {
            "Effects…".to_string()
        } else {
            format!("Effects ({})…", c.filters.len())
        };
        items.push(menu_entry(
            None,
            effects,
            TEXT,
            Some(MixerMessage::OpenModal(Modal::Filters { id: id.clone() })),
        ));
        items.push(menu_entry(
            None,
            "Rename…".into(),
            TEXT,
            Some(MixerMessage::OpenModal(Modal::Rename {
                id: id.clone(),
                value: c.name.clone(),
            })),
        ));
        items.push(menu_entry(
            None,
            "Colour…".into(),
            TEXT,
            Some(MixerMessage::OpenModal(Modal::Colour {
                id: id.clone(),
                colour: c.colour,
            })),
        ));
        items.push(menu_rule());
        // Moving happens within the channel's own group (pinned, normal or hidden)
        let list = if c.is_source() {
            &self.model.sources
        } else {
            &self.model.targets
        };
        let group: Vec<&str> = list
            .iter()
            .filter(|o| o.group == c.group)
            .map(|o| o.id.as_str())
            .collect();
        let position = group.iter().position(|o| *o == c.id).unwrap_or(0);
        let can_left = position > 0;
        let can_right = position + 1 < group.len();
        items.push(menu_entry(
            None,
            "Move left".into(),
            if can_left { TEXT } else { TEXT_DIM },
            can_left.then(|| MixerMessage::Move(id.clone(), -1)),
        ));
        items.push(menu_entry(
            None,
            "Move right".into(),
            if can_right { TEXT } else { TEXT_DIM },
            can_right.then(|| MixerMessage::Move(id.clone(), 1)),
        ));
        let (pin_label, pin_group) = if c.group == OrderGroup::Pinned {
            ("Unpin", OrderGroup::Default)
        } else {
            ("Pin to the left", OrderGroup::Pinned)
        };
        items.push(menu_entry(
            None,
            pin_label.into(),
            TEXT,
            Some(MixerMessage::SetOrderGroup(id.clone(), pin_group)),
        ));
        items.push(menu_entry(
            None,
            "Hide".into(),
            TEXT,
            Some(MixerMessage::SetOrderGroup(id.clone(), OrderGroup::Hidden)),
        ));
        items.push(menu_rule());
        items.push(menu_entry(
            None,
            "Remove…".into(),
            MUTE,
            Some(MixerMessage::Remove(id)),
        ));
        menu(items, 250.0)
    }
}

/// A thin level meter that fills from the bottom
fn meter_bar<'a>(level: u8, colour: Color) -> Element<'a, MixerMessage> {
    let level = level.min(100) as u16;
    let lit: Element<'a, MixerMessage> = container(Space::new().width(4).height(Length::Fill))
        .style(style::fill(style::with_alpha(colour, 0.9), 2.0))
        .height(Length::FillPortion(level.max(1)))
        .into();
    let dark: Element<'a, MixerMessage> = Space::new()
        .width(4)
        .height(Length::FillPortion((100 - level).max(1)))
        .into();
    let inner: Element<'a, MixerMessage> = match level {
        0 => Space::new().width(4).height(Length::Fill).into(),
        100 => container(Space::new().width(4).height(Length::Fill))
            .style(style::fill(colour, 2.0))
            .into(),
        _ => column![dark, lit].into(),
    };
    container(inner)
        .width(4)
        .height(Length::Fill)
        .padding(Padding::from([7, 0]))
        .style(style::fill(style::RAIL, 2.0))
        .into()
}
