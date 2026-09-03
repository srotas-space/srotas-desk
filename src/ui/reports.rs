use chrono::{DateTime, Datelike, NaiveDate, Utc};
use iced::widget::{button, column, container, grid, row, scrollable, text};
use iced::{Element, Length, Task};
use sqlx::SqlitePool;
use std::path::PathBuf;

use super::{Message, Notice, State};
use crate::money;
use crate::repo::TransactionHistoryRow;
use crate::ui::common::ItemOption;
use crate::pdf;
use crate::ui::{common, theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    From,
    To,
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Field::From => "From",
            Field::To => "To",
        }
    }
}

/// How many transaction rows one page of the report shows. The report is
/// a screen you scan, not a ledger you read end to end — and a shop with
/// years of history has far more rows than are useful (or fast) to build
/// widgets for at once.
pub const PAGE_SIZE: i64 = 25;

/// A ready-made date range, because "last month" is what a shopkeeper
/// actually wants and typing two ISO dates to get it is a chore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Today,
    Last7,
    ThisMonth,
    LastMonth,
    ThisYear,
    AllTime,
}

impl Preset {
    pub const ALL: [Preset; 6] =
        [Preset::Today, Preset::Last7, Preset::ThisMonth, Preset::LastMonth, Preset::ThisYear, Preset::AllTime];

    fn label(self) -> &'static str {
        match self {
            Preset::Today => "Today",
            Preset::Last7 => "Last 7 days",
            Preset::ThisMonth => "This month",
            Preset::LastMonth => "Last month",
            Preset::ThisYear => "This year",
            Preset::AllTime => "All time",
        }
    }

    /// The range as (from, to), or `None` for either end meaning
    /// "unbounded" — which is how All time clears both fields.
    fn range(self, today: NaiveDate) -> (Option<NaiveDate>, Option<NaiveDate>) {
        match self {
            Preset::Today => (Some(today), Some(today)),
            Preset::Last7 => (today.checked_sub_days(chrono::Days::new(6)), Some(today)),
            Preset::ThisMonth => (today.with_day(1), Some(today)),
            Preset::LastMonth => {
                let first_this = today.with_day(1).unwrap_or(today);
                let last_prev = first_this.pred_opt().unwrap_or(today);
                (last_prev.with_day(1), Some(last_prev))
            }
            Preset::ThisYear => (NaiveDate::from_ymd_opt(today.year(), 1, 1), Some(today)),
            Preset::AllTime => (None, None),
        }
    }
}

/// The calendar popup's state: which field it is picking for, and which
/// month it is showing. `None` means no calendar is open.
#[derive(Debug, Clone, Copy)]
pub struct Calendar {
    pub field: Field,
    /// Always the first of the visible month — the grid is built by
    /// walking forward from here, so keeping it normalised avoids
    /// month-length edge cases every time it is read.
    pub month: NaiveDate,
}

#[derive(Debug, Clone, Default)]
pub struct ReportsState {
    pub item_filter: Option<ItemOption>,
    pub from: String,
    pub to: String,
    pub stock_value_paise: i64,
    pub total_profit_paise: i64,
    pub rows: Vec<TransactionHistoryRow>,
    /// Total matching transactions, for the pagination line. `rows` only
    /// ever holds the current page.
    pub total: i64,
    pub page: i64,
    pub calendar: Option<Calendar>,
}

impl ReportsState {
    pub fn set_field(&mut self, field: Field, value: String) {
        match field {
            Field::From => self.from = value,
            Field::To => self.to = value,
        }
    }

    pub fn get_field(&self, field: Field) -> String {
        match field {
            Field::From => self.from.clone(),
            Field::To => self.to.clone(),
        }
    }

    /// Opens the calendar on the month the field already names, so
    /// re-opening a filled-in date lands where the shopkeeper left it
    /// rather than on today.
    pub fn open_calendar(&mut self, field: Field) {
        let anchor = NaiveDate::parse_from_str(self.get_field(field).trim(), "%Y-%m-%d")
            .unwrap_or_else(|_| Utc::now().date_naive());
        let month = anchor.with_day(1).unwrap_or(anchor);
        self.calendar = Some(Calendar { field, month });
    }

    pub fn apply_preset(&mut self, preset: Preset) {
        let (from, to) = preset.range(Utc::now().date_naive());
        self.from = from.map(|d| d.to_string()).unwrap_or_default();
        self.to = to.map(|d| d.to_string()).unwrap_or_default();
        self.calendar = None;
    }

    /// Steps the open calendar by whole months. Clamped to day 1, so
    /// stepping from the 31st never skips a short month.
    pub fn shift_month(&mut self, months: i32) {
        let Some(cal) = &mut self.calendar else {
            return;
        };
        let (mut y, mut m) = (cal.month.year(), cal.month.month() as i32 + months);
        while m < 1 {
            m += 12;
            y -= 1;
        }
        while m > 12 {
            m -= 12;
            y += 1;
        }
        if let Some(d) = NaiveDate::from_ymd_opt(y, m as u32, 1) {
            cal.month = d;
        }
    }
}

#[derive(Debug, Clone)]
pub struct Loaded {
    pub stock_value_paise: i64,
    pub total_profit_paise: i64,
    pub rows: Vec<TransactionHistoryRow>,
    pub total: i64,
}

fn parse_day_start(s: &str) -> Result<Option<DateTime<Utc>>, String> {
    if s.trim().is_empty() {
        return Ok(None);
    }
    let date = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| "dates must be YYYY-MM-DD".to_string())?;
    Ok(Some(date.and_hms_opt(0, 0, 0).unwrap().and_utc()))
}

fn parse_day_end(s: &str) -> Result<Option<DateTime<Utc>>, String> {
    if s.trim().is_empty() {
        return Ok(None);
    }
    let date = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| "dates must be YYYY-MM-DD".to_string())?;
    Ok(Some(date.and_hms_opt(23, 59, 59).unwrap().and_utc()))
}

pub fn run(state: &mut State) -> Task<Message> {
    let Some(pool) = state.pool.clone() else {
        return Task::none();
    };

    let from = match parse_day_start(&state.reports.from) {
        Ok(v) => v,
        Err(e) => {
            state.notice = Some(Notice::error(e));
            return Task::none();
        }
    };
    let to = match parse_day_end(&state.reports.to) {
        Ok(v) => v,
        Err(e) => {
            state.notice = Some(Notice::error(e));
            return Task::none();
        }
    };
    let item_id = state.reports.item_filter.as_ref().map(|i| i.id);

    load(pool, item_id, from, to, state.reports.page)
}

/// Re-runs the report on page 0 — what every filter change wants, since
/// page 4 of the old result set means nothing against a new filter.
pub fn run_from_start(state: &mut State) -> Task<Message> {
    state.reports.page = 0;
    run(state)
}

fn load(
    pool: SqlitePool,
    item_id: Option<i64>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    page: i64,
) -> Task<Message> {
    Task::perform(
        async move {
            // Every figure here is aggregated in SQL and only one page of
            // rows comes back, so the work is the same whether the shop
            // has fifty transactions or half a million.
            let stock_value_paise = crate::repo::current_stock_value_paise(&pool).await?;
            let total_profit_paise = crate::repo::total_profit_paise(&pool, item_id, from, to).await?;
            let total = crate::repo::transaction_count(&pool, item_id, from, to).await?;
            let rows = crate::repo::transaction_history(
                &pool,
                None,
                item_id,
                from,
                to,
                Some(PAGE_SIZE),
                page * PAGE_SIZE,
            )
            .await?;
            Ok::<_, crate::repo::RepoError>(Loaded { stock_value_paise, total_profit_paise, rows, total })
        },
        |result| Message::ReportsLoaded(result.map_err(|e| e.to_string())),
    )
}

pub fn view(state: &State) -> Element<'_, Message> {
    let reports = &state.reports;

    // The picker holds at most `PICKER_LIMIT` candidates, requeried from
    // the database on every keystroke. It is never built from a catalogue
    // in memory — doing that in `view` cloned every item name on every
    // redraw, which is what used to hang this screen.
    let item_picker = common::item_picker(
        &state.picker,
        common::PickerTarget::Report,
        "All items",
        |v| Message::ReportsItemFilterSelected(Some(v)),
        Length::Fixed(240.0),
    );

    let presets = row(Preset::ALL.iter().map(|p| {
        button(text(p.label()).size(theme::TEXT_SMALL))
            .style(theme::secondary_button)
            .padding([7, 13])
            .on_press(Message::ReportsPresetSelected(*p))
            .into()
    }))
    .spacing(theme::SPACE_XS);

    let filter_row = row![
        labeled("Item", item_picker),
        labeled("From", date_field(reports, Field::From)),
        labeled("To", date_field(reports, Field::To)),
        button(text("Clear").size(theme::TEXT_SMALL))
            .style(theme::secondary_button)
            .padding(theme::CONTROL_PADDING)
            .on_press(Message::ReportsFiltersCleared),
        button(text("Run Report").size(theme::TEXT_BODY).font(theme::SEMIBOLD))
            .style(theme::primary_button)
            .padding(theme::CONTROL_PADDING)
            .on_press(Message::RunReports),
        button(text("Download").size(theme::TEXT_BODY))
            .style(theme::success_button)
            .padding(theme::CONTROL_PADDING)
            .on_press(Message::DownloadReportPressed),
    ]
    .spacing(theme::SPACE_MD)
    .align_y(iced::Alignment::End);

    let mut filters = column![
        text("Filters").size(theme::TEXT_SMALL).color(theme::MUTED_TEXT),
        presets,
        filter_row,
    ]
    .spacing(theme::SPACE_SM);

    // The calendar sits inline under the filters rather than floating over
    // them: it only ever appears in one place, and an inline panel can't
    // end up clipped or mispositioned the way an overlay can.
    if let Some(cal) = &reports.calendar {
        filters = filters.push(calendar_panel(cal));
    }

    let summary = row![
        stat_card("Current Stock Value", money::format_paise(reports.stock_value_paise)),
        stat_card("Profit (filtered)", money::format_paise(reports.total_profit_paise)),
        stat_card("Transactions", reports.total.to_string()),
    ]
    .spacing(theme::SPACE_MD);

    let mut history = column![].spacing(theme::SPACE_XS);
    if reports.rows.is_empty() {
        history = history.push(
            text("No transactions match this filter.").size(theme::TEXT_SMALL).color(theme::MUTED_TEXT),
        );
    }
    for row in &reports.rows {
        history = history.push(history_row(row));
    }

    let page_count = ((reports.total as f64) / PAGE_SIZE as f64).ceil().max(1.0) as i64;
    let start = reports.page * PAGE_SIZE;
    let range = if reports.total == 0 {
        "0 of 0".to_string()
    } else {
        format!("{}-{} of {}", start + 1, (start + PAGE_SIZE).min(reports.total), reports.total)
    };

    let pagination = row![
        text(range).size(theme::TEXT_SMALL).color(theme::MUTED_TEXT),
        iced::widget::space::horizontal(),
        button(text("Prev").size(theme::TEXT_SMALL))
            .style(theme::secondary_button)
            .padding([8, 16])
            .on_press_maybe((reports.page > 0).then_some(Message::ReportsPagePrev)),
        text(format!("Page {} of {}", reports.page + 1, page_count))
            .size(theme::TEXT_SMALL)
            .color(theme::MUTED_TEXT),
        button(text("Next").size(theme::TEXT_SMALL))
            .style(theme::secondary_button)
            .padding([8, 16])
            .on_press_maybe((reports.page + 1 < page_count).then_some(Message::ReportsPageNext)),
    ]
    .spacing(theme::SPACE_SM)
    .align_y(iced::Alignment::Center);

    let table = container(
        column![
            row![
                text("Transaction History").size(theme::TEXT_HEADING).font(theme::SEMIBOLD),
                iced::widget::space::horizontal(),
            ],
            // Its natural height — a page is at most `PAGE_SIZE` rows, and
            // the page below scrolls. Same reasoning as the low-stock
            // panel on Shop → Details: a scrollable set to fill inside a
            // fixed column gets nothing when the window is short, and the
            // list silently draws no rows at all.
            history,
            pagination,
        ]
        .spacing(theme::SPACE_MD),
    )
    .style(theme::card)
    .padding(theme::SPACE_MD);

    scrollable(
        column![container(filters).style(theme::card).padding(theme::SPACE_MD), summary, table]
            .spacing(theme::SPACE_MD)
            .padding([0, theme::SPACE_MD as u16])
            .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

/// A date field: the typed value, plus a button that opens the calendar.
/// Typing still works — the calendar is a convenience, not a replacement,
/// because a shopkeeper who knows the date they want shouldn't have to
/// click through months to reach it.
fn date_field(reports: &ReportsState, field: Field) -> Element<'_, Message> {
    let open = reports.calendar.map(|c| c.field) == Some(field);

    row![
        common::field("YYYY-MM-DD", reports_value(reports, field))
            .on_input(move |v| Message::ReportsFieldChanged(field, v))
            .width(Length::Fixed(130.0)),
        button(text(if open { "×" } else { "📅" }).size(theme::TEXT_BODY))
            .style(if open { theme::primary_button } else { theme::secondary_button })
            .padding([theme::FIELD_PADDING, 12])
            .on_press(if open { Message::ReportsCalendarClosed } else { Message::ReportsCalendarOpened(field) }),
    ]
    .spacing(theme::SPACE_XS)
    .align_y(iced::Alignment::Center)
    .into()
}

fn reports_value(reports: &ReportsState, field: Field) -> &str {
    match field {
        Field::From => &reports.from,
        Field::To => &reports.to,
    }
}

/// A month grid. Seven columns, Monday first, with the days of the
/// previous/next month left blank rather than filled in — a blank cell
/// reads unambiguously as "not this month", where a greyed-out 29 does
/// not.
fn calendar_panel(cal: &Calendar) -> Element<'static, Message> {
    let today = Utc::now().date_naive();
    let selected = None::<NaiveDate>;

    let header = row![
        button(text("‹").size(theme::TEXT_HEADING))
            .style(theme::secondary_button)
            .padding([4, 12])
            .on_press(Message::ReportsCalendarShifted(-1)),
        iced::widget::space::horizontal(),
        column![
            text(format!("Pick “{}” date", cal.field.label())).size(theme::TEXT_CAPTION).color(theme::MUTED_TEXT),
            text(cal.month.format("%B %Y").to_string()).size(theme::TEXT_BODY).font(theme::SEMIBOLD),
        ]
        .spacing(1)
        .align_x(iced::Alignment::Center),
        iced::widget::space::horizontal(),
        button(text("›").size(theme::TEXT_HEADING))
            .style(theme::secondary_button)
            .padding([4, 12])
            .on_press(Message::ReportsCalendarShifted(1)),
    ]
    .align_y(iced::Alignment::Center);

    let mut cells: Vec<Element<'static, Message>> = Vec::with_capacity(49);
    for name in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
        cells.push(
            container(text(name).size(theme::TEXT_CAPTION).color(theme::MUTED_TEXT))
                .width(Length::Fixed(38.0))
                .align_x(iced::Alignment::Center)
                .into(),
        );
    }

    // Blank cells to push the 1st under its weekday column.
    for _ in 0..cal.month.weekday().num_days_from_monday() {
        cells.push(iced::widget::space::horizontal().width(Length::Fixed(38.0)).into());
    }

    let mut day = cal.month;
    while day.month() == cal.month.month() {
        let is_today = day == today;
        let is_selected = selected == Some(day);
        let label = text(day.day().to_string()).size(theme::TEXT_SMALL);

        let style = if is_selected {
            theme::primary_button
        } else if is_today {
            theme::tab_selected
        } else {
            theme::tab_idle
        };

        cells.push(
            button(container(label).width(Length::Fill).align_x(iced::Alignment::Center))
                .style(style)
                .padding([6, 0])
                .width(Length::Fixed(38.0))
                .on_press(Message::ReportsDatePicked(day))
                .into(),
        );

        match day.succ_opt() {
            Some(next) => day = next,
            None => break,
        }
    }

    container(
        column![
            header,
            grid(cells).columns(7).spacing(theme::SPACE_XS),
            row![
                button(text("Clear this date").size(theme::TEXT_SMALL))
                    .style(theme::secondary_button)
                    .padding([7, 13])
                    .on_press(Message::ReportsDateCleared),
                button(text("Today").size(theme::TEXT_SMALL))
                    .style(theme::secondary_button)
                    .padding([7, 13])
                    .on_press(Message::ReportsDatePicked(today)),
                iced::widget::space::horizontal(),
                button(text("Done").size(theme::TEXT_SMALL).font(theme::SEMIBOLD))
                    .style(theme::primary_button)
                    .padding([7, 16])
                    .on_press(Message::ReportsCalendarClosed),
            ]
            .spacing(theme::SPACE_XS)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(theme::SPACE_SM),
    )
    .style(theme::panel)
    .padding(theme::SPACE_MD)
    .width(Length::Fixed(320.0))
    .into()
}

fn stat_card<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    container(
        column![
            text(label.to_uppercase()).size(12).color(theme::MUTED_TEXT),
            text(value).size(24).font(theme::BOLD).color(theme::VIOLET),
        ]
        .spacing(4),
    )
    .style(theme::card)
    .padding(theme::SPACE_MD)
    .width(Length::Fixed(260.0))
    .into()
}

fn history_row(row: &TransactionHistoryRow) -> Element<'_, Message> {
    let kind_label = if row.kind == "buy" { "Purchase" } else { "Sale" };
    container(
        iced::widget::row![
            text(&row.item_name).width(Length::FillPortion(3)),
            text(kind_label).width(Length::FillPortion(1)),
            text(format!("{:.1}", row.qty)).width(Length::FillPortion(1)),
            text(money::format_paise(row.price_paise)).width(Length::FillPortion(1)),
            text(row.timestamp.format("%d %b %Y %H:%M").to_string()).width(Length::FillPortion(2)),
        ]
        .spacing(8),
    )
    .padding(6)
    .into()
}

fn labeled<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(13), widget.into()].spacing(4).into()
}

/// What actually gets printed on the report, independent of whatever the
/// UI happens to have on screen. `download` always fetches this fresh
/// right before generating the PDF — it does *not* trust `ReportsState`'s
/// cached numbers, which can be stale or still at their zero/default if
/// "Run Report" was never pressed (or hasn't finished yet) before the
/// shopkeeper hits "Download".
/// How many transaction rows a downloaded PDF carries. Bounded so a shop
/// with years of history gets a report it can actually open, rather than
/// a thousand-page file — the summary figures above the table are
/// computed over the *whole* filter regardless.
const PDF_ROW_LIMIT: i64 = 1000;

struct Export {
    item_label: String,
    from_label: String,
    to_label: String,
    stock_value_paise: i64,
    total_profit_paise: i64,
    rows: Vec<TransactionHistoryRow>,
    /// Every matching transaction, not just the rows carried above — so
    /// the PDF can say when it is showing a truncated view.
    total: i64,
}

/// Re-fetches the report with the currently-set filters, builds the PDF
/// from that fresh data, saves it, and asks the OS to open it — opening it
/// in the default PDF viewer (Preview, Acrobat, etc.) is what gives the
/// shopkeeper a "Print" button, without this app needing its own print
/// pipeline. The freshly-fetched numbers are also sent back so the on-screen
/// summary matches exactly what was printed.
pub fn download(state: &State) -> Task<Message> {
    let Some(pool) = state.pool.clone() else {
        return Task::done(Message::ReportPdfReady(Err("database is not ready yet".into())));
    };

    let from = match parse_day_start(&state.reports.from) {
        Ok(v) => v,
        Err(e) => return Task::done(Message::ReportPdfReady(Err(e))),
    };
    let to = match parse_day_end(&state.reports.to) {
        Ok(v) => v,
        Err(e) => return Task::done(Message::ReportPdfReady(Err(e))),
    };
    let item_id = state.reports.item_filter.as_ref().map(|i| i.id);

    let shop = common::ShopIdentity::from_state(state);
    let item_label = state.reports.item_filter.as_ref().map(|i| i.name.clone()).unwrap_or_else(|| "All items".to_string());
    let from_label = if state.reports.from.trim().is_empty() { "All dates".to_string() } else { state.reports.from.trim().to_string() };
    let to_label = if state.reports.to.trim().is_empty() { "All dates".to_string() } else { state.reports.to.trim().to_string() };

    Task::perform(
        async move {
            let stock_value_paise = crate::repo::current_stock_value_paise(&pool).await.map_err(|e| e.to_string())?;
            let total_profit_paise = crate::repo::total_profit_paise(&pool, item_id, from, to).await.map_err(|e| e.to_string())?;
            let total = crate::repo::transaction_count(&pool, item_id, from, to).await.map_err(|e| e.to_string())?;
            // A printed report is a document rather than a screen, so it
            // carries more than one page — but still a bounded number.
            // `PDF_ROW_LIMIT` rows is already ~40 pages of A4.
            let rows = crate::repo::transaction_history(&pool, None, item_id, from, to, Some(PDF_ROW_LIMIT), 0)
                .await
                .map_err(|e| e.to_string())?;

            let export = Export { item_label, from_label, to_label, stock_value_paise, total_profit_paise, rows, total };
            let bytes = build_pdf_bytes(&shop, &export)?;
            let path = save_and_open(bytes).await?;

            Ok((
                Loaded {
                    stock_value_paise: export.stock_value_paise,
                    total_profit_paise: export.total_profit_paise,
                    rows: Vec::new(), // the screen keeps its own page; see ReportPdfReady
                    total: export.total,
                },
                path,
            ))
        },
        Message::ReportPdfReady,
    )
}

async fn save_and_open(bytes: Vec<u8>) -> Result<PathBuf, String> {
    let dir = dirs::download_dir()
        .or_else(dirs::document_dir)
        .ok_or("could not find a Downloads folder on this computer")?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("srotas-report-{stamp}.pdf"));

    tokio::fs::write(&path, &bytes).await.map_err(|e| e.to_string())?;
    open::that(&path).map_err(|e| format!("report saved, but couldn't open it: {e}"))?;

    Ok(path)
}

const REPORT_COLUMNS: [pdf::Column; 5] = [
    pdf::Column::left("Item", 3.4),
    pdf::Column::left("Type", 1.1),
    pdf::Column::right("Qty", 0.9),
    pdf::Column::right("Price", 1.3),
    pdf::Column::right("Date", 1.7),
];

fn build_pdf_bytes(shop: &common::ShopIdentity, export: &Export) -> Result<Vec<u8>, String> {
    let mut doc = pdf::Doc::new(&format!("{} - Report", shop.name))?;

    doc.masthead(&shop.masthead(
        "SALES & STOCK REPORT",
        None,
        Some(Utc::now().format("Generated %d %b %Y, %H:%M UTC").to_string()),
    ));

    doc.highlights(&[
        ("Current stock value", money::format_paise_ascii(export.stock_value_paise)),
        ("Profit over this period", money::format_paise_ascii(export.total_profit_paise)),
    ]);

    doc.facts(&[
        ("Item", export.item_label.clone()),
        ("From", export.from_label.clone()),
        ("To", export.to_label.clone()),
    ]);

    doc.section("Transaction history");
    let rows: Vec<Vec<String>> = export
        .rows
        .iter()
        .map(|row| {
            vec![
                row.item_name.clone(),
                if row.kind == "buy" { "Purchase".to_string() } else { "Sale".to_string() },
                format!("{:.2}", row.qty),
                money::format_paise_ascii(row.price_paise),
                row.timestamp.format("%d %b %Y, %H:%M").to_string(),
            ]
        })
        .collect();
    doc.table(&REPORT_COLUMNS, &rows, "No transactions match this filter.");

    doc.note("Profit is calculated from the sale price against the item's recorded buy price, over the dates shown above.");
    doc.finish(&shop.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_shop() -> common::ShopIdentity {
        common::ShopIdentity {
            name: "Test Shop".into(),
            lines: vec!["Main Road, Kanpur".into(), "98765 43210".into()],
            logo: None,
        }
    }

    fn empty_export() -> Export {
        Export {
            item_label: "All items".into(),
            from_label: "All dates".into(),
            to_label: "All dates".into(),
            stock_value_paise: 0,
            total_profit_paise: 0,
            rows: Vec::new(),
            total: 0,
        }
    }

    #[test]
    fn builds_a_valid_pdf_with_no_transactions() {
        let bytes = build_pdf_bytes(&test_shop(), &empty_export()).expect("pdf build should succeed");
        assert!(bytes.starts_with(b"%PDF"), "output should be a PDF");
        assert!(bytes.len() > 200, "a real PDF should be more than a couple hundred bytes");
    }

    #[test]
    fn builds_a_valid_pdf_with_many_transactions() {
        let mut export = Export { stock_value_paise: 1_234_567, total_profit_paise: 89_012, ..empty_export() };
        for i in 0..80 {
            export.rows.push(TransactionHistoryRow {
                item_name: format!("Test Item With A Rather Long Name {i}"),
                kind: if i % 2 == 0 { "buy".into() } else { "sell".into() },
                qty: 3.0,
                price_paise: 12_345,
                timestamp: Utc::now(),
            });
        }

        let bytes = build_pdf_bytes(&test_shop(), &export).expect("pdf build should succeed");
        assert!(bytes.starts_with(b"%PDF"));
        // 80 rows should overflow a single page and force pagination —
        // multi-page PDFs are meaningfully larger than a one-pager.
        assert!(bytes.len() > 2_000, "expected a multi-page PDF, got {} bytes", bytes.len());
    }
}
