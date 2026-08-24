mod backup;
mod common;
mod home;
mod items;
mod login;
mod register;
mod reports;
mod sale;
mod theme;

use std::path::PathBuf;

use iced::widget::{button, column, container, row, svg, text};
use iced::{Element, Length, Task};
use sqlx::SqlitePool;

use crate::models::{Item, ShopProfile, Transaction};
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
    Register,
    Login,
    Home,
    Inventory,
    Shop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopTab {
    PurchasesAndSales,
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
    reports: reports::ReportsState,
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
            confirming_delete_id: None,
            items_page: 0,
            unit_filter: items::UnitFilter::All,
            low_stock_only: false,
            viewing_item_id: None,
            view_image: None,
            inventory_tab: InventoryTab::Items,
            register_form: register::RegisterForm::default(),
            login_pin_input: String::new(),
            login_error: None,
            shop_tab: ShopTab::PurchasesAndSales,
            sale_form: sale::SaleForm::default(),
            sale_item_combo: iced::widget::combo_box::State::new(Vec::new()),
            recent_sales: Vec::new(),
            sale_page: 0,
            reports: reports::ReportsState::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    DbReady(Result<SqlitePool, String>),
    ShopProfileLoaded(Result<Option<ShopProfile>, String>),
    ItemsLoaded(Result<Vec<Item>, String>),

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
    .run()
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::DbReady(Ok(pool)) => {
            state.settings = crate::settings::load();
            let for_items = pool.clone();
            let for_shop = pool.clone();
            state.pool = Some(pool);
            Task::batch([load_items(for_items), load_shop_profile(for_shop)])
        }
        Message::DbReady(Err(e)) => {
            state.status = Some(format!("could not open database: {e}"));
            Task::none()
        }
        Message::ShopProfileLoaded(Ok(Some(profile))) => {
            state.shop = Some(profile);
            state.stage = Stage::Login;
            Task::none()
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
            state.sale_item_combo = iced::widget::combo_box::State::new(options);
            state.items = items;
            Task::none()
        }
        Message::ItemsLoaded(Err(e)) => {
            state.status = Some(format!("could not load items: {e}"));
            Task::none()
        }

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
            state.search_query = query;
            state.items_page = 0;
            Task::none()
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
            if let Some(form) = &mut state.item_form {
                form.set_field(field, value);
            }
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
        Message::ItemSaved(Ok(_)) => {
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
        Message::ItemDeleted(Ok(_)) => {
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
            Task::none()
        }
        Message::LowStockOnlyToggled(enabled) => {
            state.low_stock_only = enabled;
            state.items_page = 0;
            Task::none()
        }
        Message::ItemsPageNext => {
            state.items_page += 1;
            Task::none()
        }
        Message::ItemsPagePrev => {
            state.items_page = state.items_page.saturating_sub(1);
            Task::none()
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

        Message::RegisterFieldChanged(field, value) => {
            state.register_form.set_field(field, value);
            Task::none()
        }
        Message::SubmitRegister => register::submit(state),
        Message::ShopRegistered(Ok(profile)) => {
            state.shop = Some(profile);
            state.status = None;
            state.stage = Stage::Home;
            backup::maybe_auto_backup(state)
        }
        Message::ShopRegistered(Err(e)) => {
            state.status = Some(e);
            Task::none()
        }

        Message::LoginPinChanged(value) => {
            state.login_pin_input = value;
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
            state.sale_form.set_field(field, value);
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

        Message::ReportsItemFilterSelected(option) => {
            state.reports.item_filter = option;
            reports::run(state)
        }
        Message::ReportsFieldChanged(field, value) => {
            match field {
                reports::Field::From => state.reports.from = value,
                reports::Field::To => state.reports.to = value,
            }
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
    }
}

fn enter_shop_tab(state: &mut State, tab: ShopTab) -> Task<Message> {
    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };
    match tab {
        ShopTab::PurchasesAndSales => sale::load_recent(pool),
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

fn reload_items(state: &State) -> Task<Message> {
    match state.pool.clone() {
        Some(pool) => load_items(pool),
        None => Task::none(),
    }
}

fn view(state: &State) -> Element<'_, Message> {
    let content: Element<'_, Message> = match state.stage {
        Stage::Loading => container(text("Loading...").size(18))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::Alignment::Center)
            .align_y(iced::Alignment::Center)
            .into(),
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
        _ => "",
    };

    let mut left = row![
        svg(logo_handle()).width(32).height(32),
        text(shop_name).size(18),
    ]
    .spacing(theme::SPACE_SM)
    .align_y(iced::Alignment::Center);

    if !title.is_empty() {
        left = left.push(text(format!("· {title}")).size(16));
    }

    let mut right = row![].spacing(theme::SPACE_SM);
    if show_home {
        right = right.push(button(text("Home").size(14)).style(theme::secondary_button).padding([8, 14]).on_press(Message::GoHome));
    }
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
        tab_button("Purchases & Sales", ShopTab::PurchasesAndSales, state.shop_tab, Message::ShopTabSelected),
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
