//! The Effects dialog: a channel's filter chain with a generic parameter editor.

use crate::ui::pages::app::mixer::model::{
    FilterConfig, FilterProperty, LV2_FILTERS, PWV_FILTERS, filter_display_name,
    filter_enable_symbol, filter_value_f32, filter_value_kind,
};
use crate::ui::pages::app::mixer::style::{self, ACCENT, MIX_A, MUTE, TEXT, TEXT_DIM, WARN};
use crate::ui::pages::app::mixer::view::{Choice, El, clip};
use crate::ui::pages::app::mixer::{MixerMessage, MixerPage};
use iced::widget::{
    Space, button, column, container, pick_list, row, rule, scrollable, slider, text, text_input,
    toggler,
};
use iced::{Alignment, Color, Length, Padding};
use serde_json::{Value, json};

impl MixerPage {
    pub(super) fn view_filters<'a>(&'a self, id: &'a str) -> El<'a> {
        let Some(channel) = self.model.channel(id) else {
            return Space::new().into();
        };
        let configs = &self.model.status.audio.filter_config;

        // Left: the chain
        let mut chain = column![].spacing(4);
        if channel.filters.is_empty() {
            chain = chain.push(text("No effects on this channel.").color(TEXT_DIM));
        }
        let count = channel.filters.len();
        for (index, f) in channel.filters.iter().enumerate() {
            let config = configs.get(&f.id);
            let name = config
                .map(|c| filter_display_name(&c.identifier, &c.name))
                .unwrap_or_else(|| "Effect".into());
            let state = match config {
                None => ("Loading", TEXT_DIM),
                Some(c) => match state_text(&c.state) {
                    ("Running", _) if !filter_enabled(c, f.is_lv2()) => ("Off", TEXT_DIM),
                    other => other,
                },
            };
            let selected = self.selected_filter.as_deref() == Some(f.id.as_str());

            let small = |icon: &str, message: Option<MixerMessage>| {
                button(container(style::icon(icon, 9.0, TEXT)).center(Length::Fill))
                    .width(20)
                    .height(20)
                    .padding(0)
                    .style(style::control(false, ACCENT))
                    .on_press_maybe(message)
            };
            let row_content = row![
                column![
                    text(clip(&name, 20)).color(Color::WHITE),
                    text(state.0).size(10).color(state.1),
                ]
                .width(Length::Fill),
                small(
                    "angle-up",
                    (index > 0).then(|| MixerMessage::MoveFilter(f.id.clone(), index - 1)),
                ),
                small(
                    "angle-down",
                    (index + 1 < count).then(|| MixerMessage::MoveFilter(f.id.clone(), index + 1)),
                ),
                small("trash", Some(MixerMessage::RemoveFilter(f.id.clone()))),
            ]
            .spacing(4)
            .align_y(Alignment::Center);

            chain = chain.push(
                button(row_content)
                    .width(Length::Fill)
                    .padding(Padding::from([5, 8]))
                    .style(style::menu_item(selected))
                    .on_press(MixerMessage::SelectFilter(f.id.clone())),
            );
        }

        let mut add: Vec<AddChoice> = PWV_FILTERS
            .iter()
            .map(|(key, name)| AddChoice {
                lv2: false,
                key,
                name,
            })
            .chain(LV2_FILTERS.iter().map(|(key, name)| AddChoice {
                lv2: true,
                key,
                name,
            }))
            .collect();
        add.sort_by_key(|a| a.name);
        let node = id.to_string();
        let add = pick_list(add, None::<AddChoice>, move |a: AddChoice| {
            MixerMessage::AddFilter(node.clone(), a.lv2, a.key.to_string())
        })
        .placeholder("Add an effect…")
        .width(Length::Fill)
        .padding(Padding::from([4, 8]));

        let left = column![
            scrollable(chain).height(Length::Fill),
            add,
            text("Effects run top to bottom.").size(10).color(TEXT_DIM),
        ]
        .spacing(8)
        .width(250);

        // Right: parameters
        let right: El<'a> = match self
            .selected_filter
            .as_ref()
            .filter(|f| channel.filters.iter().any(|c| &c.id == *f))
            .and_then(|f| configs.get(f).map(|c| (f, c)))
        {
            None => container(text("Select an effect to edit it.").color(TEXT_DIM))
                .center(Length::Fill)
                .into(),
            Some((fid, config)) => {
                let lv2 = channel
                    .filters
                    .iter()
                    .find(|f| &f.id == fid)
                    .is_some_and(|f| f.is_lv2());
                let settable = |p: &&FilterProperty| p.is_input || !lv2;
                let (enable_symbol, inverted) = filter_enable_symbol(&config.identifier);
                let enable = config
                    .parameters
                    .iter()
                    .filter(settable)
                    .find(|p| p.symbol == enable_symbol);
                let mut head = row![
                    text(filter_display_name(&config.identifier, &config.name))
                        .size(15)
                        .color(Color::WHITE)
                        .width(Length::Fill)
                ]
                .spacing(10)
                .align_y(Alignment::Center);
                if let Some(p) = enable {
                    let on = filter_value_f32(&p.value).unwrap_or(0.0) > 0.5;
                    let on = on != inverted;
                    let (fid, pid) = (fid.clone(), p.id);
                    head = head.push(toggler(on).label("Enabled").on_toggle(move |v| {
                        MixerMessage::FilterValue(
                            fid.clone(),
                            pid,
                            json!({ "Bool": v != inverted }),
                        )
                    }));
                }

                let search = self.filter_search.to_lowercase();
                let mut params = column![].spacing(6);
                let mut shown = 0;
                for p in config
                    .parameters
                    .iter()
                    .filter(settable)
                    .filter(|p| p.symbol != enable_symbol)
                {
                    if !search.is_empty() && !p.name.to_lowercase().contains(&search) {
                        continue;
                    }
                    shown += 1;
                    params = params.push(self.parameter(fid, p));
                }
                if shown == 0 {
                    params = params.push(text("No adjustable settings.").color(TEXT_DIM));
                }

                let mut col = column![head].spacing(8);
                if let (_, Some(message)) = state_message(&config.state) {
                    col = col.push(text(message).size(11).color(WARN));
                }
                if config.parameters.len() > 12 {
                    col = col.push(
                        text_input("Filter settings by name", &self.filter_search)
                            .on_input(MixerMessage::FilterSearch)
                            .padding(5),
                    );
                }
                col.push(rule::horizontal(1))
                    .push(
                        scrollable(container(params).padding(Padding {
                            top: 0.0,
                            right: 12.0,
                            bottom: 0.0,
                            left: 0.0,
                        }))
                        .height(Length::Fill),
                    )
                    .into()
            }
        };

        let body = row![
            left,
            rule::vertical(1),
            container(right).width(Length::Fill)
        ]
        .spacing(14)
        .height(Length::Fill);

        container(
            column![
                row![
                    text(format!("Effects: {}", channel.name))
                        .size(16)
                        .color(Color::WHITE)
                        .width(Length::Fill),
                    button(text("Done"))
                        .padding(Padding::from([5, 16]))
                        .style(style::control(true, ACCENT))
                        .on_press(MixerMessage::CloseModal),
                ]
                .align_y(Alignment::Center),
                body,
            ]
            .spacing(12),
        )
        .padding(16)
        .width(820)
        .height(470)
        .style(style::sheet)
        .into()
    }

    fn parameter<'a>(&'a self, filter: &'a str, p: &'a FilterProperty) -> El<'a> {
        let fid = filter.to_string();
        let pid = p.id;
        let label = text(clip(&p.name, 34)).color(TEXT).width(230);
        let kind = filter_value_kind(&p.value);

        let control: El<'a> = if let Some(def) = &p.enum_def {
            let mut options: Vec<Choice> = def
                .iter()
                .map(|(k, v)| Choice {
                    id: Some(k.clone()),
                    label: v.clone(),
                })
                .collect();
            options.sort_by_key(|c| {
                c.id.as_ref()
                    .and_then(|k| k.parse::<i64>().ok())
                    .unwrap_or(0)
            });
            let current = filter_value_f32(&p.value).map(|v| (v as i64).to_string());
            let selected = options.iter().find(|c| c.id == current).cloned();
            pick_list(options, selected, move |c: Choice| {
                let v: i32 = c.id.and_then(|k| k.parse().ok()).unwrap_or(0);
                MixerMessage::FilterValue(fid.clone(), pid, json!({ "Int32": v }))
            })
            .width(220)
            .text_size(11)
            .padding(Padding::from([3, 8]))
            .into()
        } else if kind == "Bool" {
            let on = filter_value_f32(&p.value).unwrap_or(0.0) > 0.5;
            toggler(on)
                .on_toggle(move |v| {
                    MixerMessage::FilterValue(fid.clone(), pid, json!({ "Bool": v }))
                })
                .into()
        } else {
            let key = format!("filter:{filter}:{pid}");
            let (min, max) = if p.max > p.min {
                (p.min, p.max)
            } else {
                (0.0, 1.0)
            };
            let actual = filter_value_f32(&p.value).unwrap_or(min);
            let value = self.value(&key, actual).clamp(min, max);
            let int = kind == "Int32";
            let step = if int {
                1.0
            } else {
                ((max - min) / 200.0).max(0.0001)
            };
            let fid_release = fid.clone();
            let shown = if int {
                format!("{}", value.round() as i64)
            } else if (max - min) >= 100.0 {
                format!("{value:.1}")
            } else {
                format!("{value:.2}")
            };
            row![
                slider(min..=max, value, move |v: f32| {
                    let v: Value = if int {
                        json!({ "Int32": v.round() as i32 })
                    } else {
                        json!({ "Float32": v })
                    };
                    MixerMessage::FilterValue(fid.clone(), pid, v)
                })
                .step(step)
                .on_release(MixerMessage::FilterValueReleased(fid_release, pid))
                .width(220)
                .style(style::hslider(MIX_A)),
                text(shown).size(11).color(TEXT_DIM).width(64),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        };

        row![label, control]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AddChoice {
    lv2: bool,
    key: &'static str,
    name: &'static str,
}

impl std::fmt::Display for AddChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

fn state_text(state: &Value) -> (&'static str, Color) {
    match state_message(state).0 {
        "Running" => ("Running", MIX_A),
        "Stopped" => ("Stopped", TEXT_DIM),
        "NotFound" => ("Plugin not installed", WARN),
        "NotCompatible" => ("Not compatible", WARN),
        "FeatureMissing" => ("Not available in this build", WARN),
        "Error" => ("Error", MUTE),
        _ => ("Unknown", TEXT_DIM),
    }
}

/// Whether the effect's own on/off switch is on (effects without one are always on)
fn filter_enabled(config: &FilterConfig, lv2: bool) -> bool {
    let (symbol, inverted) = filter_enable_symbol(&config.identifier);
    config
        .parameters
        .iter()
        .filter(|p| p.is_input || !lv2)
        .find(|p| p.symbol == symbol)
        .and_then(|p| filter_value_f32(&p.value))
        .is_none_or(|v| (v > 0.5) != inverted)
}

fn state_message(state: &Value) -> (&str, Option<String>) {
    match state {
        Value::String(s) => (s.as_str(), None),
        Value::Object(o) => match o.iter().next() {
            Some((k, v)) => (k.as_str(), v.as_str().map(|s| s.to_string())),
            None => ("", None),
        },
        _ => ("", None),
    }
}
