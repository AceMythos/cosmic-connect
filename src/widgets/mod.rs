use cosmic::iced::core::Alignment;
use cosmic::iced::{Background, Border, Color, Length, Shadow, Vector};
use cosmic::theme;
use cosmic::widget::button;
use cosmic::widget::container as iced_container;
use cosmic::widget::tooltip;
use cosmic::widget::{icon, text};
use cosmic::{iced, Element};

// --- Design Tokens ---

// Radius tiers
pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 8.0;
pub const RADIUS_LG: f32 = 12.0;
pub const RADIUS_PILL: f32 = 18.0;

// Colors — semantic (static)
pub const COLOR_ACCENT: Color = Color::from_rgb8(0x4D, 0x8D, 0xFF);
pub const COLOR_TEXT_PRIMARY: Color = Color::from_rgb8(0xF3, 0xF1, 0xEC);
pub const COLOR_TEXT_HOVER: Color = Color::from_rgb8(0xFF, 0xFF, 0xFF);
pub const COLOR_TEXT_DISABLED: Color = Color::from_rgba8(0xF3, 0xF1, 0xEC, 0.4);
pub const COLOR_TEXT_DIM: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.6);
pub const COLOR_SUCCESS: Color = Color::from_rgb8(0x4F, 0xD2, 0x6A);
pub const COLOR_ERROR: Color = Color::from_rgb8(0xFF, 0x5C, 0x5C);

// --- clipboard-style glass derivation constants ---
pub const RADIUS_CARD: f32 = 12.0;
pub const OPACITY_CARD: f32 = 0.78;
pub const OPACITY_CARD_HOVER: f32 = 0.82;
pub const OPACITY_COATING: f32 = 0.40;
pub const OPACITY_BANNER: f32 = 0.75;
pub const CARD_LUM_DARKEN: f32 = 0.92;
pub const CARD_LUM_HOVER: f32 = 0.96;
pub const BORDER_GLASS_ALPHA: f32 = 0.08;
pub const DIVIDER_ALPHA: f32 = 0.06;
pub const HOVER_OVERLAY_ALPHA: f32 = 0.06;
pub const PRESSED_SUBTLE_ALPHA: f32 = 0.08;
pub const PRESSED_ALPHA: f32 = 0.10;
pub const SHADOW_CARD_ALPHA: f32 = 0.20;
pub const SHADOW_PANEL_ALPHA: f32 = 0.30;

pub fn frosted_bg(theme: &cosmic::Theme, lum: f32, alpha: f32) -> Color {
    let base: Color = theme.cosmic().background(theme.transparent).base.into();
    Color::from_rgba(base.r * lum, base.g * lum, base.b * lum, alpha)
}

pub fn on_color(theme: &cosmic::Theme) -> Color {
    theme.cosmic().background(theme.transparent).on.into()
}

pub fn on_overlay(theme: &cosmic::Theme, alpha: f32) -> Color {
    let on: Color = theme.cosmic().background(theme.transparent).on.into();
    Color::from_rgba(on.r, on.g, on.b, alpha)
}

pub fn accent_color(theme: &cosmic::Theme) -> Color {
    Color::from(theme.cosmic().accent_color())
}

pub fn destructive_color(theme: &cosmic::Theme) -> Color {
    Color::from(theme.cosmic().destructive_color())
}

// Typography sizes
pub const SIZE_HEADING: u16 = 15;
pub const SIZE_BODY: u16 = 13;
pub const SIZE_CAPTION: u16 = 12;
pub const SIZE_ICON: u16 = 18;

pub fn device_selector_card<'a, Message: Clone + 'static>(
    local_icon: &'a str,
    local_label: &'a str,
    local_sub: &'a str,
    remote_icon: &'a str,
    remote_label: &'a str,
    remote_sub: String,
    _is_selected: bool,
    on_select: Option<Message>,
) -> Element<'a, Message> {
    fn make_card<'a, Message: Clone + 'static>(
        icon_name: &'a str,
        label: &'a str,
        sub: String,
        selected: bool,
        interactive: bool,
        on_select: Option<Message>,
    ) -> Element<'a, Message> {
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

        let mut row = iced::widget::row![
            icon::from_name(icon_name).size(16),
            iced::widget::column![label_elem, text::caption(sub).size(SIZE_CAPTION),].spacing(1),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        row = row.push(iced::widget::container(iced::widget::row![]).width(Length::Fill));
        if interactive {
            row = row.push(icon::from_name("pan-down-symbolic").size(12));
        }

        let inner = iced_container(row)
            .class(theme::Container::custom(move |theme| {
                let accent = accent_color(theme);
                let border_color = if selected {
                    accent
                } else {
                    on_overlay(theme, BORDER_GLASS_ALPHA)
                };
                iced_container::Style {
                    background: Some(Background::Color(frosted_bg(
                        theme,
                        if selected {
                            CARD_LUM_HOVER
                        } else {
                            CARD_LUM_DARKEN
                        },
                        if selected {
                            OPACITY_CARD_HOVER
                        } else {
                            OPACITY_CARD
                        },
                    ))),
                    border: Border {
                        radius: RADIUS_LG.into(),
                        width: 1.0,
                        color: border_color,
                    },
                    shadow: if selected {
                        Shadow {
                            color: Color::from_rgba(accent.r, accent.g, accent.b, 0.15),
                            offset: Vector::new(0.0, 0.0),
                            blur_radius: RADIUS_MD,
                        }
                    } else {
                        Shadow {
                            color: Color::from_rgba(0.0, 0.0, 0.0, SHADOW_CARD_ALPHA),
                            offset: Vector::new(0.0, 2.0),
                            blur_radius: RADIUS_MD,
                        }
                    },
                    ..Default::default()
                }
            }))
            .clip(true)
            .padding([6, RADIUS_CARD as u16])
            .width(Length::Fill);

        if !interactive {
            return inner.into();
        }

        let Some(msg) = on_select else {
            return inner.into();
        };

        button::custom(inner)
            .on_press(msg)
            .class(theme::Button::Custom {
                active: Box::new(|focused, theme| button::Style {
                    background: None,
                    border_radius: 0.0.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused {
                        accent_color(theme)
                    } else {
                        Color::TRANSPARENT
                    },
                    ..button::Style::new()
                }),
                hovered: Box::new(|focused, theme| button::Style {
                    background: Some(Background::Color(on_overlay(theme, HOVER_OVERLAY_ALPHA))),
                    border_radius: 0.0.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused {
                        accent_color(theme)
                    } else {
                        Color::TRANSPARENT
                    },
                    ..button::Style::new()
                }),
                pressed: Box::new(|focused, theme| button::Style {
                    background: Some(Background::Color(on_overlay(theme, PRESSED_ALPHA))),
                    border_radius: 0.0.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused {
                        accent_color(theme)
                    } else {
                        Color::TRANSPARENT
                    },
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
    }

    iced::widget::row![
        make_card(
            local_icon,
            local_label,
            local_sub.to_string(),
            false,
            false,
            None
        ),
        make_card(
            remote_icon,
            remote_label,
            remote_sub,
            true,
            on_select.is_some(),
            on_select
        ),
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
        active: Box::new(move |focused, theme| {
            let accent = accent_color(theme);
            let (bg, border) = if active {
                (
                    Background::Color(frosted_bg(theme, CARD_LUM_HOVER, OPACITY_CARD_HOVER)),
                    accent,
                )
            } else {
                (
                    Background::Color(frosted_bg(theme, CARD_LUM_DARKEN, OPACITY_CARD)),
                    on_overlay(theme, BORDER_GLASS_ALPHA),
                )
            };
            button::Style {
                background: Some(bg),
                border_radius: RADIUS_PILL.into(),
                border_width: if active || focused { 1.0 } else { 0.0 },
                border_color: if focused { accent } else { border },
                text_color: active.then_some(accent),
                icon_color: active.then_some(accent),
                ..button::Style::new()
            }
        }),
        hovered: Box::new(|focused, theme| button::Style {
            background: Some(Background::Color(on_overlay(theme, HOVER_OVERLAY_ALPHA))),
            border_radius: RADIUS_PILL.into(),
            border_width: if focused { 1.0 } else { 0.0 },
            border_color: if focused {
                accent_color(theme)
            } else {
                Color::TRANSPARENT
            },
            ..button::Style::new()
        }),
        pressed: Box::new(|focused, theme| button::Style {
            background: Some(Background::Color(on_overlay(theme, PRESSED_ALPHA))),
            border_radius: RADIUS_PILL.into(),
            border_width: if focused { 1.0 } else { 0.0 },
            border_color: if focused {
                accent_color(theme)
            } else {
                Color::TRANSPARENT
            },
            ..button::Style::new()
        }),
        disabled: Box::new(|theme| button::Style {
            background: Some(Background::Color(frosted_bg(
                theme,
                CARD_LUM_DARKEN,
                OPACITY_CARD,
            ))),
            border_radius: RADIUS_PILL.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(on_overlay(theme, 0.4)),
            icon_color: Some(on_overlay(theme, 0.4)),
            ..button::Style::new()
        }),
    })
    .padding([8, 14])
    .width(Length::Shrink)
    .into()
}

pub fn text_button<'a, Message: Clone + 'static>(
    label: &'a str,
    message: Message,
) -> Element<'a, Message> {
    button::custom(text::caption(label).size(SIZE_CAPTION))
        .on_press(message)
        .class(theme::Button::Custom {
            active: Box::new(|_focused, theme| button::Style {
                background: None,
                border_radius: RADIUS_SM.into(),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
                text_color: Some(on_overlay(theme, 0.7)),
                ..button::Style::new()
            }),
            hovered: Box::new(|focused, theme| button::Style {
                background: Some(Background::Color(on_overlay(theme, HOVER_OVERLAY_ALPHA))),
                border_radius: RADIUS_SM.into(),
                border_width: if focused { 1.0 } else { 0.0 },
                border_color: if focused {
                    accent_color(theme)
                } else {
                    Color::TRANSPARENT
                },
                text_color: Some(on_color(theme)),
                ..button::Style::new()
            }),
            pressed: Box::new(|focused, theme| button::Style {
                background: Some(Background::Color(on_overlay(theme, PRESSED_ALPHA))),
                border_radius: RADIUS_SM.into(),
                border_width: if focused { 1.0 } else { 0.0 },
                border_color: if focused {
                    accent_color(theme)
                } else {
                    Color::TRANSPARENT
                },
                text_color: Some(on_color(theme)),
                ..button::Style::new()
            }),
            disabled: Box::new(|theme| button::Style {
                background: None,
                border_radius: RADIUS_SM.into(),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
                text_color: Some(on_overlay(theme, 0.4)),
                ..button::Style::new()
            }),
        })
        .padding([4, 6])
        .into()
}

pub fn danger_button<'a, Message: Clone + 'static>(
    icon_name: &'a str,
    label: &'a str,
    message: Message,
) -> Element<'a, Message> {
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
        active: Box::new(|focused, theme| {
            let danger = destructive_color(theme);
            button::Style {
                background: Some(Background::Color(frosted_bg(
                    theme,
                    CARD_LUM_DARKEN,
                    OPACITY_CARD,
                ))),
                border_radius: RADIUS_PILL.into(),
                border_width: 1.0,
                border_color: if focused { accent_color(theme) } else { danger },
                text_color: Some(danger),
                icon_color: Some(danger),
                ..button::Style::new()
            }
        }),
        hovered: Box::new(|focused, theme| {
            let danger = destructive_color(theme);
            button::Style {
                background: Some(Background::Color(Color::from_rgba(
                    danger.r, danger.g, danger.b, 0.12,
                ))),
                border_radius: RADIUS_PILL.into(),
                border_width: 1.0,
                border_color: if focused { accent_color(theme) } else { danger },
                text_color: Some(danger),
                icon_color: Some(danger),
                ..button::Style::new()
            }
        }),
        pressed: Box::new(|focused, theme| {
            let danger = destructive_color(theme);
            button::Style {
                background: Some(Background::Color(Color::from_rgba(
                    danger.r, danger.g, danger.b, 0.2,
                ))),
                border_radius: RADIUS_PILL.into(),
                border_width: 1.0,
                border_color: if focused { accent_color(theme) } else { danger },
                text_color: Some(danger),
                icon_color: Some(danger),
                ..button::Style::new()
            }
        }),
        disabled: Box::new(|theme| button::Style {
            background: None,
            border_radius: RADIUS_PILL.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(on_overlay(theme, 0.4)),
            icon_color: Some(on_overlay(theme, 0.4)),
            ..button::Style::new()
        }),
    })
    .padding([8, 14])
    .width(Length::Shrink)
    .into()
}

pub fn icon_button<'a, Message: Clone + 'static>(
    icon_name: &'a str,
    tooltip_label: &'a str,
    message: Message,
) -> Element<'a, Message> {
    let btn = button::custom(icon::from_name(icon_name).size(16))
        .on_press(message)
        .class(theme::Button::Custom {
            active: Box::new(|focused, theme| button::Style {
                background: None,
                border_radius: RADIUS_SM.into(),
                border_width: if focused { 1.0 } else { 0.0 },
                border_color: if focused {
                    accent_color(theme)
                } else {
                    Color::TRANSPARENT
                },
                text_color: Some(on_color(theme)),
                icon_color: Some(on_color(theme)),
                ..button::Style::new()
            }),
            hovered: Box::new(|focused, theme| button::Style {
                background: Some(Background::Color(on_overlay(theme, HOVER_OVERLAY_ALPHA))),
                border_radius: RADIUS_SM.into(),
                border_width: if focused { 1.0 } else { 0.0 },
                border_color: if focused {
                    accent_color(theme)
                } else {
                    Color::TRANSPARENT
                },
                icon_color: Some(on_color(theme)),
                ..button::Style::new()
            }),
            pressed: Box::new(|focused, theme| button::Style {
                background: Some(Background::Color(on_overlay(theme, PRESSED_ALPHA))),
                border_radius: RADIUS_SM.into(),
                border_width: if focused { 1.0 } else { 0.0 },
                border_color: if focused {
                    accent_color(theme)
                } else {
                    Color::TRANSPARENT
                },
                icon_color: Some(on_color(theme)),
                ..button::Style::new()
            }),
            disabled: Box::new(|theme| button::Style {
                background: None,
                border_radius: RADIUS_SM.into(),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
                icon_color: Some(on_overlay(theme, 0.4)),
                ..button::Style::new()
            }),
        })
        .padding([7, 7]);

    tooltip(btn, tooltip_label, tooltip::Position::Top).into()
}

pub fn battery_icon(charge: i32, charging: bool) -> &'static str {
    let base = if charge <= 5 {
        "battery-empty"
    } else if charge <= 20 {
        "battery-caution"
    } else if charge <= 50 {
        "battery-low"
    } else if charge <= 80 {
        "battery-good"
    } else {
        "battery-full"
    };
    match (base, charging) {
        ("battery-empty", true) => "battery-empty-charging-symbolic",
        ("battery-caution", true) => "battery-caution-charging-symbolic",
        ("battery-low", true) => "battery-low-charging-symbolic",
        ("battery-good", true) => "battery-good-charging-symbolic",
        ("battery-full", true) => "battery-full-charging-symbolic",
        ("battery-empty", false) => "battery-empty-symbolic",
        ("battery-caution", false) => "battery-caution-symbolic",
        ("battery-low", false) => "battery-low-symbolic",
        ("battery-good", false) => "battery-good-symbolic",
        _ => "battery-full-symbolic",
    }
}

pub fn network_icon(network_type: &str) -> &'static str {
    match network_type.to_ascii_uppercase().as_str() {
        "5G" => "network-cellular-5g-symbolic",
        "4G" | "LTE" => "network-cellular-4g-symbolic",
        "3G" | "HSPA" | "UMTS" => "network-cellular-3g-symbolic",
        "2G" | "EDGE" | "GPRS" => "network-cellular-2g-symbolic",
        _ => "network-cellular-symbolic",
    }
}

pub fn signal_icon(strength: i32) -> &'static str {
    match strength.clamp(0, 4) {
        4 => "network-cellular-signal-excellent-symbolic",
        3 => "network-cellular-signal-good-symbolic",
        2 => "network-cellular-signal-ok-symbolic",
        1 => "network-cellular-signal-weak-symbolic",
        _ => "network-cellular-signal-none-symbolic",
    }
}

pub fn status_card<'a, Message: Clone + 'static>(
    icon_name: &'a str,
    name: &'a str,
    connected: bool,
    battery: Option<(i32, bool)>,
    network: Option<(&'a str, i32)>,
    updated: Option<String>,
    on_toggle: Option<Message>,
    on_refresh: Option<Message>,
) -> Element<'a, Message> {
    let dot = if connected { "●" } else { "○" };
    let status_text = if connected { "Connected" } else { "Offline" };

    let mut status_row = iced::widget::row![
        text::body(dot).size(SIZE_CAPTION),
        text::caption(status_text).size(SIZE_CAPTION),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    if let Some(updated) = updated {
        status_row = status_row
            .push(text::caption("·").size(SIZE_CAPTION))
            .push(text::caption(updated).size(SIZE_CAPTION));
    }
    let mut metrics = iced::widget::row![].spacing(12).align_y(Alignment::Center);
    let has_metrics = battery.is_some() || network.is_some();
    if let Some((charge, charging)) = battery {
        let mut battery_row = iced::widget::row![
            icon::from_name(battery_icon(charge, charging)).size(16),
            text::caption(format!("{charge}%")).size(SIZE_CAPTION),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        if charging {
            battery_row = battery_row.push(text::caption("Charging").size(SIZE_CAPTION));
        }
        metrics = metrics.push(battery_row);
    }
    if let Some((network_type, strength)) = network {
        metrics = metrics.push(
            iced::widget::row![
                icon::from_name(network_icon(network_type)).size(16),
                text::caption(network_type).size(SIZE_CAPTION),
                icon::from_name(signal_icon(strength)).size(16),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }

    let mut info = iced::widget::column![text::body(name).size(SIZE_HEADING), status_row,]
        .spacing(3)
        .width(Length::Fill);

    if has_metrics {
        info = info.push(metrics);
    }

    let mut header = iced::widget::row![icon::from_name(icon_name).size(20), info,]
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    if on_toggle.is_some() {
        header = header.push(icon::from_name("pan-down-symbolic").size(14));
    }

    let main: Element<'a, Message> = if let Some(msg) = on_toggle {
        button::custom(header)
            .on_press(msg)
            .class(theme::Button::Custom {
                active: Box::new(|focused, theme| button::Style {
                    background: None,
                    border_radius: RADIUS_MD.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused {
                        accent_color(theme)
                    } else {
                        Color::TRANSPARENT
                    },
                    ..button::Style::new()
                }),
                hovered: Box::new(|focused, theme| button::Style {
                    background: Some(Background::Color(on_overlay(theme, HOVER_OVERLAY_ALPHA))),
                    border_radius: RADIUS_MD.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused {
                        accent_color(theme)
                    } else {
                        Color::TRANSPARENT
                    },
                    ..button::Style::new()
                }),
                pressed: Box::new(|focused, theme| button::Style {
                    background: Some(Background::Color(on_overlay(theme, PRESSED_ALPHA))),
                    border_radius: RADIUS_MD.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused {
                        accent_color(theme)
                    } else {
                        Color::TRANSPARENT
                    },
                    ..button::Style::new()
                }),
                disabled: Box::new(|_theme| button::Style {
                    background: None,
                    border_radius: RADIUS_MD.into(),
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                    ..button::Style::new()
                }),
            })
            .padding([4, 4])
            .width(Length::Fill)
            .into()
    } else {
        iced_container(header)
            .padding([4, 4])
            .width(Length::Fill)
            .into()
    };

    let mut card_row = iced::widget::row![main]
        .spacing(4)
        .align_y(Alignment::Center);
    if let Some(msg) = on_refresh {
        card_row = card_row.push(icon_button("view-refresh-symbolic", "Refresh status", msg));
    }

    iced_container(card_row)
        .class(theme::Container::custom(|theme| iced_container::Style {
            background: Some(Background::Color(frosted_bg(
                theme,
                CARD_LUM_DARKEN,
                OPACITY_CARD,
            ))),
            border: Border {
                radius: RADIUS_LG.into(),
                width: 1.0,
                color: on_overlay(theme, BORDER_GLASS_ALPHA),
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, SHADOW_CARD_ALPHA),
                offset: Vector::new(0.0, 2.0),
                blur_radius: RADIUS_MD,
            },
            ..Default::default()
        }))
        .padding([10, 12])
        .width(Length::Fill)
        .into()
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
    .class(theme::Container::custom(|theme| iced_container::Style {
        background: Some(Background::Color(frosted_bg(
            theme,
            CARD_LUM_DARKEN,
            OPACITY_BANNER,
        ))),
        border: Border {
            radius: RADIUS_MD.into(),
            width: 1.0,
            color: on_overlay(theme, BORDER_GLASS_ALPHA),
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
        active: Box::new(|focused, theme| button::Style {
            background: None,
            border_radius: RADIUS_CARD.into(),
            border_width: if focused { 1.0 } else { 0.0 },
            border_color: if focused {
                accent_color(theme)
            } else {
                Color::TRANSPARENT
            },
            ..button::Style::new()
        }),
        hovered: Box::new(|focused, theme| button::Style {
            background: Some(Background::Color(on_overlay(theme, HOVER_OVERLAY_ALPHA))),
            border_radius: RADIUS_CARD.into(),
            border_width: if focused { 1.0 } else { 0.0 },
            border_color: if focused {
                accent_color(theme)
            } else {
                Color::TRANSPARENT
            },
            ..button::Style::new()
        }),
        pressed: Box::new(|focused, theme| button::Style {
            background: Some(Background::Color(on_overlay(theme, PRESSED_SUBTLE_ALPHA))),
            border_radius: RADIUS_CARD.into(),
            border_width: if focused { 1.0 } else { 0.0 },
            border_color: if focused {
                accent_color(theme)
            } else {
                Color::TRANSPARENT
            },
            ..button::Style::new()
        }),
        disabled: Box::new(|theme| button::Style {
            background: None,
            border_radius: RADIUS_CARD.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(on_overlay(theme, 0.4)),
            icon_color: Some(on_overlay(theme, 0.4)),
            ..button::Style::new()
        }),
    })
    .padding([12, 16])
    .width(Length::Fill)
    .into()
}

pub fn quick_action_btn<'a, Message: Clone + 'static>(
    icon_name: &'a str,
    label: &'a str,
    message: Message,
    is_active: bool,
) -> Element<'a, Message> {
    button::custom(
        iced::widget::column![
            icon::from_name(icon_name).size(22),
            text::caption(label).size(SIZE_CAPTION),
        ]
        .spacing(3)
        .align_x(Alignment::Center),
    )
    .on_press(message)
    .class(theme::Button::Custom {
        active: Box::new(move |focused, theme| {
            let accent = accent_color(theme);
            button::Style {
                background: Some(Background::Color(frosted_bg(
                    theme,
                    CARD_LUM_DARKEN,
                    OPACITY_CARD,
                ))),
                border_radius: RADIUS_MD.into(),
                border_width: 1.0,
                border_color: if focused || is_active {
                    accent
                } else {
                    on_overlay(theme, BORDER_GLASS_ALPHA)
                },
                text_color: is_active.then_some(accent),
                icon_color: Some(if is_active { accent } else { on_color(theme) }),
                ..button::Style::new()
            }
        }),
        hovered: Box::new(move |focused, theme| {
            let accent = accent_color(theme);
            button::Style {
                background: Some(Background::Color(frosted_bg(
                    theme,
                    CARD_LUM_HOVER,
                    OPACITY_CARD_HOVER,
                ))),
                border_radius: RADIUS_MD.into(),
                border_width: 1.0,
                border_color: if focused || is_active {
                    accent
                } else {
                    on_overlay(theme, BORDER_GLASS_ALPHA)
                },
                text_color: is_active.then_some(accent),
                icon_color: Some(if is_active { accent } else { on_color(theme) }),
                ..button::Style::new()
            }
        }),
        pressed: Box::new(|focused, theme| button::Style {
            background: Some(Background::Color(on_overlay(theme, PRESSED_ALPHA))),
            border_radius: RADIUS_MD.into(),
            border_width: 1.0,
            border_color: if focused {
                accent_color(theme)
            } else {
                on_overlay(theme, BORDER_GLASS_ALPHA)
            },
            icon_color: Some(on_color(theme)),
            ..button::Style::new()
        }),
        disabled: Box::new(|theme| button::Style {
            background: Some(Background::Color(frosted_bg(
                theme,
                CARD_LUM_DARKEN,
                OPACITY_CARD,
            ))),
            border_radius: RADIUS_MD.into(),
            border_width: 1.0,
            border_color: on_overlay(theme, BORDER_GLASS_ALPHA),
            text_color: Some(on_overlay(theme, 0.4)),
            icon_color: Some(on_overlay(theme, 0.4)),
            ..button::Style::new()
        }),
    })
    .padding([9, 8])
    .width(Length::Fill)
    .into()
}

pub fn media_button<'a, Message: Clone + 'static>(
    icon_name: &'a str,
    tooltip_label: &'a str,
    message: Message,
    primary: bool,
) -> Element<'a, Message> {
    let btn = button::custom(icon::from_name(icon_name).size(18))
        .on_press(message)
        .class(theme::Button::Custom {
            active: Box::new(move |focused, theme| {
                let accent = accent_color(theme);
                let (bg, border) = if primary {
                    (
                        Color::from_rgba(accent.r, accent.g, accent.b, 0.18),
                        Color::from_rgba(accent.r, accent.g, accent.b, 0.45),
                    )
                } else {
                    (
                        frosted_bg(theme, CARD_LUM_DARKEN, OPACITY_CARD),
                        on_overlay(theme, BORDER_GLASS_ALPHA),
                    )
                };
                button::Style {
                    background: Some(Background::Color(bg)),
                    border_radius: RADIUS_PILL.into(),
                    border_width: 1.0,
                    border_color: if focused { accent } else { border },
                    icon_color: Some(if primary { accent } else { on_color(theme) }),
                    ..button::Style::new()
                }
            }),
            hovered: Box::new(move |_focused, theme| {
                let accent = accent_color(theme);
                button::Style {
                    background: Some(Background::Color(if primary {
                        Color::from_rgba(accent.r, accent.g, accent.b, 0.26)
                    } else {
                        frosted_bg(theme, CARD_LUM_HOVER, OPACITY_CARD_HOVER)
                    })),
                    border_radius: RADIUS_PILL.into(),
                    border_width: 1.0,
                    border_color: if primary {
                        Color::from_rgba(accent.r, accent.g, accent.b, 0.55)
                    } else {
                        on_overlay(theme, BORDER_GLASS_ALPHA)
                    },
                    icon_color: Some(if primary { accent } else { on_color(theme) }),
                    ..button::Style::new()
                }
            }),
            pressed: Box::new(|_focused, theme| button::Style {
                background: Some(Background::Color(on_overlay(theme, PRESSED_ALPHA))),
                border_radius: RADIUS_PILL.into(),
                border_width: 1.0,
                border_color: on_overlay(theme, BORDER_GLASS_ALPHA),
                icon_color: Some(on_color(theme)),
                ..button::Style::new()
            }),
            disabled: Box::new(|theme| button::Style {
                background: Some(Background::Color(frosted_bg(
                    theme,
                    CARD_LUM_DARKEN,
                    OPACITY_CARD,
                ))),
                border_radius: RADIUS_PILL.into(),
                border_width: 1.0,
                border_color: on_overlay(theme, BORDER_GLASS_ALPHA),
                icon_color: Some(on_overlay(theme, 0.4)),
                ..button::Style::new()
            }),
        })
        .padding([7, 10]);

    tooltip(btn, tooltip_label, tooltip::Position::Top).into()
}

pub fn nav_row<'a, Message: Clone + 'static>(
    icon_name: &'a str,
    label: String,
    trailing: String,
    message: Message,
) -> Element<'a, Message> {
    let mut content = iced::widget::row![
        icon::from_name(icon_name).size(16),
        text::body(label).size(SIZE_BODY),
    ]
    .spacing(9)
    .align_y(Alignment::Center);

    content = content.push(iced::widget::container(iced::widget::row![]).width(Length::Fill));
    if !trailing.is_empty() {
        content = content.push(text::caption(trailing).size(SIZE_CAPTION));
    }
    content = content.push(icon::from_name("pan-end-symbolic").size(14));

    let btn = button::custom(content)
        .on_press(message)
        .class(theme::Button::Custom {
            active: Box::new(|focused, theme| button::Style {
                background: None,
                border_radius: RADIUS_CARD.into(),
                border_width: if focused { 1.0 } else { 0.0 },
                border_color: if focused {
                    accent_color(theme)
                } else {
                    Color::TRANSPARENT
                },
                ..button::Style::new()
            }),
            hovered: Box::new(|focused, theme| button::Style {
                background: Some(Background::Color(on_overlay(theme, HOVER_OVERLAY_ALPHA))),
                border_radius: RADIUS_CARD.into(),
                border_width: if focused { 1.0 } else { 0.0 },
                border_color: if focused {
                    accent_color(theme)
                } else {
                    Color::TRANSPARENT
                },
                ..button::Style::new()
            }),
            pressed: Box::new(|focused, theme| button::Style {
                background: Some(Background::Color(on_overlay(theme, PRESSED_ALPHA))),
                border_radius: RADIUS_CARD.into(),
                border_width: if focused { 1.0 } else { 0.0 },
                border_color: if focused {
                    accent_color(theme)
                } else {
                    Color::TRANSPARENT
                },
                ..button::Style::new()
            }),
            disabled: Box::new(|_theme| button::Style::new()),
        })
        .padding([10, 12])
        .width(Length::Fill);

    iced_container(btn)
        .class(theme::Container::custom(|theme| iced_container::Style {
            background: Some(Background::Color(frosted_bg(
                theme,
                CARD_LUM_DARKEN,
                OPACITY_CARD,
            ))),
            border: Border {
                radius: RADIUS_CARD.into(),
                width: 1.0,
                color: on_overlay(theme, BORDER_GLASS_ALPHA),
            },
            ..Default::default()
        }))
        .width(Length::Fill)
        .into()
}

pub fn disclosure_row<'a, Message: Clone + 'static>(
    label: &'a str,
    is_open: bool,
    message: Message,
) -> Element<'a, Message> {
    let chevron = if is_open {
        "pan-down-symbolic"
    } else {
        "pan-end-symbolic"
    };

    button::custom(
        iced::widget::row![
            icon::from_name(chevron).size(14),
            text::body(label).size(SIZE_BODY),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .on_press(message)
    .class(theme::Button::Custom {
        active: Box::new(|focused, theme| button::Style {
            background: None,
            border_radius: RADIUS_CARD.into(),
            border_width: if focused { 1.0 } else { 0.0 },
            border_color: if focused {
                accent_color(theme)
            } else {
                Color::TRANSPARENT
            },
            ..button::Style::new()
        }),
        hovered: Box::new(|focused, theme| button::Style {
            background: Some(Background::Color(on_overlay(theme, HOVER_OVERLAY_ALPHA))),
            border_radius: RADIUS_CARD.into(),
            border_width: if focused { 1.0 } else { 0.0 },
            border_color: if focused {
                accent_color(theme)
            } else {
                Color::TRANSPARENT
            },
            ..button::Style::new()
        }),
        pressed: Box::new(|focused, theme| button::Style {
            background: Some(Background::Color(on_overlay(theme, PRESSED_SUBTLE_ALPHA))),
            border_radius: RADIUS_CARD.into(),
            border_width: if focused { 1.0 } else { 0.0 },
            border_color: if focused {
                accent_color(theme)
            } else {
                Color::TRANSPARENT
            },
            ..button::Style::new()
        }),
        disabled: Box::new(|theme| button::Style {
            background: None,
            border_radius: RADIUS_CARD.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            text_color: Some(on_overlay(theme, 0.4)),
            icon_color: Some(on_overlay(theme, 0.4)),
            ..button::Style::new()
        }),
    })
    .padding([10, 12])
    .width(Length::Fill)
    .into()
}

pub fn card_default(theme: &cosmic::Theme) -> iced_container::Style {
    iced_container::Style {
        background: Some(Background::Color(frosted_bg(
            theme,
            CARD_LUM_DARKEN,
            OPACITY_CARD,
        ))),
        border: Border {
            radius: RADIUS_CARD.into(),
            width: 1.0,
            color: on_overlay(theme, BORDER_GLASS_ALPHA),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, SHADOW_CARD_ALPHA),
            offset: Vector::new(0.0, 2.0),
            blur_radius: RADIUS_CARD,
        },
        ..Default::default()
    }
}

pub fn card_elevated(theme: &cosmic::Theme) -> iced_container::Style {
    iced_container::Style {
        background: Some(Background::Color(frosted_bg(
            theme,
            CARD_LUM_HOVER,
            OPACITY_CARD_HOVER,
        ))),
        border: Border {
            radius: RADIUS_CARD.into(),
            width: 1.0,
            color: on_overlay(theme, BORDER_GLASS_ALPHA),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, SHADOW_CARD_ALPHA),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
        ..Default::default()
    }
}

pub fn card_sunken(_theme: &cosmic::Theme) -> iced_container::Style {
    iced_container::Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.12))),
        border: Border {
            radius: RADIUS_CARD.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    }
}

pub fn section_header<'a, Message: 'static>(title: &'a str) -> Element<'a, Message> {
    iced_container(text::caption(title).size(SIZE_CAPTION))
        .padding([6, 0, 2, 0])
        .width(Length::Fill)
        .into()
}

pub fn paired_device_row<'a, Message: Clone + 'static>(
    icon_name: &'a str,
    name: &'a str,
    is_reachable: bool,
    is_selected: bool,
    on_select: Option<Message>,
    on_unpair: Option<Message>,
) -> Element<'a, Message> {
    let dot = if is_reachable { "●" } else { "○" };
    let status_text = if is_reachable { "Connected" } else { "Offline" };

    let body = iced_container(
        iced::widget::row![
            icon::from_name(icon_name).size(16),
            iced::widget::column![
                text::body(name).size(SIZE_BODY),
                iced::widget::row![
                    text::body(dot).size(SIZE_CAPTION),
                    text::caption(status_text).size(SIZE_CAPTION),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ]
            .spacing(1),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .class(theme::Container::custom(move |theme| {
        let accent = accent_color(theme);
        let border_color = if is_selected {
            accent
        } else {
            on_overlay(theme, BORDER_GLASS_ALPHA)
        };
        iced_container::Style {
            background: Some(Background::Color(frosted_bg(
                theme,
                if is_selected {
                    CARD_LUM_HOVER
                } else {
                    CARD_LUM_DARKEN
                },
                if is_selected {
                    OPACITY_CARD_HOVER
                } else {
                    OPACITY_CARD
                },
            ))),
            border: Border {
                radius: RADIUS_MD.into(),
                width: 1.0,
                color: border_color,
            },
            shadow: if is_selected {
                Shadow {
                    color: Color::from_rgba(accent.r, accent.g, accent.b, 0.15),
                    offset: Vector::new(0.0, 0.0),
                    blur_radius: RADIUS_MD,
                }
            } else {
                Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, SHADOW_CARD_ALPHA),
                    offset: Vector::new(0.0, 2.0),
                    blur_radius: RADIUS_MD,
                }
            },
            ..Default::default()
        }
    }))
    .clip(true)
    .padding([8, 12])
    .width(Length::Fill);

    let select_btn = match on_select {
        Some(msg) => button::custom(body)
            .on_press(msg)
            .class(theme::Button::Custom {
                active: Box::new(|focused, theme| button::Style {
                    background: None,
                    border_radius: 0.0.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused {
                        accent_color(theme)
                    } else {
                        Color::TRANSPARENT
                    },
                    ..button::Style::new()
                }),
                hovered: Box::new(|focused, theme| button::Style {
                    background: Some(Background::Color(on_overlay(theme, HOVER_OVERLAY_ALPHA))),
                    border_radius: 0.0.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused {
                        accent_color(theme)
                    } else {
                        Color::TRANSPARENT
                    },
                    ..button::Style::new()
                }),
                pressed: Box::new(|focused, theme| button::Style {
                    background: Some(Background::Color(on_overlay(theme, PRESSED_SUBTLE_ALPHA))),
                    border_radius: 0.0.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused {
                        accent_color(theme)
                    } else {
                        Color::TRANSPARENT
                    },
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
            .width(Length::Fill),
        None => button::custom(body)
            .class(theme::Button::Custom {
                active: Box::new(|_focused, _theme| button::Style {
                    background: None,
                    border_radius: 0.0.into(),
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                    ..button::Style::new()
                }),
                hovered: Box::new(|_focused, _theme| button::Style {
                    background: None,
                    border_radius: 0.0.into(),
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                    ..button::Style::new()
                }),
                pressed: Box::new(|_focused, _theme| button::Style {
                    background: None,
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
            .width(Length::Fill),
    };

    let unpair_btn: Element<'a, Message> = match on_unpair {
        Some(msg) => {
            let inner = button::custom(
                iced_container(
                    iced::widget::row![icon::from_name("user-trash-symbolic").size(14),]
                        .align_y(Alignment::Center),
                )
                .padding([6, 8]),
            )
            .on_press(msg)
            .class(theme::Button::Custom {
                active: Box::new(|focused, theme| button::Style {
                    background: Some(Background::Color(frosted_bg(
                        theme,
                        CARD_LUM_DARKEN,
                        OPACITY_CARD,
                    ))),
                    border_radius: RADIUS_MD.into(),
                    border_width: 1.0,
                    border_color: if focused {
                        accent_color(theme)
                    } else {
                        on_overlay(theme, BORDER_GLASS_ALPHA)
                    },
                    icon_color: Some(on_color(theme)),
                    ..button::Style::new()
                }),
                hovered: Box::new(|focused, theme| button::Style {
                    background: Some(Background::Color(on_overlay(theme, HOVER_OVERLAY_ALPHA))),
                    border_radius: RADIUS_MD.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused {
                        accent_color(theme)
                    } else {
                        Color::TRANSPARENT
                    },
                    icon_color: Some(on_color(theme)),
                    ..button::Style::new()
                }),
                pressed: Box::new(|focused, theme| button::Style {
                    background: Some(Background::Color(on_overlay(theme, PRESSED_ALPHA))),
                    border_radius: RADIUS_MD.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused {
                        accent_color(theme)
                    } else {
                        Color::TRANSPARENT
                    },
                    icon_color: Some(on_color(theme)),
                    ..button::Style::new()
                }),
                disabled: Box::new(|theme| button::Style {
                    background: None,
                    border_radius: RADIUS_MD.into(),
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                    icon_color: Some(on_overlay(theme, 0.4)),
                    ..button::Style::new()
                }),
            });
            tooltip(inner, "Unpair device", tooltip::Position::Top).into()
        }
        None => iced::widget::row![].into(),
    };

    iced::widget::row![select_btn, unpair_btn]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
}
