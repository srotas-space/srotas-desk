use iced::widget::{button, column, container, row, svg, text};
use iced::{Element, Length};

use super::{Message, State};
use crate::ui::theme;

/// The two tile icons, embedded at compile time — same reasoning as the
/// logo in `ui::mod`: the running app never depends on the project's
/// asset files still being at some relative path on disk.
///
/// These replace the emoji the tiles used to show. Emoji come from the
/// system, not from the app's bundled typeface, so on a machine without
/// an emoji font (a minimal Ubuntu install, say) both tiles rendered as
/// empty boxes. An embedded SVG has no such dependency and draws the same
/// on every platform.
const STORE_SVG: &[u8] = include_bytes!("../../assets/store.svg");
const INVENTORY_SVG: &[u8] = include_bytes!("../../assets/inventory.svg");

pub fn view(state: &State) -> Element<'_, Message> {
    let shop_name = state.shop.as_ref().map(|s| s.shop_name.as_str()).unwrap_or("Srotas Desk");
    // Counted in SQL when the shop loads, not by walking a catalogue
    // held in memory — see `repo::low_stock_count`.
    let low_stock = state.low_stock_total;

    let mut heading = column![
        text(format!("Welcome, {shop_name}")).size(theme::TEXT_DISPLAY).font(theme::SEMIBOLD),
        text("Pick where you're headed.").size(theme::TEXT_BODY).color(theme::MUTED_TEXT),
    ]
    .spacing(theme::SPACE_XS)
    .align_x(iced::Alignment::Center);

    // A count that's zero is worth no pixels — the badge only earns its
    // place when there's actually something to restock.
    if low_stock > 0 {
        heading = heading.push(
            container(
                text(format!(
                    "{low_stock} item{} running low on stock",
                    if low_stock == 1 { "" } else { "s" }
                ))
                .size(theme::TEXT_SMALL)
                .font(theme::SEMIBOLD),
            )
            .style(theme::low_stock_badge)
            .padding([theme::SPACE_SM as u16, theme::SPACE_MD as u16]),
        );
    }

    let tiles = column![
        heading,
        row![
            tile(STORE_SVG, "Shop", "Buy stock, bill sales, view reports & backups", Message::GoToShop),
            tile(INVENTORY_SVG, "Inventory", "Manage items, prices & stock levels", Message::GoToInventory),
        ]
        .spacing(theme::SPACE_LG),
    ]
    .spacing(theme::SPACE_LG)
    .align_x(iced::Alignment::Center);

    container(tiles)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
}

fn tile<'a>(icon: &'static [u8], label: &'a str, hint: &'a str, message: Message) -> Element<'a, Message> {
    button(
        column![
            svg(svg::Handle::from_memory(icon)).width(72).height(72),
            text(label).size(theme::TEXT_TITLE).font(theme::SEMIBOLD),
            text(hint).size(theme::TEXT_SMALL).align_x(iced::Alignment::Center),
        ]
        .spacing(theme::SPACE_SM)
        .align_x(iced::Alignment::Center)
        .width(Length::Fixed(230.0)),
    )
    .style(theme::tile_button)
    .padding(theme::SPACE_LG)
    .on_press(message)
    .into()
}
