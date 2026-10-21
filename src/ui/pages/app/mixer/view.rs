//! Page frame: tab bar, connection state, error toast and dialogs.

use crate::ui::pages::app::mixer::model::{Colour, NodeKind};
use crate::ui::pages::app::mixer::style::{self, ACCENT, MUTE, TEXT, TEXT_DIM};
use crate::ui::pages::app::mixer::{MixerMessage, MixerPage, Modal, Tab};
use iced::widget::{
    Space, button, center, column, container, mouse_area, opaque, row, stack, text, text_input,
};
use iced::{Alignment, Color, Element, Length, Padding};
use iced_color_picker::{ColorPicker, Hsv, Spectrum};
use std::fmt::{Display, Formatter};

pub(super) type El<'a> = Element<'a, MixerMessage>;

/// An option in a pick list: `id: None` is the "Default" / "none" choice
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Choice {
    pub id: Option<String>,
    pub label: String,
}

impl Display for Choice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

/// Shortens a label to `max` characters with an ellipsis
pub(super) fn clip(label: &str, max: usize) -> String {
    if label.chars().count() <= max {
        label.to_string()
    } else {
        let mut s: String = label.chars().take(max.saturating_sub(1)).collect();
        s.push('…');
        s
    }
}

pub(super) fn menu_entry<'a>(
    check: Option<bool>,
    label: String,
    colour: Color,
    message: Option<MixerMessage>,
) -> El<'a> {
    let mut content = row![].spacing(8).align_y(Alignment::Center);
    if let Some(checked) = check {
        content = content.push(
            container(if checked {
                style::icon("check", 11.0, style::MIX_A)
            } else {
                Space::new().into()
            })
            .width(14),
        );
    }
    content = content.push(text(label).color(colour));
    button(content)
        .width(Length::Fill)
        .padding(Padding::from([5, 10]))
        .style(style::menu_item(false))
        .on_press_maybe(message)
        .into()
}

pub(super) fn menu_heading<'a>(label: &str) -> El<'a> {
    container(text(label.to_uppercase()).size(10).color(TEXT_DIM))
        .padding(Padding::from([4, 10]))
        .into()
}

pub(super) fn menu_rule<'a>() -> El<'a> {
    container(Space::new().height(1).width(Length::Fill))
        .padding(Padding::from([3, 6]))
        .style(|_| container::Style::default())
        .into()
}

pub(super) fn menu<'a>(items: Vec<El<'a>>, width: f32) -> El<'a> {
    container(column(items).spacing(1))
        .padding(4)
        .width(width)
        .style(style::sheet)
        .into()
}

impl MixerPage {
    pub fn view(&self) -> El<'_> {
        if !self.connected {
            return self.view_waiting();
        }

        let tabs = [
            (Tab::Mixer, "Mixer"),
            (Tab::Routing, "Routing"),
            (Tab::Applications, "Applications"),
            (Tab::Devices, "Devices"),
            (Tab::Settings, "Settings"),
        ];
        let mut bar = row![].spacing(6).align_y(Alignment::Center);
        for (tab, label) in tabs {
            bar = bar.push(
                button(text(label).size(13))
                    .padding(Padding::from([6, 14]))
                    .style(style::tab(self.tab == tab))
                    .on_press(MixerMessage::Tab(tab)),
            );
        }
        bar = bar.push(Space::new().width(Length::Fill)).push(
            row![
                container(Space::new().width(8).height(8)).style(style::fill(style::MIX_A, 4.0)),
                text("Audio engine running").size(11).color(TEXT_DIM)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );

        let content = match self.tab {
            Tab::Mixer => self.view_mixer(),
            Tab::Routing => self.view_routing(),
            Tab::Applications => self.view_apps(),
            Tab::Devices => self.view_devices(),
            Tab::Settings => self.view_settings(),
        };

        let mut page = column![
            container(bar).padding(Padding {
                top: 10.0,
                right: 14.0,
                bottom: 8.0,
                left: 12.0
            }),
            container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(Padding {
                    top: 0.0,
                    right: 12.0,
                    bottom: 10.0,
                    left: 12.0
                })
        ];

        if let Some((error, _)) = &self.error {
            page = page.push(
                container(
                    row![
                        style::icon("xmark", 12.0, Color::WHITE),
                        text(error.clone()).color(Color::WHITE).width(Length::Fill),
                        button(text("Dismiss").size(11))
                            .padding(Padding::from([3, 10]))
                            .style(style::control(false, ACCENT))
                            .on_press(MixerMessage::DismissError),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding(Padding::from([6, 14]))
                .width(Length::Fill)
                .style(style::fill(style::with_alpha(MUTE, 0.85), 0.0)),
            );
        }

        let page: El<'_> = page.into();
        match &self.modal {
            None => page,
            Some(modal) => stack![page, self.view_modal(modal)].into(),
        }
    }

    fn view_waiting(&self) -> El<'_> {
        center(
            column![
                style::icon("sliders", 40.0, TEXT_DIM),
                text("Starting the audio mixer…").size(20).color(TEXT),
                text("Waiting for the audio engine. This page connects on its own as soon as it's running.")
                    .color(TEXT_DIM),
            ]
            .spacing(12)
            .align_x(Alignment::Center),
        )
        .into()
    }

    fn view_modal<'a>(&'a self, modal: &'a Modal) -> El<'a> {
        let (title, body, ok): (String, El<'a>, Option<(&str, bool, bool)>) = match modal {
            Modal::Rename { id, value } => {
                let current = self.model.channel_name(id);
                let error = (!value.is_empty())
                    .then(|| self.model.name_error(value, current))
                    .flatten();
                (
                    format!("Rename {}", current.unwrap_or("channel")),
                    self.name_body(value, error),
                    Some(("Rename", self.model.name_error(value, current).is_none(), false)),
                )
            }
            Modal::Create { kind, value } => {
                let error = (!value.is_empty()).then(|| self.model.name_error(value, None)).flatten();
                let what = match kind {
                    NodeKind::PhysicalSource => "physical source",
                    NodeKind::VirtualSource => "virtual source",
                    NodeKind::PhysicalTarget => "physical target",
                    NodeKind::VirtualTarget => "virtual target",
                };
                (
                    format!("Add a {what}"),
                    self.name_body(value, error),
                    Some(("Add", self.model.name_error(value, None).is_none(), false)),
                )
            }
            Modal::Remove(id) => (
                format!("Remove {}?", self.model.channel_name(id).unwrap_or("channel")),
                text("The channel, its routes and its effects are removed from the mixer.")
                    .color(TEXT)
                    .into(),
                Some(("Remove", true, true)),
            ),
            Modal::ResetAudio => (
                "Restart the audio engine?".into(),
                column![
                    text("All mixer channels are disconnected and destroyed, then recreated after about 2 seconds. Audio drops out while this happens.").color(TEXT),
                    text("Use this only if something is stuck and nothing else helps.").color(TEXT_DIM),
                ]
                .spacing(8)
                .into(),
                Some(("Restart", true, true)),
            ),
            Modal::Colour { id, colour } => (
                format!("Colour for {}", self.model.channel_name(id).unwrap_or("channel")),
                colour_body(*colour),
                None,
            ),
            Modal::Filters { id } => {
                return self.backdrop(self.view_filters(id));
            }
        };

        let mut buttons = row![Space::new().width(Length::Fill)].spacing(8);
        match ok {
            Some((label, enabled, danger)) => {
                buttons = buttons
                    .push(
                        button(text("Cancel"))
                            .padding(Padding::from([5, 16]))
                            .style(style::control(false, ACCENT))
                            .on_press(MixerMessage::CloseModal),
                    )
                    .push(
                        button(text(label))
                            .padding(Padding::from([5, 16]))
                            .style(style::control(true, if danger { MUTE } else { ACCENT }))
                            .on_press_maybe(enabled.then_some(MixerMessage::ModalSubmit)),
                    );
            }
            None => {
                buttons = buttons.push(
                    button(text("Done"))
                        .padding(Padding::from([5, 16]))
                        .style(style::control(true, ACCENT))
                        .on_press(MixerMessage::CloseModal),
                );
            }
        }

        let dialog =
            container(column![text(title).size(16).color(Color::WHITE), body, buttons].spacing(14))
                .padding(18)
                .width(380)
                .style(style::sheet);

        self.backdrop(dialog.into())
    }

    fn backdrop<'a>(&'a self, dialog: El<'a>) -> El<'a> {
        opaque(
            mouse_area(center(opaque(dialog)).style(style::backdrop))
                .on_press(MixerMessage::CloseModal),
        )
    }

    fn name_body<'a>(&'a self, value: &'a str, error: Option<&'static str>) -> El<'a> {
        let mut body = column![
            text("Name").size(11).color(TEXT_DIM),
            text_input("Channel name", value)
                .id(super::MODAL_INPUT)
                .on_input(MixerMessage::ModalInput)
                .on_submit(MixerMessage::ModalSubmit)
                .padding(6),
        ]
        .spacing(6);
        body = body.push(
            text(error.unwrap_or("Up to 20 letters, numbers, spaces, - and _."))
                .size(11)
                .color(if error.is_some() { MUTE } else { TEXT_DIM }),
        );
        body.into()
    }
}

fn colour_body<'a>(colour: Colour) -> El<'a> {
    let c = style::colour(colour);
    let hsv = Hsv::from(c);
    let to_colour = |hsv: Hsv| {
        let [red, green, blue, _] = hsv.to_rgba8();
        Colour { red, green, blue }
    };
    let presets: [(u8, u8, u8); 10] = [
        (0x59, 0xB1, 0xB6),
        (0xF4, 0x7C, 0x24),
        (0xD9, 0x3A, 0x3A),
        (0xF0, 0xB4, 0x29),
        (0x6C, 0xC0, 0x4A),
        (0x3D, 0x8B, 0xFD),
        (0x9B, 0x59, 0xD0),
        (0xE8, 0x5C, 0xA8),
        (0xC8, 0xC8, 0xC8),
        (0x2F, 0x18, 0x47),
    ];
    let mut swatches = row![].spacing(6);
    for (r, g, b) in presets {
        let colour = Colour {
            red: r,
            green: g,
            blue: b,
        };
        swatches = swatches.push(
            button(Space::new().width(Length::Fill).height(Length::Fill))
                .width(26)
                .height(20)
                .style(move |_, status| button::Style {
                    background: Some(Color::from_rgb8(r, g, b).into()),
                    border: iced::Border {
                        radius: 3.0.into(),
                        width: if matches!(status, button::Status::Hovered) {
                            2.0
                        } else {
                            1.0
                        },
                        color: Color::from_rgb8(0x60, 0x60, 0x60),
                    },
                    ..Default::default()
                })
                .on_press(MixerMessage::ColourChanged(colour)),
        );
    }
    column![
        row![
            container(Space::new().width(Length::Fill).height(18))
                .width(Length::Fill)
                .style(style::fill(c, 3.0)),
            text(format!(
                "#{:02X}{:02X}{:02X}",
                colour.red, colour.green, colour.blue
            ))
            .color(TEXT_DIM)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        ColorPicker::new(hsv, move |v| MixerMessage::ColourChanged(to_colour(v)))
            .width(Length::Fill)
            .height(170),
        ColorPicker::new(hsv, move |v| MixerMessage::ColourChanged(to_colour(v)))
            .spectrum(Spectrum::HueHorizontal)
            .width(Length::Fill)
            .height(16),
        swatches,
    ]
    .spacing(10)
    .into()
}
