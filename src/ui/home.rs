use iced::widget::{button, column, container, text};
use iced::{Element, Length};

use super::{Message, State};
use crate::ui::theme;

pub fn view(state: &State) -> Element<'_, Message> {
    let shop_name = state.shop.as_ref().map(|s| s.shop_name.as_str()).unwrap_or("Srotas Desk");

    let tiles = column![
        text(format!("Welcome, {shop_name}")).size(24),
        iced::widget::row![
            tile("🏪", "Shop", "Buy stock, bill sales, view reports & backups", Message::GoToShop),
            tile("📦", "Inventory", "Manage items, prices & stock levels", Message::GoToInventory),
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

fn tile<'a>(emoji: &'a str, label: &'a str, hint: &'a str, message: Message) -> Element<'a, Message> {
    button(
        column![
            text(emoji).size(48),
            text(label).size(22),
            text(hint).size(13),
        ]
        .spacing(6)
        .align_x(iced::Alignment::Center)
        .width(Length::Fixed(220.0)),
    )
    .style(theme::tile_button)
    .padding(theme::SPACE_LG)
    .on_press(message)
    .into()
}
