use iced::widget::{button, column, container, scrollable, text, text_input, TextInput};
use iced::{Element, Length};

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

/// Which screen's item picker a message is about. All three pickers share
/// one query and one set of candidates — they are the same question
/// ("which item?") — but they hand the answer to different places.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerTarget {
    Sale,
    Bill,
    Report,
}

/// What the shopkeeper has typed into an item picker, and the candidates
/// that came back for it.
#[derive(Debug, Default)]
pub struct PickerState {
    /// The text in the field. Held here rather than inside a `combo_box`,
    /// which owns its own text and filters its own fixed option list —
    /// the wrong shape for a search that goes to the database.
    pub query: String,
    /// At most `PICKER_LIMIT` candidates, replaced on every keystroke.
    pub options: Vec<ItemOption>,
    /// Which picker the query belongs to, so one screen's typing doesn't
    /// open another's results list.
    pub target: Option<PickerTarget>,
}

impl PickerState {
    /// Records a keystroke against one picker.
    pub fn typed(&mut self, target: PickerTarget, query: String) {
        self.target = Some(target);
        self.query = query;
    }

    /// Called when an item is chosen: the field shows its name and the
    /// results list closes.
    pub fn chose(&mut self, name: String) {
        self.query = name;
        self.target = None;
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.options.clear();
        self.target = None;
    }
}

/// An item picker: a search field, and the matching items beneath it.
///
/// Results come from the database — twenty at a time, requeried on every
/// keystroke — so this never holds a catalogue. The list shows only while
/// `state.target` names this picker, which is set by typing and cleared
/// by choosing, so picking an item closes it without needing to track
/// focus.
pub fn item_picker<'a>(
    state: &'a PickerState,
    target: PickerTarget,
    placeholder: &'a str,
    on_select: fn(ItemOption) -> Message,
    width: Length,
) -> Element<'a, Message> {
    let field = text_input(placeholder, &state.query)
        .on_input(move |v| Message::PickerInputChanged(target, v))
        .style(theme::field)
        .padding(theme::FIELD_PADDING)
        .size(theme::TEXT_BODY)
        .width(width);

    if state.target != Some(target) {
        return field.into();
    }

    let mut results = column![].spacing(1);
    if state.options.is_empty() {
        results = results.push(
            container(
                text(if state.query.trim().is_empty() {
                    "Type to search items"
                } else {
                    "No items match"
                })
                .size(theme::TEXT_SMALL)
                .color(theme::MUTED_TEXT),
            )
            .padding(theme::SPACE_SM),
        );
    }
    for option in &state.options {
        results = results.push(
            button(text(&option.name).size(theme::TEXT_SMALL))
                .style(theme::tab_idle)
                .padding([7, 10])
                .width(Length::Fill)
                .on_press(on_select(option.clone())),
        );
    }

    column![
        field,
        container(scrollable(results).height(Length::Shrink))
            .style(theme::card)
            .padding(theme::SPACE_XS)
            .max_height(220)
            .width(width),
    ]
    .spacing(theme::SPACE_XS)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(id: i64, name: &str) -> ItemOption {
        ItemOption { id, name: name.to_string() }
    }

    #[test]
    fn typing_is_remembered() {
        let mut picker = PickerState::default();
        picker.typed(PickerTarget::Sale, "wire".into());

        assert_eq!(picker.query, "wire");
        assert_eq!(picker.target, Some(PickerTarget::Sale));
    }

    /// The bug this component exists to fix: the old `combo_box` owned its
    /// own text, and every keystroke replaced the whole widget state to
    /// install fresh candidates — which reset that text to empty. Typing
    /// "wire" put a "w" in the box, fired a query, and the reply wiped it.
    /// Here the query lives outside the results, so new candidates can
    /// never disturb what was typed.
    #[test]
    fn arriving_results_never_disturb_what_was_typed() {
        let mut picker = PickerState::default();
        picker.typed(PickerTarget::Sale, "wire".into());

        picker.options = vec![option(1, "Copper Wire 2.5 sqmm"), option(2, "Aluminium Wire 1.0 sqmm")];

        assert_eq!(picker.query, "wire", "results must not clear the field");
        assert_eq!(picker.options.len(), 2);
    }

    #[test]
    fn choosing_an_item_shows_its_name_and_closes_the_list() {
        let mut picker = PickerState::default();
        picker.typed(PickerTarget::Sale, "wir".into());
        picker.options = vec![option(1, "Copper Wire 2.5 sqmm")];

        picker.chose("Copper Wire 2.5 sqmm".into());

        assert_eq!(picker.query, "Copper Wire 2.5 sqmm");
        assert_eq!(picker.target, None, "no target means the results list is hidden");
    }

    #[test]
    fn one_picker_typing_does_not_open_anothers_list() {
        let mut picker = PickerState::default();
        picker.typed(PickerTarget::Bill, "bolt".into());

        // The three screens share one query and one result set, so the
        // target is what stops Billings' typing from dropping a list under
        // the Sales field.
        assert_eq!(picker.target, Some(PickerTarget::Bill));
        assert_ne!(picker.target, Some(PickerTarget::Sale));
        assert_ne!(picker.target, Some(PickerTarget::Report));
    }

    #[test]
    fn clearing_resets_everything() {
        let mut picker = PickerState::default();
        picker.typed(PickerTarget::Report, "cement".into());
        picker.options = vec![option(1, "Cement Bag 50kg")];

        picker.clear();

        assert!(picker.query.is_empty());
        assert!(picker.options.is_empty());
        assert_eq!(picker.target, None);
    }
}
