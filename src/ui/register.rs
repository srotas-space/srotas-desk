use iced::widget::{button, column, container, row, rule, scrollable, svg, text, text_input};
use iced::{Element, Length, Task};

use super::{Message, Notice, State};
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

    Ok(Parsed {
        shop_name,
        owner_name: form.owner_name.trim().to_string(),
        phone: form.phone.trim().to_string(),
        address: form.address.trim().to_string(),
        pin: crate::pin::validate_new(&form.pin, &form.pin_confirm)?,
    })
}

pub fn submit(state: &mut State) -> Task<Message> {
    let parsed = match parse(&state.register_form) {
        Ok(p) => p,
        Err(e) => {
            state.notice = Some(Notice::error(e));
            return Task::none();
        }
    };
    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            // Hashing is deliberately slow — off the UI thread, same as
            // every other place a PIN is turned into a hash.
            let pin_hash = match parsed.pin {
                Some(pin) => Some(
                    tokio::task::spawn_blocking(move || crate::pin::hash(&pin))
                        .await
                        .map_err(|e| format!("could not secure that PIN: {e}"))??,
                ),
                None => None,
            };

            crate::repo::register_shop(
                &pool,
                &parsed.shop_name,
                &parsed.owner_name,
                &parsed.phone,
                &parsed.address,
                pin_hash.as_deref(),
            )
            .await
            .map_err(|e| e.to_string())
        },
        Message::ShopRegistered,
    )
}

pub fn view(state: &State) -> Element<'_, Message> {
    let form = &state.register_form;

    let fields = column![
        column![
            text("Set up your shop").size(theme::TEXT_DISPLAY).font(theme::SEMIBOLD),
            text("This runs once. Everything you enter stays on this computer.")
                .size(theme::TEXT_SMALL)
                .color(theme::MUTED_TEXT),
        ]
        .spacing(theme::SPACE_XS)
        .align_x(iced::Alignment::Center)
        .width(Length::Fill),
        labeled("Shop name *", input("e.g. Sharma Hardware Store", &form.shop_name, Field::ShopName)),
        labeled("Owner name", input("e.g. Ramesh Sharma", &form.owner_name, Field::OwnerName)),
        row![
            labeled("Phone", input("e.g. 98765 43210", &form.phone, Field::Phone)),
            labeled("Address", input("Shop address", &form.address, Field::Address)),
        ]
        .spacing(theme::SPACE_MD),
        rule::horizontal(1).style(theme::divider),
        text("Screen-lock PIN (optional) — keeps the till closed when you step away.")
            .size(theme::TEXT_SMALL)
            .color(theme::MUTED_TEXT),
        row![
            labeled("PIN", secure_input("4-6 digits", &form.pin, Field::Pin)),
            labeled("Confirm PIN", secure_input("repeat PIN", &form.pin_confirm, Field::PinConfirm)),
        ]
        .spacing(theme::SPACE_MD),
        button(text("Register Shop").size(theme::TEXT_BODY).font(theme::SEMIBOLD))
            .style(theme::primary_button)
            .padding([13, 28])
            .on_press(Message::SubmitRegister),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(480);

    let card = container(
        column![svg(super::logo_handle()).width(64).height(64), fields]
            .spacing(theme::SPACE_MD)
            .align_x(iced::Alignment::Center),
    )
    .style(theme::card)
    .padding(theme::SPACE_LG)
    .max_width(560);

    scrollable(
        container(card)
            .width(Length::Fill)
            .padding(theme::SPACE_LG)
            .align_x(iced::Alignment::Center),
    )
    .height(Length::Fill)
    .into()
}

fn input<'a>(placeholder: &'a str, value: &'a str, field: Field) -> Element<'a, Message> {
    text_input(placeholder, value)
        .on_input(move |v| Message::RegisterFieldChanged(field, v))
        .style(theme::field)
        .padding(theme::FIELD_PADDING)
        .size(theme::TEXT_BODY)
        .into()
}

fn secure_input<'a>(placeholder: &'a str, value: &'a str, field: Field) -> Element<'a, Message> {
    text_input(placeholder, value)
        .on_input(move |v| Message::RegisterFieldChanged(field, v))
        .secure(true)
        .style(theme::field)
        .padding(theme::FIELD_PADDING)
        .size(theme::TEXT_BODY)
        .into()
}

fn labeled<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(theme::TEXT_SMALL).color(theme::MUTED_TEXT), widget.into()]
        .spacing(theme::SPACE_XS)
        .width(Length::Fill)
        .into()
}
