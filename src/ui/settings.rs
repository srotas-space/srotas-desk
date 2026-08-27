use iced::widget::{button, column, container, image, row, scrollable, text, text_input};
use iced::{Element, Length, Task};

use super::{Message, State};
use crate::ui::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Profile,
    Security,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileField {
    ShopName,
    OwnerName,
    Phone,
    Address,
    Gstin,
    DefaultGstRate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityField {
    CurrentPin,
    NewPin,
    ConfirmPin,
}

pub struct ProfileForm {
    pub shop_name: String,
    pub owner_name: String,
    pub phone: String,
    pub address: String,
    pub gstin: String,
    /// Default GST rate as a percentage string (e.g. "18"), applied to any
    /// item that doesn't set its own override.
    pub default_gst_rate: String,
    /// `None` means "leave the logo as it is"; `Some(None)` would be
    /// "explicitly removed" — modeled here as a separate flag instead so
    /// the common case (untouched) doesn't need a nested Option.
    pub logo: Option<Vec<u8>>,
    pub logo_removed: bool,
}

impl Default for ProfileForm {
    fn default() -> Self {
        Self {
            shop_name: String::new(),
            owner_name: String::new(),
            phone: String::new(),
            address: String::new(),
            gstin: String::new(),
            default_gst_rate: String::new(),
            logo: None,
            logo_removed: false,
        }
    }
}

impl ProfileForm {
    pub fn from_shop(shop: &crate::models::ShopProfile) -> Self {
        Self {
            shop_name: shop.shop_name.clone(),
            owner_name: shop.owner_name.clone(),
            phone: shop.phone.clone(),
            address: shop.address.clone(),
            gstin: shop.gstin.clone().unwrap_or_default(),
            default_gst_rate: crate::money::paise_to_input(shop.gst_rate_bp),
            logo: None,
            logo_removed: false,
        }
    }

    pub fn set_field(&mut self, field: ProfileField, value: String) {
        match field {
            ProfileField::ShopName => self.shop_name = value,
            ProfileField::OwnerName => self.owner_name = value,
            ProfileField::Phone => self.phone = value,
            ProfileField::Address => self.address = value,
            ProfileField::Gstin => self.gstin = value,
            ProfileField::DefaultGstRate => self.default_gst_rate = value,
        }
    }

    pub fn get_field(&self, field: ProfileField) -> String {
        match field {
            ProfileField::ShopName => self.shop_name.clone(),
            ProfileField::OwnerName => self.owner_name.clone(),
            ProfileField::Phone => self.phone.clone(),
            ProfileField::Address => self.address.clone(),
            ProfileField::Gstin => self.gstin.clone(),
            ProfileField::DefaultGstRate => self.default_gst_rate.clone(),
        }
    }
}

#[derive(Default)]
pub struct SecurityForm {
    pub current_pin: String,
    pub new_pin: String,
    pub confirm_pin: String,
}

impl SecurityForm {
    pub fn set_field(&mut self, field: SecurityField, value: String) {
        match field {
            SecurityField::CurrentPin => self.current_pin = value,
            SecurityField::NewPin => self.new_pin = value,
            SecurityField::ConfirmPin => self.confirm_pin = value,
        }
    }

    pub fn get_field(&self, field: SecurityField) -> String {
        match field {
            SecurityField::CurrentPin => self.current_pin.clone(),
            SecurityField::NewPin => self.new_pin.clone(),
            SecurityField::ConfirmPin => self.confirm_pin.clone(),
        }
    }
}

pub fn choose_logo() -> Task<Message> {
    Task::perform(
        async {
            let handle = rfd::AsyncFileDialog::new()
                .set_title("Choose a shop logo")
                .add_filter("Image", &["png", "jpg", "jpeg"])
                .pick_file()
                .await;
            let Some(handle) = handle else {
                return Ok(None);
            };
            tokio::fs::read(handle.path()).await.map(Some).map_err(|e| e.to_string())
        },
        Message::ProfileLogoPicked,
    )
}

pub fn submit_profile(state: &mut State) -> Task<Message> {
    let form = &state.profile_form;
    if form.shop_name.trim().is_empty() {
        state.status = Some("shop name cannot be empty".into());
        return Task::none();
    }
    let gst_rate_bp = if form.default_gst_rate.trim().is_empty() {
        0
    } else {
        match crate::money::rupees_to_paise(&form.default_gst_rate) {
            Some(v) => v,
            None => {
                state.status = Some("default GST rate must be a valid percentage, e.g. 18 or 18.00".into());
                return Task::none();
            }
        }
    };
    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };
    let shop_name = form.shop_name.trim().to_string();
    let owner_name = form.owner_name.trim().to_string();
    let phone = form.phone.trim().to_string();
    let address = form.address.trim().to_string();
    let gstin = form.gstin.trim().to_string();
    let gstin = if gstin.is_empty() { None } else { Some(gstin) };
    let logo = form.logo.clone();
    let logo_removed = form.logo_removed;

    Task::perform(
        async move {
            let profile = crate::repo::update_profile(&pool, &shop_name, &owner_name, &phone, &address, gstin.as_deref(), gst_rate_bp).await?;
            if logo_removed {
                crate::repo::update_logo(&pool, None).await?;
            } else if let Some(bytes) = &logo {
                crate::repo::update_logo(&pool, Some(bytes)).await?;
            }
            Ok(profile)
        },
        |result: Result<crate::models::ShopProfile, crate::repo::RepoError>| Message::ProfileSaved(result.map_err(|e| e.to_string())),
    )
}

pub fn submit_pin(state: &mut State) -> Task<Message> {
    let form = &state.security_form;
    let has_existing_pin = state.shop.as_ref().and_then(|s| s.pin.as_deref()).is_some();

    if has_existing_pin {
        let expected = state.shop.as_ref().and_then(|s| s.pin.as_deref()).unwrap_or("");
        if form.current_pin.trim() != expected {
            state.status = Some("current PIN is incorrect".into());
            return Task::none();
        }
    }

    let new_pin = form.new_pin.trim();
    let new_pin = if new_pin.is_empty() {
        None
    } else {
        if !(4..=6).contains(&new_pin.len()) || !new_pin.chars().all(|c| c.is_ascii_digit()) {
            state.status = Some("new PIN must be 4 to 6 digits".into());
            return Task::none();
        }
        if new_pin != form.confirm_pin.trim() {
            state.status = Some("new PIN and confirmation don't match".into());
            return Task::none();
        }
        Some(new_pin.to_string())
    };

    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };
    Task::perform(
        async move { crate::repo::update_pin(&pool, new_pin.as_deref()).await.map(|_| new_pin) },
        |result| Message::PinChanged(result.map_err(|e| e.to_string())),
    )
}

pub fn view(state: &State) -> Element<'_, Message> {
    let content = match state.settings_tab {
        SettingsTab::Profile => profile_view(state),
        SettingsTab::Security => security_view(state),
    };

    let tabs = row![
        tab_button("Profile", SettingsTab::Profile, state.settings_tab),
        tab_button("Security", SettingsTab::Security, state.settings_tab),
    ]
    .spacing(8)
    .padding(12);

    column![tabs, content].width(Length::Fill).height(Length::Fill).into()
}

fn tab_button(label: &str, target: SettingsTab, current: SettingsTab) -> Element<'static, Message> {
    let btn = button(text(label.to_string())).on_press(Message::SettingsTabSelected(target));
    if target == current {
        btn.style(theme::primary_button).into()
    } else {
        btn.style(theme::secondary_button).into()
    }
}

fn profile_view(state: &State) -> Element<'_, Message> {
    let form = &state.profile_form;

    let logo_preview: Element<'_, Message> = if let Some(bytes) = &form.logo {
        image::Image::new(image::Handle::from_bytes(bytes.clone())).width(96).height(96).content_fit(iced::ContentFit::Cover).into()
    } else if form.logo_removed {
        container(text("No logo").size(13)).width(96).height(96).style(theme::card).padding(theme::SPACE_SM).align_x(iced::Alignment::Center).align_y(iced::Alignment::Center).into()
    } else if state.shop.as_ref().map(|s| s.has_logo).unwrap_or(false) {
        match &state.shop_logo {
            Some(bytes) => image::Image::new(image::Handle::from_bytes(bytes.clone())).width(96).height(96).content_fit(iced::ContentFit::Cover).into(),
            None => container(text("Loading...").size(13)).width(96).height(96).style(theme::card).padding(theme::SPACE_SM).into(),
        }
    } else {
        container(text("No logo").size(13)).width(96).height(96).style(theme::card).padding(theme::SPACE_SM).align_x(iced::Alignment::Center).align_y(iced::Alignment::Center).into()
    };

    let logo_controls = column![
        logo_preview,
        row![
            button(text("Choose Logo").size(13)).style(theme::secondary_button).padding([8, 14]).on_press(Message::ChooseProfileLogo),
            button(text("Remove").size(13)).style(theme::secondary_button).padding([8, 14]).on_press(Message::RemoveProfileLogo),
        ]
        .spacing(theme::SPACE_SM),
    ]
    .spacing(theme::SPACE_SM);

    let fields = column![
        text("Shop Profile").size(20),
        labeled("Shop name *", text_input("Shop name", &form.shop_name).on_input(|v| Message::ProfileFieldChanged(ProfileField::ShopName, v)).padding(10).size(16)),
        labeled("Owner name", text_input("Owner name", &form.owner_name).on_input(|v| Message::ProfileFieldChanged(ProfileField::OwnerName, v)).padding(10).size(16)),
        labeled("Phone", text_input("Phone", &form.phone).on_input(|v| Message::ProfileFieldChanged(ProfileField::Phone, v)).padding(10).size(16)),
        labeled("Address", text_input("Address", &form.address).on_input(|v| Message::ProfileFieldChanged(ProfileField::Address, v)).padding(10).size(16)),
        labeled("GSTIN (optional)", text_input("e.g. 22AAAAA0000A1Z5", &form.gstin).on_input(|v| Message::ProfileFieldChanged(ProfileField::Gstin, v)).padding(10).size(16)),
        labeled(
            "Default GST rate % (applies unless an item overrides it)",
            text_input("e.g. 18", &form.default_gst_rate).on_input(|v| Message::ProfileFieldChanged(ProfileField::DefaultGstRate, v)).padding(10).size(16),
        ),
        labeled("Logo (optional)", logo_controls),
        button(text("Save Profile").size(15)).style(theme::success_button).padding([10, 24]).on_press(Message::SubmitProfile),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(420);

    scrollable(
        container(container(fields).style(theme::card).padding(theme::SPACE_LG))
            .width(Length::Fill)
            .padding(theme::SPACE_MD)
            .align_x(iced::Alignment::Center),
    )
    .height(Length::Fill)
    .into()
}

fn security_view(state: &State) -> Element<'_, Message> {
    let form = &state.security_form;
    let has_existing_pin = state.shop.as_ref().and_then(|s| s.pin.as_deref()).is_some();

    let mut fields = column![
        text("Security").size(20),
        text("Set a PIN to keep a casual passerby from opening this screen — this is a soft screen lock, not account security.").size(13).color(theme::MUTED_TEXT),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(420);

    if has_existing_pin {
        fields = fields.push(labeled(
            "Current PIN",
            text_input("Enter current PIN", &form.current_pin).on_input(|v| Message::SecurityFieldChanged(SecurityField::CurrentPin, v)).secure(true).padding(10).size(16),
        ));
    }
    fields = fields.push(labeled(
        "New PIN (blank to remove)",
        text_input("4-6 digits", &form.new_pin).on_input(|v| Message::SecurityFieldChanged(SecurityField::NewPin, v)).secure(true).padding(10).size(16),
    ));
    fields = fields.push(labeled(
        "Confirm new PIN",
        text_input("repeat PIN", &form.confirm_pin).on_input(|v| Message::SecurityFieldChanged(SecurityField::ConfirmPin, v)).secure(true).padding(10).size(16),
    ));
    fields = fields.push(button(text("Save").size(15)).style(theme::success_button).padding([10, 24]).on_press(Message::SubmitPinChange));

    scrollable(
        container(container(fields).style(theme::card).padding(theme::SPACE_LG))
            .width(Length::Fill)
            .padding(theme::SPACE_MD)
            .align_x(iced::Alignment::Center),
    )
    .height(Length::Fill)
    .into()
}

fn labeled<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(14), widget.into()].spacing(4).into()
}
