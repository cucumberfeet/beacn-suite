//! Colours, styles and small building blocks shared by the Mixer page.
//! Palette: beacn-utility's own (#1B1B1B / #3C3C3C / #005C80) plus the Mix Create's mix colours.

use crate::ui::SVG;
use crate::ui::pages::app::mixer::model::Colour;
use iced::gradient::Linear;
use iced::widget::{Button, button, container, slider, svg, text, vertical_slider};
use iced::{Background, Border, Color, Element, Gradient, Length, Radians, Shadow, Theme};

pub const MIX_A: Color = Color::from_rgb8(0x59, 0xB1, 0xB6);
pub const MIX_B: Color = Color::from_rgb8(0xF4, 0x7C, 0x24);
pub const MUTE: Color = Color::from_rgb8(0xD9, 0x3A, 0x3A);
pub const WARN: Color = Color::from_rgb8(0xF0, 0xB4, 0x29);
pub const ACCENT: Color = Color::from_rgb8(0x00, 0x5C, 0x80);
pub const TEXT: Color = Color::from_rgb8(0xDA, 0xDA, 0xDA);
pub const TEXT_DIM: Color = Color::from_rgb8(0x8A, 0x8A, 0x8A);
pub const PANEL: Color = Color::from_rgb8(0x23, 0x23, 0x23);
pub const PANEL_RAISED: Color = Color::from_rgb8(0x2B, 0x2B, 0x2B);
pub const CONTROL: Color = Color::from_rgb8(0x3C, 0x3C, 0x3C);
pub const CONTROL_HOVER: Color = Color::from_rgb8(0x46, 0x46, 0x46);
pub const BORDER: Color = Color::from_rgb8(0x36, 0x36, 0x36);
pub const RAIL: Color = Color::from_rgb8(0x31, 0x31, 0x31);

pub fn colour(c: Colour) -> Color {
    Color::from_rgb8(c.red, c.green, c.blue)
}

pub fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

pub fn mix_colour(mix: crate::ui::pages::app::mixer::model::Mix) -> Color {
    match mix {
        crate::ui::pages::app::mixer::model::Mix::A => MIX_A,
        crate::ui::pages::app::mixer::model::Mix::B => MIX_B,
    }
}

/// A tinted FA icon
pub fn icon<'a, M: 'a>(name: &str, size: f32, tint: Color) -> Element<'a, M> {
    match SVG.get(format!("mixer-{name}").as_str()) {
        Some(handle) => svg(handle.clone())
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .style(move |_: &Theme, _| svg::Style { color: Some(tint) })
            .into(),
        None => text("").into(),
    }
}

pub fn panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(PANEL.into()),
        border: Border {
            radius: 6.0.into(),
            width: 1.0,
            color: BORDER,
        },
        ..Default::default()
    }
}

pub fn sheet(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(0x1F, 0x1F, 0x1F).into()),
        border: Border {
            radius: 6.0.into(),
            width: 1.0,
            color: Color::from_rgb8(0x4A, 0x4A, 0x4A),
        },
        shadow: Shadow {
            color: with_alpha(Color::BLACK, 0.5),
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 16.0,
        },
        text_color: Some(TEXT),
        ..Default::default()
    }
}

pub fn backdrop(_: &Theme) -> container::Style {
    container::Style {
        background: Some(with_alpha(Color::BLACK, 0.55).into()),
        ..Default::default()
    }
}

/// The channel strip: channel-coloured glow at the top fading into the panel
pub fn strip(c: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Gradient(Gradient::Linear(
            Linear::new(Radians(std::f32::consts::PI))
                .add_stop(0.0, with_alpha(c, 0.16))
                .add_stop(0.35, PANEL_RAISED)
                .add_stop(1.0, PANEL_RAISED),
        ))),
        border: Border {
            radius: 6.0.into(),
            width: 1.0,
            color: BORDER,
        },
        ..Default::default()
    }
}

pub fn colour_bar(c: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(c.into()),
        border: Border {
            radius: 2.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn fill(c: Color, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(c.into()),
        border: Border {
            radius: radius.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Flat control button; `active` paints it in `on` (accent, mute red, ...)
pub fn control(active: bool, on: Color) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let base = if active {
            on
        } else if hovered {
            CONTROL_HOVER
        } else {
            CONTROL
        };
        let disabled = matches!(status, button::Status::Disabled);
        button::Style {
            background: Some(
                if disabled {
                    with_alpha(base, 0.4)
                } else {
                    base
                }
                .into(),
            ),
            text_color: if active {
                Color::WHITE
            } else if disabled {
                TEXT_DIM
            } else {
                TEXT
            },
            border: Border {
                radius: 4.0.into(),
                width: if hovered && !active { 1.0 } else { 0.0 },
                color: Color::from_rgb8(0x96, 0x96, 0x96),
            },
            shadow: Shadow::default(),
            snap: false,
        }
    }
}

/// Button with a coloured outline (used to flag a channel that needs attention)
pub fn outlined(colour: Color) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let mut style = control(false, ACCENT)(theme, status);
        style.border = Border {
            radius: 4.0.into(),
            width: 1.5,
            color: colour,
        };
        style
    }
}

/// Transparent row button for menus
pub fn menu_item(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: if hovered {
                Some(CONTROL_HOVER.into())
            } else if selected {
                Some(with_alpha(ACCENT, 0.35).into())
            } else {
                None
            },
            text_color: if matches!(status, button::Status::Disabled) {
                TEXT_DIM
            } else {
                TEXT
            },
            border: Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            shadow: Shadow::default(),
            snap: false,
        }
    }
}

/// Tab buttons along the top of the page
pub fn tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(
                if active {
                    ACCENT
                } else if hovered {
                    CONTROL_HOVER
                } else {
                    CONTROL
                }
                .into(),
            ),
            text_color: if active { Color::WHITE } else { TEXT },
            border: Border {
                radius: 5.0.into(),
                ..Default::default()
            },
            shadow: Shadow::default(),
            snap: false,
        }
    }
}

pub fn icon_button<'a, M: Clone + 'a>(name: &str, tint: Color) -> Button<'a, M> {
    button(
        container(icon(name, 12.0, tint))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(24)
    .height(24)
    .padding(0)
    .style(control(false, ACCENT))
}

/// Fader in a mix colour
pub fn fader(c: Color) -> impl Fn(&Theme, vertical_slider::Status) -> vertical_slider::Style {
    move |_, status| {
        let hover = !matches!(status, vertical_slider::Status::Active);
        vertical_slider::Style {
            rail: slider::Rail {
                backgrounds: (c.into(), RAIL.into()),
                width: 6.0,
                border: Border {
                    radius: 3.0.into(),
                    ..Default::default()
                },
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Rectangle {
                    width: 22,
                    border_radius: 3.0.into(),
                },
                background: Color::from_rgb8(0x50, 0x50, 0x50).into(),
                border_width: if hover { 2.0 } else { 1.0 },
                border_color: if hover {
                    Color::WHITE
                } else {
                    Color::from_rgb8(0xB4, 0xB4, 0xB4)
                },
            },
        }
    }
}

/// Horizontal slider in a mix colour
pub fn hslider(c: Color) -> impl Fn(&Theme, slider::Status) -> slider::Style {
    move |_, status| {
        let hover = !matches!(status, slider::Status::Active);
        slider::Style {
            rail: slider::Rail {
                backgrounds: (c.into(), RAIL.into()),
                width: 6.0,
                border: Border {
                    radius: 3.0.into(),
                    ..Default::default()
                },
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Rectangle {
                    width: 12,
                    border_radius: 2.5.into(),
                },
                background: Color::from_rgb8(0x50, 0x50, 0x50).into(),
                border_width: if hover { 2.0 } else { 1.0 },
                border_color: Color::from_rgb8(0xB4, 0xB4, 0xB4),
            },
        }
    }
}

pub fn title<'a, M: 'a>(label: &str, c: Color) -> Element<'a, M> {
    text(label.to_uppercase())
        .size(13)
        .color(c)
        .font(BOLD)
        .into()
}

pub const BOLD: iced::Font = iced::Font {
    family: iced::font::Family::Name("Noto Sans"),
    weight: iced::font::Weight::Bold,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};
