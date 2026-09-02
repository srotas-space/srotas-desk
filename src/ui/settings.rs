use iced::widget::{button, column, container, image, row, scrollable, text, text_input};
use iced::{Element, Length, Task};

use super::{Message, Notice, State};
use crate::ui::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Profile,
    Security,
    Performance,
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
        state.notice = Some(Notice::error("shop name cannot be empty"));
        return Task::none();
    }
    let gst_rate_bp = if form.default_gst_rate.trim().is_empty() {
        0
    } else {
        match crate::money::rupees_to_paise(&form.default_gst_rate) {
            Some(v) => v,
            None => {
                state.notice = Some(Notice::error("default GST rate must be a valid percentage, e.g. 18 or 18.00"));
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

/// Sets, changes, or removes the screen-lock PIN.
///
/// The format checks stay here (they're instant, and the shopkeeper should
/// see "PIN must be 4 to 6 digits" the moment they press Save), but both
/// the current-PIN check and the new hash are Argon2 work — tens of
/// milliseconds each — so they happen off the UI thread.
pub fn submit_pin(state: &mut State) -> Task<Message> {
    let form = &state.security_form;
    let has_existing_pin = state.shop.as_ref().is_some_and(|s| s.has_pin());

    let new_pin = match crate::pin::validate_new(&form.new_pin, &form.confirm_pin) {
        Ok(pin) => pin,
        Err(e) => {
            state.notice = Some(Notice::error(e));
            return Task::none();
        }
    };
    if has_existing_pin && form.current_pin.trim().is_empty() {
        state.notice = Some(Notice::error("enter your current PIN to change it"));
        return Task::none();
    }

    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };
    let current_pin = form.current_pin.trim().to_string();

    Task::perform(
        async move {
            if has_existing_pin {
                let stored = crate::repo::get_pin_hash(&pool).await.map_err(|e| e.to_string())?;
                let matches = match stored {
                    Some(stored) => tokio::task::spawn_blocking(move || crate::pin::verify(&current_pin, &stored))
                        .await
                        .map_err(|e| format!("could not check that PIN: {e}"))?,
                    // The PIN vanished from under us (a reset in another
                    // window); nothing left to prove.
                    None => true,
                };
                if !matches {
                    return Err("current PIN is incorrect".to_string());
                }
            }

            let hash = match new_pin {
                Some(pin) => Some(
                    tokio::task::spawn_blocking(move || crate::pin::hash(&pin))
                        .await
                        .map_err(|e| format!("could not secure that PIN: {e}"))??,
                ),
                None => None,
            };
            crate::repo::update_pin(&pool, hash.as_deref()).await.map_err(|e| e.to_string())?;
            Ok(hash)
        },
        Message::PinChanged,
    )
}

pub fn view(state: &State) -> Element<'_, Message> {
    let content = match state.settings_tab {
        SettingsTab::Profile => profile_view(state),
        SettingsTab::Security => security_view(state),
        SettingsTab::Performance => performance_view(state),
    };

    let tabs = container(
        container(
            row![
                tab_button("Profile", SettingsTab::Profile, state.settings_tab),
                tab_button("Security", SettingsTab::Security, state.settings_tab),
                tab_button("Performance", SettingsTab::Performance, state.settings_tab),
            ]
            .spacing(theme::SPACE_XS),
        )
        .style(theme::panel)
        .padding(theme::SPACE_XS as u16),
    )
    .padding([theme::SPACE_MD as u16, theme::SPACE_MD as u16]);

    column![tabs, content].width(Length::Fill).height(Length::Fill).into()
}

fn tab_button(label: &str, target: SettingsTab, current: SettingsTab) -> Element<'static, Message> {
    let btn = button(text(label.to_string()).size(theme::TEXT_SMALL).font(theme::SEMIBOLD))
        .padding([9, 20])
        .on_press(Message::SettingsTabSelected(target));
    if target == current {
        btn.style(theme::tab_selected).into()
    } else {
        btn.style(theme::tab_idle).into()
    }
}

/// The shared frame around both settings tabs — a centred card on a
/// scrollable page, so the two never drift apart in width or padding.
fn form_page<'a>(fields: iced::widget::Column<'a, Message>) -> Element<'a, Message> {
    scrollable(
        container(container(fields).style(theme::card).padding(theme::SPACE_LG))
            .width(Length::Fill)
            .padding([0, theme::SPACE_MD as u16])
            .align_x(iced::Alignment::Center),
    )
    .height(Length::Fill)
    .into()
}

fn profile_view(state: &State) -> Element<'_, Message> {
    let form = &state.profile_form;

    let logo_preview: Element<'_, Message> = if let Some(bytes) = &form.logo {
        logo_image(bytes)
    } else if form.logo_removed {
        logo_placeholder("No logo")
    } else if state.shop.as_ref().is_some_and(|s| s.has_logo) {
        match &state.shop_logo {
            Some(bytes) => logo_image(bytes),
            None => logo_placeholder("Loading..."),
        }
    } else {
        logo_placeholder("No logo")
    };

    let logo_controls = row![
        logo_preview,
        column![
            text("Shown in the app header and on printed bills.").size(theme::TEXT_SMALL).color(theme::MUTED_TEXT),
            row![
                button(text("Choose Logo").size(theme::TEXT_SMALL))
                    .style(theme::secondary_button)
                    .padding([8, 16])
                    .on_press(Message::ChooseProfileLogo),
                button(text("Remove").size(theme::TEXT_SMALL))
                    .style(theme::secondary_button)
                    .padding([8, 16])
                    .on_press(Message::RemoveProfileLogo),
            ]
            .spacing(theme::SPACE_SM),
        ]
        .spacing(theme::SPACE_SM),
    ]
    .spacing(theme::SPACE_MD)
    .align_y(iced::Alignment::Center);

    let fields = column![
        text("Shop Profile").size(theme::TEXT_TITLE).font(theme::SEMIBOLD),
        text("These details head every bill and receipt you print.").size(theme::TEXT_SMALL).color(theme::MUTED_TEXT),
        labeled("Shop name *", profile_input("Shop name", &form.shop_name, ProfileField::ShopName)),
        labeled("Owner name", profile_input("Owner name", &form.owner_name, ProfileField::OwnerName)),
        labeled("Phone", profile_input("Phone", &form.phone, ProfileField::Phone)),
        labeled("Address", profile_input("Shop address", &form.address, ProfileField::Address)),
        labeled("GSTIN (optional)", profile_input("e.g. 22AAAAA0000A1Z5", &form.gstin, ProfileField::Gstin)),
        labeled(
            "Default GST rate % (applies unless an item overrides it)",
            profile_input("e.g. 18", &form.default_gst_rate, ProfileField::DefaultGstRate),
        ),
        labeled("Logo (optional)", logo_controls),
        button(text("Save Profile").size(theme::TEXT_BODY).font(theme::SEMIBOLD))
            .style(theme::success_button)
            .padding(theme::CONTROL_PADDING)
            .on_press(Message::SubmitProfile),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(440);

    form_page(fields)
}

fn profile_input<'a>(placeholder: &'a str, value: &'a str, field: ProfileField) -> Element<'a, Message> {
    text_input(placeholder, value)
        .on_input(move |v| Message::ProfileFieldChanged(field, v))
        .style(theme::field)
        .padding(theme::FIELD_PADDING)
        .size(theme::TEXT_BODY)
        .into()
}

fn logo_image(bytes: &[u8]) -> Element<'static, Message> {
    image::Image::new(image::Handle::from_bytes(bytes.to_vec()))
        .width(88)
        .height(88)
        .content_fit(iced::ContentFit::Cover)
        .into()
}

fn logo_placeholder(label: &str) -> Element<'static, Message> {
    container(text(label.to_string()).size(theme::TEXT_SMALL).color(theme::MUTED_TEXT))
        .width(88)
        .height(88)
        .style(theme::panel)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
}

/// How the catalogue is held. Two honest options rather than a hidden
/// heuristic: the shopkeeper knows their machine and their catalogue
/// better than a guess baked into the code would.
fn performance_view(state: &State) -> Element<'_, Message> {
    let preload = state.settings.preload_catalogue;

    let option = |selected: bool, title: &'static str, detail: &'static str, on: bool| {
        // The selected card is filled violet, so its text has to be light
        // — the muted grey used on the unselected cards is unreadable
        // against it.
        let (mark_colour, detail_colour) = if selected {
            (iced::Color::WHITE, iced::Color::from_rgba(1.0, 1.0, 1.0, 0.85))
        } else {
            (theme::MUTED_TEXT, theme::MUTED_TEXT)
        };
        button(
            row![
                text(if selected { "●" } else { "○" }).size(theme::TEXT_BODY).color(mark_colour),
                column![
                    text(title).size(theme::TEXT_BODY).font(theme::SEMIBOLD),
                    text(detail).size(theme::TEXT_SMALL).color(detail_colour),
                ]
                .spacing(theme::SPACE_XS),
            ]
            .spacing(theme::SPACE_SM)
            .align_y(iced::Alignment::Start),
        )
        .style(if selected { theme::tab_selected } else { theme::secondary_button })
        .padding(theme::SPACE_MD)
        .width(Length::Fill)
        .on_press(Message::PreloadToggled(on))
    };

    let fields = column![
        text("Performance").size(theme::TEXT_TITLE).font(theme::SEMIBOLD),
        text(
            "How the app gets items onto a screen. Both show the same items — \
             they differ in when the work happens."
        )
        .size(theme::TEXT_SMALL)
        .color(theme::MUTED_TEXT),
        option(
            !preload,
            "Load items as needed",
            "Recommended. Each screen asks for just the rows it shows, and searching \
             queries as you type. Uses very little memory, and works the same whether \
             you stock fifty items or a hundred thousand.",
            false,
        ),
        option(
            preload,
            "Keep the catalogue in memory",
            "Loads every item once when the app starts, so searching and paging are \
             instant with no lookup. Uses more memory and makes startup slower — worth \
             it on a machine with RAM to spare.",
            true,
        ),
        container(
            column![
                text(if preload { "Currently: kept in memory" } else { "Currently: loaded as needed" })
                    .size(theme::TEXT_SMALL)
                    .font(theme::SEMIBOLD),
                text(if preload {
                    "Switching this off frees the memory straight away."
                } else {
                    "Nothing is loaded until a screen needs it."
                })
                .size(theme::TEXT_SMALL)
                .color(theme::MUTED_TEXT),
            ]
            .spacing(theme::SPACE_XS),
        )
        .style(theme::panel)
        .padding(theme::SPACE_MD)
        .width(Length::Fill),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(440);

    form_page(fields)
}

fn security_view(state: &State) -> Element<'_, Message> {
    let form = &state.security_form;
    let has_existing_pin = state.shop.as_ref().is_some_and(|s| s.has_pin());

    let mut fields = column![
        text("Security").size(theme::TEXT_TITLE).font(theme::SEMIBOLD),
        text(
            "A PIN locks this screen when you step away from the counter. \
             It's stored as a one-way hash, so it stays unreadable even in a backup copy \
             of the shop database."
        )
        .size(theme::TEXT_SMALL)
        .color(theme::MUTED_TEXT),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(440);

    if has_existing_pin {
        fields = fields.push(labeled(
            "Current PIN",
            text_input("Enter current PIN", &form.current_pin)
                .on_input(|v| Message::SecurityFieldChanged(SecurityField::CurrentPin, v))
                .secure(true)
                .style(theme::field)
                .padding(theme::FIELD_PADDING)
                .size(theme::TEXT_BODY),
        ));
    }
    fields = fields.push(labeled(
        "New PIN (blank to remove the lock)",
        text_input("4-6 digits", &form.new_pin)
            .on_input(|v| Message::SecurityFieldChanged(SecurityField::NewPin, v))
            .secure(true)
            .style(theme::field)
            .padding(theme::FIELD_PADDING)
            .size(theme::TEXT_BODY),
    ));
    fields = fields.push(labeled(
        "Confirm new PIN",
        text_input("repeat PIN", &form.confirm_pin)
            .on_input(|v| Message::SecurityFieldChanged(SecurityField::ConfirmPin, v))
            .on_submit(Message::SubmitPinChange)
            .secure(true)
            .style(theme::field)
            .padding(theme::FIELD_PADDING)
            .size(theme::TEXT_BODY),
    ));

    fields = fields.push(
        container(
            column![
                text(format!(
                    "After {} wrong PINs the lock screen pauses for 30 seconds, doubling with each \
                     further attempt.",
                    crate::pin::MAX_ATTEMPTS
                ))
                .size(theme::TEXT_SMALL),
                text(
                    "Forgotten the PIN? The lock screen's \"Forgot PIN?\" link resets it using \
                     your license key."
                )
                .size(theme::TEXT_SMALL),
            ]
            .spacing(theme::SPACE_XS),
        )
        .style(theme::panel)
        .padding(theme::SPACE_MD)
        .width(Length::Fill),
    );

    fields = fields.push(
        button(text("Save").size(theme::TEXT_BODY).font(theme::SEMIBOLD))
            .style(theme::success_button)
            .padding(theme::CONTROL_PADDING)
            .on_press(Message::SubmitPinChange),
    );

    form_page(fields)
}

fn labeled<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(theme::TEXT_SMALL).color(theme::MUTED_TEXT), widget.into()]
        .spacing(theme::SPACE_XS)
        .into()
}
