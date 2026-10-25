//! The Settings tab: audio buffer size, engine restart, and the fallback window.

use crate::ui::pages::app::mixer::model::QUANTUMS;
use crate::ui::pages::app::mixer::style::{self, ACCENT, MIX_A, MUTE, TEXT, TEXT_DIM};
use crate::ui::pages::app::mixer::view::{Choice, El};
use crate::ui::pages::app::mixer::{MixerMessage, MixerPage};
use iced::widget::{Space, button, column, container, pick_list, row, scrollable, text};
use iced::{Alignment, Length, Padding};

impl MixerPage {
    pub(super) fn view_settings(&self) -> El<'_> {
        let current = self.model.status.audio.profile.audio_node_quantum.clone();
        let choices: Vec<Choice> = std::iter::once(Choice {
            id: None,
            label: "PipeWire default".into(),
        })
        .chain(QUANTUMS.iter().map(|q| Choice {
            id: Some(format!("Quantum{q}")),
            label: format!("{q} samples"),
        }))
        .collect();
        let selected = choices.iter().find(|c| c.id == current).cloned();

        let buffer = section(
            "Audio buffer size",
            column![
                text("The buffer size PipeWire uses when processing the mixer's audio. Leave it on \"PipeWire default\" unless you need manual control and understand the effect on latency and CPU use.").color(TEXT),
                text("Changing it restarts the audio engine.").size(11).color(TEXT_DIM),
                pick_list(choices, selected, |c: Choice| {
                    MixerMessage::SetQuantum(c.id.and_then(|q| q.trim_start_matches("Quantum").parse().ok()))
                })
                .width(220)
                .padding(Padding::from([4, 8])),
            ]
            .spacing(8)
            .into(),
        );

        let restart = section(
            "Restart audio engine",
            column![
                text("Disconnects and destroys all mixer channels, then recreates them after about 2 seconds. Use this only if something is stuck and nothing else helps.").color(TEXT),
                button(text("Restart audio engine…"))
                    .padding(Padding::from([5, 14]))
                    .style(style::control(false, MUTE))
                    .on_press(MixerMessage::ResetAudio),
            ]
            .spacing(8)
            .into(),
        );

        let web = section(
            "Web mixer",
            column![
                text("Opens the previous mixer window. Its effect panels have graphs (equaliser curve, compressor) that this page shows as plain controls.").color(TEXT),
                button(text("Open web mixer"))
                    .padding(Padding::from([5, 14]))
                    .style(style::control(false, ACCENT))
                    .on_press(MixerMessage::OpenWebMixer),
            ]
            .spacing(8)
            .into(),
        );

        let engine = section(
            "Audio engine",
            row![
                container(Space::new().width(8).height(8)).style(style::fill(MIX_A, 4.0)),
                text("Connected. When Beacn runs as beacn.service it starts and stops the engine itself.").color(TEXT),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        );

        scrollable(
            column![engine, buffer, restart, web]
                .spacing(12)
                .padding(Padding {
                    top: 0.0,
                    right: 12.0,
                    bottom: 0.0,
                    left: 0.0,
                }),
        )
        .height(Length::Fill)
        .into()
    }
}

fn section<'a>(title: &str, body: El<'a>) -> El<'a> {
    container(column![style::title(title, TEXT_DIM), body].spacing(10))
        .padding(12)
        .width(Length::Fill)
        .style(style::panel)
        .into()
}
