mod backup;
mod items;
mod reports;
mod shop;
mod transactions;

pub use backup::*;
pub use items::*;
pub use reports::*;
pub use shop::*;
pub use transactions::*;

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error("item not found")]
    ItemNotFound,
    #[error("quantity must be positive")]
    InvalidQty,
    #[error("insufficient stock: have {available}, need {requested}")]
    InsufficientStock { available: f64, requested: f64 },
    #[error("an item named \"{name}\" already exists")]
    DuplicateItemName { name: String },
}
