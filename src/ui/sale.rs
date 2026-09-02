//! The Shop → Details screen: pick an item, see what the shop knows about
//! it, and sell stock off it.
//!
//! Selling used to sit above a Sales History list, which is where a sale's
//! view/edit/print/delete actions lived. That list is gone; the space now
//! carries the low-stock items, which is the thing a shopkeeper standing
//! at the counter can actually act on. Past sales still exist as
//! transactions and still show up in Reports — bills (Shop → Billings) are
//! the printable record.
use iced::widget::{button, column, combo_box, container, row, scrollable, text};
use iced::{Element, Length, Task};

use super::{Message, Notice, State};
use crate::models::Item;
use crate::money;
use crate::ui::common::ItemOption;
use crate::ui::{common, theme};

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

    pub fn get_field(&self, field: Field) -> String {
        match field {
            Field::Qty => self.qty.clone(),
            Field::Price => self.price.clone(),
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
        state.notice = Some(Notice::error("choose an item first"));
        return Task::none();
    };
    let Some(qty) = form.qty.trim().parse::<f64>().ok().filter(|q| *q > 0.0) else {
        state.notice = Some(Notice::error("quantity must be a positive number"));
        return Task::none();
    };
    let Some(price_paise) = money::rupees_to_paise(&form.price) else {
        state.notice = Some(Notice::error("sell price must be a valid amount, e.g. 120.00"));
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

/// The low-stock items on the current page, plus that page's (clamped)
/// index and the total number of pages. Clamping here rather than at the
/// message handlers means restocking the last item on the last page can't
/// strand the screen on a page that no longer exists.
pub fn low_stock_page(state: &State) -> (Vec<&Item>, usize, usize) {
    let matches: Vec<&Item> = state.items.iter().filter(|item| item.is_low_stock()).collect();

    let page_count = matches.len().div_ceil(PAGE_SIZE).max(1);
    let page = state.low_stock_page.min(page_count - 1);
    let visible = matches.into_iter().skip(page * PAGE_SIZE).take(PAGE_SIZE).collect();

    (visible, page, page_count)
}

// ----------------------------------------------------------------- view

pub fn view(state: &State) -> Element<'_, Message> {
    column![
        text("Item Details").size(theme::TEXT_TITLE).font(theme::SEMIBOLD),
        items_panel(state),
        details_panel(state),
        low_stock_panel(state),
    ]
    .spacing(theme::SPACE_MD)
    // No top padding — the tab strip above already provides it. The
    // bottom padding keeps the low-stock panel, which stretches to fill,
    // off the window edge.
    .padding(iced::Padding { top: 0.0, right: theme::SPACE_MD, bottom: theme::SPACE_MD, left: theme::SPACE_MD })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// What you're selling. The two actions sit on the same row as the three
/// fields, bottom-aligned with them, so the whole thing reads as one line
/// of work: pick, quantify, price, sell.
fn items_panel(state: &State) -> Element<'_, Message> {
    let form = &state.sale_form;

    let item_picker = combo_box(&state.sale_item_combo, "Search item...", form.item.as_ref(), Message::SaleItemSelected)
        .padding(theme::FIELD_PADDING)
        .width(Length::Fill);

    let body = column![
        text("Items").size(theme::TEXT_HEADING).font(theme::SEMIBOLD),
        row![
            labeled("Item", item_picker).width(Length::FillPortion(3)),
            labeled(
                "Quantity",
                common::field("e.g. 2", &form.qty).on_input(|v| Message::SaleFieldChanged(Field::Qty, v)),
            )
            .width(Length::FillPortion(2)),
            labeled(
                "Sell price (₹) per unit",
                common::field("120.00", &form.price).on_input(|v| Message::SaleFieldChanged(Field::Price, v)),
            )
            .width(Length::FillPortion(2)),
            button(text("Sell").size(theme::TEXT_BODY).font(theme::SEMIBOLD))
                .style(theme::accent_button)
                .padding(theme::CONTROL_PADDING)
                .on_press(Message::SubmitSale),
            button(text("Billings").size(theme::TEXT_BODY))
                .style(theme::secondary_button)
                .padding(theme::CONTROL_PADDING)
                .on_press(Message::ShopTabSelected(super::ShopTab::Billings)),
        ]
        .spacing(theme::SPACE_MD)
        // Bottom-aligned, so the buttons line up with the foot of the
        // fields rather than floating against their labels.
        .align_y(iced::Alignment::End),
    ]
    .spacing(theme::SPACE_MD);

    container(body).style(theme::card).padding(theme::SPACE_MD).width(Length::Fill).into()
}

/// What the shop knows about the picked item, as three figures big enough
/// to read from across the counter.
fn details_panel(state: &State) -> Element<'_, Message> {
    let selected_item =
        state.sale_form.item.as_ref().and_then(|sel| state.items.iter().find(|i| i.id == sel.id));

    // Always visible — zeroed until an item is picked, then reflecting
    // that item's real stock/buy/sell figures.
    let (stock_value, buy_value, sell_value, location_value) = match selected_item {
        Some(item) => (
            format!("{:.1} {}", item.stock_qty, item.unit),
            money::format_paise(item.buy_price_paise),
            money::format_paise(item.sell_price_paise),
            if item.location.is_empty() { "Not recorded".to_string() } else { item.location.clone() },
        ),
        None => ("0.0".to_string(), money::format_paise(0), money::format_paise(0), "—".to_string()),
    };

    let mut heading = row![text("Details").size(theme::TEXT_HEADING).font(theme::SEMIBOLD)]
        .spacing(theme::SPACE_SM)
        .align_y(iced::Alignment::Center);
    if let Some(item) = selected_item {
        heading = heading.push(text("·").size(theme::TEXT_HEADING).color(theme::MUTED_TEXT));
        heading = heading.push(text(&item.name).size(theme::TEXT_BODY).color(theme::MUTED_TEXT));
    }

    let status: Element<'_, Message> = match selected_item {
        Some(item) if item.is_low_stock() => container(
            text(format!("Below the low-stock threshold of {:.1} {}", item.low_stock_threshold, item.unit))
                .size(theme::TEXT_SMALL)
                .font(theme::SEMIBOLD),
        )
        .style(theme::low_stock_badge)
        .padding([theme::SPACE_XS as u16 + 2, theme::SPACE_MD as u16])
        .into(),
        Some(item) => text(format!(
            "Stock is healthy — above the low-stock threshold of {:.1} {}.",
            item.low_stock_threshold, item.unit
        ))
        .size(theme::TEXT_SMALL)
        .color(theme::MUTED_TEXT)
        .into(),
        None => text("Pick an item above to see its stock and prices.")
            .size(theme::TEXT_SMALL)
            .color(theme::MUTED_TEXT)
            .into(),
    };

    let body = column![
        heading,
        row![
            stat("Stock", stock_value),
            stat("Buy price", buy_value),
            stat("Sell price", sell_value),
            // Where to walk to. Set in Inventory; the tile reads "Not
            // recorded" until somebody fills it in.
            stat_text("Kept at", location_value),
        ]
        .spacing(theme::SPACE_MD)
        .width(Length::Fill),
        status,
    ]
    .spacing(theme::SPACE_MD);

    container(body).style(theme::card).padding(theme::SPACE_MD).width(Length::Fill).into()
}

/// Everything the shop is about to run out of. Takes the rest of the
/// window, so the page fills its height rather than trailing off into
/// empty space the way the old Sales History did.
fn low_stock_panel(state: &State) -> Element<'_, Message> {
    let (visible, page, page_count) = low_stock_page(state);
    let total = state.items.iter().filter(|item| item.is_low_stock()).count();

    let mut list = column![].spacing(theme::SPACE_XS);
    if total == 0 {
        list = list.push(
            text("Nothing is running low — every item is above its threshold.")
                .size(theme::TEXT_SMALL)
                .color(theme::MUTED_TEXT),
        );
    }
    for item in visible {
        list = list.push(low_stock_row(item));
    }

    let start = page * PAGE_SIZE;
    let range_label = if total == 0 {
        "0 of 0".to_string()
    } else {
        format!("{}-{} of {}", start + 1, (start + PAGE_SIZE).min(total), total)
    };

    let pagination = row![
        text(range_label).size(theme::TEXT_SMALL).color(theme::MUTED_TEXT),
        iced::widget::space::horizontal(),
        button(text("Prev").size(theme::TEXT_SMALL))
            .style(theme::secondary_button)
            .padding([8, 16])
            .on_press_maybe((page > 0).then_some(Message::LowStockPagePrev)),
        text(format!("Page {} of {}", page + 1, page_count)).size(theme::TEXT_SMALL).color(theme::MUTED_TEXT),
        button(text("Next").size(theme::TEXT_SMALL))
            .style(theme::secondary_button)
            .padding([8, 16])
            .on_press_maybe((page + 1 < page_count).then_some(Message::LowStockPageNext)),
    ]
    .spacing(theme::SPACE_SM)
    .align_y(iced::Alignment::Center);

    let header = row![
        text("Low Stock Items").size(theme::TEXT_HEADING).font(theme::SEMIBOLD),
        iced::widget::space::horizontal(),
        button(text("Go to Inventory").size(theme::TEXT_SMALL))
            .style(theme::secondary_button)
            .padding([8, 16])
            .on_press(Message::GoToInventory),
    ]
    .align_y(iced::Alignment::Center);

    container(
        column![
            header,
            // The list scrolls inside the panel so the pagination row stays
            // pinned to the bottom instead of sliding off the page.
            scrollable(list).height(Length::Fill),
            pagination,
        ]
        .spacing(theme::SPACE_MD),
    )
    .style(theme::card)
    .padding(theme::SPACE_MD)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn low_stock_row(item: &Item) -> Element<'_, Message> {
    container(
        row![
            text(&item.name).size(theme::TEXT_BODY).width(Length::FillPortion(4)),
            text(format!("{:.1} {} left", item.stock_qty, item.unit))
                .size(theme::TEXT_SMALL)
                .width(Length::FillPortion(2)),
            text(format!("threshold {:.1}", item.low_stock_threshold))
                .size(theme::TEXT_SMALL)
                .color(theme::MUTED_TEXT)
                .width(Length::FillPortion(2)),
            text(money::format_paise(item.sell_price_paise)).size(theme::TEXT_SMALL).width(Length::FillPortion(2)),
            container(text("LOW").size(theme::TEXT_CAPTION).font(theme::SEMIBOLD))
                .style(theme::low_stock_badge)
                .padding([theme::SPACE_XS as u16, theme::SPACE_SM as u16 + 2]),
        ]
        .spacing(theme::SPACE_SM)
        .align_y(iced::Alignment::Center),
    )
    .style(theme::panel)
    .padding(theme::SPACE_SM)
    .width(Length::Fill)
    .into()
}

/// One figure in the Details panel, as a filled tile that takes an equal
/// share of the width — three of these read as a dashboard, where three
/// bare label/value pairs read as a caption.
fn stat<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    tile(label, value, 26.0, theme::VIOLET, theme::BOLD)
}

/// Same tile, but for a value that is words rather than a figure — a shelf
/// name set at 26pt violet would shout louder than the numbers beside it
/// and wrap out of its tile besides.
fn stat_text<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    tile(label, value, theme::TEXT_HEADING, theme::INK, theme::SEMIBOLD)
}

fn tile<'a>(
    label: &'a str,
    value: String,
    size: f32,
    color: iced::Color,
    font: iced::Font,
) -> Element<'a, Message> {
    container(
        column![
            text(label.to_uppercase()).size(theme::TEXT_CAPTION).color(theme::MUTED_TEXT),
            text(value).size(size).font(font).color(color),
        ]
        .spacing(theme::SPACE_XS),
    )
    .style(theme::panel)
    .padding(theme::SPACE_MD)
    .width(Length::FillPortion(1))
    .into()
}

fn labeled<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> iced::widget::Column<'a, Message> {
    column![text(label).size(theme::TEXT_SMALL).color(theme::MUTED_TEXT), widget.into()].spacing(theme::SPACE_XS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i64, stock: f64, threshold: f64) -> Item {
        Item {
            id,
            name: format!("Item {id}"),
            buy_price_paise: 1000,
            sell_price_paise: 2000,
            stock_qty: stock,
            unit: "piece".into(),
            low_stock_threshold: threshold,
            description: String::new(),
            location: String::new(),
            has_image: false,
            gst_rate_bp: None,
        }
    }

    #[test]
    fn only_items_below_their_threshold_are_listed() {
        let mut state = State::default();
        state.items = vec![item(1, 2.0, 5.0), item(2, 9.0, 5.0), item(3, 0.0, 1.0)];

        let (visible, _, _) = low_stock_page(&state);
        let ids: Vec<i64> = visible.iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn low_stock_paginates() {
        let mut state = State::default();
        state.items = (0..25).map(|i| item(i, 0.0, 1.0)).collect();

        let (visible, page, page_count) = low_stock_page(&state);
        assert_eq!(visible.len(), PAGE_SIZE);
        assert_eq!((page, page_count), (0, 3));

        state.low_stock_page = 2;
        let (visible, page, _) = low_stock_page(&state);
        assert_eq!(visible.len(), 5);
        assert_eq!(page, 2);
    }

    #[test]
    fn a_stale_page_is_clamped_once_items_are_restocked() {
        let mut state = State::default();
        state.items = (0..25).map(|i| item(i, 0.0, 1.0)).collect();
        state.low_stock_page = 2;

        // Everything but one item gets restocked while the screen sits on
        // page 3 — it must fall back to the only page there is.
        state.items = vec![item(1, 0.0, 1.0)];
        let (visible, page, page_count) = low_stock_page(&state);
        assert_eq!(visible.len(), 1);
        assert_eq!((page, page_count), (0, 1));
    }

    #[test]
    fn an_empty_catalog_still_reports_one_page() {
        let state = State::default();
        let (visible, page, page_count) = low_stock_page(&state);
        assert!(visible.is_empty());
        assert_eq!((page, page_count), (0, 1));
    }
}
