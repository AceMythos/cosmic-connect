use cosmic::iced::core::Alignment;
use cosmic::iced::{Background, Border, Color, Length, Shadow, Vector};
use cosmic::theme;
use cosmic::widget::button;
use cosmic::widget::container as iced_container;
use cosmic::widget::{icon, text};
use cosmic::{Element, iced};

// --- Design Tokens ---

// Radius tiers
pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 8.0;
pub const RADIUS_LG: f32 = 12.0;
pub const RADIUS_PILL: f32 = 18.0;

// Colors
pub const COLOR_ACCENT: Color = Color::from_rgb8(0x4D, 0x8D, 0xFF);
pub const COLOR_TEXT_PRIMARY: Color = Color::from_rgb8(0xF3, 0xF1, 0xEC);
pub const COLOR_TEXT_HOVER: Color = Color::from_rgb8(0xFF, 0xFF, 0xFF);
pub const COLOR_TEXT_DISABLED: Color = Color::from_rgba8(0xF3, 0xF1, 0xEC, 0.4);
pub const COLOR_TEXT_DIM: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.6);
#[allow(dead_code)]
pub const COLOR_BG_CARD: Color = Color::from_rgb8(0x27, 0x27, 0x27);
#[allow(dead_code)]
pub const COLOR_BG_SELECTED: Color = Color::from_rgb8(0x2B, 0x2B, 0x2B);
pub const COLOR_BG_CARD_FROSTED: Color = Color::from_rgba8(0x27, 0x27, 0x27, 0.80);
pub const COLOR_BG_SELECTED_FROSTED: Color = Color::from_rgba8(0x2B, 0x2B, 0x2B, 0.85);
pub const COLOR_BG_BANNER_FROSTED: Color = Color::from_rgba8(0x27, 0x27, 0x27, 0.75);
pub const COLOR_BG_COATING_FROSTED: Color = Color::from_rgba8(0x27, 0x27, 0x27, 0.40);
pub const COLOR_BG_HOVER: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.06);
pub const COLOR_BG_PRESSED_SUBTLE: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.08);
pub const COLOR_BG_PRESSED: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.10);
pub const COLOR_BORDER_SUBTLE: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.08);
pub const COLOR_BORDER_GLASS: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.06);
pub const COLOR_SHADOW_CARD: Color = Color::from_rgba8(0x00, 0x00, 0x00, 0.20);
pub const COLOR_SHADOW_ACCENT: Color = Color::from_rgba8(0x4D, 0x8D, 0xFF, 0.15);
pub const COLOR_SHADOW_PANEL: Color = Color::from_rgba8(0x00, 0x00, 0x00, 0.30);
pub const COLOR_SUCCESS: Color = Color::from_rgb8(0x4F, 0xD2, 0x6A);
pub const COLOR_ERROR: Color = Color::from_rgb8(0xFF, 0x5C, 0x5C);

// Typography sizes
pub const SIZE_HEADING: u16 = 15;
pub const SIZE_BODY: u16 = 13;
pub const SIZE_CAPTION: u16 = 11;
pub const SIZE_ICON: u16 = 18;

pub fn device_selector_card<'a, Message: Clone + 'static>(
    local_icon: &'a str,
    local_label: &'a str,
    local_sub: &'a str,
    remote_icon: &'a str,
    remote_label: &'a str,
    remote_sub: &'a str,
    _is_selected: bool,
    on_select: Option<Message>,
) -> Element<'a, Message> {
    fn make_card<'a, Message: Clone + 'static>(
        icon_name: &'a str,
        label: &'a str,
        sub: &'a str,
        selected: bool,
        on_select: &Option<Message>,
    ) -> Element<'a, Message> {
        let border_color = if selected {
            COLOR_ACCENT
        } else {
            COLOR_BORDER_SUBTLE
        };

        let label_elem: Element<'a, Message> = if selected {
            iced::widget::row![
                icon::from_name("object-select-symbolic").size(SIZE_CAPTION),
                text::body(label).size(SIZE_CAPTION),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
        } else {
            text::body(label).size(SIZE_CAPTION).into()
        };

        let inner = iced_container(
            iced::widget::row![
                icon::from_name(icon_name).size(16),
                iced::widget::column![
                    label_elem,
                    text::caption(sub).size(SIZE_CAPTION),
                ]
                .spacing(1)
                .align_x(Alignment::Center),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .class(theme::Container::custom(move |_theme| {
            let bg = if selected {
                Background::Color(COLOR_BG_SELECTED_FROSTED)
            } else {
                Background::Color(COLOR_BG_CARD_FROSTED)
            };
            iced_container::Style {
                background: Some(bg),
                border: Border {
                    radius: RADIUS_LG.into(),
                    width: 1.0,
                    color: border_color,
                },
                shadow: if selected {
                    Shadow {
                        color: COLOR_SHADOW_ACCENT,
                        offset: Vector::new(0.0, 0.0),
                        blur_radius: RADIUS_MD,
                    }
                } else {
                    Shadow {
                        color: COLOR_SHADOW_CARD,
                        offset: Vector::new(0.0, 2.0),
                        blur_radius: RADIUS_MD,
                    }
                },
                ..Default::default()
            }
        }))
        .clip(true)
        .padding([6, RADIUS_MD as u16])
        .width(Length::Fill);

        if selected || on_select.is_none() {
            Element::from(inner)
        } else {
            let msg = on_select.clone().unwrap();
            let btn = button::custom(Element::from(inner))
                .on_press(msg)
                .class(theme::Button::Custom {
                    active: Box::new(|_focused, _theme| button::Style {
                        background: None,
                        border_radius: 0.0.into(),
                        border_width: 0.0,
                        border_color: Color::TRANSPARENT,
                        ..button::Style::new()
                    }),
                    hovered: Box::new(|_focused, _theme| button::Style {
                        background: Some(Background::Color(COLOR_BG_HOVER)),
                        border_radius: 0.0.into(),
                        border_width: 0.0,
                        border_color: Color::TRANSPARENT,
                        ..button::Style::new()
                    }),
                    pressed: Box::new(|_focused, _theme| button::Style {
                        background: Some(Background::Color(COLOR_BG_PRESSED)),
                        border_radius: 0.0.into(),
                        border_width: 0.0,
                        border_color: Color::TRANSPARENT,
                        ..button::Style::new()
                    }),
                    disabled: Box::new(|_theme| button::Style {
                        background: None,
                        border_radius: 0.0.into(),
                        border_width: 0.0,
                        border_color: Color::TRANSPARENT,
                        ..button::Style::new()
                    }),
                })
                .width(Length::Fill);
            Element::from(btn)
        }
    }

    iced::widget::row![
        make_card(local_icon, local_label, local_sub, false, &on_select),
        make_card(remote_icon, remote_label, remote_sub, true, &on_select),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

pub fn pill_button<'a, Message: Clone + 'static>(
    icon_name: &'a str,
    label: &'a str,
    message: Message,
    active: bool,
) -> Element<'a, Message> {

    let text_color = if active {
        COLOR_ACCENT
    } else {
        COLOR_TEXT_PRIMARY
    };

    button::custom(
        iced::widget::row![
            icon::from_name(icon_name).size(16),
            text::caption(label).size(SIZE_CAPTION),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .on_press(message)
    .class(theme::Button::Custom {
        active: Box::new(move |_focused, _theme| {
            let (bg, border) = if active {
                (
                    Background::Color(COLOR_BG_SELECTED_FROSTED),
                    COLOR_ACCENT,
                )
            } else {
                (
                    Background::Color(COLOR_BG_CARD_FROSTED),
                    COLOR_BORDER_SUBTLE,
                )
            };
            button::Style {
                background: Some(bg),
                border_radius: RADIUS_PILL.into(),
                border_width: if active { 1.0 } else { 0.0 },
                border_color: border,
                text_color: Some(text_color),
                icon_color: Some(text_color),
                ..button::Style::new()
            }
        }),
        hovered: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(COLOR_BG_HOVER)),
            border_radius: RADIUS_PILL.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(COLOR_TEXT_HOVER),
            icon_color: Some(COLOR_TEXT_HOVER),
            ..button::Style::new()
        }),
        pressed: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(COLOR_BG_PRESSED)),
            border_radius: RADIUS_PILL.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(COLOR_TEXT_HOVER),
            icon_color: Some(COLOR_TEXT_PRIMARY),
            ..button::Style::new()
        }),
        disabled: Box::new(|_theme| {
            button::Style {
                background: Some(Background::Color(COLOR_BG_CARD_FROSTED)),
                border_radius: RADIUS_PILL.into(),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
                text_color: Some(COLOR_TEXT_DIM),
                icon_color: Some(COLOR_TEXT_DIM),
                ..button::Style::new()
            }
        }),
    })
    .padding([6, 16])
    .width(Length::Shrink)
    .into()
}

pub fn status_card<'a, Message: Clone + 'static>(
    name: &'a str,
    connected: bool,
    battery: Option<(&'a str, i32)>,
    _network: Option<(&'a str, i32)>,
    on_toggle: Option<Message>,
) -> Element<'a, Message> {
    let dot = if connected { "●" } else { "○" };
    let status_text = if connected { "Connected" } else { "Offline" };

    let mut status_row = iced::widget::row![
        text::body(dot).size(SIZE_CAPTION),
        text::caption(status_text).size(SIZE_CAPTION),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    if let Some((_icon, charge)) = battery {
        let charge_color = if charge <= 15 {
            COLOR_ERROR
        } else {
            COLOR_TEXT_PRIMARY
        };
        status_row = status_row.push(
            text::caption(format!("🔋 {}%", charge)).size(SIZE_CAPTION).class(charge_color),
        );
    }

    let mut header = iced::widget::row![
        icon::from_name("phone-symbolic").size(SIZE_ICON),
        iced::widget::column![
            text::body(name).size(SIZE_HEADING),
            status_row,
        ]
        .spacing(4),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    header = header.push(iced::widget::container(iced::widget::row![]).width(Length::Fill));

    if on_toggle.is_some() {
        header = header.push(icon::from_name("pan-down-symbolic").size(14));
    }

        let card = iced_container(header)
        .class(theme::Container::custom(|_theme| {
            iced_container::Style {
                background: Some(Background::Color(COLOR_BG_CARD_FROSTED)),
                border: Border {
                    radius: RADIUS_LG.into(),
                    width: 1.0,
                    color: COLOR_BORDER_SUBTLE,
                },
                shadow: Shadow {
                    color: COLOR_SHADOW_CARD,
                    offset: Vector::new(0.0, 2.0),
                    blur_radius: RADIUS_MD,
                },
                ..Default::default()
            }
        }))
        .padding([12, 14])
        .width(Length::Fill);

    if let Some(msg) = on_toggle {
        button::custom(card)
            .on_press(msg)
            .class(theme::Button::Custom {
                active: Box::new(|_focused, _theme| button::Style {
                    background: None,
                    border_radius: 0.0.into(),
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                    ..button::Style::new()
                }),
                hovered: Box::new(|_focused, _theme| button::Style {
                    background: Some(Background::Color(COLOR_BG_HOVER)),
                    border_radius: 0.0.into(),
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                    ..button::Style::new()
                }),
                pressed: Box::new(|_focused, _theme| button::Style {
                    background: Some(Background::Color(COLOR_BG_PRESSED_SUBTLE)),
                    border_radius: 0.0.into(),
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                    ..button::Style::new()
                }),
                disabled: Box::new(|_theme| button::Style {
                    background: None,
                    border_radius: 0.0.into(),
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                    ..button::Style::new()
                }),
            })
            .width(Length::Fill)
            .into()
    } else {
        card.into()
    }
}

pub fn info_banner<'a, Message: 'static>(
    title: &'a str,
    description: &'a str,
) -> Element<'a, Message> {
    iced_container(
        iced::widget::row![
            icon::from_name("dialog-information-symbolic").size(16),
            iced::widget::column![
                text::body(title).size(SIZE_BODY),
                text::caption(description).size(SIZE_CAPTION),
            ]
            .spacing(1),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .class(theme::Container::custom(|_theme| {
        iced_container::Style {
            background: Some(Background::Color(COLOR_BG_BANNER_FROSTED)),
            border: Border {
                radius: RADIUS_MD.into(),
                width: 1.0,
                color: COLOR_BORDER_SUBTLE,
            },
            ..Default::default()
        }
    }))
    .padding([10, 14])
    .width(Length::Fill)
    .into()
}

pub fn list_row<'a, Message: Clone + 'static>(
    icon_name: &'a str,
    label: &'a str,
    message: Message,
) -> Element<'a, Message> {
    button::custom(
        iced::widget::row![
            icon::from_name(icon_name).size(SIZE_ICON),
            text::body(label).size(SIZE_BODY),
            iced::widget::container(iced::widget::row![]).width(Length::Fill),
            icon::from_name("pan-end-symbolic").size(14),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .on_press(message)
    .class(theme::Button::Custom {
        active: Box::new(|_focused, _theme| button::Style {
            background: None,
            border_radius: 0.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(COLOR_TEXT_HOVER),
            icon_color: Some(COLOR_TEXT_PRIMARY),
            ..button::Style::new()
        }),
        hovered: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(COLOR_BG_HOVER)),
            border_radius: RADIUS_MD.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(COLOR_TEXT_HOVER),
            icon_color: Some(COLOR_TEXT_PRIMARY),
            ..button::Style::new()
        }),
        pressed: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(COLOR_BG_PRESSED_SUBTLE)),
            border_radius: RADIUS_MD.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(COLOR_TEXT_HOVER),
            icon_color: Some(COLOR_TEXT_PRIMARY),
            ..button::Style::new()
        }),
        disabled: Box::new(|_theme| button::Style {
            background: None,
            border_radius: 0.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(COLOR_TEXT_DIM),
            icon_color: Some(COLOR_TEXT_DISABLED),
            ..button::Style::new()
        }),
    })
    .padding([12, 14])
    .width(Length::Fill)
    .into()
}

pub fn quick_action_btn<'a, Message: Clone + 'static>(
    icon_name: &'a str,
    label: &'a str,
    message: Message,
    is_active: bool,
) -> Element<'a, Message> {
    let icon_color = if is_active {
        COLOR_ACCENT
    } else {
        COLOR_TEXT_HOVER
    };
    let border = if is_active {
        COLOR_ACCENT
    } else {
        Color::TRANSPARENT
    };

    button::custom(
        iced::widget::column![
            icon::from_name(icon_name).size(22),
            text::caption(label).size(SIZE_CAPTION),
        ]
        .spacing(2)
        .align_x(Alignment::Center),
    )
    .on_press(message)
    .class(theme::Button::Custom {
        active: Box::new(move |_focused, _theme| button::Style {
            background: None,
            border_radius: RADIUS_MD.into(),
            border_width: if is_active { 1.0 } else { 0.0 },
            border_color: border,
            text_color: Some(COLOR_TEXT_PRIMARY),
            icon_color: Some(icon_color),
            ..button::Style::new()
        }),
        hovered: Box::new(move |_focused, _theme| button::Style {
            background: Some(Background::Color(COLOR_BG_HOVER)),
            border_radius: RADIUS_MD.into(),
            border_width: 1.0,
            border_color: if is_active { COLOR_ACCENT } else { COLOR_BORDER_SUBTLE },
            text_color: Some(COLOR_TEXT_PRIMARY),
            icon_color: Some(COLOR_TEXT_HOVER),
            ..button::Style::new()
        }),
            pressed: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(COLOR_BG_PRESSED)),
            border_radius: RADIUS_MD.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(COLOR_TEXT_PRIMARY),
            icon_color: Some(COLOR_TEXT_HOVER),
            ..button::Style::new()
        }),
        disabled: Box::new(|_theme| button::Style {
            background: None,
            border_radius: RADIUS_MD.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(COLOR_TEXT_DISABLED),
            icon_color: Some(COLOR_TEXT_DISABLED),
            ..button::Style::new()
        }),
    })
    .padding([8, 10])
    .width(Length::Shrink)
    .into()
}

pub fn disclosure_row<'a, Message: Clone + 'static>(
    label: &'a str,
    is_open: bool,
    message: Message,
) -> Element<'a, Message> {
    let chevron = if is_open { "⌄" } else { "▸" };

    button::custom(
        iced::widget::row![
            text::body(chevron).size(SIZE_CAPTION),
            text::caption(label).size(SIZE_CAPTION),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .on_press(message)
    .class(theme::Button::Custom {
        active: Box::new(|_focused, _theme| button::Style {
            background: None,
            border_radius: RADIUS_MD.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(COLOR_TEXT_PRIMARY),
            icon_color: Some(COLOR_TEXT_PRIMARY),
            ..button::Style::new()
        }),
        hovered: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(COLOR_BG_HOVER)),
            border_radius: RADIUS_MD.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(COLOR_TEXT_HOVER),
            icon_color: Some(COLOR_TEXT_PRIMARY),
            ..button::Style::new()
        }),
        pressed: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(COLOR_BG_PRESSED_SUBTLE)),
            border_radius: RADIUS_MD.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(COLOR_TEXT_HOVER),
            icon_color: Some(COLOR_TEXT_PRIMARY),
            ..button::Style::new()
        }),
        disabled: Box::new(|_theme| button::Style {
            background: None,
            border_radius: RADIUS_MD.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(COLOR_TEXT_DISABLED),
            icon_color: Some(COLOR_TEXT_DISABLED),
            ..button::Style::new()
        }),
    })
    .padding([10, 14])
    .width(Length::Fill)
    .into()
}

pub fn card_default(_theme: &cosmic::Theme) -> iced_container::Style {
    iced_container::Style {
        background: Some(Background::Color(COLOR_BG_CARD_FROSTED)),
        border: Border {
            radius: RADIUS_MD.into(),
            width: 1.0,
            color: COLOR_BORDER_GLASS,
        },
        shadow: Shadow {
            color: COLOR_SHADOW_CARD,
            offset: Vector::new(0.0, 2.0),
            blur_radius: RADIUS_MD,
        },
        ..Default::default()
    }
}

pub fn card_elevated(_theme: &cosmic::Theme) -> iced_container::Style {
    iced_container::Style {
        background: Some(Background::Color(COLOR_BG_SELECTED_FROSTED)),
        border: Border {
            radius: RADIUS_MD.into(),
            width: 1.0,
            color: COLOR_BORDER_SUBTLE,
        },
        shadow: Shadow {
            color: COLOR_SHADOW_CARD,
            offset: Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
        ..Default::default()
    }
}

pub fn card_sunken(_theme: &cosmic::Theme) -> iced_container::Style {
    iced_container::Style {
        background: Some(Background::Color(Color::from_rgba8(0x00, 0x00, 0x00, 0.12))),
        border: Border {
            radius: RADIUS_MD.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    }
}

pub fn section_header<'a, Message: 'static>(title: &'a str) -> Element<'a, Message> {
    iced_container(
        text::caption(title).size(SIZE_CAPTION),
    )
    .padding([4, 14, 2, 14])
    .width(Length::Fill)
    .into()
}
