//! The app's visual identity — a small design system rather than a bag of
//! one-off styles. Everything the screens draw pulls its colour, spacing,
//! radius and elevation from the tokens here, so the whole app moves
//! together when one of them changes.
//!
//! The palette is built around the brand's violet/pink/orange (the same
//! three in the logo), sitting on a warm off-white canvas with white
//! cards — the layered look a counter app needs so a shopkeeper glancing
//! down from a customer can immediately tell what's a surface, what's a
//! field, and what's an action.
use iced::border::Radius;
use iced::gradient;
use iced::widget::{button, container, rule, text_input};
use iced::{Background, Border, Color, Shadow, Theme, Vector};

// ---------------------------------------------------------------- palette

pub const VIOLET: Color = Color::from_rgb(0.486, 0.227, 0.929);
/// A deeper violet used as the far end of the header gradient and for
/// pressed states — the same hue, just walked down in lightness.
pub const VIOLET_DEEP: Color = Color::from_rgb(0.322, 0.129, 0.706);
pub const PINK: Color = Color::from_rgb(0.925, 0.286, 0.600);
pub const ORANGE: Color = Color::from_rgb(0.984, 0.573, 0.137);

pub const SUCCESS: Color = Color::from_rgb(0.086, 0.639, 0.373);
pub const DANGER: Color = Color::from_rgb(0.863, 0.208, 0.271);

/// Page background — a faintly violet-tinted off-white, so white cards
/// read as raised rather than blending into the window.
pub const CANVAS: Color = Color::from_rgb(0.965, 0.961, 0.976);
/// Card / field background.
pub const SURFACE: Color = Color::from_rgb(1.0, 1.0, 1.0);
/// A slightly recessed surface for table headers and inert panels.
pub const SURFACE_SUNK: Color = Color::from_rgb(0.957, 0.953, 0.969);

/// Primary body text.
pub const INK: Color = Color::from_rgb(0.098, 0.086, 0.141);
/// Muted label colour for small caption-style text (stat labels, hints).
pub const MUTED_TEXT: Color = Color::from_rgb(0.443, 0.427, 0.502);
/// Hairline borders — card edges, field outlines, table rules.
pub const LINE: Color = Color::from_rgb(0.886, 0.878, 0.914);

// ------------------------------------------------------------- typography

/// The app bundles its own typeface rather than inheriting each OS's
/// default sans-serif. Without this the same screen renders in SF on
/// macOS, Ubuntu Sans on Linux and Segoe UI on Windows — different
/// metrics, so text that fits on one line on a Mac wraps to two on
/// Ubuntu. A shop's name is exactly the kind of string that sits near a
/// container's width, and a bill is a document a shopkeeper hands to a
/// customer, so "looks the same everywhere" is worth ~1.2 MB of binary.
///
/// Inter is designed for user interfaces at small sizes, has unambiguous
/// numerals (which is most of what this app displays), and is licensed
/// under the SIL Open Font License — see `assets/fonts/Inter-LICENSE.txt`.
/// The three weights below are the only ones the app asks for; each is
/// registered separately in `ui::run` because iced matches a bundled face
/// by family name *and* weight.
pub const FONT_FAMILY: &str = "Inter";

/// The regular weight, and the app's default for all text.
pub const REGULAR: iced::Font = iced::Font::with_name(FONT_FAMILY);

/// A bold variant, for figures that should stand out (stat values, big
/// numbers) without switching typeface.
pub const BOLD: iced::Font =
    iced::Font { weight: iced::font::Weight::Bold, ..iced::Font::with_name(FONT_FAMILY) };

/// Semibold — for headings and table headers, where full bold is heavier
/// than the hierarchy needs.
pub const SEMIBOLD: iced::Font =
    iced::Font { weight: iced::font::Weight::Semibold, ..iced::Font::with_name(FONT_FAMILY) };

/// Type scale. Screens pick from these rather than inventing sizes, which
/// is what keeps headings on one screen the same weight as the next.
pub const TEXT_DISPLAY: f32 = 28.0;
pub const TEXT_TITLE: f32 = 21.0;
pub const TEXT_HEADING: f32 = 17.0;
pub const TEXT_BODY: f32 = 15.0;
pub const TEXT_SMALL: f32 = 13.0;
pub const TEXT_CAPTION: f32 = 12.0;

// ---------------------------------------------------------------- metrics

pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 16.0;
pub const SPACE_LG: f32 = 28.0;

/// Corner radius shared by every button in the app, so a button always
/// reads as a distinct, tappable shape rather than a flat label.
const BUTTON_RADIUS: f32 = 10.0;
const CARD_RADIUS: f32 = 16.0;
const FIELD_RADIUS: f32 = 10.0;

/// Standard control padding — `[vertical, horizontal]`. Buttons and text
/// fields share it so a button sitting next to a field lines up with it.
pub const CONTROL_PADDING: [u16; 2] = [11, 20];
pub const FIELD_PADDING: u16 = 11;

/// Builds the app's theme. Passed to `iced::application(...).theme(...)` —
/// once set, every built-in widget style (buttons, containers, text
/// selection, etc.) derives from this palette automatically.
pub fn theme() -> Theme {
    Theme::custom(
        "Srotas".to_string(),
        iced::theme::Palette {
            background: CANVAS,
            text: INK,
            primary: VIOLET,
            success: SUCCESS,
            warning: ORANGE,
            danger: DANGER,
        },
    )
}

// ---------------------------------------------------------------- shadows

/// Barely-there lift for resting cards and list rows.
fn shadow_soft() -> Shadow {
    Shadow {
        color: Color::from_rgba(0.098, 0.086, 0.141, 0.07),
        offset: Vector::new(0.0, 2.0),
        blur_radius: 8.0,
    }
}

/// The lift a pressable control carries, so it reads as sitting above the
/// card it's on.
fn shadow_control() -> Shadow {
    Shadow {
        color: Color::from_rgba(0.098, 0.086, 0.141, 0.14),
        offset: Vector::new(0.0, 2.0),
        blur_radius: 5.0,
    }
}

/// The deliberately generous lift under the Home tiles.
fn shadow_raised() -> Shadow {
    Shadow {
        color: Color::from_rgba(0.192, 0.078, 0.404, 0.28),
        offset: Vector::new(0.0, 8.0),
        blur_radius: 22.0,
    }
}

// ---------------------------------------------------------------- buttons

/// Nudges a colour towards black — used for hover/pressed states so every
/// button variant darkens by the same amount instead of each hard-coding
/// its own second colour.
fn darken(color: Color, amount: f32) -> Color {
    Color {
        r: color.r * (1.0 - amount),
        g: color.g * (1.0 - amount),
        b: color.b * (1.0 - amount),
        a: color.a,
    }
}

/// Shared shape/shadow for every solid button below — only the fill
/// colour differs between semantic variants.
fn solid_button(fill: Color, text_color: Color, status: button::Status) -> button::Style {
    let (background, shadow) = match status {
        button::Status::Hovered => (darken(fill, 0.10), shadow_control()),
        // Pressed loses its shadow, so the button visibly settles onto the
        // surface rather than only changing colour.
        button::Status::Pressed => (darken(fill, 0.20), Shadow::default()),
        button::Status::Disabled => (fill.scale_alpha(0.45), Shadow::default()),
        button::Status::Active => (fill, shadow_control()),
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: if status == button::Status::Disabled { text_color.scale_alpha(0.6) } else { text_color },
        border: Border { radius: BUTTON_RADIUS.into(), width: 0.0, color: Color::TRANSPARENT },
        shadow,
        snap: false,
    }
}

/// A primary action button (rounded, brand violet).
pub fn primary_button(_theme: &Theme, status: button::Status) -> button::Style {
    solid_button(VIOLET, Color::WHITE, status)
}

/// A secondary/neutral action button — an outlined chip rather than a
/// second block of colour, so a row of actions has exactly one obvious
/// primary in it.
pub fn secondary_button(_theme: &Theme, status: button::Status) -> button::Style {
    let (background, border_color, text_color) = match status {
        button::Status::Hovered => (Color::from_rgb(0.976, 0.973, 0.988), VIOLET.scale_alpha(0.55), VIOLET),
        button::Status::Pressed => (Color::from_rgb(0.949, 0.941, 0.976), VIOLET, VIOLET_DEEP),
        button::Status::Disabled => (SURFACE, LINE, MUTED_TEXT.scale_alpha(0.6)),
        button::Status::Active => (SURFACE, LINE, INK),
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border { radius: BUTTON_RADIUS.into(), width: 1.0, color: border_color },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// A positive/confirming action button (rounded, green) — e.g. "Save".
pub fn success_button(_theme: &Theme, status: button::Status) -> button::Style {
    solid_button(SUCCESS, Color::WHITE, status)
}

/// A destructive action button (rounded, red) — e.g. "Delete".
pub fn danger_button(_theme: &Theme, status: button::Status) -> button::Style {
    solid_button(DANGER, Color::WHITE, status)
}

/// The "Sell" action button — brand pink, so billing a sale reads as a
/// visually distinct action from stocking-in (green) at a glance.
pub fn accent_button(_theme: &Theme, status: button::Status) -> button::Style {
    solid_button(PINK, Color::WHITE, status)
}

/// A plain-text, no-background button styled like a hyperlink — for
/// actions that open something external (e.g. "Terms & Conditions") or
/// step out of a flow ("Forgot PIN?"), so they don't compete visually
/// with real buttons.
pub fn link_button(_theme: &Theme, status: button::Status) -> button::Style {
    let color = match status {
        button::Status::Hovered | button::Status::Pressed => VIOLET_DEEP,
        button::Status::Disabled => MUTED_TEXT.scale_alpha(0.6),
        button::Status::Active => VIOLET,
    };
    button::Style {
        background: None,
        text_color: color,
        border: Border::default(),
        shadow: Shadow::default(),
        snap: false,
    }
}

/// A button sitting on the coloured app bar — transparent until touched,
/// so the bar stays a clean band instead of a row of competing chips.
pub fn app_bar_button(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Color::from_rgba(1.0, 1.0, 1.0, 0.20),
        button::Status::Pressed => Color::from_rgba(1.0, 1.0, 1.0, 0.30),
        _ => Color::from_rgba(1.0, 1.0, 1.0, 0.10),
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: Color::WHITE,
        border: Border { radius: BUTTON_RADIUS.into(), width: 1.0, color: Color::from_rgba(1.0, 1.0, 1.0, 0.28) },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// One segment of a tab strip. The selected segment is a solid violet
/// pill; the rest are quiet text, which reads as a single control rather
/// than as several buttons that happen to sit next to each other.
pub fn tab_selected(_theme: &Theme, status: button::Status) -> button::Style {
    let fill = if status == button::Status::Hovered { darken(VIOLET, 0.08) } else { VIOLET };
    button::Style {
        background: Some(Background::Color(fill)),
        text_color: Color::WHITE,
        border: Border { radius: BUTTON_RADIUS.into(), width: 0.0, color: Color::TRANSPARENT },
        shadow: shadow_control(),
        snap: false,
    }
}

pub fn tab_idle(_theme: &Theme, status: button::Status) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Hovered | button::Status::Pressed => (Color::from_rgba(0.486, 0.227, 0.929, 0.10), VIOLET_DEEP),
        _ => (Color::TRANSPARENT, MUTED_TEXT),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border { radius: BUTTON_RADIUS.into(), width: 0.0, color: Color::TRANSPARENT },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// Large, rounded, brand-violet tile button for the Home screen ("Shop" /
/// "Inventory") — sized and shadowed to read clearly from an arm's length
/// away at a shop counter.
pub fn tile_button(_theme: &Theme, status: button::Status) -> button::Style {
    let angle = gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2 * 1.35));
    let (from, to) = match status {
        button::Status::Hovered => (darken(VIOLET, 0.06), darken(VIOLET_DEEP, 0.06)),
        button::Status::Pressed => (darken(VIOLET, 0.16), darken(VIOLET_DEEP, 0.16)),
        button::Status::Disabled => (VIOLET.scale_alpha(0.4), VIOLET_DEEP.scale_alpha(0.4)),
        button::Status::Active => (VIOLET, VIOLET_DEEP),
    };

    button::Style {
        background: Some(Background::Gradient(
            angle.add_stop(0.0, from).add_stop(1.0, to).into(),
        )),
        text_color: Color::WHITE,
        border: Border { radius: 22.0.into(), width: 0.0, color: Color::TRANSPARENT },
        shadow: if status == button::Status::Pressed { shadow_control() } else { shadow_raised() },
        snap: false,
    }
}

// ------------------------------------------------------------- containers

/// Soft card container — used for list rows and grouped form sections so
/// content reads as distinct blocks rather than a flat wall of text.
pub fn card(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE)),
        text_color: None,
        border: Border { radius: CARD_RADIUS.into(), width: 1.0, color: LINE },
        shadow: shadow_soft(),
        snap: false,
    }
}

/// A card with no shadow, for panels nested inside another card (where a
/// second shadow would just read as noise).
pub fn panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE_SUNK)),
        text_color: None,
        border: Border { radius: 12.0.into(), width: 1.0, color: LINE },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// The header banner (logo + shop name) shown across every screen — a
/// violet gradient rather than a flat fill, which is most of what makes
/// the window read as a finished app rather than a form.
pub fn header_bar(_theme: &Theme) -> container::Style {
    let angle = gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2));
    container::Style {
        background: Some(Background::Gradient(
            angle.add_stop(0.0, VIOLET_DEEP).add_stop(0.55, VIOLET).add_stop(1.0, Color::from_rgb(0.612, 0.259, 0.855)).into(),
        )),
        text_color: Some(Color::WHITE),
        border: Border::default(),
        shadow: Shadow {
            color: Color::from_rgba(0.192, 0.078, 0.404, 0.25),
            offset: Vector::new(0.0, 3.0),
            blur_radius: 12.0,
        },
        snap: false,
    }
}

/// The tinted banner behind the low-stock badge on an item row.
pub fn low_stock_badge(_theme: &Theme) -> container::Style {
    tinted(DANGER)
}

/// A soft tinted pill in an arbitrary semantic colour — the shared shape
/// behind every badge and inline notice in the app.
fn tinted(color: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(color.scale_alpha(0.12))),
        text_color: Some(darken(color, 0.25)),
        border: Border { radius: 999.0.into(), width: 1.0, color: color.scale_alpha(0.35) },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// The three severities the status bar can show. Errors used to be the
/// only styling available, so "security settings saved" arrived in the
/// same alarming red as a failed save.
pub fn notice_error(_theme: &Theme) -> container::Style {
    notice(DANGER)
}

pub fn notice_success(_theme: &Theme) -> container::Style {
    notice(SUCCESS)
}

pub fn notice_warning(_theme: &Theme) -> container::Style {
    notice(ORANGE)
}

fn notice(color: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(color.scale_alpha(0.10))),
        text_color: Some(darken(color, 0.30)),
        border: Border {
            radius: Radius::new(10.0),
            width: 1.0,
            color: color.scale_alpha(0.30),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

// ----------------------------------------------------------------- fields

/// Text fields, styled to match the buttons — a violet focus ring is the
/// only way a keyboard-driven counter operator can tell where they are.
pub fn field(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let (background, border) = match status {
        text_input::Status::Focused { .. } => (
            SURFACE,
            Border { radius: FIELD_RADIUS.into(), width: 2.0, color: VIOLET },
        ),
        text_input::Status::Hovered => (
            SURFACE,
            Border { radius: FIELD_RADIUS.into(), width: 1.0, color: VIOLET.scale_alpha(0.45) },
        ),
        text_input::Status::Disabled => (
            SURFACE_SUNK,
            Border { radius: FIELD_RADIUS.into(), width: 1.0, color: LINE },
        ),
        text_input::Status::Active => (SURFACE, Border { radius: FIELD_RADIUS.into(), width: 1.0, color: LINE }),
    };

    text_input::Style {
        background: Background::Color(background),
        border,
        icon: MUTED_TEXT,
        placeholder: MUTED_TEXT.scale_alpha(0.8),
        value: if status == text_input::Status::Disabled { MUTED_TEXT } else { INK },
        selection: VIOLET.scale_alpha(0.28),
    }
}

/// A read-only field showing a value the user copies rather than edits
/// (the Device ID on the activation screen).
pub fn field_readonly(_theme: &Theme, _status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(SURFACE_SUNK),
        border: Border { radius: FIELD_RADIUS.into(), width: 1.0, color: LINE },
        icon: MUTED_TEXT,
        placeholder: MUTED_TEXT,
        value: INK,
        selection: VIOLET.scale_alpha(0.28),
    }
}

/// Hairline divider.
pub fn divider(_theme: &Theme) -> rule::Style {
    rule::Style {
        color: LINE,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}
