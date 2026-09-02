use iced::widget::{button, column, container, row, text};
use iced::{Element, Length, Task};
use sqlx::SqlitePool;
use std::path::PathBuf;

use super::{Message, State};
use crate::ui::theme;

pub fn choose_folder() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .set_title("Choose a backup folder (pendrive / synced folder)")
                .pick_folder()
                .await
                .map(|handle| handle.path().to_path_buf())
        },
        Message::BackupFolderChosen,
    )
}

fn backup_task(pool: SqlitePool, folder: PathBuf) -> Task<Message> {
    Task::perform(
        async move {
            let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            let dest = folder.join(format!("shop-backup-{stamp}.db"));
            crate::repo::backup_to(&pool, &dest).await.map(|_| dest).map_err(|e| e.to_string())
        },
        Message::BackupCompleted,
    )
}

pub fn backup_now(state: &State) -> Task<Message> {
    let (Some(pool), Some(folder)) = (state.pool.clone(), state.settings.backup_folder.clone()) else {
        return Task::none();
    };
    backup_task(pool, folder)
}

/// Called once after login/registration: if a backup folder is remembered
/// and today doesn't already have a backup, silently run one — this is the
/// "automatic daily" half of the backup requirement. It reuses the same
/// task as the manual button, so a successful auto-backup updates the
/// status bar exactly like a manual one would.
pub fn maybe_auto_backup(state: &State) -> Task<Message> {
    let today = chrono::Utc::now().date_naive();
    if state.settings.last_backup_date == Some(today) {
        return Task::none();
    }
    backup_now(state)
}

pub fn view(state: &State) -> Element<'_, Message> {
    let folder_label = state
        .settings
        .backup_folder
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "No folder chosen yet".to_string());

    let last_backup_label = state
        .settings
        .last_backup_date
        .map(|d| format!("Last backup: {}", d.format("%d %b %Y")))
        .unwrap_or_else(|| "No backup taken yet".to_string());

    let body = column![
        column![
            text("Backup").size(theme::TEXT_TITLE).font(theme::SEMIBOLD),
            text(
                "Copy the shop database to a pendrive or a folder that syncs to Google Drive. \
                 This is the only thing standing between you and losing every record if this \
                 computer's disk fails."
            )
            .size(theme::TEXT_SMALL)
            .color(theme::MUTED_TEXT),
        ]
        .spacing(theme::SPACE_XS),
        container(
            column![
                text("Backup folder").size(theme::TEXT_CAPTION).color(theme::MUTED_TEXT),
                text(folder_label).size(theme::TEXT_BODY),
                text(last_backup_label).size(theme::TEXT_SMALL).color(theme::MUTED_TEXT),
            ]
            .spacing(theme::SPACE_XS)
        )
        .style(theme::panel)
        .padding(theme::SPACE_MD)
        .width(Length::Fill),
        row![
            button(text("Choose Folder").size(theme::TEXT_BODY))
                .style(theme::secondary_button)
                .padding(theme::CONTROL_PADDING)
                .on_press(Message::ChooseBackupFolder),
            button(text("Backup Now").size(theme::TEXT_BODY).font(theme::SEMIBOLD))
                .style(theme::primary_button)
                .padding(theme::CONTROL_PADDING)
                .on_press(Message::BackupNowPressed),
        ]
        .spacing(theme::SPACE_SM),
        text("Once a folder is chosen, a backup also runs by itself the first time you open the app each day.")
            .size(theme::TEXT_CAPTION)
            .color(theme::MUTED_TEXT),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(560);

    container(container(body).style(theme::card).padding(theme::SPACE_LG))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([0, theme::SPACE_MD as u16])
        .into()
}
