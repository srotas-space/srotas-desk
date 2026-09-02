use iced::widget::{text_input, TextInput};

use super::Message;
use crate::models::Item;
use crate::ui::theme;

/// `pick_list` needs its options to be plain `Clone + PartialEq + Display`
/// values it can copy around and compare — an `&Item` reference doesn't fit,
/// so this is the small stand-in used by the Purchases/Sales item pickers.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemOption {
    pub id: i64,
    pub name: String,
}

impl std::fmt::Display for ItemOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

pub fn item_options(items: &[Item]) -> Vec<ItemOption> {
    items.iter().map(|i| ItemOption { id: i.id, name: i.name.clone() }).collect()
}

/// A text field wearing the app's field style. Screens build their inputs
/// through this rather than calling `text_input` directly, so the focus
/// ring, radius and padding are the same in every form.
pub fn field<'a>(placeholder: &'a str, value: &'a str) -> TextInput<'a, Message> {
    text_input(placeholder, value).style(theme::field).padding(theme::FIELD_PADDING).size(theme::TEXT_BODY)
}

/// Everything the printed masthead needs about the shop, snapshotted out
/// of `State` so an export task owns its data rather than borrowing the
/// screen it was launched from.
#[derive(Debug, Clone)]
pub struct ShopIdentity {
    pub name: String,
    /// Address, phone and GSTIN, minus whatever the shop left blank.
    pub lines: Vec<String>,
    pub logo: Option<Vec<u8>>,
}

impl ShopIdentity {
    pub fn from_state(state: &super::State) -> Self {
        let Some(shop) = &state.shop else {
            return ShopIdentity { name: "Srotas Desk".to_string(), lines: Vec::new(), logo: None };
        };

        let mut lines = Vec::new();
        for value in [shop.address.trim(), shop.phone.trim()] {
            if !value.is_empty() {
                lines.push(value.to_string());
            }
        }
        if let Some(gstin) = shop.gstin.as_deref().map(str::trim).filter(|g| !g.is_empty()) {
            lines.push(format!("GSTIN: {gstin}"));
        }

        ShopIdentity { name: shop.shop_name.clone(), lines, logo: state.shop_logo.clone() }
    }

    /// The masthead this identity produces, given what kind of document is
    /// being printed.
    pub fn masthead<'a>(
        &'a self,
        doc_label: &'a str,
        doc_ref: Option<String>,
        doc_date: Option<String>,
    ) -> crate::pdf::Masthead<'a> {
        crate::pdf::Masthead {
            shop_name: &self.name,
            lines: self.lines.clone(),
            logo: self.logo.as_deref(),
            doc_label,
            doc_ref,
            doc_date,
        }
    }
}
