use iced::widget::{button, column, container, svg, text, text_input};
use iced::{Element, Length};

use super::{Message, State};
use crate::ui::theme;

pub fn view(state: &State) -> Element<'_, Message> {
    let Some(shop) = &state.shop else {
        return text("").into();
    };

    let logo = svg(super::logo_handle()).width(80).height(80);

    let mut body = column![logo, text(&shop.shop_name).size(28)]
        .spacing(theme::SPACE_SM)
        .align_x(iced::Alignment::Center);

    if !shop.owner_name.is_empty() {
        body = body.push(text(&shop.owner_name).size(14));
    }

    if shop.pin.is_some() {
        body = body.push(
            text_input("Enter PIN", &state.login_pin_input)
                .on_input(Message::LoginPinChanged)
                .on_submit(Message::SubmitLogin)
                .secure(true)
                .padding(10)
                .size(18)
                .width(Length::Fixed(200.0))
                .align_x(iced::Alignment::Center),
        );
    }

    if let Some(error) = &state.login_error {
        body = body.push(text(error).color(iced::Color::from_rgb(0.83, 0.16, 0.16)).size(14));
    }

    body = body.push(
        button(text(if shop.pin.is_some() { "Unlock" } else { "Continue" }).size(16))
            .style(theme::primary_button)
            .padding([12, 32])
            .on_press(Message::SubmitLogin),
    );

    container(container(body).style(theme::card).padding(theme::SPACE_LG))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
}
