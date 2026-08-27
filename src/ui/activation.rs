use iced::widget::{button, column, container, row, svg, text, text_input};
use iced::{Element, Length, Task};

use super::{Message, State};
use crate::ui::theme;

#[derive(Debug, Clone, Default)]
pub struct ActivationState {
    /// This machine's permanent device id — generated once, shown here so
    /// the shopkeeper can send it to whoever issues their license key.
    pub device_id: String,
    pub key_input: String,
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
    Valid { expiry_warning: Option<String> },
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
            Ok(payload) => Ok(Outcome::Valid { expiry_warning: expiry_warning(&payload) }),
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
    let logo = svg(super::logo_handle()).width(72).height(72);

    let device_id_row = row![
        text_input("", &activation.device_id).padding(10).size(15),
        button(text("Copy").size(14)).style(theme::secondary_button).padding([10, 16]).on_press(Message::CopyDeviceId),
    ]
    .spacing(theme::SPACE_SM);

    let mut fields = column![
        text("Activate Srotas Desk").size(26),
        text("This computer's Device ID — send it to us to receive your license key.").size(14).color(theme::MUTED_TEXT),
        device_id_row,
        labeled(
            "License key",
            text_input("Paste your license key here", &activation.key_input)
                .on_input(Message::ActivationKeyChanged)
                .padding(10)
                .size(15),
        ),
        button(text("Activate").size(16)).style(theme::primary_button).padding([12, 24]).on_press(Message::SubmitActivation),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(480);

    if let Some(error) = &activation.error {
        fields = fields.push(text(error).size(13).color(iced::Color::from_rgb(0.83, 0.16, 0.16)));
    }

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
    column![text(label).size(13), widget.into()].spacing(4).into()
}
