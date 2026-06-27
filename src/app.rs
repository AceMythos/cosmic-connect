use std::sync::Arc;
use std::time::Duration;

use cosmic::app::Core;
use cosmic::iced::core::Alignment;
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Length, Limits, Subscription};
use cosmic::widget::{button, column, container, divider, icon, progress_bar, row, text};
use cosmic::{Action, Element, Task};
use futures_util::stream::unfold;

use crate::backend::KdeConnectBackend;
use crate::model::{ActionType, Device};

const ID: &str = "io.github.acemythos.Connect";
const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct CosmicConnect {
    core: Core,
    popup: Option<Id>,
    devices: Vec<Device>,
    error: Option<String>,
    backend: Option<Arc<KdeConnectBackend>>,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    DevicesUpdated(Vec<Device>),
    BackendError(String),
    PerformAction(String, ActionType),
}

impl CosmicConnect {
    fn connected_count(&self) -> usize {
        self.devices.iter().filter(|d| d.is_reachable).count()
    }

    fn header_view(&self) -> Element<'_, Message> {
        let connected = self.connected_count();

        let icon_name = if connected > 0 {
            "phone-connected-symbolic"
        } else if self.devices.is_empty() {
            "phone-symbolic"
        } else {
            "phone-offline-symbolic"
        };

        let content: Element<'_, Message> = if connected > 0 {
            row![
                icon::from_name(icon_name).size(14),
                text::body(format!("{}", connected)),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
        } else {
            icon::from_name(icon_name).size(18).into()
        };

        button::custom(content)
            .on_press_down(Message::TogglePopup)
            .padding([4, 8])
            .into()
    }
}

impl cosmic::Application for CosmicConnect {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        (Self { core, ..Default::default() }, Task::none())
    }

    fn on_close_requested(&self, id: cosmic::iced::window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        match message {
            Message::TogglePopup => {
                return if let Some(popup_id) = self.popup.take() {
                    destroy_popup(popup_id)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(420.0)
                        .min_width(280.0)
                        .min_height(100.0)
                        .max_height(600.0);
                    get_popup(popup_settings)
                };
            }
            Message::PopupClosed(popup_id) => {
                if self.popup.as_ref() == Some(&popup_id) {
                    self.popup = None;
                }
            }
            Message::DevicesUpdated(devices) => {
                self.devices = devices;
                self.error = None;
            }
            Message::BackendError(e) => {
                self.error = Some(e);
            }
            Message::PerformAction(device_id, action) => {
                if let Some(backend) = &self.backend {
                    let backend = backend.clone();
                    tokio::spawn(async move {
                        backend.perform_action(&device_id, &action).await;
                    });
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        self.core.applet.autosize_window(self.header_view()).into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let mut content: Vec<Element<Message>> = Vec::new();

        content.push(
            container(
                row![
                    icon::from_name("phone-symbolic").size(20),
                    text::title4("COSMIC Connect"),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([12, 16])
            .into(),
        );

        content.push(divider::horizontal::default().into());

        if let Some(err) = &self.error {
            content.push(
                container(
                    column![
                        icon::from_name("dialog-warning-symbolic").size(32),
                        text::caption(err),
                    ]
                    .spacing(8)
                    .align_x(Alignment::Center),
                )
                .padding(24)
                .width(Length::Fill)
                .into(),
            );
        } else if self.devices.is_empty() {
            content.push(
                container(
                    column![
                        icon::from_name("phone-symbolic").size(48),
                        text::caption("No devices found"),
                        text::caption(
                            "Make sure KDE Connect is installed\nand a device is paired."
                        ),
                    ]
                    .spacing(8)
                    .align_x(Alignment::Center),
                )
                .padding(24)
                .width(Length::Fill)
                .into(),
            );
        } else {
            for (i, device) in self.devices.iter().enumerate() {
                if i > 0 {
                    content.push(divider::horizontal::default().into());
                }
                content.push(device_row(device).into());
            }
        }

        content.push(divider::horizontal::default().into());
        content.push(
            container(
                text::caption(format!("{} device(s)", self.devices.len())),
            )
            .padding([6, 16])
            .into(),
        );

        self.core.applet.popup_container(column::with_children(content)).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::run_with(
            std::any::TypeId::of::<()>(),
            |_state| {
                unfold(PollState::new(), |mut state| async {
                    tokio::time::sleep(POLL_INTERVAL).await;
                    let msg = match state.poll().await {
                        Ok(devices) => Message::DevicesUpdated(devices),
                        Err(e) => Message::BackendError(e),
                    };
                    Some((msg, state))
                })
            },
        )
    }
}

struct PollState {
    backend: Option<KdeConnectBackend>,
}

impl PollState {
    fn new() -> Self {
        Self { backend: None }
    }

    async fn poll(&mut self) -> Result<Vec<Device>, String> {
        if self.backend.is_none() {
            match KdeConnectBackend::new().await {
                Ok(b) => self.backend = Some(b),
                Err(e) => return Err(format!("D-Bus: {e}")),
            }
        }
        let backend = self.backend.as_ref().unwrap();
        Ok(backend.devices().await)
    }
}

fn device_row(device: &Device) -> Element<'_, Message> {
    let icon_name = device.device_type.icon_name();

    let status_text = if device.is_reachable {
        "Connected"
    } else if device.is_paired {
        "Offline"
    } else {
        "Not paired"
    };

    let mut info = row![
        icon::from_name(if device.is_reachable {
            "network-transmit-receive-symbolic"
        } else {
            "network-offline-symbolic"
        })
        .size(10),
        text::caption(status_text),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    if let Some(b) = &device.battery {
        info = info.push(
            progress_bar::determinate_linear(b.charge as f32 / 100.0)
                .width(Length::Fixed(60.0))
                .girth(6),
        );
        let label = if b.is_charging {
            format!("{}%", b.charge)
        } else {
            format!("{}%", b.charge)
        };
        info = info.push(text::caption(label));
    }

    let header = row![
        icon::from_name(icon_name).size(24),
        column![
            text::body(&device.name),
            info,
        ]
        .spacing(2),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut rows = vec![header.into()];

    if device.is_reachable {
        let mut btns: Vec<Element<Message>> = Vec::new();
        if device.has_plugin("kdeconnect_ping") {
            btns.push(
                button::custom(text::caption("Ping"))
                    .on_press(Message::PerformAction(
                        device.id.clone(),
                        ActionType::Ping,
                    ))
                    .padding([2, 8])
                    .into(),
            );
        }
        if device.has_plugin("kdeconnect_findmyphone") {
            btns.push(
                button::custom(text::caption("Ring"))
                    .on_press(Message::PerformAction(
                        device.id.clone(),
                        ActionType::Ring,
                    ))
                    .padding([2, 8])
                    .into(),
            );
        }
        if device.has_plugin("kdeconnect_clipboard") {
            btns.push(
                button::custom(text::caption("Clipboard"))
                    .on_press(Message::PerformAction(
                        device.id.clone(),
                        ActionType::SendClipboard,
                    ))
                    .padding([2, 8])
                    .into(),
            );
        }
        if !btns.is_empty() {
            rows.push(
                row::with_children(btns)
                    .spacing(4)
                    .padding([4, 0, 0, 0])
                    .into(),
            );
        }
    } else if !device.is_paired {
        rows.push(
            button::custom(text::caption("Pair"))
                .on_press(Message::PerformAction(
                    device.id.clone(),
                    ActionType::Pair,
                ))
                .padding([2, 8])
                .into(),
        );
    }

    container(column::with_children(rows).spacing(4))
        .padding([8, 16])
        .into()
}
