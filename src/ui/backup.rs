use iced::widget::{button, column, container, text};
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
        text("Backup").size(22),
        text("Copy the shop database to a pendrive or a folder that syncs to Google Drive. \
              Do this regularly — this is the only protection against losing all data if this computer's disk fails.").size(13),
        container(column![text("Backup folder").size(13), text(folder_label).size(15)].spacing(4))
            .style(theme::card)
            .padding(theme::SPACE_MD),
        iced::widget::row![
            button(text("Choose Folder").size(15)).style(theme::secondary_button).padding([10, 20]).on_press(Message::ChooseBackupFolder),
            button(text("Backup Now").size(15)).style(theme::primary_button).padding([10, 20]).on_press(Message::BackupNowPressed),
        ]
        .spacing(theme::SPACE_MD),
        text(last_backup_label).size(13),
        text("A backup also runs automatically once a day, the first time you open the app, once a folder is chosen.").size(12),
    ]
    .spacing(theme::SPACE_MD)
    .max_width(560);

    container(container(body).style(theme::card).padding(theme::SPACE_LG))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(theme::SPACE_MD)
        .into()
}
