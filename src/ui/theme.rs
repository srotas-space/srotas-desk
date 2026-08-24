//! The app's visual identity — a custom `iced::Theme` built from the
//! brand's palette (the same violet/pink/orange used in the logo), plus a
//! handful of style helpers for the bits the built-in styles don't cover
//! (big touch-friendly home tiles, card-style list rows).
use iced::theme::palette;
use iced::widget::{button, container};
use iced::{Background, Border, Color, Shadow, Theme, Vector};

/// Corner radius shared by every button in the app, so a button always
/// reads as a distinct, tappable shape rather than a flat label.
const BUTTON_RADIUS: f32 = 8.0;

pub const VIOLET: Color = Color::from_rgb(0.486, 0.227, 0.929);
pub const PINK: Color = Color::from_rgb(0.925, 0.286, 0.600);
pub const ORANGE: Color = Color::from_rgb(0.984, 0.573, 0.137);

/// Muted label color for small caption-style text (stat labels, hints).
pub const MUTED_TEXT: Color = Color::from_rgb(0.50, 0.48, 0.55);

/// A bold variant of the default font, for figures that should stand out
/// (stat values, big numbers) without switching typeface.
pub const BOLD: iced::Font = iced::Font { weight: iced::font::Weight::Bold, ..iced::Font::DEFAULT };

pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 16.0;
pub const SPACE_LG: f32 = 28.0;

/// Builds the app's theme. Passed to `iced::application(...).theme(...)` —
/// once set, every built-in widget style (buttons, containers, text
/// selection, etc.) derives from this palette automatically.
pub fn theme() -> Theme {
    Theme::custom(
        "Srotas".to_string(),
        iced::theme::Palette {
            background: Color::from_rgb(0.975, 0.968, 0.984),
            text: Color::from_rgb(0.11, 0.10, 0.15),
            primary: VIOLET,
            success: Color::from_rgb(0.13, 0.72, 0.40),
            warning: ORANGE,
            danger: Color::from_rgb(0.83, 0.16, 0.16),
        },
    )
}

/// Shared shape/shadow for every rounded button below — only the colors
/// differ between semantic variants (primary/secondary/success/danger).
fn rounded_button(base: palette::Pair, hovered: palette::Pair, status: button::Status) -> button::Style {
    let pair = if status == button::Status::Hovered { hovered } else { base };

    let style = button::Style {
        background: Some(Background::Color(pair.color)),
        text_color: pair.text,
        border: Border { radius: BUTTON_RADIUS.into(), width: 0.0, color: Color::TRANSPARENT },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.10),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 4.0,
        },
        snap: false,
    };

    if status == button::Status::Disabled {
        button::Style {
            background: style.background.map(|b| b.scale_alpha(0.5)),
            text_color: style.text_color.scale_alpha(0.5),
            ..style
        }
    } else {
        style
    }
}

/// A primary action button (rounded, brand violet).
pub fn primary_button(theme: &Theme, status: button::Status) -> button::Style {
    let p = theme.extended_palette();
    rounded_button(p.primary.base, p.primary.strong, status)
}

/// A secondary/neutral action button (rounded).
pub fn secondary_button(theme: &Theme, status: button::Status) -> button::Style {
    let p = theme.extended_palette();
    rounded_button(p.secondary.base, p.secondary.strong, status)
}

/// A positive/confirming action button (rounded, green) — e.g. "Save".
pub fn success_button(theme: &Theme, status: button::Status) -> button::Style {
    let p = theme.extended_palette();
    rounded_button(p.success.base, p.success.strong, status)
}

/// A destructive action button (rounded, red) — e.g. "Delete".
pub fn danger_button(theme: &Theme, status: button::Status) -> button::Style {
    let p = theme.extended_palette();
    rounded_button(p.danger.base, p.danger.strong, status)
}

/// The "Sell" action button — brand pink, so billing a sale reads as a
/// visually distinct action from stocking-in (green) at a glance.
pub fn accent_button(_theme: &Theme, status: button::Status) -> button::Style {
    let base_color = match status {
        button::Status::Hovered | button::Status::Pressed => Color::from_rgb(0.82, 0.20, 0.51),
        button::Status::Disabled => Color::from_rgba(0.925, 0.286, 0.600, 0.5),
        button::Status::Active => PINK,
    };

    button::Style {
        background: Some(Background::Color(base_color)),
        text_color: Color::WHITE,
        border: Border { radius: BUTTON_RADIUS.into(), width: 0.0, color: Color::TRANSPARENT },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.10),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 4.0,
        },
        snap: false,
    }
}

/// Large, rounded, brand-violet tile button for the Home screen ("Shop" /
/// "Inventory") — sized and shadowed to read clearly from an arm's length
/// away at a shop counter.
pub fn tile_button(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let base_color = match status {
        button::Status::Hovered | button::Status::Pressed => palette.primary.strong.color,
        button::Status::Disabled => palette.primary.weak.color,
        button::Status::Active => palette.primary.base.color,
    };

    button::Style {
        background: Some(Background::Color(base_color)),
        text_color: palette.primary.base.text,
        border: Border { radius: 20.0.into(), width: 0.0, color: Color::TRANSPARENT },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.18),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        snap: false,
    }
}

/// Soft card container — used for list rows and grouped form sections so
/// content reads as distinct blocks rather than a flat wall of text.
pub fn card(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        text_color: None,
        border: Border {
            radius: 14.0.into(),
            width: 1.0,
            color: palette.background.weak.color,
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.06),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 6.0,
        },
        snap: false,
    }
}

/// The tinted banner behind the low-stock badge on an item row.
pub fn low_stock_badge(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.danger.weak.color)),
        text_color: Some(palette.danger.weak.text),
        border: Border { radius: 8.0.into(), width: 0.0, color: Color::TRANSPARENT },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// The header banner (logo + shop name) shown across every screen.
pub fn header_bar(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.primary.base.color)),
        text_color: Some(palette.primary.base.text),
        border: Border::default(),
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.12),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        snap: false,
    }
}
