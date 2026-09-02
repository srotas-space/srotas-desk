use iced::widget::{button, checkbox, column, container, row, svg, text, text_input};
use iced::{Element, Length, Task};

use super::{Message, State};
use crate::ui::theme;

/// Where the Terms & Conditions checkbox below links out to — kept in sync
/// by hand with `business/fe/open-source`'s `/products/desk/tnc` page;
/// there's no shared source of truth between the two projects.
pub const TNC_URL: &str = "https://open-source.srotas.space/products/desk/tnc";

#[derive(Debug, Clone, Default)]
pub struct ActivationState {
    /// This machine's permanent device id — generated once, shown here so
    /// the shopkeeper can send it to whoever issues their license key.
    pub device_id: String,
    pub key_input: String,
    pub agreed_to_tnc: bool,
    pub error: Option<String>,
}

/// What `check_license`/`submit` found, driving whether the app can move
/// past the Activation screen. Carries `device_id` even when invalid so
/// the screen always has something to display.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// `expiry_warning` is `Some` when the license is still valid but
    /// expires soon — see `expiry_warning` below. The app proceeds either
    /// way; this is a heads-up, not a block.
    ///
    /// `device_id` rides along even on the happy path because the
    /// forgotten-PIN reset needs it to check a license key, long after the
    /// activation screen has been left behind.
    Valid { device_id: String, expiry_warning: Option<String> },
    NeedsActivation { device_id: String, message: Option<String> },
}

/// How far ahead of a license's actual expiry to start warning — enough
/// runway for a shopkeeper to notice, contact us, and get a renewal key
/// signed before the app hard-blocks on `LicenseError::Expired`.
const EXPIRY_WARNING_WINDOW_DAYS: i64 = 14;

fn expiry_warning(payload: &crate::license::LicensePayload) -> Option<String> {
    let expires_at = payload.expires_at?;
    let days_left = (expires_at - chrono::Utc::now()).num_days();
    if days_left > EXPIRY_WARNING_WINDOW_DAYS {
        return None;
    }
    Some(format!(
        "Your license expires on {} ({} day{} left) — contact us for a renewal key before then.",
        expires_at.format("%d %b %Y"),
        days_left.max(0),
        if days_left == 1 { "" } else { "s" },
    ))
}

/// Re-checked on every launch (not just the first) — see `ui/mod.rs`'s
/// `DbReady` handler. A license that was valid last time but has since
/// expired must still block the app.
pub async fn check(pool: sqlx::SqlitePool) -> Result<Outcome, String> {
    let row = crate::repo::get_or_create(&pool).await.map_err(|e| e.to_string())?;
    match row.key_text {
        Some(key_text) => match crate::license::verify(&key_text, &row.device_id, chrono::Utc::now()) {
            Ok(payload) => {
                Ok(Outcome::Valid { device_id: row.device_id.clone(), expiry_warning: expiry_warning(&payload) })
            }
            Err(e) => Ok(Outcome::NeedsActivation { device_id: row.device_id, message: Some(e.to_string()) }),
        },
        None => Ok(Outcome::NeedsActivation { device_id: row.device_id, message: None }),
    }
}

pub fn submit(state: &mut State) -> Task<Message> {
    let key_text = state.activation.key_input.trim().to_string();
    if key_text.is_empty() {
        state.activation.error = Some("enter your license key".into());
        return Task::none();
    }

    if let Err(e) = crate::license::verify(&key_text, &state.activation.device_id, chrono::Utc::now()) {
        state.activation.error = Some(e.to_string());
        return Task::none();
    }

    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };
    Task::perform(
        async move { crate::repo::activate(&pool, &key_text).await.map_err(|e| e.to_string()) },
        Message::ActivationCompleted,
    )
}

pub fn view(state: &State) -> Element<'_, Message> {
    let activation = &state.activation;

    let device_id_row = row![
        text_input("", &activation.device_id)
            .style(theme::field_readonly)
            .padding(theme::FIELD_PADDING)
            .size(theme::TEXT_BODY),
        button(text("Copy").size(theme::TEXT_SMALL))
            .style(theme::secondary_button)
            .padding(theme::CONTROL_PADDING)
            .on_press(Message::CopyDeviceId),
    ]
    .spacing(theme::SPACE_SM)
    .align_y(iced::Alignment::Center);

    let tnc_row = row![
        checkbox::Checkbox::new(activation.agreed_to_tnc).label("I agree to the").on_toggle(Message::ActivationTncToggled),
        button(text("Terms & Conditions").size(theme::TEXT_SMALL))
            .style(theme::link_button)
            .padding(0)
            .on_press(Message::OpenTnc),
    ]
    .spacing(theme::SPACE_SM)
    .align_y(iced::Alignment::Center);

    let mut fields = column![
        column![
            text("Activate Srotas Desk").size(theme::TEXT_DISPLAY).font(theme::SEMIBOLD),
            text("One key, one computer — no internet needed after this.")
                .size(theme::TEXT_SMALL)
                .color(theme::MUTED_TEXT),
        ]
        .spacing(theme::SPACE_XS)
        .align_x(iced::Alignment::Center)
        .width(Length::Fill),
        labeled_with_hint(
            "This computer's Device ID",
            "Send it to us and we'll issue a key bound to this machine.",
            device_id_row,
        ),
        labeled(
            "License key",
            text_input("Paste your license key here", &activation.key_input)
                .on_input(Message::ActivationKeyChanged)
                .on_submit(Message::SubmitActivation)
                .style(theme::field)
                .padding(theme::FIELD_PADDING)
                .size(theme::TEXT_BODY),
        ),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(480);

    if let Some(error) = &activation.error {
        fields = fields.push(
            container(text(error).size(theme::TEXT_SMALL))
                .style(theme::notice_error)
                .padding([theme::SPACE_SM as u16, theme::SPACE_MD as u16])
                .width(Length::Fill),
        );
    }

    fields = fields.push(tnc_row);
    fields = fields.push(
        button(text("Activate").size(theme::TEXT_BODY).font(theme::SEMIBOLD))
            .style(theme::primary_button)
            .padding([13, 28])
            .on_press_maybe(activation.agreed_to_tnc.then_some(Message::SubmitActivation)),
    );

    let card = container(
        column![svg(super::logo_handle()).width(64).height(64), fields]
            .spacing(theme::SPACE_MD)
            .align_x(iced::Alignment::Center),
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn payload(expires_at: Option<chrono::DateTime<chrono::Utc>>) -> crate::license::LicensePayload {
        crate::license::LicensePayload { shop_name: "Test Shop".into(), issued_at: chrono::Utc::now(), expires_at }
    }

    #[test]
    fn no_warning_for_a_perpetual_license() {
        assert!(expiry_warning(&payload(None)).is_none());
    }

    #[test]
    fn no_warning_when_expiry_is_well_in_the_future() {
        let expires = chrono::Utc::now() + Duration::days(EXPIRY_WARNING_WINDOW_DAYS + 5);
        assert!(expiry_warning(&payload(Some(expires))).is_none());
    }

    #[test]
    fn warns_once_inside_the_window() {
        let expires = chrono::Utc::now() + Duration::days(EXPIRY_WARNING_WINDOW_DAYS - 1);
        let warning = expiry_warning(&payload(Some(expires))).expect("should warn inside the window");
        assert!(warning.contains("expires on"));
    }

    #[test]
    fn still_warns_right_up_to_expiry() {
        let expires = chrono::Utc::now() + Duration::hours(1);
        let warning = expiry_warning(&payload(Some(expires))).expect("should still warn just before expiry");
        assert!(warning.contains("0 day"));
    }
}

fn labeled<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(theme::TEXT_SMALL).color(theme::MUTED_TEXT), widget.into()]
        .spacing(theme::SPACE_XS)
        .width(Length::Fill)
        .into()
}

fn labeled_with_hint<'a>(
    label: &'a str,
    hint: &'a str,
    widget: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    column![
        text(label).size(theme::TEXT_SMALL).color(theme::MUTED_TEXT),
        widget.into(),
        text(hint).size(theme::TEXT_CAPTION).color(theme::MUTED_TEXT),
    ]
    .spacing(theme::SPACE_XS)
    .width(Length::Fill)
    .into()
}
