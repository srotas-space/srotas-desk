use iced::widget::{button, column, combo_box, container, row, scrollable, text, text_input};
use iced::{Element, Length, Task};
use std::path::PathBuf;

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
    pub editing_id: Option<i64>,
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

/// Loads a past sale into the form for editing — pre-fills item/qty/price
/// and switches Submit over to `edit_sale`.
pub fn load_for_edit(state: &mut State, row: TransactionHistoryRow) {
    state.sale_form.editing_id = Some(row.id);
    state.sale_form.item = Some(ItemOption { id: row.item_id, name: row.item_name });
    state.sale_form.qty = format!("{}", row.qty);
    state.sale_form.price = money::paise_to_input(row.price_paise);
    state.sale_viewing = None;
}

pub fn cancel_edit(state: &mut State) {
    state.sale_form = SaleForm::default();
    state.status = None;
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
    let editing_id = form.editing_id;

    Task::perform(
        async move {
            match editing_id {
                Some(id) => crate::repo::edit_sale(&pool, id, item.id, qty, price_paise).await,
                None => crate::repo::record_sale(&pool, item.id, qty, price_paise, chrono::Utc::now()).await,
            }
        },
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

/// Re-fetches the sale fresh rather than trusting whatever's on screen —
/// same reasoning as the Reports/Bills PDF export. `open_after` controls
/// whether this is "Print" (opens the file, ready to print) or "Download"
/// (just saves it and reports where).
pub fn export_pdf(state: &State, id: i64, open_after: bool) -> Task<Message> {
    let Some(pool) = state.pool.clone() else {
        return Task::done(Message::SalePdfReady(Err("database is not ready yet".into())));
    };
    let shop_name = state.shop.as_ref().map(|s| s.shop_name.clone()).unwrap_or_else(|| "Srotas Desk".to_string());

    Task::perform(
        async move {
            let sale = crate::repo::get_sale(&pool, id).await.map_err(|e| e.to_string())?;
            let bytes = build_pdf_bytes(&shop_name, &sale)?;
            save(bytes, sale.id, open_after).await
        },
        move |result| Message::SalePdfReady(result.map(|path| (path, open_after))),
    )
}

async fn save(bytes: Vec<u8>, id: i64, open_after: bool) -> Result<PathBuf, String> {
    let dir = dirs::download_dir().or_else(dirs::document_dir).ok_or("could not find a Downloads folder on this computer")?;
    let path = dir.join(format!("srotas-sale-{id}.pdf"));

    tokio::fs::write(&path, &bytes).await.map_err(|e| e.to_string())?;
    if open_after {
        open::that(&path).map_err(|e| format!("sale receipt saved, but couldn't open it: {e}"))?;
    }

    Ok(path)
}

fn build_pdf_bytes(shop_name: &str, sale: &TransactionHistoryRow) -> Result<Vec<u8>, String> {
    let mut w = crate::pdf::Writer::new(&format!("{shop_name} - Sale #{}", sale.id))?;

    w.line(shop_name, 18.0, true);
    w.line(&format!("Sale #{}", sale.id), 14.0, true);
    w.line(&format!("Date: {}", sale.timestamp.format("%d %b %Y %H:%M")), 10.0, false);
    w.gap(6.0);

    const ITEM_X: f32 = crate::pdf::LEFT_MM;
    const QTY_X: f32 = 100.0;
    const PRICE_X: f32 = 130.0;
    const TOTAL_X: f32 = 160.0;

    w.row(&[("Item", ITEM_X), ("Qty", QTY_X), ("Price", PRICE_X), ("Total", TOTAL_X)], 10.0, true);

    let total_paise = (sale.price_paise as f64 * sale.qty).round() as i64;
    w.row(
        &[
            (sale.item_name.as_str(), ITEM_X),
            (&format!("{:.1}", sale.qty), QTY_X),
            (&money::format_paise_ascii(sale.price_paise), PRICE_X),
            (&money::format_paise_ascii(total_paise), TOTAL_X),
        ],
        10.0,
        false,
    );
    w.gap(6.0);
    w.line(&format!("Total: {}", money::format_paise_ascii(total_paise)), 14.0, true);

    w.finish()
}

pub fn view(state: &State) -> Element<'_, Message> {
    if let Some(sale) = &state.sale_viewing {
        return detail_view(state, sale);
    }

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

    let title = match form.editing_id {
        Some(id) => format!("Edit Sale #{id}"),
        None => "Item (Billing)".to_string(),
    };

    let mut actions = row![
        labeled("Item", item_picker),
        labeled("Quantity", text_input("e.g. 2", &form.qty).on_input(|v| Message::SaleFieldChanged(Field::Qty, v)).padding(10).width(Length::Fixed(120.0))),
        labeled("Sell Price (₹) per unit", text_input("120.00", &form.price).on_input(|v| Message::SaleFieldChanged(Field::Price, v)).padding(10).width(Length::Fixed(140.0))),
        button(text(if form.editing_id.is_some() { "Save" } else { "Sell Stock" }).size(15)).style(theme::accent_button).padding([10, 24]).on_press(Message::SubmitSale),
    ]
    .spacing(theme::SPACE_MD)
    .align_y(iced::Alignment::End);
    if form.editing_id.is_some() {
        actions = actions.push(button(text("Cancel").size(15)).style(theme::secondary_button).padding([10, 24]).on_press(Message::CancelSaleEdit));
    } else {
        actions = actions.push(button(text("Billings").size(15)).style(theme::secondary_button).padding([10, 24]).on_press(Message::ShopTabSelected(super::ShopTab::Billings)));
    }

    let form_fields = column![text(title).size(20), actions].spacing(theme::SPACE_SM);

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

    let mut history = column![text("Sales History").size(16)].spacing(6);
    if total == 0 {
        history = history.push(text("No sales recorded yet.").size(13));
    }
    for row in state.recent_sales.iter().skip(start).take(PAGE_SIZE) {
        history = history.push(history_row(row, state.sale_confirming_delete_id == Some(row.id)));
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

fn history_row(row: &TransactionHistoryRow, confirming_delete: bool) -> Element<'_, Message> {
    let delete_label = if confirming_delete { "Confirm?" } else { "Delete" };

    container(
        iced::widget::row![
            text(&row.item_name).width(Length::FillPortion(3)),
            text(format!("{:.1}", row.qty)).width(Length::FillPortion(1)),
            text(money::format_paise(row.price_paise)).width(Length::FillPortion(1)),
            text(row.timestamp.format("%d %b %H:%M").to_string()).width(Length::FillPortion(2)),
            row![
                button(text("View").size(12)).style(theme::secondary_button).padding([6, 10]).on_press(Message::OpenSaleView(row.id)),
                button(text("Edit").size(12)).style(theme::secondary_button).padding([6, 10]).on_press(Message::OpenSaleEdit(row.id)),
                button(text("Print").size(12)).style(theme::secondary_button).padding([6, 10]).on_press(Message::PrintSalePressed(row.id)),
                button(text("Download").size(12)).style(theme::secondary_button).padding([6, 10]).on_press(Message::DownloadSalePressed(row.id)),
                button(text(delete_label).size(12)).style(theme::danger_button).padding([6, 10]).on_press(Message::DeleteSalePressed(row.id)),
            ]
            .spacing(6)
            .width(Length::FillPortion(5)),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    )
    .padding(6)
    .into()
}

const THUMBNAIL_SIZE: f32 = 48.0;

fn detail_view<'a>(state: &'a State, sale: &'a TransactionHistoryRow) -> Element<'a, Message> {
    let total_paise = (sale.price_paise as f64 * sale.qty).round() as i64;

    let item_row: Element<'_, Message> = match state.item_thumbnails.get(&sale.item_id) {
        Some(bytes) => row![
            iced::widget::image::Image::new(iced::widget::image::Handle::from_bytes(bytes.clone()))
                .width(THUMBNAIL_SIZE)
                .height(THUMBNAIL_SIZE)
                .content_fit(iced::ContentFit::Cover),
            detail_row("Item", sale.item_name.clone()),
        ]
        .spacing(theme::SPACE_SM)
        .align_y(iced::Alignment::Center)
        .into(),
        None => detail_row("Item", sale.item_name.clone()),
    };

    let body = column![
        text(format!("Sale #{}", sale.id)).size(22),
        text(sale.timestamp.format("%d %b %Y %H:%M").to_string()).size(13).color(theme::MUTED_TEXT),
        item_row,
        detail_row("Quantity", format!("{:.1}", sale.qty)),
        detail_row("Price", money::format_paise(sale.price_paise)),
        row![
            text("Total").width(Length::Fixed(140.0)).size(18),
            text(money::format_paise(total_paise)).size(18).font(theme::BOLD).color(theme::VIOLET),
        ],
        row![
            button(text("Edit").size(15)).style(theme::primary_button).padding([10, 24]).on_press(Message::OpenSaleEdit(sale.id)),
            button(text("Print").size(15)).style(theme::success_button).padding([10, 24]).on_press(Message::PrintSalePressed(sale.id)),
            button(text("Download").size(15)).style(theme::secondary_button).padding([10, 24]).on_press(Message::DownloadSalePressed(sale.id)),
            button(text("Back").size(15)).style(theme::secondary_button).padding([10, 24]).on_press(Message::CloseSaleView),
        ]
        .spacing(theme::SPACE_MD),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(560);

    container(container(body).style(theme::card).padding(theme::SPACE_LG))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(theme::SPACE_MD)
        .align_x(iced::Alignment::Center)
        .into()
}

fn detail_row<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    row![text(label).size(14).width(Length::Fixed(140.0)), text(value).size(14)].into()
}

fn labeled<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(13), widget.into()].spacing(4).into()
}
