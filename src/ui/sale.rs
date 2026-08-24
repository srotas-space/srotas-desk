use iced::widget::{button, column, combo_box, container, row, scrollable, text, text_input};
use iced::{Element, Length, Task};

use super::{Message, State};
use crate::money;
use crate::repo::TransactionHistoryRow;
use crate::ui::common::ItemOption;
use crate::ui::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Qty,
    Price,
}

#[derive(Debug, Clone, Default)]
pub struct SaleForm {
    pub item: Option<ItemOption>,
    pub qty: String,
    pub price: String,
}

impl SaleForm {
    pub fn set_field(&mut self, field: Field, value: String) {
        match field {
            Field::Qty => self.qty = value,
            Field::Price => self.price = value,
        }
    }
}

pub fn select_item(state: &mut State, option: ItemOption) {
    if let Some(item) = state.items.iter().find(|i| i.id == option.id) {
        state.sale_form.price = money::paise_to_input(item.sell_price_paise);
    }
    state.sale_form.item = Some(option);
}

pub fn submit(state: &mut State) -> Task<Message> {
    let form = &state.sale_form;
    let Some(item) = form.item.clone() else {
        state.status = Some("choose an item first".into());
        return Task::none();
    };
    let Some(qty) = form.qty.trim().parse::<f64>().ok().filter(|q| *q > 0.0) else {
        state.status = Some("quantity must be a positive number".into());
        return Task::none();
    };
    let Some(price_paise) = money::rupees_to_paise(&form.price) else {
        state.status = Some("sell price must be a valid amount, e.g. 120.00".into());
        return Task::none();
    };
    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };

    Task::perform(
        async move { crate::repo::record_sale(&pool, item.id, qty, price_paise, chrono::Utc::now()).await },
        |result| Message::SaleRecorded(result.map_err(|e| e.to_string())),
    )
}

pub const PAGE_SIZE: usize = 10;

pub fn load_recent(pool: sqlx::SqlitePool) -> Task<Message> {
    Task::perform(
        async move { crate::repo::transaction_history(&pool, Some("sell"), None, None, None, Some(200)).await },
        |result| Message::SaleHistoryLoaded(result.map_err(|e| e.to_string())),
    )
}

pub fn view(state: &State) -> Element<'_, Message> {
    let form = &state.sale_form;

    let selected_item = form.item.as_ref().and_then(|sel| state.items.iter().find(|i| i.id == sel.id));

    let item_picker = combo_box(
        &state.sale_item_combo,
        "Search item...",
        form.item.as_ref(),
        Message::SaleItemSelected,
    )
    .padding(10)
    .width(Length::Fixed(260.0));

    let form_fields = column![
        text("Item (Billing)").size(20),
        row![
            labeled("Item", item_picker),
            labeled("Quantity", text_input("e.g. 2", &form.qty).on_input(|v| Message::SaleFieldChanged(Field::Qty, v)).padding(10).width(Length::Fixed(120.0))),
            labeled("Sell Price (₹) per unit", text_input("120.00", &form.price).on_input(|v| Message::SaleFieldChanged(Field::Price, v)).padding(10).width(Length::Fixed(140.0))),
            button(text("Sell Stock").size(15)).style(theme::accent_button).padding([10, 24]).on_press(Message::SubmitSale),
        ]
        .spacing(theme::SPACE_MD)
        .align_y(iced::Alignment::End),
    ]
    .spacing(theme::SPACE_SM);

    // Always visible — defaults to zero until an item is picked, then
    // reflects that item's real stock/buy/sell figures.
    let (stock_value, buy_value, sell_value) = match selected_item {
        Some(item) => (
            format!("{:.1} {}", item.stock_qty, item.unit),
            money::format_paise(item.buy_price_paise),
            money::format_paise(item.sell_price_paise),
        ),
        None => ("0.0".to_string(), money::format_paise(0), money::format_paise(0)),
    };

    let stats_panel = row![
        stat("Stock", stock_value),
        stat("Buy", buy_value),
        stat("Sell", sell_value),
    ]
    .spacing(theme::SPACE_LG);

    let entry = row![
        container(form_fields).style(theme::card).padding(theme::SPACE_MD).width(Length::FillPortion(3)),
        container(stats_panel).style(theme::card).padding(theme::SPACE_MD).width(Length::FillPortion(2)),
    ]
    .spacing(theme::SPACE_MD)
    .align_y(iced::Alignment::Center);

    let total = state.recent_sales.len();
    let page_count = total.div_ceil(PAGE_SIZE).max(1);
    let page = state.sale_page.min(page_count - 1);
    let start = page * PAGE_SIZE;

    let mut history = column![text("Recent Sales").size(16)].spacing(6);
    if total == 0 {
        history = history.push(text("No sales recorded yet.").size(13));
    }
    for row in state.recent_sales.iter().skip(start).take(PAGE_SIZE) {
        history = history.push(history_row(row));
    }

    let range_label = if total == 0 {
        "0 of 0".to_string()
    } else {
        format!("{}-{} of {}", start + 1, (start + PAGE_SIZE).min(total), total)
    };

    let pagination = row![
        text(range_label).size(13),
        iced::widget::space::horizontal(),
        button(text("Prev").size(14)).style(theme::secondary_button).padding([8, 16]).on_press_maybe((page > 0).then_some(Message::SalesPagePrev)),
        text(format!("Page {} of {}", page + 1, page_count)).size(13),
        button(text("Next").size(14)).style(theme::secondary_button).padding([8, 16]).on_press_maybe((page + 1 < page_count).then_some(Message::SalesPageNext)),
    ]
    .spacing(theme::SPACE_MD)
    .align_y(iced::Alignment::Center);

    scrollable(
        column![
            entry,
            container(history).style(theme::card).padding(theme::SPACE_MD),
            pagination,
        ]
        .spacing(theme::SPACE_MD)
        .padding(theme::SPACE_MD),
    )
    .height(Length::Fill)
    .into()
}

fn stat<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    column![
        text(label.to_uppercase()).size(12).color(theme::MUTED_TEXT),
        text(value).size(20).font(theme::BOLD).color(theme::VIOLET),
    ]
    .spacing(4)
    .into()
}

fn history_row(row: &TransactionHistoryRow) -> Element<'_, Message> {
    container(
        iced::widget::row![
            text(&row.item_name).width(Length::FillPortion(3)),
            text(format!("{:.1}", row.qty)).width(Length::FillPortion(1)),
            text(money::format_paise(row.price_paise)).width(Length::FillPortion(1)),
            text(row.timestamp.format("%d %b %H:%M").to_string()).width(Length::FillPortion(2)),
        ]
        .spacing(8),
    )
    .padding(6)
    .into()
}

fn labeled<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(13), widget.into()].spacing(4).into()
}
