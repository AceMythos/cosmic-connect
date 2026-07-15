use cosmic::iced::core::Alignment;
use cosmic::iced::{Background, Border, Color, Length, Shadow, Vector};
use cosmic::theme;
use cosmic::widget::button;
use cosmic::widget::container as iced_container;
use cosmic::widget::{icon, text};
use cosmic::{Element, iced};

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
        let bg = if selected {
            Background::Color(Color::from_rgb8(0x24, 0x27, 0x2F))
        } else {
            Background::Color(Color::from_rgb8(0x23, 0x23, 0x23))
        };
        let border_color = if selected {
            Color::from_rgb8(0x4D, 0x8D, 0xFF)
        } else {
            Color::TRANSPARENT
        };

        let label_elem: Element<'a, Message> = if selected {
            iced::widget::row![
                icon::from_name("object-select-symbolic").size(12),
                text::body(label).size(14),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
        } else {
            text::body(label).size(14).into()
        };

        let inner = iced_container(
            iced::widget::row![
                icon::from_name(icon_name).size(18),
                iced::widget::column![
                    label_elem,
                    text::caption(sub).size(11),
                ]
                .spacing(1)
                .align_x(Alignment::Center),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .class(theme::Container::custom(move |_theme| {
            iced_container::Style {
                background: Some(bg),
                border: Border {
                    radius: 12.0.into(),
                    width: if selected { 1.0 } else { 0.0 },
                    color: border_color,
                },
                shadow: if selected {
                    Shadow {
                        color: Color::from_rgba8(0x4D, 0x8D, 0xFF, 0.15),
                        offset: Vector::new(0.0, 0.0),
                        blur_radius: 8.0,
                    }
                } else {
                    Shadow::default()
                },
                ..Default::default()
            }
        }))
        .padding([12, 14])
        .width(Length::Fill);

        let card_element: Element<'a, Message> = if selected {
            let strip = iced_container(iced::widget::row![])
                .width(Length::Fixed(3.0))
                .height(Length::Fill)
                .class(theme::Container::custom(|_theme| iced_container::Style {
                    background: Some(Background::Color(Color::from_rgb8(0x4D, 0x8D, 0xFF))),
                    ..Default::default()
                }));
            iced::widget::row![strip, inner]
                .spacing(0)
                .align_y(Alignment::Center)
                .into()
        } else {
            Element::from(inner)
        };

        if selected || on_select.is_none() {
            card_element
        } else {
            let msg = on_select.clone().unwrap();
            let btn = button::custom(card_element)
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
                        background: Some(Background::Color(Color::from_rgba8(
                            0xFF, 0xFF, 0xFF, 0.04,
                        ))),
                        border_radius: 0.0.into(),
                        border_width: 0.0,
                        border_color: Color::TRANSPARENT,
                        ..button::Style::new()
                    }),
                    pressed: Box::new(|_focused, _theme| button::Style {
                        background: Some(Background::Color(Color::from_rgba8(
                            0xFF, 0xFF, 0xFF, 0.08,
                        ))),
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
    let (bg, border) = if active {
        (
            Background::Color(Color::from_rgb8(0x24, 0x27, 0x2F)),
            Color::from_rgb8(0x4D, 0x8D, 0xFF),
        )
    } else {
        (
            Background::Color(Color::from_rgb8(0x23, 0x23, 0x23)),
            Color::TRANSPARENT,
        )
    };

    let text_color = if active {
        Color::from_rgb8(0x4D, 0x8D, 0xFF)
    } else {
        Color::from_rgb8(0xB7, 0xB7, 0xB7)
    };

    button::custom(
        iced::widget::row![
            icon::from_name(icon_name).size(16),
            text::caption(label).size(12),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .on_press(message)
    .class(theme::Button::Custom {
        active: Box::new(move |_focused, _theme| button::Style {
            background: Some(bg),
            border_radius: 18.0.into(),
            border_width: if active { 1.0 } else { 0.0 },
            border_color: border,
            text_color: Some(text_color),
            icon_color: Some(text_color),
            ..button::Style::new()
        }),
        hovered: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(Color::from_rgb8(0x2A, 0x2A, 0x2A))),
            border_radius: 18.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
            icon_color: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
            ..button::Style::new()
        }),
        pressed: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(Color::from_rgb8(0x30, 0x30, 0x30))),
            border_radius: 18.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
            icon_color: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
            ..button::Style::new()
        }),
        disabled: Box::new(|_theme| button::Style {
            background: Some(Background::Color(Color::from_rgb8(0x18, 0x18, 0x18))),
            border_radius: 18.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.4)),
            icon_color: Some(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.4)),
            ..button::Style::new()
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
    network: Option<(&'a str, i32)>,
    on_toggle: Option<Message>,
) -> Element<'a, Message> {
    let dot = if connected { "●" } else { "○" };
    let status_text = if connected { "Connected" } else { "Offline" };
    let _status_color = if connected {
        Color::from_rgb8(0x4F, 0xD2, 0x6A)
    } else {
        Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.4)
    };

    let mut status_row = iced::widget::row![
        text::body(dot).size(10),
        text::caption(status_text).size(11),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    if let Some((_icon, charge)) = battery {
        let charge_color = if charge <= 15 {
            Color::from_rgb8(0xFF, 0x5C, 0x5C)
        } else {
            Color::from_rgb8(0xB7, 0xB7, 0xB7)
        };
        status_row = status_row.push(
            text::caption(format!("🔋 {}%", charge)).size(11).class(charge_color),
        );
    }

    if let Some((net_type, _strength)) = network {
        status_row = status_row.push(
            iced::widget::row![
                icon::from_name("network-wireless-symbolic").size(11),
                text::caption(format!("{}", net_type)).size(11),
            ]
            .spacing(3)
            .align_y(Alignment::Center),
        );
    }

    let mut header = iced::widget::row![
        if connected {
            icon::from_name("phone-symbolic").size(18)
        } else {
            icon::from_name("phone-symbolic").size(18)
        },
        iced::widget::column![
            text::body(name).size(15),
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
        .class(theme::Container::custom(|_theme| iced_container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x23, 0x23, 0x23))),
            border: Border {
                radius: 12.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            ..Default::default()
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
                    background: Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.04))),
                    border_radius: 0.0.into(),
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                    ..button::Style::new()
                }),
                pressed: Box::new(|_focused, _theme| button::Style {
                    background: Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.08))),
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
                text::body(title).size(13),
                text::caption(description).size(11),
            ]
            .spacing(1),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .class(theme::Container::custom(|_theme| iced_container::Style {
        background: Some(Background::Color(Color::from_rgb8(0x21, 0x25, 0x2D))),
        border: Border {
            radius: 10.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
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
            icon::from_name(icon_name).size(18),
            text::body(label).size(14),
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
            text_color: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
            icon_color: Some(Color::from_rgb8(0xB7, 0xB7, 0xB7)),
            ..button::Style::new()
        }),
        hovered: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.04))),
            border_radius: 8.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
            icon_color: Some(Color::from_rgb8(0xB7, 0xB7, 0xB7)),
            ..button::Style::new()
        }),
        pressed: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.08))),
            border_radius: 8.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
            icon_color: Some(Color::from_rgb8(0xB7, 0xB7, 0xB7)),
            ..button::Style::new()
        }),
        disabled: Box::new(|_theme| button::Style {
            background: None,
            border_radius: 0.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.4)),
            icon_color: Some(Color::from_rgba8(0xB7, 0xB7, 0xB7, 0.4)),
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
        Color::from_rgb8(0x4D, 0x8D, 0xFF)
    } else {
        Color::from_rgb8(0xFF, 0xFF, 0xFF)
    };
    let border = if is_active {
        Color::from_rgb8(0x4D, 0x8D, 0xFF)
    } else {
        Color::TRANSPARENT
    };

    button::custom(
        iced::widget::column![
            icon::from_name(icon_name).size(22),
            text::caption(label).size(11),
        ]
        .spacing(2)
        .align_x(Alignment::Center),
    )
    .on_press(message)
    .class(theme::Button::Custom {
        active: Box::new(move |_focused, _theme| button::Style {
            background: None,
            border_radius: 10.0.into(),
            border_width: if is_active { 1.0 } else { 0.0 },
            border_color: border,
            text_color: Some(Color::from_rgb8(0xB7, 0xB7, 0xB7)),
            icon_color: Some(icon_color),
            ..button::Style::new()
        }),
        hovered: Box::new(move |_focused, _theme| button::Style {
            background: Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.04))),
            border_radius: 10.0.into(),
            border_width: 1.0,
            border_color: if is_active { Color::from_rgb8(0x4D, 0x8D, 0xFF) } else { Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.08) },
            text_color: Some(Color::from_rgb8(0xB7, 0xB7, 0xB7)),
            icon_color: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
            ..button::Style::new()
        }),
        pressed: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.08))),
            border_radius: 10.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(Color::from_rgb8(0xB7, 0xB7, 0xB7)),
            icon_color: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
            ..button::Style::new()
        }),
        disabled: Box::new(|_theme| button::Style {
            background: None,
            border_radius: 10.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(Color::from_rgba8(0xB7, 0xB7, 0xB7, 0.4)),
            icon_color: Some(Color::from_rgba8(0xB7, 0xB7, 0xB7, 0.4)),
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
            text::body(chevron).size(14),
            text::caption(label).size(12),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .on_press(message)
    .class(theme::Button::Custom {
        active: Box::new(|_focused, _theme| button::Style {
            background: None,
            border_radius: 8.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(Color::from_rgb8(0xB7, 0xB7, 0xB7)),
            icon_color: Some(Color::from_rgb8(0xB7, 0xB7, 0xB7)),
            ..button::Style::new()
        }),
        hovered: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.04))),
            border_radius: 8.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
            icon_color: Some(Color::from_rgb8(0xB7, 0xB7, 0xB7)),
            ..button::Style::new()
        }),
        pressed: Box::new(|_focused, _theme| button::Style {
            background: Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.08))),
            border_radius: 8.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
            icon_color: Some(Color::from_rgb8(0xB7, 0xB7, 0xB7)),
            ..button::Style::new()
        }),
        disabled: Box::new(|_theme| button::Style {
            background: None,
            border_radius: 8.0.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(Color::from_rgba8(0xB7, 0xB7, 0xB7, 0.4)),
            icon_color: Some(Color::from_rgba8(0xB7, 0xB7, 0xB7, 0.4)),
            ..button::Style::new()
        }),
    })
    .padding([10, 14])
    .width(Length::Fill)
    .into()
}

pub fn section_header<'a, Message: 'static>(title: &'a str) -> Element<'a, Message> {
    iced_container(
        text::caption(title).size(11),
    )
    .padding([4, 14, 2, 14])
    .width(Length::Fill)
    .into()
}
