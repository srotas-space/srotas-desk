//! The screen-lock screen: PIN entry, the failed-attempt lockout, and the
//! license-key reset path for a forgotten PIN.
//!
//! Verification is deliberately asynchronous. The stored PIN is an Argon2
//! hash (see `crate::pin`), and checking one takes tens of milliseconds —
//! long enough to visibly stutter the window if it ran inline in `update`.
use chrono::{DateTime, Utc};
use iced::widget::{button, column, container, row, rule, svg, text, text_input};
use iced::{Element, Length, Task};
use sqlx::SqlitePool;

use super::{Message, Notice, State};
use crate::ui::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetField {
    Key,
    NewPin,
    ConfirmPin,
}

#[derive(Debug, Default)]
pub struct LoginState {
    pub pin_input: String,
    pub error: Option<String>,
    /// True while an Argon2 verification is in flight — the Unlock button
    /// goes disabled so a second Enter doesn't queue up another check.
    pub verifying: bool,

    /// Whether the "Forgot PIN?" panel is expanded.
    pub reset_open: bool,
    pub reset_key: String,
    pub reset_pin: String,
    pub reset_confirm: String,
    pub reset_error: Option<String>,
    pub resetting: bool,
}

impl LoginState {
    pub fn set_field(&mut self, field: ResetField, value: String) {
        match field {
            ResetField::Key => self.reset_key = value,
            ResetField::NewPin => self.reset_pin = value,
            ResetField::ConfirmPin => self.reset_confirm = value,
        }
    }

    /// Wipes everything typed here — called on a successful unlock and
    /// whenever the app locks again, so a PIN never lingers in memory (or
    /// on screen) after it's been used.
    pub fn clear(&mut self) {
        *self = LoginState::default();
    }
}

/// What a verification attempt concluded.
#[derive(Debug, Clone)]
pub enum Outcome {
    Unlocked,
    /// Wrong PIN. `attempts_left` counts down to the lockout; once it hits
    /// zero `locked_until` is set.
    Rejected { attempts_left: i64, locked_until: Option<DateTime<Utc>> },
    /// The screen was already locked when the attempt arrived — checked
    /// against the database, not the in-memory profile, so relaunching the
    /// app can't shake off a lockout.
    Locked { locked_until: DateTime<Utc> },
}

pub fn submit(state: &mut State) -> Task<Message> {
    if state.login.verifying {
        return Task::none();
    }
    let Some(shop) = &state.shop else {
        return Task::none();
    };

    if let Some(secs) = shop.lock_remaining_secs(Utc::now()) {
        state.login.error = Some(lock_message(secs));
        return Task::none();
    }
    if !shop.has_pin() {
        return Task::done(Message::LoginVerified(Ok(Outcome::Unlocked)));
    }

    let pin = state.login.pin_input.trim().to_string();
    if pin.is_empty() {
        state.login.error = Some("Enter your PIN to unlock.".into());
        return Task::none();
    }
    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };

    state.login.verifying = true;
    state.login.error = None;
    Task::perform(check_pin(pool, pin), Message::LoginVerified)
}

async fn check_pin(pool: SqlitePool, pin: String) -> Result<Outcome, String> {
    // Re-read the lock from the database rather than trusting whatever the
    // screen is holding: the in-memory profile starts empty on every
    // launch, so a memory-only lockout would last exactly until the
    // shopkeeper quit and reopened the app.
    let locked_until = crate::repo::get_pin_lock(&pool).await.map_err(|e| e.to_string())?;
    if let Some(until) = locked_until
        && crate::pin::remaining_lock_secs(Some(until), Utc::now()).is_some()
    {
        return Ok(Outcome::Locked { locked_until: until });
    }

    let Some(stored) = crate::repo::get_pin_hash(&pool).await.map_err(|e| e.to_string())? else {
        // The PIN was removed from Settings while this screen was open.
        return Ok(Outcome::Unlocked);
    };

    let matched = tokio::task::spawn_blocking(move || crate::pin::verify(&pin, &stored))
        .await
        .map_err(|e| format!("could not check that PIN: {e}"))?;

    if matched {
        crate::repo::clear_pin_failures(&pool).await.map_err(|e| e.to_string())?;
        return Ok(Outcome::Unlocked);
    }

    let (attempts, locked_until) = crate::repo::record_failed_pin(&pool).await.map_err(|e| e.to_string())?;
    Ok(Outcome::Rejected { attempts_left: (crate::pin::MAX_ATTEMPTS - attempts).max(0), locked_until })
}

/// Folds a finished verification back into the screen. Returns whatever
/// task should follow an unlock (the daily auto-backup).
pub fn verified(state: &mut State, result: Result<Outcome, String>) -> Task<Message> {
    state.login.verifying = false;

    match result {
        Ok(Outcome::Unlocked) => {
            state.login.clear();
            if let Some(shop) = &mut state.shop {
                shop.pin_failed_attempts = 0;
                shop.pin_locked_until = None;
            }
            state.stage = super::Stage::Home;
            super::backup::maybe_auto_backup(state)
        }
        Ok(Outcome::Rejected { attempts_left, locked_until }) => {
            if let Some(shop) = &mut state.shop {
                shop.pin_locked_until = locked_until;
            }
            state.login.pin_input.clear();
            state.login.error = Some(match crate::pin::remaining_lock_secs(locked_until, Utc::now()) {
                Some(secs) => lock_message(secs),
                None if attempts_left == 1 => "Incorrect PIN — 1 attempt left before this screen locks.".to_string(),
                None => format!("Incorrect PIN — {attempts_left} attempts left before this screen locks."),
            });
            Task::none()
        }
        Ok(Outcome::Locked { locked_until }) => {
            if let Some(shop) = &mut state.shop {
                shop.pin_locked_until = Some(locked_until);
            }
            state.login.pin_input.clear();
            let secs = crate::pin::remaining_lock_secs(Some(locked_until), Utc::now()).unwrap_or(0);
            state.login.error = Some(lock_message(secs));
            Task::none()
        }
        Err(e) => {
            state.login.error = Some(e);
            Task::none()
        }
    }
}

fn lock_message(secs: i64) -> String {
    format!("Too many wrong PINs — try again in {}.", crate::pin::format_remaining(secs))
}

// ------------------------------------------------------------ PIN reset

/// Resets a forgotten PIN. The proof of ownership is the shop's own
/// license key: it's signed, bound to this machine's device id, and the
/// shopkeeper already has it — which makes it the one secret this
/// entirely-offline app can check without a server or a recovery email.
pub fn submit_reset(state: &mut State) -> Task<Message> {
    if state.login.resetting {
        return Task::none();
    }

    let key = state.login.reset_key.trim().to_string();
    if key.is_empty() {
        state.login.reset_error = Some("Paste your license key to confirm this is your shop.".into());
        return Task::none();
    }
    if let Err(e) = crate::license::verify(&key, &state.device_id, Utc::now()) {
        state.login.reset_error = Some(e.to_string());
        return Task::none();
    }

    let new_pin = match crate::pin::validate_new(&state.login.reset_pin, &state.login.reset_confirm) {
        Ok(pin) => pin,
        Err(e) => {
            state.login.reset_error = Some(e);
            return Task::none();
        }
    };

    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };
    state.login.resetting = true;
    state.login.reset_error = None;

    Task::perform(
        async move {
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
        Message::PinResetCompleted,
    )
}

/// Applies a completed reset: `update_pin` already cleared the lockout, so
/// this drops straight into the app rather than bouncing back to a PIN
/// prompt the shopkeeper would then have to satisfy again.
pub fn reset_completed(state: &mut State, result: Result<Option<String>, String>) -> Task<Message> {
    state.login.resetting = false;
    match result {
        Ok(hash) => {
            if let Some(shop) = &mut state.shop {
                shop.pin_hash = hash;
                shop.pin_failed_attempts = 0;
                shop.pin_locked_until = None;
            }
            state.login.clear();
            state.stage = super::Stage::Home;
            state.notice = Some(Notice::success("PIN reset — you're signed in."));
            super::backup::maybe_auto_backup(state)
        }
        Err(e) => {
            state.login.reset_error = Some(e);
            Task::none()
        }
    }
}

// ----------------------------------------------------------------- view

pub fn view(state: &State) -> Element<'_, Message> {
    let Some(shop) = &state.shop else {
        return text("").into();
    };

    let logo: Element<'_, Message> = match &state.shop_logo {
        Some(bytes) => iced::widget::image::Image::new(iced::widget::image::Handle::from_bytes(bytes.clone()))
            .width(76)
            .height(76)
            .content_fit(iced::ContentFit::Cover)
            .into(),
        None => svg(super::logo_handle()).width(76).height(76).into(),
    };

    let mut head = column![logo, text(&shop.shop_name).size(theme::TEXT_DISPLAY).font(theme::SEMIBOLD)]
        .spacing(theme::SPACE_SM)
        .align_x(iced::Alignment::Center);

    if !shop.owner_name.is_empty() {
        head = head.push(text(&shop.owner_name).size(theme::TEXT_SMALL).color(theme::MUTED_TEXT));
    }

    let body: Element<'_, Message> =
        if state.login.reset_open { reset_panel(state) } else { unlock_panel(state, shop) };

    let card = container(column![head, body].spacing(theme::SPACE_LG).align_x(iced::Alignment::Center))
        .style(theme::card)
        .padding(theme::SPACE_LG)
        .width(Length::Fixed(400.0));

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
}

fn unlock_panel<'a>(state: &'a State, shop: &'a crate::models::ShopProfile) -> Element<'a, Message> {
    let locked_secs = shop.lock_remaining_secs(Utc::now());
    let mut body = column![].spacing(theme::SPACE_MD).align_x(iced::Alignment::Center).width(Length::Fill);

    if shop.has_pin() {
        body = body.push(
            text_input("Enter PIN", &state.login.pin_input)
                .on_input_maybe((locked_secs.is_none() && !state.login.verifying).then_some(Message::LoginPinChanged))
                .on_submit(Message::SubmitLogin)
                .secure(true)
                .style(theme::field)
                .padding(theme::FIELD_PADDING)
                .size(20)
                .width(Length::Fixed(220.0))
                .align_x(iced::Alignment::Center),
        );
    }

    if let Some(secs) = locked_secs {
        body = body.push(
            container(
                text(format!("Locked — try again in {}", crate::pin::format_remaining(secs)))
                    .size(theme::TEXT_SMALL)
                    .font(theme::SEMIBOLD),
            )
            .style(theme::notice_warning)
            .padding([theme::SPACE_SM as u16, theme::SPACE_MD as u16]),
        );
    } else if let Some(error) = &state.login.error {
        body = body.push(
            container(text(error).size(theme::TEXT_SMALL))
                .style(theme::notice_error)
                .padding([theme::SPACE_SM as u16, theme::SPACE_MD as u16]),
        );
    }

    let label = if state.login.verifying {
        "Checking..."
    } else if shop.has_pin() {
        "Unlock"
    } else {
        "Continue"
    };
    let can_submit = locked_secs.is_none() && !state.login.verifying;

    body = body.push(
        button(text(label).size(theme::TEXT_BODY).font(theme::SEMIBOLD))
            .style(theme::primary_button)
            .padding([13, 40])
            .on_press_maybe(can_submit.then_some(Message::SubmitLogin)),
    );

    if shop.has_pin() {
        body = body.push(
            button(text("Forgot PIN?").size(theme::TEXT_SMALL))
                .style(theme::link_button)
                .padding(0)
                .on_press(Message::ForgotPinPressed),
        );
    }

    body.into()
}

fn reset_panel(state: &State) -> Element<'_, Message> {
    let login = &state.login;

    let mut body = column![
        text("Reset your PIN").size(theme::TEXT_TITLE).font(theme::SEMIBOLD),
        text(
            "Paste the license key this copy of Srotas Desk was activated with. \
             It's signed for this computer, so only your shop's key will work."
        )
        .size(theme::TEXT_SMALL)
        .color(theme::MUTED_TEXT),
        labeled(
            "License key",
            text_input("Paste your license key", &login.reset_key)
                .on_input(|v| Message::PinResetFieldChanged(ResetField::Key, v))
                .style(theme::field)
                .padding(theme::FIELD_PADDING)
                .size(theme::TEXT_BODY),
        ),
        rule::horizontal(1).style(theme::divider),
        labeled(
            "New PIN (leave blank to remove the lock)",
            text_input("4-6 digits", &login.reset_pin)
                .on_input(|v| Message::PinResetFieldChanged(ResetField::NewPin, v))
                .secure(true)
                .style(theme::field)
                .padding(theme::FIELD_PADDING)
                .size(theme::TEXT_BODY),
        ),
        labeled(
            "Confirm new PIN",
            text_input("repeat PIN", &login.reset_confirm)
                .on_input(|v| Message::PinResetFieldChanged(ResetField::ConfirmPin, v))
                .on_submit(Message::SubmitPinReset)
                .secure(true)
                .style(theme::field)
                .padding(theme::FIELD_PADDING)
                .size(theme::TEXT_BODY),
        ),
    ]
    .spacing(theme::SPACE_MD)
    .width(Length::Fill);

    if let Some(error) = &login.reset_error {
        body = body.push(
            container(text(error).size(theme::TEXT_SMALL))
                .style(theme::notice_error)
                .padding([theme::SPACE_SM as u16, theme::SPACE_MD as u16])
                .width(Length::Fill),
        );
    }

    body = body.push(
        row![
            button(text("Back").size(theme::TEXT_BODY))
                .style(theme::secondary_button)
                .padding(theme::CONTROL_PADDING)
                .on_press(Message::CancelPinReset),
            iced::widget::space::horizontal(),
            button(text(if login.resetting { "Resetting..." } else { "Reset PIN" }).size(theme::TEXT_BODY).font(theme::SEMIBOLD))
                .style(theme::primary_button)
                .padding(theme::CONTROL_PADDING)
                .on_press_maybe((!login.resetting).then_some(Message::SubmitPinReset)),
        ]
        .align_y(iced::Alignment::Center)
        .width(Length::Fill),
    );

    body.into()
}

fn labeled<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(theme::TEXT_SMALL).color(theme::MUTED_TEXT), widget.into()]
        .spacing(theme::SPACE_XS)
        .into()
}
