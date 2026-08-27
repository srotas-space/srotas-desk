use iced::widget::{button, column, combo_box, container, row, scrollable, text, text_input};
use iced::{Element, Length, Task};
use std::path::PathBuf;

use super::{Message, State};
use crate::money;
use crate::models::{BillDetail, BillSummary};
use crate::repo::BillLineInput;
use crate::ui::common::ItemOption;
use crate::ui::theme;

pub const PAGE_SIZE: i64 = 10;

/// One line in the bill currently being built/edited — a UI-local shape;
/// converted to `repo::BillLineInput` only at submit time.
#[derive(Debug, Clone)]
pub struct CartLine {
    pub item_id: i64,
    pub item_name: String,
    pub qty: f64,
    pub price_paise: i64,
    /// Resolved at the moment the line is added (item override, falling
    /// back to the shop default) — basis points, 1800 = 18.00%.
    pub gst_rate_bp: i64,
}

impl CartLine {
    pub fn line_total_paise(&self) -> i64 {
        (self.price_paise as f64 * self.qty).round() as i64
    }
}

/// What the cart adds up to, tax included — mirrors `repo::bills`'s
/// discount-proration-then-tax calculation exactly (same per-line discount
/// share, same round-half-up split), so what's shown here is what actually
/// gets persisted on Save.
struct CartTotals {
    subtotal_paise: i64,
    cgst_paise: i64,
    sgst_paise: i64,
    total_paise: i64,
}

fn compute_totals(cart: &[CartLine], discount_paise: i64) -> CartTotals {
    let line_totals: Vec<i64> = cart.iter().map(CartLine::line_total_paise).collect();
    let subtotal_paise: i64 = line_totals.iter().sum();

    let mut allocated_discount = 0i64;
    let mut cgst_paise = 0i64;
    let mut sgst_paise = 0i64;
    for (i, (line, &total)) in cart.iter().zip(&line_totals).enumerate() {
        let discount_share = if subtotal_paise == 0 {
            0
        } else if i == cart.len() - 1 {
            discount_paise - allocated_discount
        } else {
            let share = (discount_paise as i128 * total as i128 / subtotal_paise as i128) as i64;
            allocated_discount += share;
            share
        };
        let taxable = (total - discount_share).max(0);
        let total_tax = ((taxable as i128 * line.gst_rate_bp as i128 + 5_000) / 10_000) as i64;
        cgst_paise += total_tax / 2;
        sgst_paise += total_tax - total_tax / 2;
    }

    let total_paise = (subtotal_paise - discount_paise).max(0) + cgst_paise + sgst_paise;
    CartTotals { subtotal_paise, cgst_paise, sgst_paise, total_paise }
}

#[derive(Default)]
pub struct BillsState {
    pub cart: Vec<CartLine>,
    pub editing_id: Option<i64>,
    pub item_selected: Option<ItemOption>,
    pub qty_input: String,
    pub price_input: String,
    pub discount_input: String,

    pub page: i64,
    pub rows: Vec<BillSummary>,
    pub total: i64,

    pub viewing: Option<BillDetail>,
    pub confirming_delete_id: Option<i64>,
}

fn subtotal_paise(cart: &[CartLine]) -> i64 {
    cart.iter().map(CartLine::line_total_paise).sum()
}

pub fn select_item(state: &mut State, option: ItemOption) {
    if let Some(item) = state.items.iter().find(|i| i.id == option.id) {
        state.bills.price_input = money::paise_to_input(item.sell_price_paise);
    }
    state.bills.item_selected = Some(option);
}

pub fn add_line(state: &mut State) {
    let Some(selected) = state.bills.item_selected.clone() else {
        state.status = Some("choose an item first".into());
        return;
    };
    let Some(qty) = state.bills.qty_input.trim().parse::<f64>().ok().filter(|q| *q > 0.0) else {
        state.status = Some("quantity must be a positive number".into());
        return;
    };
    let Some(price_paise) = money::rupees_to_paise(&state.bills.price_input) else {
        state.status = Some("price must be a valid amount, e.g. 120.00".into());
        return;
    };

    let gst_rate_bp = state
        .items
        .iter()
        .find(|i| i.id == selected.id)
        .and_then(|i| i.gst_rate_bp)
        .unwrap_or_else(|| state.shop.as_ref().map(|s| s.gst_rate_bp).unwrap_or(0));

    state.bills.cart.push(CartLine { item_id: selected.id, item_name: selected.name, qty, price_paise, gst_rate_bp });
    state.bills.item_selected = None;
    state.bills.qty_input.clear();
    state.bills.price_input.clear();
    state.status = None;
}

pub fn remove_line(state: &mut State, index: usize) {
    if index < state.bills.cart.len() {
        state.bills.cart.remove(index);
    }
}

pub fn start_new(state: &mut State) {
    state.bills.cart.clear();
    state.bills.editing_id = None;
    state.bills.discount_input.clear();
    state.bills.item_selected = None;
    state.bills.qty_input.clear();
    state.bills.price_input.clear();
    state.status = None;
}

pub fn load_for_edit(state: &mut State, detail: BillDetail) {
    state.bills.editing_id = Some(detail.id);
    state.bills.discount_input = money::paise_to_input(detail.discount_paise);
    state.bills.cart = detail
        .lines
        .iter()
        .map(|l| CartLine { item_id: l.item_id, item_name: l.item_name.clone(), qty: l.qty, price_paise: l.price_paise, gst_rate_bp: l.gst_rate_bp })
        .collect();
    state.viewing_item_id = None;
}

pub fn submit(state: &mut State) -> Task<Message> {
    if state.bills.cart.is_empty() {
        state.status = Some("add at least one item to the bill".into());
        return Task::none();
    }
    let discount_paise = if state.bills.discount_input.trim().is_empty() {
        0
    } else {
        match money::rupees_to_paise(&state.bills.discount_input) {
            Some(v) => v,
            None => {
                state.status = Some("discount must be a valid amount, e.g. 20.00".into());
                return Task::none();
            }
        }
    };
    let subtotal = subtotal_paise(&state.bills.cart);
    if discount_paise > subtotal {
        state.status = Some("discount can't be more than the subtotal".into());
        return Task::none();
    }

    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };
    let lines: Vec<BillLineInput> = state
        .bills
        .cart
        .iter()
        .map(|l| BillLineInput { item_id: l.item_id, item_name: l.item_name.clone(), qty: l.qty, price_paise: l.price_paise, gst_rate_bp: l.gst_rate_bp })
        .collect();
    let editing_id = state.bills.editing_id;

    Task::perform(
        async move {
            match editing_id {
                Some(id) => crate::repo::edit_bill(&pool, id, &lines, discount_paise).await.map(|_| id),
                None => crate::repo::create_bill(&pool, &lines, discount_paise).await,
            }
        },
        |result| Message::BillSaved(result.map_err(|e| e.to_string())),
    )
}

pub fn load_history(pool: sqlx::SqlitePool, page: i64) -> Task<Message> {
    Task::perform(
        async move { crate::repo::list_bills(&pool, page, PAGE_SIZE).await },
        |result| Message::BillsLoaded(result.map_err(|e| e.to_string())),
    )
}

/// Re-fetches the bill fresh (rather than trusting whatever's on screen —
/// same reasoning as the Reports download), builds a one-page receipt PDF,
/// saves it, and opens it in the OS default viewer so "Print" is just
/// whatever print pipeline that viewer already offers.
pub fn print_bill(state: &State, id: i64) -> Task<Message> {
    let Some(pool) = state.pool.clone() else {
        return Task::done(Message::BillPdfReady(Err("database is not ready yet".into())));
    };
    let shop_name = state.shop.as_ref().map(|s| s.shop_name.clone()).unwrap_or_else(|| "Srotas Desk".to_string());
    let gstin = state.shop.as_ref().and_then(|s| s.gstin.clone());

    Task::perform(
        async move {
            let detail = crate::repo::get_bill(&pool, id).await.map_err(|e| e.to_string())?;
            let bytes = build_pdf_bytes(&shop_name, gstin.as_deref(), &detail)?;
            save_and_open(bytes, detail.id).await
        },
        Message::BillPdfReady,
    )
}

async fn save_and_open(bytes: Vec<u8>, id: i64) -> Result<PathBuf, String> {
    let dir = dirs::download_dir()
        .or_else(dirs::document_dir)
        .ok_or("could not find a Downloads folder on this computer")?;
    let path = dir.join(format!("srotas-bill-{id}.pdf"));

    tokio::fs::write(&path, &bytes).await.map_err(|e| e.to_string())?;
    open::that(&path).map_err(|e| format!("bill saved, but couldn't open it: {e}"))?;

    Ok(path)
}

const TABLE_ITEM_X: f32 = crate::pdf::LEFT_MM;
const TABLE_QTY_X: f32 = 85.0;
const TABLE_PRICE_X: f32 = 105.0;
const TABLE_GST_X: f32 = 135.0;
const TABLE_TOTAL_X: f32 = 160.0;

fn build_pdf_bytes(shop_name: &str, gstin: Option<&str>, detail: &BillDetail) -> Result<Vec<u8>, String> {
    let mut w = crate::pdf::Writer::new(&format!("{shop_name} - Bill #{}", detail.id))?;

    w.line(shop_name, 18.0, true);
    if let Some(gstin) = gstin.filter(|g| !g.trim().is_empty()) {
        w.line(&format!("GSTIN: {gstin}"), 10.0, false);
    }
    w.line(&format!("Bill #{}", detail.id), 14.0, true);
    w.line(&format!("Date: {}", detail.timestamp.format("%d %b %Y %H:%M")), 10.0, false);
    w.gap(6.0);

    w.row(
        &[("Item", TABLE_ITEM_X), ("Qty", TABLE_QTY_X), ("Price", TABLE_PRICE_X), ("GST", TABLE_GST_X), ("Total", TABLE_TOTAL_X)],
        10.0,
        true,
    );
    for l in &detail.lines {
        let name = truncate(&l.item_name, 32);
        let qty = format!("{:.1}", l.qty);
        let price = money::format_paise_ascii(l.price_paise);
        let gst = money::format_gst_rate_bp(l.gst_rate_bp);
        let total = money::format_paise_ascii(l.line_total_paise);
        w.row(
            &[
                (name.as_str(), TABLE_ITEM_X),
                (qty.as_str(), TABLE_QTY_X),
                (price.as_str(), TABLE_PRICE_X),
                (gst.as_str(), TABLE_GST_X),
                (total.as_str(), TABLE_TOTAL_X),
            ],
            9.0,
            false,
        );
    }
    w.gap(6.0);

    w.line(&format!("Subtotal: {}", money::format_paise_ascii(detail.subtotal_paise)), 11.0, false);
    w.line(&format!("Discount: {}", money::format_paise_ascii(detail.discount_paise)), 11.0, false);
    w.line(&format!("CGST: {}", money::format_paise_ascii(detail.cgst_paise)), 11.0, false);
    w.line(&format!("SGST: {}", money::format_paise_ascii(detail.sgst_paise)), 11.0, false);
    w.line(&format!("Total: {}", money::format_paise_ascii(detail.total_paise)), 14.0, true);

    w.finish()
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}

pub fn view(state: &State) -> Element<'_, Message> {
    if let Some(detail) = &state.bills.viewing {
        return detail_view(state, detail);
    }

    let bills = &state.bills;
    let title = match bills.editing_id {
        Some(id) => format!("Edit Bill #{id}"),
        None => "New Bill".to_string(),
    };

    let item_picker = combo_box(&state.bill_item_combo, "Search item...", bills.item_selected.as_ref(), Message::BillItemSelected)
        .padding(10)
        .width(Length::Fixed(240.0));

    let add_row = row![
        labeled("Item", item_picker),
        labeled("Quantity", text_input("e.g. 2", &bills.qty_input).on_input(Message::BillQtyChanged).padding(10).width(Length::Fixed(100.0))),
        labeled("Price (₹)", text_input("120.00", &bills.price_input).on_input(Message::BillPriceChanged).padding(10).width(Length::Fixed(120.0))),
        button(text("Add Line").size(14)).style(theme::secondary_button).padding([10, 16]).on_press(Message::AddBillLine),
    ]
    .spacing(theme::SPACE_MD)
    .align_y(iced::Alignment::End);

    let mut cart_list = column![].spacing(4);
    if bills.cart.is_empty() {
        cart_list = cart_list.push(text("No items added yet.").size(13).color(theme::MUTED_TEXT));
    }
    for (i, line) in bills.cart.iter().enumerate() {
        cart_list = cart_list.push(
            row![
                text(&line.item_name).width(Length::FillPortion(3)),
                text(format!("{:.1}", line.qty)).width(Length::FillPortion(1)),
                text(money::format_paise(line.price_paise)).width(Length::FillPortion(1)),
                text(money::format_gst_rate_bp(line.gst_rate_bp)).size(12).color(theme::MUTED_TEXT).width(Length::FillPortion(1)),
                text(money::format_paise(line.line_total_paise())).width(Length::FillPortion(1)),
                button(text("Remove").size(12)).style(theme::danger_button).padding([4, 10]).on_press(Message::RemoveBillLine(i)),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );
    }

    let discount_paise = money::rupees_to_paise(&bills.discount_input).unwrap_or(0);
    let computed = compute_totals(&bills.cart, discount_paise);

    let totals = column![
        row![text("Subtotal").width(Length::Fixed(140.0)), text(money::format_paise(computed.subtotal_paise))],
        row![
            text("Discount (₹)").width(Length::Fixed(140.0)),
            text_input("0.00", &bills.discount_input).on_input(Message::BillDiscountChanged).padding(8).width(Length::Fixed(140.0)),
        ]
        .align_y(iced::Alignment::Center),
        row![text("CGST").width(Length::Fixed(140.0)), text(money::format_paise(computed.cgst_paise))],
        row![text("SGST").width(Length::Fixed(140.0)), text(money::format_paise(computed.sgst_paise))],
        row![
            text("Total").width(Length::Fixed(140.0)).size(18),
            text(money::format_paise(computed.total_paise)).size(18).font(theme::BOLD).color(theme::VIOLET),
        ],
    ]
    .spacing(8);

    let mut actions = row![
        button(text("Save Bill").size(15)).style(theme::success_button).padding([10, 24]).on_press(Message::SubmitBill),
    ]
    .spacing(theme::SPACE_MD);
    if bills.editing_id.is_some() {
        actions = actions.push(button(text("Cancel Edit").size(15)).style(theme::secondary_button).padding([10, 24]).on_press(Message::CancelBillEdit));
    }

    let form = column![
        text(title).size(20),
        add_row,
        container(cart_list).style(theme::card).padding(theme::SPACE_MD),
        totals,
        actions,
    ]
    .spacing(theme::SPACE_MD);

    let total_pages = (bills.total.max(0) as f64 / PAGE_SIZE as f64).ceil().max(1.0) as i64;
    let mut history = column![text("Bill History").size(16)].spacing(6);
    if bills.rows.is_empty() {
        history = history.push(text("No bills recorded yet.").size(13).color(theme::MUTED_TEXT));
    }
    for b in &bills.rows {
        let confirming = bills.confirming_delete_id == Some(b.id);
        history = history.push(
            container(
                row![
                    text(format!("Bill #{}", b.id)).width(Length::FillPortion(2)),
                    text(format!("{} item(s)", b.item_count)).width(Length::FillPortion(2)),
                    text(money::format_paise(b.total_paise)).width(Length::FillPortion(2)),
                    text(b.timestamp.format("%d-%b-%y %H:%M").to_string()).width(Length::FillPortion(2)),
                    row![
                        button(text("View").size(12)).style(theme::secondary_button).padding([6, 10]).on_press(Message::OpenBillView(b.id)),
                        button(text("Edit").size(12)).style(theme::secondary_button).padding([6, 10]).on_press(Message::OpenBillEdit(b.id)),
                        button(text("Print").size(12)).style(theme::secondary_button).padding([6, 10]).on_press(Message::PrintBillPressed(b.id)),
                        button(text(if confirming { "Confirm?" } else { "Delete" }).size(12)).style(theme::danger_button).padding([6, 10]).on_press(Message::DeleteBillPressed(b.id)),
                    ]
                    .spacing(6)
                    .width(Length::FillPortion(4)),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            )
            .style(theme::card)
            .padding(theme::SPACE_SM),
        );
    }
    let pagination = row![
        text(format!("Page {} of {}", bills.page + 1, total_pages)).size(13).color(theme::MUTED_TEXT),
        iced::widget::space::horizontal(),
        button(text("Prev").size(14)).style(theme::secondary_button).padding([8, 16]).on_press_maybe((bills.page > 0).then_some(Message::BillsPagePrev)),
        button(text("Next").size(14)).style(theme::secondary_button).padding([8, 16]).on_press_maybe((bills.page + 1 < total_pages).then_some(Message::BillsPageNext)),
    ]
    .spacing(theme::SPACE_MD)
    .align_y(iced::Alignment::Center);

    scrollable(
        column![
            container(form).style(theme::card).padding(theme::SPACE_MD),
            container(history).style(theme::card).padding(theme::SPACE_MD),
            pagination,
        ]
        .spacing(theme::SPACE_MD)
        .padding(theme::SPACE_MD),
    )
    .height(Length::Fill)
    .into()
}

const DETAIL_THUMBNAIL_SIZE: f32 = 32.0;

fn detail_view<'a>(state: &'a State, detail: &'a BillDetail) -> Element<'a, Message> {
    let mut lines = column![].spacing(4);
    for l in &detail.lines {
        let mut item_cell = row![].spacing(theme::SPACE_SM).align_y(iced::Alignment::Center).width(Length::FillPortion(3));
        if let Some(bytes) = state.item_thumbnails.get(&l.item_id) {
            item_cell = item_cell.push(
                iced::widget::image::Image::new(iced::widget::image::Handle::from_bytes(bytes.clone()))
                    .width(DETAIL_THUMBNAIL_SIZE)
                    .height(DETAIL_THUMBNAIL_SIZE)
                    .content_fit(iced::ContentFit::Cover),
            );
        }
        item_cell = item_cell.push(text(&l.item_name));

        lines = lines.push(
            row![
                item_cell,
                text(format!("{:.1}", l.qty)).width(Length::FillPortion(1)),
                text(money::format_paise(l.price_paise)).width(Length::FillPortion(1)),
                text(money::format_gst_rate_bp(l.gst_rate_bp)).size(12).color(theme::MUTED_TEXT).width(Length::FillPortion(1)),
                text(money::format_paise(l.line_total_paise)).width(Length::FillPortion(1)),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );
    }

    let body = column![
        text(format!("Bill #{}", detail.id)).size(22),
        text(detail.timestamp.format("%d %b %Y %H:%M").to_string()).size(13).color(theme::MUTED_TEXT),
        container(lines).style(theme::card).padding(theme::SPACE_MD),
        row![text("Subtotal").width(Length::Fixed(140.0)), text(money::format_paise(detail.subtotal_paise))],
        row![text("Discount").width(Length::Fixed(140.0)), text(money::format_paise(detail.discount_paise))],
        row![text("CGST").width(Length::Fixed(140.0)), text(money::format_paise(detail.cgst_paise))],
        row![text("SGST").width(Length::Fixed(140.0)), text(money::format_paise(detail.sgst_paise))],
        row![
            text("Total").width(Length::Fixed(140.0)).size(18),
            text(money::format_paise(detail.total_paise)).size(18).font(theme::BOLD).color(theme::VIOLET),
        ],
        row![
            button(text("Edit").size(15)).style(theme::primary_button).padding([10, 24]).on_press(Message::OpenBillEdit(detail.id)),
            button(text("Print").size(15)).style(theme::success_button).padding([10, 24]).on_press(Message::PrintBillPressed(detail.id)),
            button(text("Back").size(15)).style(theme::secondary_button).padding([10, 24]).on_press(Message::CloseBillView),
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

fn labeled<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(13), widget.into()].spacing(4).into()
}
