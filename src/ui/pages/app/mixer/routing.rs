//! The Routing tab: which sources reach which targets.

use crate::ui::pages::app::mixer::model::{Channel, OrderGroup};
use crate::ui::pages::app::mixer::style::{self, BORDER, PANEL_RAISED, TEXT, TEXT_DIM};
use crate::ui::pages::app::mixer::view::{El, clip};
use crate::ui::pages::app::mixer::{MixerMessage, MixerPage};
use iced::gradient::Linear;
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Gradient, Length, Radians, Theme};

const CELL_W: f32 = 108.0;
const CELL_H: f32 = 40.0;
const LABEL_W: f32 = 170.0;

impl MixerPage {
    pub(super) fn view_routing(&self) -> El<'_> {
        let sources = &self.model.sources;
        let targets = &self.model.targets;
        if sources.is_empty() || targets.is_empty() {
            return container(text("Add a source and a target to route audio.").color(TEXT_DIM))
                .center(Length::Fill)
                .into();
        }

        let mut header = row![
            container(
                column![
                    text("Sources →").size(11).color(style::MIX_A),
                    text("Targets ↓").size(11).color(style::MIX_B),
                ]
                .spacing(2)
            )
            .width(LABEL_W)
            .height(CELL_H + 10.0)
            .align_y(Alignment::End)
            .padding(6)
        ]
        .spacing(2);
        for s in sources {
            header = header.push(source_header(s));
        }

        let mut grid = column![header].spacing(2);
        for t in targets {
            let mut line = row![target_label(t)].spacing(2);
            for s in sources {
                line = line.push(self.cell(s, t));
            }
            grid = grid.push(line);
        }

        let title = row![
            style::title("Routing", TEXT),
            text("Click a cell to send a source to a target. Ticks take the colour of the mix the target carries.")
                .size(11)
                .color(TEXT_DIM),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        container(
            scrollable(column![title, grid].spacing(12).padding(10)).direction(
                scrollable::Direction::Both {
                    vertical: scrollable::Scrollbar::new().width(8).scroller_width(8),
                    horizontal: scrollable::Scrollbar::new().width(8).scroller_width(8),
                },
            ),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(style::panel)
        .into()
    }

    fn cell<'a>(&'a self, source: &'a Channel, target: &'a Channel) -> El<'a> {
        let routed = self.model.is_routed(&source.id, &target.id);
        let colour = style::mix_colour(target.mix);
        let content: El<'a> = if routed {
            style::icon("check", 17.0, colour)
        } else {
            Space::new().into()
        };
        button(container(content).center(Length::Fill))
            .width(CELL_W)
            .height(CELL_H)
            .padding(0)
            .style(move |_: &Theme, status| {
                let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: Some(
                        if hovered {
                            style::CONTROL_HOVER
                        } else if routed {
                            style::with_alpha(colour, 0.12)
                        } else {
                            PANEL_RAISED
                        }
                        .into(),
                    ),
                    border: Border {
                        radius: 3.0.into(),
                        width: 1.0,
                        color: if routed {
                            style::with_alpha(colour, 0.5)
                        } else {
                            BORDER
                        },
                    },
                    ..Default::default()
                }
            })
            .on_press(MixerMessage::ToggleRoute(
                source.id.clone(),
                target.id.clone(),
            ))
            .into()
    }
}

fn name_colour(c: &Channel) -> Color {
    if c.group == OrderGroup::Hidden {
        TEXT_DIM
    } else {
        TEXT
    }
}

fn source_header(c: &Channel) -> El<'_> {
    let colour = style::colour(c.colour);
    let label = if c.group == OrderGroup::Hidden {
        format!("{} (hidden)", clip(&c.name, 10))
    } else {
        clip(&c.name, 14)
    };
    container(text(label).size(12).color(name_colour(c)).center())
        .width(CELL_W)
        .height(CELL_H + 10.0)
        .center_x(CELL_W)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(Background::Gradient(Gradient::Linear(
                Linear::new(Radians(0.0))
                    .add_stop(0.0, style::with_alpha(colour, 0.35))
                    .add_stop(1.0, PANEL_RAISED),
            ))),
            border: Border {
                radius: 3.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            ..Default::default()
        })
        .into()
}

fn target_label(c: &Channel) -> El<'_> {
    let colour = style::colour(c.colour);
    let label = if c.group == OrderGroup::Hidden {
        format!("{} (hidden)", clip(&c.name, 12))
    } else {
        clip(&c.name, 20)
    };
    row![
        container(Space::new().width(3).height(Length::Fill)).style(style::colour_bar(colour)),
        container(
            row![
                text(label)
                    .size(12)
                    .color(name_colour(c))
                    .width(Length::Fill),
                text(c.mix.as_str())
                    .size(10)
                    .color(style::mix_colour(c.mix)),
            ]
            .align_y(Alignment::Center)
        )
        .padding([0, 8])
        .center_y(Length::Fill)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Gradient(Gradient::Linear(
                Linear::new(Radians(std::f32::consts::FRAC_PI_2))
                    .add_stop(0.0, style::with_alpha(colour, 0.22))
                    .add_stop(1.0, PANEL_RAISED),
            ))),
            ..Default::default()
        }),
    ]
    .width(LABEL_W)
    .height(CELL_H)
    .into()
}
