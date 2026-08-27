use iced::widget::{button, checkbox, column, container, image, pick_list, row, scrollable, text, text_input};
use iced::{Element, Length, Task};

use super::{Message, State};
use crate::models::{Item, Unit};
use crate::money;
use crate::ui::theme;

pub const PAGE_SIZE: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurchaseField {
    Qty,
    Price,
}

/// Restocking an existing item — the counterpart to "Sell Stock", opened
/// from an item's detail screen rather than needing its own item picker
/// (the item is already the one you're looking at).
#[derive(Debug, Clone)]
pub struct PurchaseForm {
    pub item_id: i64,
    pub item_name: String,
    pub qty: String,
    pub price: String,
}

impl PurchaseForm {
    pub fn set_field(&mut self, field: PurchaseField, value: String) {
        match field {
            PurchaseField::Qty => self.qty = value,
            PurchaseField::Price => self.price = value,
        }
    }

    pub fn get_field(&self, field: PurchaseField) -> String {
        match field {
            PurchaseField::Qty => self.qty.clone(),
            PurchaseField::Price => self.price.clone(),
        }
    }
}

pub fn submit_purchase(state: &mut State) -> Task<Message> {
    let Some(form) = &state.purchase_form else {
        return Task::none();
    };
    let Some(qty) = form.qty.trim().parse::<f64>().ok().filter(|q| *q > 0.0) else {
        state.status = Some("quantity must be a positive number".into());
        return Task::none();
    };
    let Some(price_paise) = money::rupees_to_paise(&form.price) else {
        state.status = Some("buy price must be a valid amount, e.g. 80.00".into());
        return Task::none();
    };
    let item_id = form.item_id;
    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };

    Task::perform(
        async move { crate::repo::record_purchase(&pool, item_id, qty, price_paise, chrono::Utc::now()).await },
        |result| Message::PurchaseRecorded(result.map_err(|e| e.to_string())),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormField {
    Name,
    BuyPrice,
    SellPrice,
    StockQty,
    LowStockThreshold,
    Description,
    GstRate,
}

/// The unit filter on the inventory list needs an "All units" option that
/// plain `Unit` doesn't have room for, so it gets its own small enum rather
/// than overloading `Option<Unit>` (which can't implement `Display` for
/// `pick_list` since neither type is local to this crate... `Option` is,
/// but a wrapper reads clearer at the call site anyway).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitFilter {
    All,
    Only(Unit),
}

impl UnitFilter {
    pub const ALL: [UnitFilter; 4] =
        [UnitFilter::All, UnitFilter::Only(Unit::Piece), UnitFilter::Only(Unit::Kg), UnitFilter::Only(Unit::Metre)];

    fn matches(self, item: &Item) -> bool {
        match self {
            UnitFilter::All => true,
            UnitFilter::Only(unit) => item.unit == unit.as_str(),
        }
    }
}

impl Default for UnitFilter {
    fn default() -> Self {
        UnitFilter::All
    }
}

impl std::fmt::Display for UnitFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnitFilter::All => f.write_str("All units"),
            UnitFilter::Only(unit) => write!(f, "{unit}"),
        }
    }
}

pub struct ItemForm {
    pub editing_id: Option<i64>,
    pub name: String,
    pub buy_price: String,
    pub sell_price: String,
    /// Opening stock quantity — only meaningful (and only shown) when
    /// adding a brand new item. Existing stock is changed only via
    /// purchases/sales, never edited directly.
    pub stock_qty: String,
    pub unit: Unit,
    pub low_stock_threshold: String,
    pub description: String,
    /// `None` means "no photo" on a new item, or "photo not loaded yet" on
    /// an item being edited (fetched separately — see `OpenEditItemForm` in
    /// `ui/mod.rs`). Either way, whatever is here when Save is pressed is
    /// exactly what gets written.
    pub image: Option<Vec<u8>>,
    /// GST rate override as a percentage string (e.g. "18"). Blank means
    /// "use the shop's default rate" — see `models::Item::gst_rate_bp`.
    pub gst_rate: String,
}

impl ItemForm {
    pub fn empty() -> Self {
        ItemForm {
            editing_id: None,
            name: String::new(),
            buy_price: String::new(),
            sell_price: String::new(),
            stock_qty: String::new(),
            unit: Unit::Piece,
            low_stock_threshold: String::new(),
            description: String::new(),
            image: None,
            gst_rate: String::new(),
        }
    }

    pub fn from_item(item: &Item) -> Self {
        ItemForm {
            editing_id: Some(item.id),
            name: item.name.clone(),
            buy_price: money::paise_to_input(item.buy_price_paise),
            sell_price: money::paise_to_input(item.sell_price_paise),
            stock_qty: String::new(),
            unit: Unit::parse(&item.unit).unwrap_or(Unit::Piece),
            low_stock_threshold: item.low_stock_threshold.to_string(),
            description: item.description.clone(),
            image: None,
            gst_rate: item.gst_rate_bp.map(money::paise_to_input).unwrap_or_default(),
        }
    }

    pub fn set_field(&mut self, field: FormField, value: String) {
        match field {
            FormField::Name => self.name = value,
            FormField::BuyPrice => self.buy_price = value,
            FormField::SellPrice => self.sell_price = value,
            FormField::StockQty => self.stock_qty = value,
            FormField::LowStockThreshold => self.low_stock_threshold = value,
            FormField::Description => self.description = value,
            FormField::GstRate => self.gst_rate = value,
        }
    }

    /// The current value of `field` — read before overwriting it, so
    /// `ui::mod::apply_edit` can hand the old value back on undo/redo.
    pub fn get_field(&self, field: FormField) -> String {
        match field {
            FormField::Name => self.name.clone(),
            FormField::BuyPrice => self.buy_price.clone(),
            FormField::SellPrice => self.sell_price.clone(),
            FormField::StockQty => self.stock_qty.clone(),
            FormField::LowStockThreshold => self.low_stock_threshold.clone(),
            FormField::Description => self.description.clone(),
            FormField::GstRate => self.gst_rate.clone(),
        }
    }
}

struct ParsedForm {
    editing_id: Option<i64>,
    name: String,
    buy_price_paise: i64,
    sell_price_paise: i64,
    stock_qty: f64,
    unit: Unit,
    low_stock_threshold: f64,
    description: String,
    image: Option<Vec<u8>>,
    gst_rate_bp: Option<i64>,
}

fn parse_form(form: &ItemForm) -> Result<ParsedForm, String> {
    if form.name.trim().is_empty() {
        return Err("item name is required".into());
    }
    let buy_price_paise =
        money::rupees_to_paise(&form.buy_price).ok_or("buy price must be a valid amount, e.g. 80.50")?;
    let sell_price_paise = money::rupees_to_paise(&form.sell_price)
        .ok_or("sell price must be a valid amount, e.g. 120.00")?;

    let stock_qty = if form.editing_id.is_some() {
        0.0 // ignored on edit — the UPDATE query never touches stock_qty
    } else {
        let raw = form.stock_qty.trim();
        if raw.is_empty() {
            return Err("opening stock is required".into());
        }
        let qty = raw.parse::<f64>().map_err(|_| "opening stock must be a number")?;
        if qty < 0.0 {
            return Err("opening stock cannot be negative".into());
        }
        qty
    };

    let threshold_raw = form.low_stock_threshold.trim();
    if threshold_raw.is_empty() {
        return Err("low-stock alert threshold is required".into());
    }
    let low_stock_threshold = threshold_raw.parse::<f64>().map_err(|_| "low-stock alert threshold must be a number")?;
    if low_stock_threshold < 0.0 {
        return Err("low-stock alert threshold cannot be negative".into());
    }

    let gst_rate_bp = if form.gst_rate.trim().is_empty() {
        None
    } else {
        Some(money::rupees_to_paise(&form.gst_rate).ok_or("GST rate must be a valid percentage, e.g. 18 or 18.00")?)
    };

    Ok(ParsedForm {
        editing_id: form.editing_id,
        name: form.name.trim().to_string(),
        buy_price_paise,
        sell_price_paise,
        stock_qty,
        unit: form.unit,
        low_stock_threshold,
        description: form.description.trim().to_string(),
        image: form.image.clone(),
        gst_rate_bp,
    })
}

pub fn submit(state: &mut State) -> Task<Message> {
    let Some(form) = &state.item_form else {
        return Task::none();
    };
    let parsed = match parse_form(form) {
        Ok(p) => p,
        Err(e) => {
            state.status = Some(e);
            return Task::none();
        }
    };
    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            match parsed.editing_id {
                Some(id) => {
                    crate::repo::edit_item(
                        &pool,
                        id,
                        &parsed.name,
                        parsed.buy_price_paise,
                        parsed.sell_price_paise,
                        parsed.unit,
                        parsed.low_stock_threshold,
                        &parsed.description,
                        parsed.image.as_deref(),
                        parsed.gst_rate_bp,
                    )
                    .await
                }
                None => {
                    crate::repo::add_item(
                        &pool,
                        &parsed.name,
                        parsed.buy_price_paise,
                        parsed.sell_price_paise,
                        parsed.stock_qty,
                        parsed.unit,
                        parsed.low_stock_threshold,
                        &parsed.description,
                        parsed.image.as_deref(),
                        parsed.gst_rate_bp,
                    )
                    .await
                }
            }
        },
        |result| Message::ItemSaved(result.map_err(|e| e.to_string())),
    )
}

pub fn choose_image() -> Task<Message> {
    Task::perform(
        async {
            let handle = rfd::AsyncFileDialog::new()
                .set_title("Choose a product photo")
                .add_filter("Image", &["png", "jpg", "jpeg"])
                .pick_file()
                .await;
            let Some(handle) = handle else {
                return Ok(None);
            };
            // Read via tokio rather than `FileHandle::read` — the latter
            // panics on an I/O error instead of returning one, which would
            // take the whole app down over something as ordinary as a
            // permissions error on the chosen file.
            tokio::fs::read(handle.path()).await.map(Some).map_err(|e| e.to_string())
        },
        Message::ItemImagePicked,
    )
}

pub fn view(state: &State) -> Element<'_, Message> {
    if let Some(form) = &state.purchase_form {
        return purchase_view(form);
    }
    if let Some(form) = &state.item_form {
        let default_gst_bp = state.shop.as_ref().map(|s| s.gst_rate_bp).unwrap_or(0);
        return form_view(form, default_gst_bp);
    }
    if let Some(id) = state.viewing_item_id {
        if let Some(item) = state.items.iter().find(|i| i.id == id) {
            let default_gst_bp = state.shop.as_ref().map(|s| s.gst_rate_bp).unwrap_or(0);
            return detail_view(item, state.view_image.as_deref(), default_gst_bp);
        }
    }
    list_view(state)
}

/// The current search/filter/pagination applied to `state.items` — shared
/// between `list_view` (to render it) and `ui::mod`'s thumbnail loader (to
/// know which items are actually on screen right now, since thumbnails are
/// only ever fetched for the visible page — see `Item`'s doc comment on
/// why photo bytes never ride along with the bulk item list).
pub fn current_page(state: &State) -> (Vec<&Item>, usize, usize) {
    let query = state.search_query.trim().to_lowercase();
    let filtered: Vec<&Item> = state
        .items
        .iter()
        .filter(|item| query.is_empty() || item.name.to_lowercase().contains(&query))
        .filter(|item| state.unit_filter.matches(item))
        .filter(|item| !state.low_stock_only || item.is_low_stock())
        .collect();

    let total = filtered.len();
    let page_count = total.div_ceil(PAGE_SIZE).max(1);
    let page = state.items_page.min(page_count - 1);
    let start = page * PAGE_SIZE;
    let page_items = filtered.into_iter().skip(start).take(PAGE_SIZE).collect();

    (page_items, total, page_count)
}

fn list_view(state: &State) -> Element<'_, Message> {
    let search = text_input("Search items...", &state.search_query)
        .on_input(Message::SearchChanged)
        .padding(10)
        .size(16)
        .width(Length::Fixed(280.0));

    let unit_filter = pick_list(UnitFilter::ALL, Some(state.unit_filter), Message::UnitFilterSelected).padding(10);

    let low_stock_only = checkbox::Checkbox::new(state.low_stock_only)
        .label("Low stock only")
        .on_toggle(Message::LowStockOnlyToggled);

    let add_button = button(text("+ Add Item").size(15)).style(theme::primary_button).padding([10, 20]).on_press(Message::OpenAddItemForm);

    let header = row![search, unit_filter, low_stock_only, iced::widget::space::horizontal(), add_button]
        .spacing(theme::SPACE_MD)
        .padding(theme::SPACE_MD)
        .align_y(iced::Alignment::Center);

    let column_labels = container(
        row![
            text("Item").size(12).width(Length::FillPortion(3)),
            text("Stock").size(12).width(Length::FillPortion(2)),
            text("Buy Price").size(12).width(Length::FillPortion(2)),
            text("Sell Price").size(12).width(Length::FillPortion(2)),
            text("").width(Length::FillPortion(3)),
        ]
        .spacing(8),
    )
    .padding([0.0, theme::SPACE_SM]);

    let (page_items, total, page_count) = current_page(state);
    let page = state.items_page.min(page_count - 1);
    let start = page * PAGE_SIZE;

    let mut list = column![].spacing(theme::SPACE_SM).padding(theme::SPACE_MD);
    for item in &page_items {
        let thumbnail = state.item_thumbnails.get(&item.id).map(|bytes| bytes.as_slice());
        list = list.push(item_row(item, state.confirming_delete_id == Some(item.id), thumbnail));
    }
    if total == 0 {
        list = list.push(text("No items match this search/filter.").size(14));
    }

    let range_label = if total == 0 {
        "0 of 0".to_string()
    } else {
        format!("{}-{} of {}", start + 1, (start + PAGE_SIZE).min(total), total)
    };

    let pagination = row![
        text(range_label).size(13),
        iced::widget::space::horizontal(),
        button(text("Prev").size(14)).style(theme::secondary_button).padding([8, 16]).on_press_maybe((page > 0).then_some(Message::ItemsPagePrev)),
        text(format!("Page {} of {}", page + 1, page_count)).size(13),
        button(text("Next").size(14)).style(theme::secondary_button).padding([8, 16]).on_press_maybe((page + 1 < page_count).then_some(Message::ItemsPageNext)),
    ]
    .spacing(theme::SPACE_MD)
    .padding(theme::SPACE_MD)
    .align_y(iced::Alignment::Center);

    column![header, column_labels, scrollable(list).height(Length::Fill), pagination]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

const THUMBNAIL_SIZE: f32 = 40.0;

fn item_row<'a>(item: &'a Item, confirming_delete: bool, thumbnail: Option<&'a [u8]>) -> Element<'a, Message> {
    let mut name_col = column![text(&item.name).size(16)].spacing(4);
    if item.is_low_stock() {
        name_col = name_col.push(container(text("LOW STOCK").size(11)).style(theme::low_stock_badge).padding([2, 8]));
    }

    let mut name_row = row![].spacing(theme::SPACE_SM).align_y(iced::Alignment::Center);
    if let Some(bytes) = thumbnail {
        name_row = name_row.push(
            image::Image::new(image::Handle::from_bytes(bytes.to_vec()))
                .width(THUMBNAIL_SIZE)
                .height(THUMBNAIL_SIZE)
                .content_fit(iced::ContentFit::Cover),
        );
    }
    name_row = name_row.push(name_col);

    let delete_label = if confirming_delete { "Confirm?" } else { "Delete" };

    let controls = row![
        button(text("View").size(14)).style(theme::secondary_button).padding([8, 14]).on_press(Message::OpenViewItem(item.id)),
        button(text("Edit").size(14)).style(theme::secondary_button).padding([8, 14]).on_press(Message::OpenEditItemForm(item.id)),
        button(text(delete_label).size(14)).style(theme::danger_button).padding([8, 14]).on_press(Message::DeleteItemPressed(item.id)),
    ]
    .spacing(6);

    container(
        row![
            container(name_row).width(Length::FillPortion(3)),
            text(format!("{:.1} {}", item.stock_qty, item.unit)).size(15).width(Length::FillPortion(2)),
            text(money::format_paise(item.buy_price_paise)).size(15).width(Length::FillPortion(2)),
            text(money::format_paise(item.sell_price_paise)).size(15).width(Length::FillPortion(2)),
            container(controls).width(Length::FillPortion(3)),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(8),
    )
    .style(theme::card)
    .padding(theme::SPACE_SM)
    .width(Length::Fill)
    .into()
}

fn form_view(form: &ItemForm, default_gst_bp: i64) -> Element<'_, Message> {
    let title = if form.editing_id.is_some() { "Edit Item" } else { "Add Item" };

    let photo: Element<'_, Message> = match &form.image {
        Some(bytes) => image::Image::new(image::Handle::from_bytes(bytes.clone()))
            .width(120)
            .height(120)
            .content_fit(iced::ContentFit::Cover)
            .into(),
        None => container(text("No photo").size(13)).width(120).height(120).style(theme::card).padding(theme::SPACE_SM).align_x(iced::Alignment::Center).align_y(iced::Alignment::Center).into(),
    };

    let photo_controls = column![
        photo,
        row![
            button(text("Choose Photo").size(13)).style(theme::secondary_button).padding([8, 14]).on_press(Message::ChooseItemImage),
            button(text("Remove").size(13)).style(theme::secondary_button).padding([8, 14]).on_press(Message::RemoveItemImage),
        ]
        .spacing(theme::SPACE_SM),
    ]
    .spacing(theme::SPACE_SM);

    let mut fields = column![
        text(title).size(22),
        labeled("Name *", text_input("e.g. PVC Pipe 1 inch", &form.name).on_input(|v| Message::FormFieldChanged(FormField::Name, v)).padding(10).size(16)),
        labeled("Buy price (₹) *", text_input("80.00", &form.buy_price).on_input(|v| Message::FormFieldChanged(FormField::BuyPrice, v)).padding(10).size(16)),
        labeled("Sell price (₹) *", text_input("120.00", &form.sell_price).on_input(|v| Message::FormFieldChanged(FormField::SellPrice, v)).padding(10).size(16)),
        labeled(
            "Unit *",
            pick_list(Unit::ALL, Some(form.unit), Message::FormUnitSelected).padding(10),
        ),
        labeled(
            "Low stock alert below *",
            text_input("5", &form.low_stock_threshold)
                .on_input(|v| Message::FormFieldChanged(FormField::LowStockThreshold, v))
                .padding(10)
                .size(16),
        ),
        labeled(
            "Description (optional)",
            text_input("e.g. 10mm dia, galvanized", &form.description)
                .on_input(|v| Message::FormFieldChanged(FormField::Description, v))
                .padding(10)
                .size(16),
        ),
        column![
            text(format!("GST rate % (blank = shop default, {})", money::format_gst_rate_bp(default_gst_bp))).size(14),
            text_input("e.g. 18", &form.gst_rate).on_input(|v| Message::FormFieldChanged(FormField::GstRate, v)).padding(10).size(16),
        ]
        .spacing(4),
        labeled("Photo (optional)", photo_controls),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(420);

    if form.editing_id.is_none() {
        fields = fields.push(labeled(
            "Opening stock *",
            text_input("0", &form.stock_qty)
                .on_input(|v| Message::FormFieldChanged(FormField::StockQty, v))
                .padding(10)
                .size(16),
        ));
    }

    let actions = row![
        button(text("Save").size(16)).style(theme::success_button).padding([10, 24]).on_press(Message::SubmitItemForm),
        button(text("Cancel").size(16)).style(theme::secondary_button).padding([10, 24]).on_press(Message::CancelItemForm),
    ]
    .spacing(theme::SPACE_MD);

    let card = container(column![fields, actions].spacing(theme::SPACE_LG))
        .style(theme::card)
        .padding(theme::SPACE_LG)
        .max_width(480);

    scrollable(
        container(card)
            .width(Length::Fill)
            .padding(theme::SPACE_MD)
            .align_x(iced::Alignment::Center),
    )
    .height(Length::Fill)
    .into()
}

fn detail_view<'a>(item: &'a Item, image_bytes: Option<&'a [u8]>, default_gst_bp: i64) -> Element<'a, Message> {
    let photo: Element<'_, Message> = match image_bytes {
        Some(bytes) => image::Image::new(image::Handle::from_bytes(bytes.to_vec()))
            .width(200)
            .height(200)
            .content_fit(iced::ContentFit::Cover)
            .into(),
        None => container(text("No photo").size(13)).width(200).height(200).style(theme::card).padding(theme::SPACE_SM).align_x(iced::Alignment::Center).align_y(iced::Alignment::Center).into(),
    };

    let description: Element<'_, Message> = if item.description.is_empty() {
        text("No description").size(14).into()
    } else {
        text(&item.description).size(14).into()
    };

    let body = column![
        row![photo, column![text(&item.name).size(24), description].spacing(theme::SPACE_SM)].spacing(theme::SPACE_LG),
        detail_row("Stock", format!("{:.1} {}", item.stock_qty, item.unit)),
        detail_row("Buy price", money::format_paise(item.buy_price_paise)),
        detail_row("Sell price", money::format_paise(item.sell_price_paise)),
        detail_row("Low-stock alert below", format!("{:.1} {}", item.low_stock_threshold, item.unit)),
        detail_row(
            "GST rate",
            match item.gst_rate_bp {
                Some(bp) => format!("{} (override)", money::format_gst_rate_bp(bp)),
                None => format!("{} (shop default)", money::format_gst_rate_bp(default_gst_bp)),
            },
        ),
        row![
            button(text("Edit").size(15)).style(theme::primary_button).padding([10, 24]).on_press(Message::OpenEditItemForm(item.id)),
            button(text("Record Purchase").size(15)).style(theme::accent_button).padding([10, 24]).on_press(Message::OpenPurchaseForm(item.id)),
            button(text("Back").size(15)).style(theme::secondary_button).padding([10, 24]).on_press(Message::CloseView),
        ]
        .spacing(theme::SPACE_MD),
    ]
    .spacing(theme::SPACE_LG)
    .max_width(520);

    let card = container(body).style(theme::card).padding(theme::SPACE_LG).max_width(600);

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(theme::SPACE_MD)
        .align_x(iced::Alignment::Center)
        .into()
}

fn purchase_view(form: &PurchaseForm) -> Element<'_, Message> {
    let fields = column![
        text(format!("Record Purchase — {}", form.item_name)).size(20),
        text("Adds to this item's stock — the same ledger entry \"Sell Stock\" writes to, just in the other direction.").size(13).color(theme::MUTED_TEXT),
        labeled(
            "Quantity received *",
            text_input("e.g. 10", &form.qty).on_input(|v| Message::PurchaseFieldChanged(PurchaseField::Qty, v)).padding(10).size(16),
        ),
        labeled(
            "Buy price (₹) per unit *",
            text_input("80.00", &form.price).on_input(|v| Message::PurchaseFieldChanged(PurchaseField::Price, v)).padding(10).size(16),
        ),
        row![
            button(text("Save").size(16)).style(theme::success_button).padding([10, 24]).on_press(Message::SubmitPurchase),
            button(text("Cancel").size(16)).style(theme::secondary_button).padding([10, 24]).on_press(Message::CancelPurchaseForm),
        ]
        .spacing(theme::SPACE_MD),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(420);

    let card = container(fields).style(theme::card).padding(theme::SPACE_LG).max_width(480);

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(theme::SPACE_MD)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
}

fn detail_row<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    row![text(label).size(14).width(Length::Fixed(180.0)), text(value).size(14)].into()
}

fn labeled<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(14), widget.into()].spacing(4).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i64, name: &str, has_image: bool) -> Item {
        Item {
            id,
            name: name.to_string(),
            buy_price_paise: 0,
            sell_price_paise: 0,
            stock_qty: 10.0,
            unit: "piece".to_string(),
            low_stock_threshold: 5.0,
            deleted: false,
            description: String::new(),
            has_image,
            gst_rate_bp: None,
        }
    }

    #[test]
    fn current_page_paginates_and_reports_the_right_totals() {
        let mut state = State::default();
        state.items = (1..=25).map(|id| item(id, &format!("Item {id}"), false)).collect();

        let (page_items, total, page_count) = current_page(&state);
        assert_eq!(total, 25);
        assert_eq!(page_count, 3);
        assert_eq!(page_items.len(), PAGE_SIZE);
        assert_eq!(page_items[0].id, 1);

        state.items_page = 2;
        let (last_page, _, _) = current_page(&state);
        assert_eq!(last_page.len(), 5); // 25 items, 10 per page -> 5 on the last page
        assert_eq!(last_page[0].id, 21);
    }

    #[test]
    fn current_page_applies_search_and_low_stock_filters_before_paginating() {
        let mut state = State::default();
        state.items = vec![item(1, "PVC Pipe", false), item(2, "Copper Wire", false), item(3, "PVC Elbow", false)];
        state.search_query = "pvc".to_string();

        let (page_items, total, _) = current_page(&state);
        assert_eq!(total, 2);
        assert!(page_items.iter().all(|i| i.name.to_lowercase().contains("pvc")));
    }

    #[test]
    fn current_page_clamps_a_stale_page_number_after_the_result_set_shrinks() {
        let mut state = State::default();
        state.items = vec![item(1, "A", false), item(2, "B", false)];
        state.items_page = 5; // stale — e.g. a filter just removed most matches

        let (page_items, _, page_count) = current_page(&state);
        assert_eq!(page_count, 1);
        assert_eq!(page_items.len(), 2);
    }
}
