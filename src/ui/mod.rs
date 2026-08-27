mod activation;
mod backup;
mod bills;
mod common;
mod home;
mod items;
mod login;
mod register;
mod reports;
mod sale;
mod settings;
mod theme;

use std::path::PathBuf;

use iced::widget::{button, column, container, row, svg, text};
use iced::{Element, Length, Task};
use sqlx::SqlitePool;

use crate::models::{Item, ShopProfile, Transaction};
use crate::money;
use crate::repo::TransactionHistoryRow;
use crate::ui::common::ItemOption;
pub use items::ItemForm;

/// Embedded at compile time so the running app never depends on the
/// project's asset files still being at some relative path on disk.
const LOGO_SVG: &[u8] = include_bytes!("../../assets/logo.svg");
const ICON_PNG: &[u8] = include_bytes!("../../assets/icon.png");

fn logo_handle() -> svg::Handle {
    svg::Handle::from_memory(LOGO_SVG)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Loading,
    Activation,
    Register,
    Login,
    Home,
    Inventory,
    Shop,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopTab {
    PurchasesAndSales,
    Billings,
    Reports,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryTab {
    Items,
    Backup,
}

pub struct State {
    pool: Option<SqlitePool>,
    stage: Stage,
    shop: Option<ShopProfile>,
    settings: crate::settings::Settings,

    items: Vec<Item>,
    /// Set when something goes wrong (DB error, validation failure) so the
    /// shopkeeper sees *something* rather than a silently-failed action.
    status: Option<String>,
    search_query: String,
    item_form: Option<ItemForm>,
    purchase_form: Option<items::PurchaseForm>,
    confirming_delete_id: Option<i64>,
    items_page: usize,
    unit_filter: items::UnitFilter,
    low_stock_only: bool,
    /// The item currently open on the read-only detail screen (`None` means
    /// the list is showing instead). Separate from `item_form`, which is
    /// the add/edit screen.
    viewing_item_id: Option<i64>,
    /// The photo for whichever item `viewing_item_id` points at, fetched
    /// lazily — see the `Item` doc comment for why images aren't part of
    /// the eagerly-loaded item list.
    view_image: Option<Vec<u8>>,
    inventory_tab: InventoryTab,
    /// Thumbnails for whatever's on the *current* item-list page, fetched
    /// lazily and cached by item id — see `items::current_page`. Never
    /// populated for the whole catalog at once, same reasoning as
    /// `view_image`/`Item::has_image`.
    item_thumbnails: std::collections::HashMap<i64, Vec<u8>>,

    activation: activation::ActivationState,
    register_form: register::RegisterForm,
    login_pin_input: String,
    login_error: Option<String>,

    shop_tab: ShopTab,
    sale_form: sale::SaleForm,
    /// Backing state for the searchable item picker on the billing screen.
    /// Rebuilt from `items` whenever that reloads — see `ItemsLoaded`.
    sale_item_combo: iced::widget::combo_box::State<ItemOption>,
    recent_sales: Vec<TransactionHistoryRow>,
    sale_page: usize,
    /// The sale currently open on the read-only detail screen (`None`
    /// means the Sales history list is showing instead).
    sale_viewing: Option<TransactionHistoryRow>,
    sale_confirming_delete_id: Option<i64>,
    reports: reports::ReportsState,

    bills: bills::BillsState,
    /// Backing state for the searchable item picker on the Billings "add
    /// line" form. Rebuilt from `items` whenever that reloads.
    bill_item_combo: iced::widget::combo_box::State<ItemOption>,

    settings_tab: settings::SettingsTab,
    profile_form: settings::ProfileForm,
    security_form: settings::SecurityForm,
    /// The shop's custom logo, shown in the header in place of the Srotas
    /// mark. Fetched once after login/registration — see `ShopLogoLoaded`.
    shop_logo: Option<Vec<u8>>,

    /// Text-field edit history for Ctrl/Cmd+Z / Ctrl/Cmd+Shift+Z — iced's
    /// `text_input` has no undo/redo of its own, so every `*FieldChanged`
    /// message routes through `push_edit` instead of writing straight into
    /// its form, recording (which field, its value before this edit) here.
    /// A single chronological stack across every field (not one per field)
    /// matches how undo works in ordinary apps: it undoes the last edit
    /// you made, wherever it was, not "the last edit to whatever's focused
    /// right now" — which also sidesteps needing to track focus at all.
    undo_stack: Vec<(EditableField, String)>,
    redo_stack: Vec<(EditableField, String)>,
}

impl Default for State {
    fn default() -> Self {
        State {
            pool: None,
            stage: Stage::Loading,
            shop: None,
            settings: crate::settings::Settings::default(),
            items: Vec::new(),
            status: None,
            search_query: String::new(),
            item_form: None,
            purchase_form: None,
            confirming_delete_id: None,
            items_page: 0,
            unit_filter: items::UnitFilter::All,
            low_stock_only: false,
            viewing_item_id: None,
            view_image: None,
            inventory_tab: InventoryTab::Items,
            item_thumbnails: std::collections::HashMap::new(),
            activation: activation::ActivationState::default(),
            register_form: register::RegisterForm::default(),
            login_pin_input: String::new(),
            login_error: None,
            shop_tab: ShopTab::PurchasesAndSales,
            sale_form: sale::SaleForm::default(),
            sale_item_combo: iced::widget::combo_box::State::new(Vec::new()),
            recent_sales: Vec::new(),
            sale_page: 0,
            sale_viewing: None,
            sale_confirming_delete_id: None,
            reports: reports::ReportsState::default(),
            bills: bills::BillsState::default(),
            bill_item_combo: iced::widget::combo_box::State::new(Vec::new()),
            settings_tab: settings::SettingsTab::Profile,
            profile_form: settings::ProfileForm::default(),
            security_form: settings::SecurityForm::default(),
            shop_logo: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }
}

/// Identifies one editable text field across the whole app, so a single
/// global undo/redo stack can record and replay edits to any of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditableField {
    ItemForm(items::FormField),
    Purchase(items::PurchaseField),
    Register(register::Field),
    Sale(sale::Field),
    Profile(settings::ProfileField),
    Security(settings::SecurityField),
    Reports(reports::Field),
    Search,
    LoginPin,
    BillQty,
    BillPrice,
    BillDiscount,
    ActivationKey,
}

/// Writes `value` into `field` and returns whatever value was there before
/// — the other half of every undo/redo step, alongside `push_edit`.
/// A field whose owning form/screen isn't currently open (e.g. undo lands
/// on `ItemForm` after the form's been closed) is a harmless no-op.
fn apply_edit(state: &mut State, field: EditableField, value: String) -> String {
    match field {
        EditableField::ItemForm(f) => match &mut state.item_form {
            Some(form) => {
                let old = form.get_field(f);
                form.set_field(f, value);
                old
            }
            None => value,
        },
        EditableField::Purchase(f) => match &mut state.purchase_form {
            Some(form) => {
                let old = form.get_field(f);
                form.set_field(f, value);
                old
            }
            None => value,
        },
        EditableField::Register(f) => {
            let old = state.register_form.get_field(f);
            state.register_form.set_field(f, value);
            old
        }
        EditableField::Sale(f) => {
            let old = state.sale_form.get_field(f);
            state.sale_form.set_field(f, value);
            old
        }
        EditableField::Profile(f) => {
            let old = state.profile_form.get_field(f);
            state.profile_form.set_field(f, value);
            old
        }
        EditableField::Security(f) => {
            let old = state.security_form.get_field(f);
            state.security_form.set_field(f, value);
            old
        }
        EditableField::Reports(f) => {
            let old = state.reports.get_field(f);
            state.reports.set_field(f, value);
            old
        }
        EditableField::Search => {
            let old = std::mem::replace(&mut state.search_query, value);
            state.items_page = 0;
            old
        }
        EditableField::LoginPin => std::mem::replace(&mut state.login_pin_input, value),
        EditableField::BillQty => std::mem::replace(&mut state.bills.qty_input, value),
        EditableField::BillPrice => std::mem::replace(&mut state.bills.price_input, value),
        EditableField::BillDiscount => std::mem::replace(&mut state.bills.discount_input, value),
        EditableField::ActivationKey => std::mem::replace(&mut state.activation.key_input, value),
    }
}

/// Records an edit onto the undo stack and applies it — call this instead
/// of writing directly into a form whenever a `*FieldChanged` message
/// carries user-typed text, so Ctrl/Cmd+Z has something to undo.
fn push_edit(state: &mut State, field: EditableField, value: String) {
    let old = apply_edit(state, field, value);
    state.undo_stack.push((field, old));
    state.redo_stack.clear();
    if state.undo_stack.len() > 200 {
        state.undo_stack.remove(0);
    }
}

fn undo(state: &mut State) {
    if let Some((field, value)) = state.undo_stack.pop() {
        let current = apply_edit(state, field, value);
        state.redo_stack.push((field, current));
    }
}

fn redo(state: &mut State) {
    if let Some((field, value)) = state.redo_stack.pop() {
        let current = apply_edit(state, field, value);
        state.undo_stack.push((field, current));
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    /// No-op target for keyboard events the global subscription below
    /// looked at but didn't recognize as a shortcut.
    NoOp,
    UndoRequested,
    RedoRequested,
    DbReady(Result<SqlitePool, String>),
    LicenseChecked(Result<activation::Outcome, String>),
    ActivationKeyChanged(String),
    CopyDeviceId,
    SubmitActivation,
    ActivationCompleted(Result<(), String>),
    ShopProfileLoaded(Result<Option<ShopProfile>, String>),
    ItemsLoaded(Result<Vec<Item>, String>),
    ThumbnailLoaded(i64, Option<Vec<u8>>),

    GoToShop,
    GoToInventory,
    GoHome,
    Lock,
    ShopTabSelected(ShopTab),
    InventoryTabSelected(InventoryTab),
    SearchChanged(String),

    OpenAddItemForm,
    OpenEditItemForm(i64),
    CancelItemForm,
    FormFieldChanged(items::FormField, String),
    FormUnitSelected(crate::models::Unit),
    ChooseItemImage,
    ItemImagePicked(Result<Option<Vec<u8>>, String>),
    RemoveItemImage,
    ItemImageLoadedForEdit(i64, Option<Vec<u8>>),
    SubmitItemForm,
    ItemSaved(Result<Item, String>),
    DeleteItemPressed(i64),
    ItemDeleted(Result<i64, String>),
    UnitFilterSelected(items::UnitFilter),
    LowStockOnlyToggled(bool),
    ItemsPageNext,
    ItemsPagePrev,
    OpenViewItem(i64),
    CloseView,
    ViewImageLoaded(i64, Option<Vec<u8>>),

    OpenPurchaseForm(i64),
    PurchaseFieldChanged(items::PurchaseField, String),
    SubmitPurchase,
    CancelPurchaseForm,
    PurchaseRecorded(Result<Transaction, String>),

    RegisterFieldChanged(register::Field, String),
    SubmitRegister,
    ShopRegistered(Result<ShopProfile, String>),

    LoginPinChanged(String),
    SubmitLogin,

    SaleItemSelected(ItemOption),
    SaleFieldChanged(sale::Field, String),
    SubmitSale,
    SaleRecorded(Result<Transaction, String>),
    SaleHistoryLoaded(Result<Vec<TransactionHistoryRow>, String>),
    SalesPageNext,
    SalesPagePrev,
    OpenSaleView(i64),
    SaleViewLoaded(Result<TransactionHistoryRow, String>),
    CloseSaleView,
    OpenSaleEdit(i64),
    SaleEditLoaded(Result<TransactionHistoryRow, String>),
    CancelSaleEdit,
    DeleteSalePressed(i64),
    SaleDeleted(Result<i64, String>),
    PrintSalePressed(i64),
    DownloadSalePressed(i64),
    SalePdfReady(Result<(PathBuf, bool), String>),

    BillItemSelected(ItemOption),
    BillQtyChanged(String),
    BillPriceChanged(String),
    BillDiscountChanged(String),
    AddBillLine,
    RemoveBillLine(usize),
    SubmitBill,
    CancelBillEdit,
    BillSaved(Result<i64, String>),
    BillsLoaded(Result<(Vec<crate::models::BillSummary>, i64), String>),
    BillsPageNext,
    BillsPagePrev,
    OpenBillView(i64),
    BillViewLoaded(Result<crate::models::BillDetail, String>),
    CloseBillView,
    OpenBillEdit(i64),
    BillEditLoaded(Result<crate::models::BillDetail, String>),
    DeleteBillPressed(i64),
    BillDeleted(Result<i64, String>),
    PrintBillPressed(i64),
    BillPdfReady(Result<PathBuf, String>),

    ReportsItemFilterSelected(Option<ItemOption>),
    ReportsFieldChanged(reports::Field, String),
    RunReports,
    ReportsLoaded(Result<reports::Loaded, String>),
    DownloadReportPressed,
    ReportPdfReady(Result<(reports::Loaded, PathBuf), String>),

    ChooseBackupFolder,
    BackupFolderChosen(Option<PathBuf>),
    BackupNowPressed,
    BackupCompleted(Result<PathBuf, String>),

    OpenSettings,
    SettingsTabSelected(settings::SettingsTab),
    ProfileFieldChanged(settings::ProfileField, String),
    ChooseProfileLogo,
    ProfileLogoPicked(Result<Option<Vec<u8>>, String>),
    RemoveProfileLogo,
    SubmitProfile,
    ProfileSaved(Result<ShopProfile, String>),
    ShopLogoLoaded(Option<Vec<u8>>),
    SecurityFieldChanged(settings::SecurityField, String),
    SubmitPinChange,
    PinChanged(Result<Option<String>, String>),
}

pub fn run() -> iced::Result {
    let icon = iced::window::icon::from_file_data(ICON_PNG, None).ok();

    iced::application(
        || {
            (
                State::default(),
                Task::perform(crate::db::connect_and_migrate(), |result| {
                    Message::DbReady(result.map_err(|e| e.to_string()))
                }),
            )
        },
        update,
        view,
    )
    .title("Srotas Desk")
    .theme(|_state: &State| theme::theme())
    .window(iced::window::Settings {
        icon,
        ..iced::window::Settings::default()
    })
    .subscription(|_state| keyboard_shortcuts())
    .run()
}

/// Ctrl/Cmd+Z (undo) and Ctrl/Cmd+Shift+Z or Ctrl/Cmd+Y (redo) for text
/// fields — `iced::widget::text_input` has no undo/redo of its own (it
/// only wires up copy/cut/paste/select-all), so this fills that gap at
/// the application level. `iced::keyboard::listen` only ever delivers
/// key presses a focused widget didn't already handle — text_input
/// doesn't recognize these combinations, so they reach here untouched.
/// Both Ctrl and the platform's native command key (⌘ on macOS) are
/// accepted, rather than only whichever one is "correct" for the OS.
fn keyboard_shortcuts() -> iced::Subscription<Message> {
    iced::keyboard::listen().map(|event| {
        let iced::keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
            return Message::NoOp;
        };
        if !(modifiers.control() || modifiers.logo()) {
            return Message::NoOp;
        }
        let iced::keyboard::Key::Character(c) = &key else {
            return Message::NoOp;
        };
        if c.eq_ignore_ascii_case("z") {
            if modifiers.shift() {
                Message::RedoRequested
            } else {
                Message::UndoRequested
            }
        } else if c.eq_ignore_ascii_case("y") {
            Message::RedoRequested
        } else {
            Message::NoOp
        }
    })
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::NoOp => Task::none(),
        Message::UndoRequested => {
            undo(state);
            Task::none()
        }
        Message::RedoRequested => {
            redo(state);
            Task::none()
        }
        Message::DbReady(Ok(pool)) => {
            state.settings = crate::settings::load();
            let for_license = pool.clone();
            state.pool = Some(pool);
            Task::perform(activation::check(for_license), Message::LicenseChecked)
        }
        Message::DbReady(Err(e)) => {
            state.status = Some(format!("could not open database: {e}"));
            Task::none()
        }
        Message::LicenseChecked(Ok(activation::Outcome::Valid { expiry_warning })) => {
            state.status = expiry_warning;
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            let for_items = pool.clone();
            Task::batch([load_items(for_items), load_shop_profile(pool)])
        }
        Message::LicenseChecked(Ok(activation::Outcome::NeedsActivation { device_id, message })) => {
            state.activation.device_id = device_id;
            state.activation.error = message;
            state.stage = Stage::Activation;
            Task::none()
        }
        Message::LicenseChecked(Err(e)) => {
            state.status = Some(format!("could not check license: {e}"));
            Task::none()
        }
        Message::ActivationKeyChanged(value) => {
            push_edit(state, EditableField::ActivationKey, value);
            Task::none()
        }
        Message::CopyDeviceId => iced::clipboard::write(state.activation.device_id.clone()),
        Message::SubmitActivation => activation::submit(state),
        Message::ActivationCompleted(Ok(())) => {
            state.activation.error = None;
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            let for_items = pool.clone();
            Task::batch([load_items(for_items), load_shop_profile(pool)])
        }
        Message::ActivationCompleted(Err(e)) => {
            state.activation.error = Some(e);
            Task::none()
        }
        Message::ShopProfileLoaded(Ok(Some(profile))) => {
            state.shop = Some(profile);
            state.stage = Stage::Login;
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            load_shop_logo(pool)
        }
        Message::ShopProfileLoaded(Ok(None)) => {
            state.stage = Stage::Register;
            Task::none()
        }
        Message::ShopProfileLoaded(Err(e)) => {
            state.status = Some(format!("could not load shop profile: {e}"));
            Task::none()
        }
        Message::ItemsLoaded(Ok(items)) => {
            let options = crate::ui::common::item_options(&items);
            state.sale_item_combo = iced::widget::combo_box::State::new(options.clone());
            state.bill_item_combo = iced::widget::combo_box::State::new(options);
            state.items = items;
            refresh_item_thumbnails(state)
        }
        Message::ItemsLoaded(Err(e)) => {
            state.status = Some(format!("could not load items: {e}"));
            Task::none()
        }
        Message::ThumbnailLoaded(id, Some(bytes)) => {
            state.item_thumbnails.insert(id, bytes);
            Task::none()
        }
        Message::ThumbnailLoaded(_, None) => Task::none(),

        Message::GoToShop => {
            state.stage = Stage::Shop;
            enter_shop_tab(state, state.shop_tab)
        }
        Message::GoToInventory => {
            state.stage = Stage::Inventory;
            Task::none()
        }
        Message::GoHome => {
            state.stage = Stage::Home;
            Task::none()
        }
        Message::Lock => {
            state.stage = Stage::Login;
            state.login_pin_input.clear();
            state.login_error = None;
            Task::none()
        }
        Message::ShopTabSelected(tab) => {
            state.shop_tab = tab;
            enter_shop_tab(state, tab)
        }
        Message::InventoryTabSelected(tab) => {
            state.inventory_tab = tab;
            Task::none()
        }
        Message::SearchChanged(query) => {
            push_edit(state, EditableField::Search, query);
            refresh_item_thumbnails(state)
        }

        Message::OpenAddItemForm => {
            state.item_form = Some(ItemForm::empty());
            state.viewing_item_id = None;
            Task::none()
        }
        Message::OpenEditItemForm(id) => {
            let Some(item) = state.items.iter().find(|i| i.id == id) else {
                return Task::none();
            };
            state.item_form = Some(ItemForm::from_item(item));
            state.viewing_item_id = None;
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { crate::repo::get_item_image(&pool, id).await.unwrap_or(None) },
                move |image| Message::ItemImageLoadedForEdit(id, image),
            )
        }
        Message::ItemImageLoadedForEdit(id, image) => {
            if let Some(form) = &mut state.item_form {
                if form.editing_id == Some(id) {
                    form.image = image;
                }
            }
            Task::none()
        }
        Message::CancelItemForm => {
            state.item_form = None;
            Task::none()
        }
        Message::FormFieldChanged(field, value) => {
            push_edit(state, EditableField::ItemForm(field), value);
            Task::none()
        }
        Message::FormUnitSelected(unit) => {
            if let Some(form) = &mut state.item_form {
                form.unit = unit;
            }
            Task::none()
        }
        Message::ChooseItemImage => items::choose_image(),
        Message::ItemImagePicked(Ok(Some(bytes))) => {
            const MAX_IMAGE_BYTES: usize = 5_000_000;
            if bytes.len() > MAX_IMAGE_BYTES {
                state.status = Some("image is too large (max 5 MB)".into());
            } else if image::load_from_memory(&bytes).is_err() {
                state.status = Some("that file doesn't look like a valid image".into());
            } else if let Some(form) = &mut state.item_form {
                form.image = Some(bytes);
                state.status = None;
            }
            Task::none()
        }
        Message::ItemImagePicked(Ok(None)) => Task::none(),
        Message::ItemImagePicked(Err(e)) => {
            state.status = Some(format!("could not read image: {e}"));
            Task::none()
        }
        Message::RemoveItemImage => {
            if let Some(form) = &mut state.item_form {
                form.image = None;
            }
            Task::none()
        }
        Message::SubmitItemForm => items::submit(state),
        Message::ItemSaved(Ok(item)) => {
            state.item_thumbnails.remove(&item.id);
            state.item_form = None;
            state.status = None;
            reload_items(state)
        }
        Message::ItemSaved(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }

        Message::DeleteItemPressed(id) => {
            if state.confirming_delete_id == Some(id) {
                state.confirming_delete_id = None;
                let Some(pool) = state.pool.clone() else {
                    return Task::none();
                };
                Task::perform(
                    async move { crate::repo::delete_item(&pool, id).await.map(|_| id) },
                    |result| Message::ItemDeleted(result.map_err(|e| e.to_string())),
                )
            } else {
                state.confirming_delete_id = Some(id);
                Task::none()
            }
        }
        Message::ItemDeleted(Ok(id)) => {
            state.item_thumbnails.remove(&id);
            state.status = None;
            reload_items(state)
        }
        Message::ItemDeleted(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }

        Message::UnitFilterSelected(filter) => {
            state.unit_filter = filter;
            state.items_page = 0;
            refresh_item_thumbnails(state)
        }
        Message::LowStockOnlyToggled(enabled) => {
            state.low_stock_only = enabled;
            state.items_page = 0;
            refresh_item_thumbnails(state)
        }
        Message::ItemsPageNext => {
            state.items_page += 1;
            refresh_item_thumbnails(state)
        }
        Message::ItemsPagePrev => {
            state.items_page = state.items_page.saturating_sub(1);
            refresh_item_thumbnails(state)
        }
        Message::OpenViewItem(id) => {
            state.viewing_item_id = Some(id);
            state.view_image = None;
            state.item_form = None;
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { crate::repo::get_item_image(&pool, id).await.unwrap_or(None) },
                move |image| Message::ViewImageLoaded(id, image),
            )
        }
        Message::ViewImageLoaded(id, image) => {
            if state.viewing_item_id == Some(id) {
                state.view_image = image;
            }
            Task::none()
        }
        Message::CloseView => {
            state.viewing_item_id = None;
            state.view_image = None;
            Task::none()
        }

        Message::OpenPurchaseForm(id) => {
            if let Some(item) = state.items.iter().find(|i| i.id == id) {
                state.purchase_form = Some(items::PurchaseForm {
                    item_id: item.id,
                    item_name: item.name.clone(),
                    qty: String::new(),
                    price: money::paise_to_input(item.buy_price_paise),
                });
                state.status = None;
            }
            Task::none()
        }
        Message::PurchaseFieldChanged(field, value) => {
            push_edit(state, EditableField::Purchase(field), value);
            Task::none()
        }
        Message::SubmitPurchase => items::submit_purchase(state),
        Message::CancelPurchaseForm => {
            state.purchase_form = None;
            state.status = None;
            Task::none()
        }
        Message::PurchaseRecorded(Ok(_)) => {
            state.purchase_form = None;
            state.status = None;
            reload_items(state)
        }
        Message::PurchaseRecorded(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }

        Message::RegisterFieldChanged(field, value) => {
            push_edit(state, EditableField::Register(field), value);
            Task::none()
        }
        Message::SubmitRegister => register::submit(state),
        Message::ShopRegistered(Ok(profile)) => {
            state.shop = Some(profile);
            state.status = None;
            state.stage = Stage::Home;
            backup::maybe_auto_backup(state)
        }
        Message::ShopLogoLoaded(logo) => {
            state.shop_logo = logo;
            Task::none()
        }
        Message::ShopRegistered(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }

        Message::LoginPinChanged(value) => {
            push_edit(state, EditableField::LoginPin, value);
            Task::none()
        }
        Message::SubmitLogin => {
            let expected = state.shop.as_ref().and_then(|s| s.pin.as_deref());
            match expected {
                None => {
                    state.stage = Stage::Home;
                    state.login_error = None;
                    backup::maybe_auto_backup(state)
                }
                Some(pin) if pin == state.login_pin_input.trim() => {
                    state.stage = Stage::Home;
                    state.login_error = None;
                    state.login_pin_input.clear();
                    backup::maybe_auto_backup(state)
                }
                Some(_) => {
                    state.login_error = Some("incorrect PIN".into());
                    Task::none()
                }
            }
        }

        Message::SaleItemSelected(option) => {
            sale::select_item(state, option);
            Task::none()
        }
        Message::SaleFieldChanged(field, value) => {
            push_edit(state, EditableField::Sale(field), value);
            Task::none()
        }
        Message::SubmitSale => sale::submit(state),
        Message::SaleRecorded(Ok(_)) => {
            state.status = None;
            state.sale_form = sale::SaleForm::default();
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            Task::batch([load_items(pool.clone()), sale::load_recent(pool)])
        }
        Message::SaleRecorded(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }
        Message::SaleHistoryLoaded(Ok(rows)) => {
            state.recent_sales = rows;
            state.sale_page = 0;
            Task::none()
        }
        Message::SaleHistoryLoaded(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }
        Message::SalesPageNext => {
            state.sale_page += 1;
            Task::none()
        }
        Message::SalesPagePrev => {
            state.sale_page = state.sale_page.saturating_sub(1);
            Task::none()
        }
        Message::OpenSaleView(id) => {
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { crate::repo::get_sale(&pool, id).await },
                |result| Message::SaleViewLoaded(result.map_err(|e| e.to_string())),
            )
        }
        Message::SaleViewLoaded(Ok(sale)) => {
            let item_id = sale.item_id;
            state.sale_viewing = Some(sale);
            fetch_thumbnails(state, vec![item_id])
        }
        Message::SaleViewLoaded(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }
        Message::CloseSaleView => {
            state.sale_viewing = None;
            Task::none()
        }
        Message::OpenSaleEdit(id) => {
            state.sale_viewing = None;
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { crate::repo::get_sale(&pool, id).await },
                |result| Message::SaleEditLoaded(result.map_err(|e| e.to_string())),
            )
        }
        Message::SaleEditLoaded(Ok(sale)) => {
            sale::load_for_edit(state, sale);
            Task::none()
        }
        Message::SaleEditLoaded(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }
        Message::CancelSaleEdit => {
            sale::cancel_edit(state);
            Task::none()
        }
        Message::DeleteSalePressed(id) => {
            if state.sale_confirming_delete_id == Some(id) {
                state.sale_confirming_delete_id = None;
                let Some(pool) = state.pool.clone() else {
                    return Task::none();
                };
                Task::perform(
                    async move { crate::repo::delete_sale(&pool, id).await.map(|_| id) },
                    |result| Message::SaleDeleted(result.map_err(|e| e.to_string())),
                )
            } else {
                state.sale_confirming_delete_id = Some(id);
                Task::none()
            }
        }
        Message::SaleDeleted(Ok(id)) => {
            state.status = None;
            if state.sale_form.editing_id == Some(id) {
                sale::cancel_edit(state);
            }
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            Task::batch([load_items(pool.clone()), sale::load_recent(pool)])
        }
        Message::SaleDeleted(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }
        Message::PrintSalePressed(id) => sale::export_pdf(state, id, true),
        Message::DownloadSalePressed(id) => sale::export_pdf(state, id, false),
        Message::SalePdfReady(Ok((path, opened))) => {
            state.status = Some(if opened {
                format!("Sale receipt saved and opened: {}", path.display())
            } else {
                format!("Sale receipt saved: {}", path.display())
            });
            Task::none()
        }
        Message::SalePdfReady(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }

        Message::BillItemSelected(option) => {
            bills::select_item(state, option);
            Task::none()
        }
        Message::BillQtyChanged(value) => {
            push_edit(state, EditableField::BillQty, value);
            Task::none()
        }
        Message::BillPriceChanged(value) => {
            push_edit(state, EditableField::BillPrice, value);
            Task::none()
        }
        Message::BillDiscountChanged(value) => {
            push_edit(state, EditableField::BillDiscount, value);
            Task::none()
        }
        Message::AddBillLine => {
            bills::add_line(state);
            Task::none()
        }
        Message::RemoveBillLine(index) => {
            bills::remove_line(state, index);
            Task::none()
        }
        Message::SubmitBill => bills::submit(state),
        Message::BillSaved(Ok(_)) => {
            state.status = None;
            bills::start_new(state);
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            Task::batch([load_items(pool.clone()), bills::load_history(pool, state.bills.page)])
        }
        Message::BillSaved(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }
        Message::BillsLoaded(Ok((rows, total))) => {
            state.bills.rows = rows;
            state.bills.total = total;
            Task::none()
        }
        Message::BillsLoaded(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }
        Message::BillsPageNext => {
            state.bills.page += 1;
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            bills::load_history(pool, state.bills.page)
        }
        Message::BillsPagePrev => {
            state.bills.page = (state.bills.page - 1).max(0);
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            bills::load_history(pool, state.bills.page)
        }
        Message::OpenBillView(id) => {
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { crate::repo::get_bill(&pool, id).await },
                |result| Message::BillViewLoaded(result.map_err(|e| e.to_string())),
            )
        }
        Message::BillViewLoaded(Ok(detail)) => {
            let item_ids: Vec<i64> = detail.lines.iter().map(|l| l.item_id).collect();
            state.bills.viewing = Some(detail);
            fetch_thumbnails(state, item_ids)
        }
        Message::BillViewLoaded(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }
        Message::CloseBillView => {
            state.bills.viewing = None;
            Task::none()
        }
        Message::OpenBillEdit(id) => {
            state.bills.viewing = None;
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { crate::repo::get_bill(&pool, id).await },
                |result| Message::BillEditLoaded(result.map_err(|e| e.to_string())),
            )
        }
        Message::BillEditLoaded(Ok(detail)) => {
            bills::load_for_edit(state, detail);
            Task::none()
        }
        Message::BillEditLoaded(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }
        Message::CancelBillEdit => {
            bills::start_new(state);
            Task::none()
        }
        Message::DeleteBillPressed(id) => {
            if state.bills.confirming_delete_id == Some(id) {
                state.bills.confirming_delete_id = None;
                let Some(pool) = state.pool.clone() else {
                    return Task::none();
                };
                Task::perform(
                    async move { crate::repo::delete_bill(&pool, id).await.map(|_| id) },
                    |result| Message::BillDeleted(result.map_err(|e| e.to_string())),
                )
            } else {
                state.bills.confirming_delete_id = Some(id);
                Task::none()
            }
        }
        Message::BillDeleted(Ok(_)) => {
            state.status = None;
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            bills::load_history(pool, state.bills.page)
        }
        Message::BillDeleted(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }
        Message::PrintBillPressed(id) => bills::print_bill(state, id),
        Message::BillPdfReady(Ok(path)) => {
            state.status = Some(format!("Bill saved and opened: {}", path.display()));
            Task::none()
        }
        Message::BillPdfReady(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }

        Message::ReportsItemFilterSelected(option) => {
            state.reports.item_filter = option;
            reports::run(state)
        }
        Message::ReportsFieldChanged(field, value) => {
            push_edit(state, EditableField::Reports(field), value);
            Task::none()
        }
        Message::RunReports => reports::run(state),
        Message::ReportsLoaded(Ok(loaded)) => {
            state.reports.stock_value_paise = loaded.stock_value_paise;
            state.reports.total_profit_paise = loaded.total_profit_paise;
            state.reports.rows = loaded.rows;
            Task::none()
        }
        Message::ReportsLoaded(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }
        Message::DownloadReportPressed => reports::download(state),
        Message::ReportPdfReady(Ok((loaded, path))) => {
            state.reports.stock_value_paise = loaded.stock_value_paise;
            state.reports.total_profit_paise = loaded.total_profit_paise;
            state.reports.rows = loaded.rows;
            state.status = Some(format!("Report saved and opened: {}", path.display()));
            Task::none()
        }
        Message::ReportPdfReady(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }

        Message::ChooseBackupFolder => backup::choose_folder(),
        Message::BackupFolderChosen(Some(path)) => {
            state.settings.backup_folder = Some(path);
            let _ = crate::settings::save(&state.settings);
            Task::none()
        }
        Message::BackupFolderChosen(None) => Task::none(),
        Message::BackupNowPressed => backup::backup_now(state),
        Message::BackupCompleted(Ok(path)) => {
            state.settings.last_backup_date = Some(chrono::Utc::now().date_naive());
            let _ = crate::settings::save(&state.settings);
            state.status = Some(format!("Backup saved: {}", path.display()));
            Task::none()
        }
        Message::BackupCompleted(Err(e)) => {
            state.status = Some(format!("backup failed: {e}"));
            Task::none()
        }

        Message::OpenSettings => {
            if let Some(shop) = &state.shop {
                state.profile_form = settings::ProfileForm::from_shop(shop);
            }
            state.security_form = settings::SecurityForm::default();
            state.settings_tab = settings::SettingsTab::Profile;
            state.stage = Stage::Settings;
            state.status = None;
            Task::none()
        }
        Message::SettingsTabSelected(tab) => {
            state.settings_tab = tab;
            Task::none()
        }
        Message::ProfileFieldChanged(field, value) => {
            push_edit(state, EditableField::Profile(field), value);
            Task::none()
        }
        Message::ChooseProfileLogo => settings::choose_logo(),
        Message::ProfileLogoPicked(Ok(Some(bytes))) => {
            const MAX_IMAGE_BYTES: usize = 5_000_000;
            if bytes.len() > MAX_IMAGE_BYTES {
                state.status = Some("image is too large (max 5 MB)".into());
            } else if image::load_from_memory(&bytes).is_err() {
                state.status = Some("that file doesn't look like a valid image".into());
            } else {
                state.profile_form.logo = Some(bytes);
                state.profile_form.logo_removed = false;
                state.status = None;
            }
            Task::none()
        }
        Message::ProfileLogoPicked(Ok(None)) => Task::none(),
        Message::ProfileLogoPicked(Err(e)) => {
            state.status = Some(format!("could not read image: {e}"));
            Task::none()
        }
        Message::RemoveProfileLogo => {
            state.profile_form.logo = None;
            state.profile_form.logo_removed = true;
            Task::none()
        }
        Message::SubmitProfile => settings::submit_profile(state),
        Message::ProfileSaved(Ok(profile)) => {
            state.shop = Some(profile);
            state.status = None;
            let logo_changed = state.profile_form.logo.is_some() || state.profile_form.logo_removed;
            if logo_changed {
                state.shop_logo = state.profile_form.logo.clone();
            }
            Task::none()
        }
        Message::ProfileSaved(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }
        Message::SecurityFieldChanged(field, value) => {
            push_edit(state, EditableField::Security(field), value);
            Task::none()
        }
        Message::SubmitPinChange => settings::submit_pin(state),
        Message::PinChanged(Ok(new_pin)) => {
            if let Some(shop) = &mut state.shop {
                shop.pin = new_pin;
            }
            state.security_form = settings::SecurityForm::default();
            state.status = Some("security settings saved".into());
            Task::none()
        }
        Message::PinChanged(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }
    }
}

fn enter_shop_tab(state: &mut State, tab: ShopTab) -> Task<Message> {
    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };
    match tab {
        ShopTab::PurchasesAndSales => sale::load_recent(pool),
        ShopTab::Billings => bills::load_history(pool, state.bills.page),
        ShopTab::Reports => reports::run(state),
    }
}

fn load_items(pool: SqlitePool) -> Task<Message> {
    Task::perform(
        async move { crate::repo::list_items(&pool).await },
        |result| Message::ItemsLoaded(result.map_err(|e| e.to_string())),
    )
}

fn load_shop_profile(pool: SqlitePool) -> Task<Message> {
    Task::perform(
        async move { crate::repo::get_shop_profile(&pool).await },
        |result| Message::ShopProfileLoaded(result.map_err(|e| e.to_string())),
    )
}

fn load_shop_logo(pool: SqlitePool) -> Task<Message> {
    Task::perform(
        async move { crate::repo::get_shop_logo(&pool).await.unwrap_or(None) },
        Message::ShopLogoLoaded,
    )
}

fn reload_items(state: &State) -> Task<Message> {
    match state.pool.clone() {
        Some(pool) => load_items(pool),
        None => Task::none(),
    }
}

/// Fetches thumbnails for whichever items are on the current item-list
/// page and don't already have one cached — call after anything that
/// could change which items are visible (search, filter, pagination, a
/// fresh reload). A no-op once every visible photo is cached.
fn refresh_item_thumbnails(state: &State) -> Task<Message> {
    let (page_items, _, _) = items::current_page(state);
    let ids: Vec<i64> = page_items.iter().filter(|item| item.has_image).map(|item| item.id).collect();
    fetch_thumbnails(state, ids)
}

/// Fetches thumbnails for `ids` not already cached — the shared fetch
/// behind `refresh_item_thumbnails` (the Inventory list) and the Bill/Sale
/// "View" screens, which show the same cache keyed by item id so a photo
/// fetched from one screen doesn't need re-fetching on another. An id for
/// an item with no photo (or that no longer exists) just resolves to
/// `None` and is never cached — harmless, not an error.
fn fetch_thumbnails(state: &State, ids: Vec<i64>) -> Task<Message> {
    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };
    let ids: Vec<i64> = ids.into_iter().filter(|id| !state.item_thumbnails.contains_key(id)).collect();

    Task::batch(ids.into_iter().map(|id| {
        let pool = pool.clone();
        Task::perform(
            async move { crate::repo::get_item_image(&pool, id).await.unwrap_or(None) },
            move |bytes| Message::ThumbnailLoaded(id, bytes),
        )
    }))
}

fn view(state: &State) -> Element<'_, Message> {
    let content: Element<'_, Message> = match state.stage {
        Stage::Loading => container(text("Loading...").size(18))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::Alignment::Center)
            .align_y(iced::Alignment::Center)
            .into(),
        Stage::Activation => activation::view(state),
        Stage::Register => register::view(state),
        Stage::Login => login::view(state),
        Stage::Home => column![app_bar(state, false), home::view(state)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        Stage::Inventory => column![app_bar(state, true), inventory_tabs(state), inventory_content(state)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        Stage::Shop => column![app_bar(state, true), shop_tabs(state), shop_content(state)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        Stage::Settings => column![app_bar(state, true), settings::view(state)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    };

    let status_bar: Element<'_, Message> = match &state.status {
        Some(message) => container(text(message).color(iced::Color::from_rgb(0.83, 0.16, 0.16)))
            .padding(8)
            .into(),
        None => container(text("")).into(),
    };

    column![content, status_bar].width(Length::Fill).height(Length::Fill).into()
}

fn app_bar(state: &State, show_home: bool) -> Element<'_, Message> {
    let shop_name = state.shop.as_ref().map(|s| s.shop_name.as_str()).unwrap_or("Srotas Desk");
    let title = match state.stage {
        Stage::Inventory => "Inventory",
        Stage::Shop => "Shop",
        Stage::Settings => "Settings",
        _ => "",
    };

    let logo: Element<'_, Message> = match &state.shop_logo {
        Some(bytes) => iced::widget::image::Image::new(iced::widget::image::Handle::from_bytes(bytes.clone()))
            .width(32)
            .height(32)
            .content_fit(iced::ContentFit::Cover)
            .into(),
        None => svg(logo_handle()).width(32).height(32).into(),
    };

    let mut left = row![logo, text(shop_name).size(18)]
        .spacing(theme::SPACE_SM)
        .align_y(iced::Alignment::Center);

    if !title.is_empty() {
        left = left.push(text(format!("· {title}")).size(16));
    }

    let mut right = row![].spacing(theme::SPACE_SM);
    if show_home {
        right = right.push(button(text("Home").size(14)).style(theme::secondary_button).padding([8, 14]).on_press(Message::GoHome));
    }
    right = right.push(button(text("Settings").size(14)).style(theme::secondary_button).padding([8, 14]).on_press(Message::OpenSettings));
    right = right.push(button(text("Lock").size(14)).style(theme::secondary_button).padding([8, 14]).on_press(Message::Lock));

    container(
        row![left, iced::widget::space::horizontal(), right]
            .align_y(iced::Alignment::Center)
            .padding(12),
    )
    .style(theme::header_bar)
    .width(Length::Fill)
    .into()
}

fn shop_tabs(state: &State) -> Element<'_, Message> {
    row![
        tab_button("Sales", ShopTab::PurchasesAndSales, state.shop_tab, Message::ShopTabSelected),
        tab_button("Billings", ShopTab::Billings, state.shop_tab, Message::ShopTabSelected),
        iced::widget::space::horizontal(),
        tab_button("Reports", ShopTab::Reports, state.shop_tab, Message::ShopTabSelected),
    ]
    .spacing(8)
    .padding(12)
    .into()
}

fn shop_content(state: &State) -> Element<'_, Message> {
    match state.shop_tab {
        ShopTab::PurchasesAndSales => sale::view(state),
        ShopTab::Billings => bills::view(state),
        ShopTab::Reports => reports::view(state),
    }
}

fn inventory_tabs(state: &State) -> Element<'_, Message> {
    row![
        tab_button("Items", InventoryTab::Items, state.inventory_tab, Message::InventoryTabSelected),
        tab_button("Backup", InventoryTab::Backup, state.inventory_tab, Message::InventoryTabSelected),
    ]
    .spacing(8)
    .padding(12)
    .into()
}

fn inventory_content(state: &State) -> Element<'_, Message> {
    match state.inventory_tab {
        InventoryTab::Items => items::view(state),
        InventoryTab::Backup => backup::view(state),
    }
}

fn tab_button<T: Copy + PartialEq>(
    label: &str,
    target: T,
    current: T,
    on_select: impl Fn(T) -> Message,
) -> Element<'static, Message> {
    let btn = button(text(label.to_string())).on_press(on_select(target));
    if target == current {
        btn.style(theme::primary_button).into()
    } else {
        btn.style(theme::secondary_button).into()
    }
}

#[cfg(test)]
mod undo_tests {
    use super::*;

    #[test]
    fn undo_restores_the_previous_value_of_a_plain_state_field() {
        let mut state = State::default();
        push_edit(&mut state, EditableField::Search, "abc".to_string());
        assert_eq!(state.search_query, "abc");

        undo(&mut state);
        assert_eq!(state.search_query, "");
    }

    #[test]
    fn redo_reapplies_an_undone_edit() {
        let mut state = State::default();
        push_edit(&mut state, EditableField::Search, "abc".to_string());
        undo(&mut state);
        redo(&mut state);
        assert_eq!(state.search_query, "abc");
    }

    #[test]
    fn multiple_edits_unwind_in_reverse_order() {
        let mut state = State::default();
        push_edit(&mut state, EditableField::Search, "a".to_string());
        push_edit(&mut state, EditableField::Search, "ab".to_string());
        push_edit(&mut state, EditableField::Search, "abc".to_string());

        undo(&mut state);
        assert_eq!(state.search_query, "ab");
        undo(&mut state);
        assert_eq!(state.search_query, "a");
        undo(&mut state);
        assert_eq!(state.search_query, "");
    }

    #[test]
    fn a_fresh_edit_clears_the_redo_stack() {
        let mut state = State::default();
        push_edit(&mut state, EditableField::Search, "a".to_string());
        push_edit(&mut state, EditableField::Search, "ab".to_string());
        undo(&mut state); // back to "a", redo has "ab" queued

        push_edit(&mut state, EditableField::Search, "az".to_string());
        redo(&mut state); // nothing left to redo — the branch to "ab" was abandoned
        assert_eq!(state.search_query, "az");
    }

    #[test]
    fn undo_and_redo_on_empty_stacks_do_nothing() {
        let mut state = State::default();
        undo(&mut state);
        redo(&mut state);
        assert_eq!(state.search_query, "");
    }

    #[test]
    fn undo_applies_to_a_field_inside_a_currently_open_form() {
        let mut state = State::default();
        state.item_form = Some(ItemForm::empty());

        push_edit(&mut state, EditableField::ItemForm(items::FormField::Name), "PVC Pipe".to_string());
        assert_eq!(state.item_form.as_ref().unwrap().name, "PVC Pipe");

        undo(&mut state);
        assert_eq!(state.item_form.as_ref().unwrap().name, "");
    }

    #[test]
    fn undo_targeting_a_closed_form_is_a_harmless_no_op() {
        let mut state = State::default();
        state.item_form = Some(ItemForm::empty());
        push_edit(&mut state, EditableField::ItemForm(items::FormField::Name), "PVC Pipe".to_string());

        state.item_form = None; // e.g. the shopkeeper cancelled the form
        undo(&mut state); // must not panic even though there's no form to apply it to
        assert!(state.item_form.is_none());
    }
}
