//! The Devices tab: the system's physical outputs and inputs.

use crate::ui::pages::app::mixer::model::PhysicalDevice;
use crate::ui::pages::app::mixer::style::{self, MIX_A, MIX_B, MUTE, TEXT, TEXT_DIM};
use crate::ui::pages::app::mixer::view::{El, clip};
use crate::ui::pages::app::mixer::{MixerMessage, MixerPage};
use iced::widget::{button, column, container, row, scrollable, slider, text};
use iced::{Alignment, Color, Length};

impl MixerPage {
    pub(super) fn view_devices(&self) -> El<'_> {
        row![
            self.device_list(false, "Outputs", MIX_B),
            self.device_list(true, "Inputs", MIX_A),
        ]
        .spacing(12)
        .height(Length::Fill)
        .into()
    }

    fn device_list(&self, source: bool, title: &str, colour: Color) -> El<'_> {
        let devices = self.model.physical_devices(source);
        let mut list = column![].spacing(6);
        if devices.is_empty() {
            list = list.push(text("No devices found.").color(TEXT_DIM));
        }
        for d in devices {
            list = list.push(self.device_row(d, source, colour));
        }
        container(
            column![
                style::title(title, colour),
                scrollable(container(list).padding(iced::Padding {
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

    /// Mixer channels a physical device is attached to
    fn used_by(&self, d: &PhysicalDevice, source: bool) -> Vec<String> {
        let list = if source {
            &self.model.sources
        } else {
            &self.model.targets
        };
        list.iter()
            .filter(|c| {
                c.attached.iter().any(|a| match &a.name {
                    Some(name) => d.name.as_ref() == Some(name),
                    None => a.description == d.description,
                })
            })
            .map(|c| c.name.clone())
            .collect()
    }

    fn device_row<'a>(&'a self, d: &'a PhysicalDevice, source: bool, colour: Color) -> El<'a> {
        let id = d.id.clone();
        let id_release = d.id.clone();
        let key = format!("phys:{}", d.id);
        let volume = self.value(&key, d.volume as f32).round() as u8;
        let used = self.used_by(d, source);
        let subtitle = if used.is_empty() {
            "Not used by the mixer".to_string()
        } else {
            format!("Mixer channel: {}", used.join(", "))
        };

        container(
            row![
                column![
                    text(clip(&d.label(), 44)).color(TEXT),
                    text(subtitle).size(10).color(TEXT_DIM),
                ]
                .width(Length::Fill),
                slider(0..=100u8, volume, move |v| MixerMessage::PhysicalVolume(
                    id.clone(),
                    v
                ))
                .on_release(MixerMessage::PhysicalVolumeReleased(id_release))
                .width(180)
                .style(style::hslider(colour)),
                text(format!("{volume}%"))
                    .size(11)
                    .color(TEXT_DIM)
                    .width(36),
                button(
                    container(style::icon(
                        if d.muted {
                            "volume-xmark"
                        } else {
                            "volume-high"
                        },
                        11.0,
                        if d.muted { Color::WHITE } else { TEXT }
                    ))
                    .center(Length::Fill)
                )
                .width(28)
                .height(24)
                .padding(0)
                .style(style::control(d.muted, MUTE))
                .on_press(MixerMessage::PhysicalMute(d.id.clone(), !d.muted)),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding(8)
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
}
