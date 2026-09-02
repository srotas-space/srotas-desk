use chrono::{DateTime, NaiveDate, Utc};
use iced::widget::{button, column, container, pick_list, row, scrollable, text};
use iced::{Element, Length, Task};
use sqlx::SqlitePool;
use std::path::PathBuf;

use super::{Message, Notice, State};
use crate::money;
use crate::repo::TransactionHistoryRow;
use crate::ui::common::{item_options, ItemOption};
use crate::pdf;
use crate::ui::{common, theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    From,
    To,
}

#[derive(Debug, Clone, Default)]
pub struct ReportsState {
    pub item_filter: Option<ItemOption>,
    pub from: String,
    pub to: String,
    pub stock_value_paise: i64,
    pub total_profit_paise: i64,
    pub rows: Vec<TransactionHistoryRow>,
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
}

#[derive(Debug, Clone)]
pub struct Loaded {
    pub stock_value_paise: i64,
    pub total_profit_paise: i64,
    pub rows: Vec<TransactionHistoryRow>,
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

    load(pool, item_id, from, to)
}

fn load(
    pool: SqlitePool,
    item_id: Option<i64>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Task<Message> {
    Task::perform(
        async move {
            let stock_value_paise = crate::repo::current_stock_value_paise(&pool).await?;
            let total_profit_paise = crate::repo::total_profit_paise(&pool, item_id, from, to).await?;
            let rows = crate::repo::transaction_history(&pool, None, item_id, from, to, Some(200)).await?;
            Ok::<_, crate::repo::RepoError>(Loaded { stock_value_paise, total_profit_paise, rows })
        },
        |result| Message::ReportsLoaded(result.map_err(|e| e.to_string())),
    )
}

pub fn view(state: &State) -> Element<'_, Message> {
    let reports = &state.reports;
    let options = item_options(&state.items);

    let filters = column![
        text("Filters").size(14).color(theme::MUTED_TEXT),
        row![
            labeled(
                "Item (blank = all)",
                pick_list(options, reports.item_filter.clone(), |v| Message::ReportsItemFilterSelected(Some(v)))
                    .padding(10)
                    .width(Length::Fixed(220.0)),
            ),
            labeled("From (YYYY-MM-DD)", common::field("2026-08-01", &reports.from).on_input(|v| Message::ReportsFieldChanged(Field::From, v)).width(Length::Fixed(160.0))),
            labeled("To (YYYY-MM-DD)", common::field("2026-08-31", &reports.to).on_input(|v| Message::ReportsFieldChanged(Field::To, v)).width(Length::Fixed(160.0))),
            button(text("Clear item").size(13)).style(theme::secondary_button).padding([10, 16]).on_press(Message::ReportsItemFilterSelected(None)),
            button(text("Run Report").size(15)).style(theme::primary_button).padding([10, 20]).on_press(Message::RunReports),
            button(text("Download").size(15)).style(theme::success_button).padding([10, 20]).on_press(Message::DownloadReportPressed),
        ]
        .spacing(theme::SPACE_MD)
        .align_y(iced::Alignment::End),
    ]
    .spacing(theme::SPACE_SM);

    let summary = row![
        stat_card("Current Stock Value", money::format_paise(reports.stock_value_paise)),
        stat_card("Total Profit (filtered)", money::format_paise(reports.total_profit_paise)),
    ]
    .spacing(theme::SPACE_MD);

    let mut history = column![text("Transaction History").size(16)].spacing(6);
    if reports.rows.is_empty() {
        history = history.push(text("No transactions match this filter.").size(13));
    }
    for row in &reports.rows {
        history = history.push(history_row(row));
    }

    container(
        column![
            container(filters).style(theme::card).padding(theme::SPACE_MD),
            summary,
            scrollable(container(history).style(theme::card).padding(theme::SPACE_MD)).height(Length::Fill),
        ]
        .spacing(theme::SPACE_MD)
        .padding(theme::SPACE_MD),
    )
    .width(Length::Fill)
    .height(Length::Fill)
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
struct Export {
    item_label: String,
    from_label: String,
    to_label: String,
    stock_value_paise: i64,
    total_profit_paise: i64,
    rows: Vec<TransactionHistoryRow>,
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
            let rows = crate::repo::transaction_history(&pool, None, item_id, from, to, Some(200)).await.map_err(|e| e.to_string())?;

            let export = Export { item_label, from_label, to_label, stock_value_paise, total_profit_paise, rows };
            let bytes = build_pdf_bytes(&shop, &export)?;
            let path = save_and_open(bytes).await?;

            Ok((
                Loaded { stock_value_paise: export.stock_value_paise, total_profit_paise: export.total_profit_paise, rows: export.rows },
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
