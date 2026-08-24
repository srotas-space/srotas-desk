use crate::models::Item;

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
