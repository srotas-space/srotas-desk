use iced::widget::{button, column, container, row, svg, text, text_input};
use iced::{Element, Length, Task};

use super::{Message, State};
use crate::ui::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    ShopName,
    OwnerName,
    Phone,
    Address,
    Pin,
    PinConfirm,
}

#[derive(Debug, Clone, Default)]
pub struct RegisterForm {
    pub shop_name: String,
    pub owner_name: String,
    pub phone: String,
    pub address: String,
    pub pin: String,
    pub pin_confirm: String,
}

impl RegisterForm {
    pub fn set_field(&mut self, field: Field, value: String) {
        match field {
            Field::ShopName => self.shop_name = value,
            Field::OwnerName => self.owner_name = value,
            Field::Phone => self.phone = value,
            Field::Address => self.address = value,
            Field::Pin => self.pin = value,
            Field::PinConfirm => self.pin_confirm = value,
        }
    }

    pub fn get_field(&self, field: Field) -> String {
        match field {
            Field::ShopName => self.shop_name.clone(),
            Field::OwnerName => self.owner_name.clone(),
            Field::Phone => self.phone.clone(),
            Field::Address => self.address.clone(),
            Field::Pin => self.pin.clone(),
            Field::PinConfirm => self.pin_confirm.clone(),
        }
    }
}

struct Parsed {
    shop_name: String,
    owner_name: String,
    phone: String,
    address: String,
    pin: Option<String>,
}

fn parse(form: &RegisterForm) -> Result<Parsed, String> {
    let shop_name = form.shop_name.trim().to_string();
    if shop_name.is_empty() {
        return Err("shop name cannot be empty".into());
    }

    let pin = form.pin.trim();
    let pin = if pin.is_empty() {
        None
    } else {
        if !(4..=6).contains(&pin.len()) || !pin.chars().all(|c| c.is_ascii_digit()) {
            return Err("PIN must be 4 to 6 digits".into());
        }
        if pin != form.pin_confirm.trim() {
            return Err("PIN and confirmation don't match".into());
        }
        Some(pin.to_string())
    };

    Ok(Parsed {
        shop_name,
        owner_name: form.owner_name.trim().to_string(),
        phone: form.phone.trim().to_string(),
        address: form.address.trim().to_string(),
        pin,
    })
}

pub fn submit(state: &mut State) -> Task<Message> {
    let parsed = match parse(&state.register_form) {
        Ok(p) => p,
        Err(e) => {
            state.status = Some(e);
            return Task::none();
        }
    };
    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            crate::repo::register_shop(
                &pool,
                &parsed.shop_name,
                &parsed.owner_name,
                &parsed.phone,
                &parsed.address,
                parsed.pin.as_deref(),
            )
            .await
        },
        |result| Message::ShopRegistered(result.map_err(|e| e.to_string())),
    )
}

pub fn view(state: &State) -> Element<'_, Message> {
    let form = &state.register_form;
    let logo = svg(super::logo_handle()).width(72).height(72);

    let fields = column![
        text("Set Up Your Shop").size(26),
        text("This runs once — your shop's details are stored on this computer only.").size(14),
        labeled("Shop name *", text_input("e.g. Sharma Hardware Store", &form.shop_name)
            .on_input(|v| Message::RegisterFieldChanged(Field::ShopName, v))
            .padding(10)
            .size(16)),
        labeled("Owner name", text_input("e.g. Ramesh Sharma", &form.owner_name)
            .on_input(|v| Message::RegisterFieldChanged(Field::OwnerName, v))
            .padding(10)
            .size(16)),
        labeled("Phone", text_input("e.g. 98765 43210", &form.phone)
            .on_input(|v| Message::RegisterFieldChanged(Field::Phone, v))
            .padding(10)
            .size(16)),
        labeled("Address", text_input("Shop address", &form.address)
            .on_input(|v| Message::RegisterFieldChanged(Field::Address, v))
            .padding(10)
            .size(16)),
        row![
            labeled("Screen-lock PIN (optional)", text_input("4-6 digits", &form.pin)
                .on_input(|v| Message::RegisterFieldChanged(Field::Pin, v))
                .secure(true)
                .padding(10)
                .size(16)),
            labeled("Confirm PIN", text_input("repeat PIN", &form.pin_confirm)
                .on_input(|v| Message::RegisterFieldChanged(Field::PinConfirm, v))
                .secure(true)
                .padding(10)
                .size(16)),
        ]
        .spacing(theme::SPACE_MD),
        button(text("Register Shop").size(16))
            .style(theme::primary_button)
            .padding([12, 24])
            .on_press(Message::SubmitRegister),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(480);

    let card = container(column![logo, fields].spacing(theme::SPACE_MD).align_x(iced::Alignment::Center))
        .style(theme::card)
        .padding(theme::SPACE_LG)
        .max_width(560);

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
}

fn labeled<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(13), widget.into()].spacing(4).into()
}
