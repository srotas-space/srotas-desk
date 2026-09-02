mod activation;
mod backup;
mod bills;
mod catalogue;
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
use iced::{Color, Element, Length, Task};
use sqlx::SqlitePool;

use crate::models::{Item, ShopProfile};
use crate::money;
use crate::ui::common::ItemOption;
pub use items::ItemForm;

/// Embedded at compile time so the running app never depends on the
/// project's asset files still being at some relative path on disk.
const LOGO_SVG: &[u8] = include_bytes!("../../assets/logo.svg");
const ICON_PNG: &[u8] = include_bytes!("../../assets/icon.png");

/// The bundled typeface, in the three weights the app uses. Registered at
/// startup so every platform renders identically — see `theme::FONT_FAMILY`
/// for why the app doesn't just take each OS's default sans-serif.
const FONT_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
const FONT_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-SemiBold.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-Bold.ttf");

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
    Details,
    Billings,
    Reports,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryTab {
    Items,
    Backup,
}

/// How loudly the status bar should say something. Every message used to
/// render in the same alarming red, so "security settings saved" looked
/// exactly like a failed save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Error,
    Success,
    Warning,
}

/// A one-line message shown in the status bar at the bottom of the window.
#[derive(Debug, Clone)]
pub struct Notice {
    pub text: String,
    pub kind: NoticeKind,
}

impl Notice {
    pub fn error(text: impl Into<String>) -> Self {
        Notice { text: text.into(), kind: NoticeKind::Error }
    }

    pub fn success(text: impl Into<String>) -> Self {
        Notice { text: text.into(), kind: NoticeKind::Success }
    }

    pub fn warning(text: impl Into<String>) -> Self {
        Notice { text: text.into(), kind: NoticeKind::Warning }
    }
}

pub struct State {
    pool: Option<SqlitePool>,
    stage: Stage,
    shop: Option<ShopProfile>,
    settings: crate::settings::Settings,

    /// The whole catalogue, held only when Settings → Performance has
    /// "keep the catalogue in memory" switched on. Empty in the default
    /// mode, where every screen queries for the rows it draws instead.
    /// See `ui::catalogue` for the in-memory half.
    resident: Vec<Item>,
    /// The current page of the Inventory list — never the whole
    /// catalogue. Every screen that shows items now asks the database for
    /// exactly the rows it is about to draw, so a shop with a hundred
    /// thousand SKUs costs the same as one with fifty.
    items: Vec<Item>,
    /// How many items match the Inventory list's current search/filters,
    /// for its page indicator.
    items_total: i64,
    /// The item behind `viewing_item_id`, fetched on demand — the list
    /// page it was opened from may not even be loaded any more.
    viewing_item: Option<Item>,
    /// Whichever item the Sales form has selected, fetched when it is
    /// picked so the Details panel has its stock and prices.
    sale_item: Option<Item>,
    /// What the item pickers have been typed into, and the candidates
    /// that came back. Shared by Sales, Billings and Reports — one query,
    /// one result set, whichever screen asked.
    picker: common::PickerState,
    /// The low-stock page shown on Shop → Details, and its total.
    low_stock: Vec<Item>,
    low_stock_total: i64,
    /// Set when an action finishes — well or badly — so the shopkeeper sees
    /// *something* rather than a silently-failed (or silently-succeeded)
    /// action.
    notice: Option<Notice>,
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
    login: login::LoginState,
    /// This machine's permanent device id, kept out of `activation` because
    /// the PIN-reset flow needs it long after the activation screen is
    /// gone — a reset is proved by a license key signed for this id.
    device_id: String,

    shop_tab: ShopTab,
    sale_form: sale::SaleForm,
    /// Page index of the low-stock list on Shop → Details. Derived from
    /// `items`, so it needs no loading of its own — only clamping, which
    /// `sale::low_stock_page` does on read.
    low_stock_page: usize,
    reports: reports::ReportsState,


    bills: bills::BillsState,

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
            resident: Vec::new(),
            items: Vec::new(),
            items_total: 0,
            viewing_item: None,
            sale_item: None,
            picker: common::PickerState::default(),
            low_stock: Vec::new(),
            low_stock_total: 0,
            notice: None,
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
            login: login::LoginState::default(),
            device_id: String::new(),
            shop_tab: ShopTab::Details,
            sale_form: sale::SaleForm::default(),
            low_stock_page: 0,
            reports: reports::ReportsState::default(),
            bills: bills::BillsState::default(),
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
    Reports(reports::Field),
    Search,
    BillQty,
    BillPrice,
    BillDiscount,
    BillCustomer,
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
        EditableField::BillQty => std::mem::replace(&mut state.bills.qty_input, value),
        EditableField::BillPrice => std::mem::replace(&mut state.bills.price_input, value),
        EditableField::BillDiscount => std::mem::replace(&mut state.bills.discount_input, value),
        EditableField::BillCustomer => std::mem::replace(&mut state.bills.customer_input, value),
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
    FocusNext,
    FocusPrevious,
    DbReady(Result<SqlitePool, String>),
    LicenseChecked(Result<activation::Outcome, String>),
    ActivationKeyChanged(String),
    ActivationTncToggled(bool),
    OpenTnc,
    CopyDeviceId,
    SubmitActivation,
    ActivationCompleted(Result<(), String>),
    ShopProfileLoaded(Result<Option<ShopProfile>, String>),
    ItemsLoaded(Result<(Vec<Item>, i64, i64), String>),
    LowStockLoaded(Result<(Vec<Item>, i64, i64), String>),
    PickerOptionsLoaded(Result<Vec<Item>, String>),
    /// The whole catalogue, for the in-memory mode. Only ever sent when
    /// that mode is on — see `load_resident`.
    CatalogueLoaded(Result<Vec<Item>, String>),
    /// Settings → Performance: switch between holding the catalogue in
    /// memory and querying for it on demand.
    PreloadToggled(bool),
    /// A keystroke in one of the item pickers — re-queries rather than
    /// filtering a list held in memory.
    PickerInputChanged(common::PickerTarget, String),
    ViewItemLoaded(Result<Option<Item>, String>),
    SaleItemLoaded(Result<Option<Item>, String>),
    BillItemLoaded(Result<Option<Item>, String>),
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
    PurchaseRecorded(Result<(), String>),

    RegisterFieldChanged(register::Field, String),
    SubmitRegister,
    ShopRegistered(Result<ShopProfile, String>),

    LoginPinChanged(String),
    SubmitLogin,
    LoginVerified(Result<login::Outcome, String>),
    ForgotPinPressed,
    CancelPinReset,
    PinResetFieldChanged(login::ResetField, String),
    SubmitPinReset,
    PinResetCompleted(Result<Option<String>, String>),
    /// One-per-second pulse, subscribed to only while the login screen is
    /// counting down a lockout — see `subscription`.
    Tick,

    SaleItemSelected(ItemOption),
    SaleFieldChanged(sale::Field, String),
    SubmitSale,
    SaleRecorded(Result<(), String>),
    LowStockPageNext,
    LowStockPagePrev,

    BillItemSelected(ItemOption),
    BillQtyChanged(String),
    BillPriceChanged(String),
    BillDiscountChanged(String),
    BillCustomerChanged(String),
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
    ReportsFiltersCleared,
    ReportsPresetSelected(reports::Preset),
    ReportsCalendarOpened(reports::Field),
    ReportsCalendarClosed,
    ReportsCalendarShifted(i32),
    ReportsDatePicked(chrono::NaiveDate),
    ReportsDateCleared,
    ReportsPageNext,
    ReportsPagePrev,
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
    .font(FONT_REGULAR)
    .font(FONT_SEMIBOLD)
    .font(FONT_BOLD)
    .default_font(theme::REGULAR)
    .theme(|_state: &State| theme::theme())
    .window(iced::window::Settings {
        icon,
        ..iced::window::Settings::default()
    })
    .subscription(subscription)
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
/// Everything the app listens to. The lockout countdown on the login
/// screen is the only thing that needs a clock, and it only needs one
/// while it's actually counting down — so the timer is subscribed
/// conditionally rather than left ticking for the life of the app.
fn subscription(state: &State) -> iced::Subscription<Message> {
    let counting_down = state.stage == Stage::Login
        && state.shop.as_ref().and_then(|s| s.lock_remaining_secs(chrono::Utc::now())).is_some();

    if counting_down {
        iced::Subscription::batch([
            keyboard_shortcuts(),
            iced::time::every(std::time::Duration::from_secs(1)).map(|_| Message::Tick),
        ])
    } else {
        keyboard_shortcuts()
    }
}

fn keyboard_shortcuts() -> iced::Subscription<Message> {
    iced::keyboard::listen().map(|event| {
        let iced::keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
            return Message::NoOp;
        };

        // Tab/Shift+Tab move focus between fields — text_input doesn't do
        // this on its own (unlike a browser's native tab order), so it has
        // to be wired up explicitly via `operation::focus_next/previous`
        // in `update` below. No modifier requirement, unlike the
        // undo/redo shortcuts past this point.
        if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab) {
            return if modifiers.shift() { Message::FocusPrevious } else { Message::FocusNext };
        }

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
        Message::FocusNext => iced::widget::operation::focus_next(),
        Message::FocusPrevious => iced::widget::operation::focus_previous(),
        Message::DbReady(Ok(pool)) => {
            state.settings = crate::settings::load();
            let for_license = pool.clone();
            state.pool = Some(pool);
            Task::perform(activation::check(for_license), Message::LicenseChecked)
        }
        Message::DbReady(Err(e)) => {
            state.notice = Some(Notice::error(format!("could not open database: {e}")));
            Task::none()
        }
        Message::LicenseChecked(Ok(activation::Outcome::Valid { device_id, expiry_warning })) => {
            state.device_id = device_id;
            state.notice = expiry_warning.map(Notice::warning);
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            Task::batch([load_shop_profile(pool), load_picker(state, String::new()), load_resident(state)])
        }
        Message::LicenseChecked(Ok(activation::Outcome::NeedsActivation { device_id, message })) => {
            state.device_id = device_id.clone();
            state.activation.device_id = device_id;
            state.activation.error = message;
            state.stage = Stage::Activation;
            Task::none()
        }
        Message::LicenseChecked(Err(e)) => {
            state.notice = Some(Notice::error(format!("could not check license: {e}")));
            Task::none()
        }
        Message::ActivationKeyChanged(value) => {
            push_edit(state, EditableField::ActivationKey, value);
            Task::none()
        }
        Message::ActivationTncToggled(agreed) => {
            state.activation.agreed_to_tnc = agreed;
            Task::none()
        }
        Message::OpenTnc => {
            if let Err(e) = open::that(activation::TNC_URL) {
                state.activation.error = Some(format!("could not open link: {e}"));
            }
            Task::none()
        }
        Message::CopyDeviceId => iced::clipboard::write(state.activation.device_id.clone()),
        Message::SubmitActivation => activation::submit(state),
        Message::ActivationCompleted(Ok(())) => {
            state.activation.error = None;
            state.device_id = state.activation.device_id.clone();
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            Task::batch([load_shop_profile(pool), load_picker(state, String::new()), load_resident(state)])
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
            state.notice = Some(Notice::error(format!("could not load shop profile: {e}")));
            Task::none()
        }
        Message::ItemsLoaded(Ok((items, total, page))) => {
            state.items = items;
            state.items_total = total;
            state.items_page = page as usize;
            refresh_item_thumbnails(state)
        }
        Message::ItemsLoaded(Err(e)) => {
            state.notice = Some(Notice::error(format!("could not load items: {e}")));
            Task::none()
        }
        Message::LowStockLoaded(Ok((items, total, page))) => {
            state.low_stock = items;
            state.low_stock_total = total;
            state.low_stock_page = page as usize;
            Task::none()
        }
        Message::LowStockLoaded(Err(e)) => {
            state.notice = Some(Notice::error(format!("could not load low stock: {e}")));
            Task::none()
        }
        Message::PickerOptionsLoaded(Ok(items)) => {
            // All three pickers share one set of candidates: they are the
            // same question ("which item?") asked from three screens, and
            // keeping one list means one query per keystroke rather than
            // three.
            state.picker.options = crate::ui::common::item_options(&items);
            Task::none()
        }
        Message::CatalogueLoaded(Ok(items)) => {
            state.resident = items;
            // The list, low-stock panel and pickers all read from the
            // resident copy in this mode, so they are refreshed now that
            // it exists rather than left showing a stale page.
            Task::batch([load_items(state), load_low_stock(state), load_picker(state, String::new())])
        }
        Message::CatalogueLoaded(Err(e)) => {
            // Falling back rather than failing: an unusable screen is a
            // worse outcome than quietly using the cheaper mode.
            state.settings.preload_catalogue = false;
            let _ = crate::settings::save(&state.settings);
            state.notice = Some(Notice::warning(format!(
                "Could not hold the catalogue in memory ({e}) — switched back to loading items as needed."
            )));
            Task::batch([load_items(state), load_low_stock(state), load_picker(state, String::new())])
        }
        Message::PreloadToggled(on) => {
            state.settings.preload_catalogue = on;
            let _ = crate::settings::save(&state.settings);

            if on {
                state.notice = Some(Notice::success("Catalogue will be kept in memory."));
                load_resident(state)
            } else {
                // Drop the resident copy immediately — the whole point of
                // switching off is to stop paying for it — then reload the
                // visible screens from the database.
                state.resident = Vec::new();
                state.resident.shrink_to_fit();
                state.notice = Some(Notice::success("Items will be loaded as needed."));
                Task::batch([load_items(state), load_low_stock(state), load_picker(state, String::new())])
            }
        }
        Message::PickerOptionsLoaded(Err(e)) => {
            state.notice = Some(Notice::error(format!("could not search items: {e}")));
            Task::none()
        }
        Message::PickerInputChanged(target, query) => {
            state.picker.typed(target, query.clone());
            load_picker(state, query)
        }
        Message::ViewItemLoaded(Ok(item)) => {
            state.viewing_item = item;
            Task::none()
        }
        Message::ViewItemLoaded(Err(e)) => {
            state.notice = Some(Notice::error(e));
            Task::none()
        }
        Message::SaleItemLoaded(Ok(item)) => {
            // Pre-fill the price from the item the shopkeeper just picked,
            // unless they have already typed one over it.
            if let Some(item) = &item {
                if state.sale_form.price.trim().is_empty() {
                    state.sale_form.price = money::paise_to_input(item.sell_price_paise);
                }
            }
            state.sale_item = item;
            Task::none()
        }
        Message::SaleItemLoaded(Err(e)) => {
            state.notice = Some(Notice::error(e));
            Task::none()
        }
        Message::BillItemLoaded(Ok(item)) => {
            if let Some(item) = &item {
                if state.bills.price_input.trim().is_empty() {
                    state.bills.price_input = money::paise_to_input(item.sell_price_paise);
                }
                state.bills.item_gst_rate_bp = item.gst_rate_bp;
            }
            Task::none()
        }
        Message::BillItemLoaded(Err(e)) => {
            state.notice = Some(Notice::error(e));
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
            load_items(state)
        }
        Message::GoHome => {
            state.stage = Stage::Home;
            Task::none()
        }
        Message::Lock => {
            state.stage = Stage::Login;
            state.login.clear();
            // Locking the counter means whoever walks up next shouldn't be
            // able to Ctrl+Z their way back through the last operator's
            // typing.
            state.undo_stack.clear();
            state.redo_stack.clear();
            Task::none()
        }
        Message::ShopTabSelected(tab) => {
            state.shop_tab = tab;
            enter_shop_tab(state, tab)
        }
        Message::InventoryTabSelected(tab) => {
            state.inventory_tab = tab;
            match tab {
                InventoryTab::Items => load_items(state),
                InventoryTab::Backup => Task::none(),
            }
        }
        Message::SearchChanged(query) => {
            // Re-queries the database on every keystroke rather than
            // filtering a catalogue held in memory. `push_edit` resets the
            // page to 0 — see `apply_edit`.
            push_edit(state, EditableField::Search, query);
            load_items(state)
        }

        Message::OpenAddItemForm => {
            state.item_form = Some(ItemForm::empty());
            state.viewing_item_id = None;
            Task::none()
        }
        Message::OpenEditItemForm(id) => {
            let Some(item) = on_screen_item(state, id) else {
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
                state.notice = Some(Notice::error("image is too large (max 5 MB)"));
            } else if image::load_from_memory(&bytes).is_err() {
                state.notice = Some(Notice::error("that file doesn't look like a valid image"));
            } else if let Some(form) = &mut state.item_form {
                form.image = Some(bytes);
                state.notice = None;
            }
            Task::none()
        }
        Message::ItemImagePicked(Ok(None)) => Task::none(),
        Message::ItemImagePicked(Err(e)) => {
            state.notice = Some(Notice::error(format!("could not read image: {e}")));
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
            state.notice = None;
            reload_items(state)
        }
        Message::ItemSaved(Err(e)) => {
            state.notice = Some(Notice::error(e));
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
            state.notice = None;
            reload_items(state)
        }
        Message::ItemDeleted(Err(e)) => {
            state.notice = Some(Notice::error(e));
            Task::none()
        }

        Message::UnitFilterSelected(filter) => {
            state.unit_filter = filter;
            state.items_page = 0;
            load_items(state)
        }
        Message::LowStockOnlyToggled(enabled) => {
            state.low_stock_only = enabled;
            state.items_page = 0;
            load_items(state)
        }
        Message::ItemsPageNext => {
            state.items_page += 1;
            load_items(state)
        }
        Message::ItemsPagePrev => {
            state.items_page = state.items_page.saturating_sub(1);
            load_items(state)
        }
        Message::OpenViewItem(id) => {
            state.viewing_item = None;
            state.viewing_item_id = Some(id);
            state.view_image = None;
            state.item_form = None;
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            let pool2 = pool.clone();
            // Fetch the row as well as the photo: the list page it was
            // opened from is only ever a page, and may not hold this item
            // by the time the detail screen draws.
            Task::batch([
                Task::perform(
                    async move { crate::repo::get_item_image(&pool2, id).await.unwrap_or(None) },
                    move |image| Message::ViewImageLoaded(id, image),
                ),
                Task::perform(
                    async move { crate::repo::get_item(&pool, id).await.map_err(|e| e.to_string()) },
                    Message::ViewItemLoaded,
                ),
            ])
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
            if let Some(item) = on_screen_item(state, id) {
                state.purchase_form = Some(items::PurchaseForm {
                    item_id: item.id,
                    item_name: item.name.clone(),
                    qty: String::new(),
                    price: money::paise_to_input(item.buy_price_paise),
                });
                state.notice = None;
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
            state.notice = None;
            Task::none()
        }
        Message::PurchaseRecorded(Ok(_)) => {
            state.purchase_form = None;
            state.notice = None;
            reload_items(state)
        }
        Message::PurchaseRecorded(Err(e)) => {
            state.notice = Some(Notice::error(e));
            Task::none()
        }

        Message::RegisterFieldChanged(field, value) => {
            push_edit(state, EditableField::Register(field), value);
            Task::none()
        }
        Message::SubmitRegister => register::submit(state),
        Message::ShopRegistered(Ok(profile)) => {
            state.shop = Some(profile);
            state.notice = None;
            state.stage = Stage::Home;
            backup::maybe_auto_backup(state)
        }
        Message::ShopLogoLoaded(logo) => {
            state.shop_logo = logo;
            Task::none()
        }
        Message::ShopRegistered(Err(e)) => {
            state.notice = Some(Notice::error(e));
            Task::none()
        }

        // PIN and license-key fields deliberately bypass `push_edit`: the
        // undo stack keeps every intermediate value of whatever it
        // records, so routing a PIN through it would leave the PIN sitting
        // in memory in plain text long after `LoginState::clear` wiped the
        // field itself — which rather undoes the point of hashing it.
        Message::LoginPinChanged(value) => {
            state.login.pin_input = value;
            Task::none()
        }
        Message::SubmitLogin => login::submit(state),
        Message::LoginVerified(result) => login::verified(state, result),
        Message::ForgotPinPressed => {
            state.login.reset_open = true;
            state.login.reset_error = None;
            state.login.error = None;
            Task::none()
        }
        Message::CancelPinReset => {
            state.login.reset_open = false;
            state.login.reset_key.clear();
            state.login.reset_pin.clear();
            state.login.reset_confirm.clear();
            state.login.reset_error = None;
            Task::none()
        }
        Message::PinResetFieldChanged(field, value) => {
            state.login.set_field(field, value);
            Task::none()
        }
        Message::SubmitPinReset => login::submit_reset(state),
        Message::PinResetCompleted(result) => login::reset_completed(state, result),
        // Nothing to do but re-render: the lockout countdown is derived
        // from `pin_locked_until`, so a repaint is the whole point.
        Message::Tick => Task::none(),

        Message::SaleItemSelected(option) => {
            state.picker.chose(option.name.clone());
            sale::select_item(state, option)
        }
        Message::SaleFieldChanged(field, value) => {
            push_edit(state, EditableField::Sale(field), value);
            Task::none()
        }
        Message::SubmitSale => sale::submit(state),
        Message::SaleRecorded(Ok(_)) => {
            state.notice = Some(Notice::success("Sale recorded."));
            state.sale_form = sale::SaleForm::default();
            reload_items(state)
        }
        Message::SaleRecorded(Err(e)) => {
            state.notice = Some(Notice::error(e));
            Task::none()
        }
        Message::LowStockPageNext => {
            state.low_stock_page += 1;
            load_low_stock(state)
        }
        Message::LowStockPagePrev => {
            state.low_stock_page = state.low_stock_page.saturating_sub(1);
            load_low_stock(state)
        }

        Message::BillItemSelected(option) => {
            state.picker.chose(option.name.clone());
            bills::select_item(state, option)
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
        Message::BillCustomerChanged(value) => {
            push_edit(state, EditableField::BillCustomer, value);
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
            state.notice = None;
            bills::start_new(state);
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            Task::batch([reload_items(state), bills::load_history(pool, state.bills.page)])
        }
        Message::BillSaved(Err(e)) => {
            state.notice = Some(Notice::error(e));
            Task::none()
        }
        Message::BillsLoaded(Ok((rows, total))) => {
            state.bills.rows = rows;
            state.bills.total = total;
            Task::none()
        }
        Message::BillsLoaded(Err(e)) => {
            state.notice = Some(Notice::error(e));
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
            state.notice = Some(Notice::error(e));
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
            state.notice = Some(Notice::error(e));
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
            state.notice = None;
            let Some(pool) = state.pool.clone() else {
                return Task::none();
            };
            bills::load_history(pool, state.bills.page)
        }
        Message::BillDeleted(Err(e)) => {
            state.notice = Some(Notice::error(e));
            Task::none()
        }
        Message::PrintBillPressed(id) => bills::print_bill(state, id),
        Message::BillPdfReady(Ok(path)) => {
            state.notice = Some(Notice::success(format!("Bill saved and opened: {}", path.display())));
            Task::none()
        }
        Message::BillPdfReady(Err(e)) => {
            state.notice = Some(Notice::error(e));
            Task::none()
        }

        Message::ReportsItemFilterSelected(option) => {
            match &option {
                Some(o) => state.picker.chose(o.name.clone()),
                None => state.picker.clear(),
            }
            state.reports.item_filter = option;
            reports::run_from_start(state)
        }
        Message::ReportsFieldChanged(field, value) => {
            push_edit(state, EditableField::Reports(field), value);
            Task::none()
        }
        Message::ReportsFiltersCleared => {
            state.picker.clear();
            state.reports.item_filter = None;
            state.reports.from.clear();
            state.reports.to.clear();
            state.reports.calendar = None;
            reports::run_from_start(state)
        }
        Message::ReportsPresetSelected(preset) => {
            state.reports.apply_preset(preset);
            reports::run_from_start(state)
        }
        Message::ReportsCalendarOpened(field) => {
            state.reports.open_calendar(field);
            Task::none()
        }
        Message::ReportsCalendarClosed => {
            state.reports.calendar = None;
            Task::none()
        }
        Message::ReportsCalendarShifted(months) => {
            state.reports.shift_month(months);
            Task::none()
        }
        Message::ReportsDatePicked(date) => {
            // Picking a date runs the report straight away — the whole
            // point of the calendar is not having to press Run as well.
            let Some(cal) = state.reports.calendar else {
                return Task::none();
            };
            state.reports.set_field(cal.field, date.to_string());
            state.reports.calendar = None;
            reports::run_from_start(state)
        }
        Message::ReportsDateCleared => {
            let Some(cal) = state.reports.calendar else {
                return Task::none();
            };
            state.reports.set_field(cal.field, String::new());
            state.reports.calendar = None;
            reports::run_from_start(state)
        }
        Message::ReportsPageNext => {
            state.reports.page += 1;
            reports::run(state)
        }
        Message::ReportsPagePrev => {
            state.reports.page = state.reports.page.saturating_sub(1);
            reports::run(state)
        }
        Message::RunReports => reports::run_from_start(state),
        Message::ReportsLoaded(Ok(loaded)) => {
            state.reports.stock_value_paise = loaded.stock_value_paise;
            state.reports.total_profit_paise = loaded.total_profit_paise;
            state.reports.rows = loaded.rows;
            state.reports.total = loaded.total;
            Task::none()
        }
        Message::ReportsLoaded(Err(e)) => {
            state.notice = Some(Notice::error(e));
            Task::none()
        }
        Message::DownloadReportPressed => reports::download(state),
        Message::ReportPdfReady(Ok((loaded, path))) => {
            state.reports.stock_value_paise = loaded.stock_value_paise;
            state.reports.total_profit_paise = loaded.total_profit_paise;
            state.reports.rows = loaded.rows;
            state.notice = Some(Notice::success(format!("Report saved and opened: {}", path.display())));
            Task::none()
        }
        Message::ReportPdfReady(Err(e)) => {
            state.notice = Some(Notice::error(e));
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
            state.notice = Some(Notice::success(format!("Backup saved: {}", path.display())));
            Task::none()
        }
        Message::BackupCompleted(Err(e)) => {
            state.notice = Some(Notice::error(format!("backup failed: {e}")));
            Task::none()
        }

        Message::OpenSettings => {
            if let Some(shop) = &state.shop {
                state.profile_form = settings::ProfileForm::from_shop(shop);
            }
            state.security_form = settings::SecurityForm::default();
            state.settings_tab = settings::SettingsTab::Profile;
            state.stage = Stage::Settings;
            state.notice = None;
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
                state.notice = Some(Notice::error("image is too large (max 5 MB)"));
            } else if image::load_from_memory(&bytes).is_err() {
                state.notice = Some(Notice::error("that file doesn't look like a valid image"));
            } else {
                state.profile_form.logo = Some(bytes);
                state.profile_form.logo_removed = false;
                state.notice = None;
            }
            Task::none()
        }
        Message::ProfileLogoPicked(Ok(None)) => Task::none(),
        Message::ProfileLogoPicked(Err(e)) => {
            state.notice = Some(Notice::error(format!("could not read image: {e}")));
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
            state.notice = Some(Notice::success("Shop profile saved."));
            let logo_changed = state.profile_form.logo.is_some() || state.profile_form.logo_removed;
            if logo_changed {
                state.shop_logo = state.profile_form.logo.clone();
            }
            Task::none()
        }
        Message::ProfileSaved(Err(e)) => {
            state.notice = Some(Notice::error(e));
            Task::none()
        }
        Message::SecurityFieldChanged(field, value) => {
            state.security_form.set_field(field, value);
            Task::none()
        }
        Message::SubmitPinChange => settings::submit_pin(state),
        Message::PinChanged(Ok(new_hash)) => {
            if let Some(shop) = &mut state.shop {
                shop.pin_hash = new_hash;
                shop.pin_failed_attempts = 0;
                shop.pin_locked_until = None;
            }
            state.security_form = settings::SecurityForm::default();
            state.notice = Some(Notice::success("Security settings saved."));
            Task::none()
        }
        Message::PinChanged(Err(e)) => {
            state.notice = Some(Notice::error(e));
            Task::none()
        }
    }
}

fn enter_shop_tab(state: &mut State, tab: ShopTab) -> Task<Message> {
    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };
    match tab {
        // Details shows low stock, which is derived from `items` — so it
        // needs those fresh rather than a list of its own.
        // Details shows low stock and an item picker — both of which ask
        // the database for just what they draw.
        ShopTab::Details => Task::batch([load_low_stock(state), load_picker(state, String::new())]),
        ShopTab::Billings => bills::load_history(pool, state.bills.page),
        ShopTab::Reports => reports::run(state),
    }
}

/// How many candidates an item picker offers at once. Twenty is more than
/// fits on screen, so the shopkeeper types to narrow rather than scrolls
/// to find — and the picker never holds more than a screenful whatever
/// the catalogue's size.
pub const PICKER_LIMIT: i64 = 20;

/// Loads the Inventory list's current page and its total. Called after
/// anything that changes what should be on it — a search keystroke, a
/// filter, a page step, or an edit.
fn load_items(state: &State) -> Task<Message> {
    let query = state.search_query.trim().to_string();
    let unit = state.unit_filter.as_unit();
    let low_only = state.low_stock_only;
    let page = state.items_page as i64;

    // The two modes differ here and nowhere else. Whichever runs, it
    // produces the same `(rows, total, page)` — so every screen below
    // reads the same fields and needs no idea which mode is on.
    if state.settings.preload_catalogue {
        let (rows, total, page) =
            catalogue::page(&state.resident, &query, unit, low_only, items::PAGE_SIZE, page.max(0) as usize);
        return Task::done(Message::ItemsLoaded(Ok((rows, total, page as i64))));
    }

    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            let total = crate::repo::count_items(&pool, &query, unit, low_only).await?;
            // Clamp here rather than at the message handlers: deleting the
            // last item on the last page would otherwise strand the list
            // on a page that no longer exists.
            let page_count = (total as f64 / items::PAGE_SIZE as f64).ceil().max(1.0) as i64;
            let page = page.min(page_count - 1).max(0);
            let rows = crate::repo::list_items_page(
                &pool,
                &query,
                unit,
                low_only,
                items::PAGE_SIZE as i64,
                page * items::PAGE_SIZE as i64,
            )
            .await?;
            Ok::<_, crate::repo::RepoError>((rows, total, page))
        },
        |result| Message::ItemsLoaded(result.map_err(|e| e.to_string())),
    )
}

/// Refreshes the item pickers from whatever the shopkeeper has typed.
/// With an empty query this is the first `PICKER_LIMIT` items
/// alphabetically — a starting point to browse from, not the catalogue.
fn load_picker(state: &State, query: String) -> Task<Message> {
    if state.settings.preload_catalogue {
        let rows = catalogue::search(&state.resident, &query, PICKER_LIMIT as usize);
        return Task::done(Message::PickerOptionsLoaded(Ok(rows)));
    }

    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };
    Task::perform(
        async move { crate::repo::search_items(&pool, &query, PICKER_LIMIT).await },
        |result| Message::PickerOptionsLoaded(result.map_err(|e| e.to_string())),
    )
}

/// Loads the whole catalogue for the in-memory mode. Only ever called
/// when that mode is on — in the default mode nothing calls this, which
/// is the entire point of the default.
fn load_resident(state: &State) -> Task<Message> {
    if !state.settings.preload_catalogue {
        return Task::none();
    }
    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };
    Task::perform(
        // `i64::MAX` rather than a special "no limit" path: the query is
        // the same one every other screen uses, just unbounded.
        async move { crate::repo::list_items_page(&pool, "", None, false, i64::MAX, 0).await },
        |result| Message::CatalogueLoaded(result.map_err(|e| e.to_string())),
    )
}

/// Loads the low-stock panel's current page and its total.
fn load_low_stock(state: &State) -> Task<Message> {
    let page = state.low_stock_page as i64;

    if state.settings.preload_catalogue {
        let (rows, total, page) = catalogue::low_stock_page(&state.resident, sale::PAGE_SIZE, page.max(0) as usize);
        return Task::done(Message::LowStockLoaded(Ok((rows, total, page as i64))));
    }

    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            let total = crate::repo::low_stock_count(&pool).await?;
            let page_count = (total as f64 / sale::PAGE_SIZE as f64).ceil().max(1.0) as i64;
            let page = page.min(page_count - 1).max(0);
            let rows =
                crate::repo::low_stock_page(&pool, sale::PAGE_SIZE as i64, page * sale::PAGE_SIZE as i64).await?;
            Ok::<_, crate::repo::RepoError>((rows, total, page))
        },
        |result| Message::LowStockLoaded(result.map_err(|e| e.to_string())),
    )
}

fn load_shop_profile(pool: SqlitePool) -> Task<Message> {
    Task::perform(
        async move {
            // Re-hash any PIN left in plain text by a pre-0009 install
            // before the profile is read, so the screen never sees the
            // half-migrated row. A no-op on every launch after the first.
            crate::repo::upgrade_legacy_pin(&pool).await?;
            crate::repo::get_shop_profile(&pool).await
        },
        |result| Message::ShopProfileLoaded(result.map_err(|e| e.to_string())),
    )
}

fn load_shop_logo(pool: SqlitePool) -> Task<Message> {
    Task::perform(
        async move { crate::repo::get_shop_logo(&pool).await.unwrap_or(None) },
        Message::ShopLogoLoaded,
    )
}

/// Everything that shows items, refreshed together — the Inventory page,
/// the low-stock panel, and the pickers. Called after any write that
/// could change what they show.
/// Finds an item among the ones already on screen — the current
/// Inventory page, whatever the detail view is showing, or the item the
/// Sales form has picked.
///
/// Every caller opens a form *from* one of those, so the row is always to
/// hand and this never needs to touch the database. Returning `None`
/// means the action came from something no longer displayed, in which
/// case doing nothing is the right answer.
fn on_screen_item(state: &State, id: i64) -> Option<&Item> {
    state
        .items
        .iter()
        .chain(state.low_stock.iter())
        .find(|i| i.id == id)
        .or_else(|| state.viewing_item.as_ref().filter(|i| i.id == id))
        .or_else(|| state.sale_item.as_ref().filter(|i| i.id == id))
}

fn reload_items(state: &State) -> Task<Message> {
    // In the in-memory mode the resident copy is the source the other
    // three read from, so it has to be refetched first; `CatalogueLoaded`
    // then re-runs them against it.
    if state.settings.preload_catalogue {
        return load_resident(state);
    }
    Task::batch([load_items(state), load_low_stock(state), load_picker(state, String::new())])
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
        Stage::Loading => loading_view(),
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

    let mut screen = column![container(content).width(Length::Fill).height(Length::Fill)];
    if let Some(notice) = &state.notice {
        screen = screen.push(status_bar(notice));
    }

    screen.width(Length::Fill).height(Length::Fill).into()
}

fn loading_view() -> Element<'static, Message> {
    container(
        column![
            svg(logo_handle()).width(64).height(64),
            text("Loading...").size(theme::TEXT_BODY).color(theme::MUTED_TEXT),
        ]
        .spacing(theme::SPACE_MD)
        .align_x(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(iced::Alignment::Center)
    .align_y(iced::Alignment::Center)
    .into()
}

/// The bottom status strip. It only takes up space when there's something
/// to say, and it's tinted by severity — a saved backup shouldn't look
/// like a failure, which is exactly how it used to read.
fn status_bar(notice: &Notice) -> Element<'_, Message> {
    let (style, icon): (fn(&iced::Theme) -> iced::widget::container::Style, &str) = match notice.kind {
        NoticeKind::Error => (theme::notice_error, "!"),
        NoticeKind::Success => (theme::notice_success, "\u{2713}"),
        NoticeKind::Warning => (theme::notice_warning, "!"),
    };

    container(
        container(
            row![
                text(icon).size(theme::TEXT_BODY).font(theme::BOLD),
                text(&notice.text).size(theme::TEXT_SMALL),
            ]
            .spacing(theme::SPACE_SM)
            .align_y(iced::Alignment::Center),
        )
        .style(style)
        .padding([theme::SPACE_SM as u16, theme::SPACE_MD as u16])
        .width(Length::Fill),
    )
    .padding([theme::SPACE_SM as u16, theme::SPACE_MD as u16])
    .width(Length::Fill)
    .into()
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
            .width(34)
            .height(34)
            .content_fit(iced::ContentFit::Cover)
            .into(),
        None => svg(logo_handle()).width(34).height(34).into(),
    };

    // Shop name and the current screen stack vertically: the name is the
    // constant, the screen is what changes, and reading them as two lines
    // is faster than parsing "Shop Name · Inventory" on one.
    let mut identity = column![text(shop_name).size(theme::TEXT_HEADING).font(theme::SEMIBOLD)].spacing(1);
    if !title.is_empty() {
        identity = identity.push(text(title).size(theme::TEXT_CAPTION).color(Color::from_rgba(1.0, 1.0, 1.0, 0.78)));
    }

    let mut right = row![].spacing(theme::SPACE_SM);
    if show_home {
        right = right.push(app_bar_action("Home", Message::GoHome));
    }
    right = right.push(app_bar_action("Settings", Message::OpenSettings));
    right = right.push(app_bar_action("Lock", Message::Lock));

    container(
        row![
            row![logo, identity].spacing(theme::SPACE_SM).align_y(iced::Alignment::Center),
            iced::widget::space::horizontal(),
            right,
        ]
        .align_y(iced::Alignment::Center)
        .padding([theme::SPACE_SM as u16 + 2, theme::SPACE_MD as u16]),
    )
    .style(theme::header_bar)
    .width(Length::Fill)
    .into()
}

fn app_bar_action(label: &str, message: Message) -> Element<'static, Message> {
    button(text(label.to_string()).size(theme::TEXT_SMALL))
        .style(theme::app_bar_button)
        .padding([8, 16])
        .on_press(message)
        .into()
}

fn shop_tabs(state: &State) -> Element<'_, Message> {
    tab_strip(row![
        tab_button("Details", ShopTab::Details, state.shop_tab, Message::ShopTabSelected),
        tab_button("Billings", ShopTab::Billings, state.shop_tab, Message::ShopTabSelected),
        tab_button("Reports", ShopTab::Reports, state.shop_tab, Message::ShopTabSelected),
    ])
}

fn shop_content(state: &State) -> Element<'_, Message> {
    match state.shop_tab {
        ShopTab::Details => sale::view(state),
        ShopTab::Billings => bills::view(state),
        ShopTab::Reports => reports::view(state),
    }
}

fn inventory_tabs(state: &State) -> Element<'_, Message> {
    tab_strip(row![
        tab_button("Items", InventoryTab::Items, state.inventory_tab, Message::InventoryTabSelected),
        tab_button("Backup", InventoryTab::Backup, state.inventory_tab, Message::InventoryTabSelected),
    ])
}

fn inventory_content(state: &State) -> Element<'_, Message> {
    match state.inventory_tab {
        InventoryTab::Items => items::view(state),
        InventoryTab::Backup => backup::view(state),
    }
}

/// Wraps a row of tab buttons in the pill that makes them read as one
/// segmented control instead of several unrelated buttons.
fn tab_strip<'a>(tabs: iced::widget::Row<'a, Message>) -> Element<'a, Message> {
    container(
        container(tabs.spacing(theme::SPACE_XS)).style(theme::panel).padding(theme::SPACE_XS as u16),
    )
    .padding([theme::SPACE_MD as u16, theme::SPACE_MD as u16])
    .into()
}

fn tab_button<T: Copy + PartialEq>(
    label: &str,
    target: T,
    current: T,
    on_select: impl Fn(T) -> Message,
) -> Element<'static, Message> {
    let btn = button(text(label.to_string()).size(theme::TEXT_SMALL).font(theme::SEMIBOLD))
        .padding([9, 20])
        .on_press(on_select(target));
    if target == current {
        btn.style(theme::tab_selected).into()
    } else {
        btn.style(theme::tab_idle).into()
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
