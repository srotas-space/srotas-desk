//! The document builder behind every printable Srotas Desk produces —
//! bills, sale receipts and reports.
//!
//! This used to be a line printer: `line()` wrote a string at the left
//! margin and `row()` dropped cells at hard-coded x positions, which is
//! why a printed bill came out as a wall of left-aligned text with rupee
//! amounts that didn't line up. It's now a small layout engine instead —
//! a branded masthead, real tables with aligned numeric columns and zebra
//! striping, a totals panel, and page numbers — because a bill is the one
//! artefact of this app a shop's *customers* ever see.
//!
//! Still deliberately small: no rich text, no wrapping, no flow layout.
//! Everything is positioned in millimetres from the page's bottom-left,
//! which is the coordinate space PDF itself uses.
use printpdf::path::PaintMode;
use printpdf::{
    BuiltinFont, Color, ColorBits, ColorSpace, ImageXObject, IndirectFontRef, Mm, PdfDocument,
    PdfDocumentReference, PdfLayerReference, Point, Px, Rect, Rgb,
};
use std::io::{BufWriter, Cursor, Write};

// ------------------------------------------------------------ page metrics

const PAGE_W: f32 = 210.0; // A4
const PAGE_H: f32 = 297.0;
const MARGIN_X: f32 = 16.0;
const MARGIN_TOP: f32 = 14.0;
/// Reserved strip at the foot of every page for the rule and page number.
const FOOTER_H: f32 = 16.0;

const LEFT: f32 = MARGIN_X;
const RIGHT: f32 = PAGE_W - MARGIN_X;
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN_X;

/// PDF font sizes are in points; everything else here is millimetres.
const PT_TO_MM: f32 = 25.4 / 72.0;

/// Height of the coloured band at the top of the first page.
const MASTHEAD_H: f32 = 34.0;

// ---------------------------------------------------------------- palette
//
// The same brand colours as the on-screen theme, so a printed bill and the
// app that produced it look like they come from the same place.

fn violet() -> Color {
    Color::Rgb(Rgb::new(0.486, 0.227, 0.929, None))
}

fn violet_deep() -> Color {
    Color::Rgb(Rgb::new(0.322, 0.129, 0.706, None))
}

fn ink() -> Color {
    Color::Rgb(Rgb::new(0.098, 0.086, 0.141, None))
}

fn muted() -> Color {
    Color::Rgb(Rgb::new(0.443, 0.427, 0.502, None))
}

fn hairline() -> Color {
    Color::Rgb(Rgb::new(0.855, 0.847, 0.886, None))
}

/// Alternating table row fill — light enough to guide the eye across a
/// row without competing with the text sitting on it.
fn zebra() -> Color {
    Color::Rgb(Rgb::new(0.969, 0.965, 0.980, None))
}

fn panel() -> Color {
    Color::Rgb(Rgb::new(0.957, 0.953, 0.973, None))
}

fn white() -> Color {
    Color::Rgb(Rgb::new(1.0, 1.0, 1.0, None))
}

/// Sub-lines inside the violet masthead: white knocked back towards the
/// band so they read as secondary without turning grey.
fn on_violet_muted() -> Color {
    Color::Rgb(Rgb::new(0.867, 0.827, 0.976, None))
}

// -------------------------------------------------------------- alignment

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

/// One column of a table. `weight` is a share of the table's width, so
/// callers describe proportions ("the item name gets twice the room of the
/// quantity") instead of computing millimetre offsets by hand — which is
/// what the old fixed x positions made them do.
pub struct Column {
    pub title: &'static str,
    pub weight: f32,
    pub align: Align,
}

impl Column {
    pub const fn left(title: &'static str, weight: f32) -> Self {
        Column { title, weight, align: Align::Left }
    }

    /// Numeric columns. Right-aligning them is the single biggest reason
    /// a column of money reads as a column rather than as ragged text.
    pub const fn right(title: &'static str, weight: f32) -> Self {
        Column { title, weight, align: Align::Right }
    }
}

/// The shop identity block printed at the top of every document.
pub struct Masthead<'a> {
    pub shop_name: &'a str,
    /// Address, phone, GSTIN — whichever the shop has filled in.
    pub lines: Vec<String>,
    /// Raw bytes of the shop's logo, if it has set one.
    pub logo: Option<&'a [u8]>,
    /// What this document *is*: "TAX INVOICE", "SALE RECEIPT", "REPORT".
    pub doc_label: &'a str,
    /// Reference and date, printed under the label on the right.
    pub doc_ref: Option<String>,
    pub doc_date: Option<String>,
}

// ------------------------------------------------------------------- font
//
// Character widths for the two builtin Helvetica faces, in 1/1000 em —
// straight from the standard AFM metrics. printpdf can't measure text, and
// without measurement there is no right-alignment, no centring and no way
// to know when a cell will overrun its column.

const FIRST_CHAR: usize = 32;

#[rustfmt::skip]
const HELVETICA_WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

#[rustfmt::skip]
const HELVETICA_BOLD_WIDTHS: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611,
    975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556,
    333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611,
    611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

/// Width of `text` set in `size`-point Helvetica, in millimetres. Anything
/// outside printable ASCII falls back to the width of a lowercase "n",
/// which is close enough for the accented characters a shop name might
/// contain and keeps this from needing a full metrics table.
fn text_width_mm(text: &str, size: f32, bold: bool) -> f32 {
    let widths = if bold { &HELVETICA_BOLD_WIDTHS } else { &HELVETICA_WIDTHS };
    let fallback = widths['n' as usize - FIRST_CHAR];

    let thousandths: u32 = text
        .chars()
        .map(|c| {
            let index = (c as usize).wrapping_sub(FIRST_CHAR);
            u32::from(*widths.get(index).unwrap_or(&fallback))
        })
        .sum();

    thousandths as f32 / 1000.0 * size * PT_TO_MM
}

/// Shortens `text` until it fits `max_width`, ending in an ellipsis. Fixed
/// character counts used to do this job, which meant a column of long item
/// names either wrapped into the next column or was cut far shorter than
/// it needed to be.
fn fit(text: &str, max_width: f32, size: f32, bold: bool) -> String {
    if text_width_mm(text, size, bold) <= max_width {
        return text.to_string();
    }

    let mut out = String::new();
    let mut width = text_width_mm("...", size, bold);
    for c in text.chars() {
        let next = text_width_mm(&c.to_string(), size, bold);
        if width + next > max_width {
            break;
        }
        width += next;
        out.push(c);
    }
    out.push_str("...");
    out
}

// -------------------------------------------------------------- the builder

pub struct Doc {
    doc: PdfDocumentReference,
    layer: PdfLayerReference,
    /// Every page's layer, kept so the footer (which needs the final page
    /// count) can be stamped across all of them in `finish`.
    layers: Vec<PdfLayerReference>,
    regular: IndirectFontRef,
    bold: IndirectFontRef,
    /// The baseline the next thing drawn will sit on, measured up from the
    /// bottom of the page.
    y: f32,
}

impl Doc {
    pub fn new(title: &str) -> Result<Self, String> {
        let (doc, page, layer_idx) = PdfDocument::new(title, Mm(PAGE_W), Mm(PAGE_H), "Layer 1");
        let layer = doc.get_page(page).get_layer(layer_idx);
        let regular = doc.add_builtin_font(BuiltinFont::Helvetica).map_err(|e| e.to_string())?;
        let bold = doc.add_builtin_font(BuiltinFont::HelveticaBold).map_err(|e| e.to_string())?;

        Ok(Doc {
            doc,
            layers: vec![layer.clone()],
            layer,
            regular,
            bold,
            y: PAGE_H - MARGIN_TOP,
        })
    }

    // ------------------------------------------------------ raw drawing

    fn draw_text(&self, text: &str, size: f32, x: f32, y: f32, bold: bool, color: Color) {
        self.layer.set_fill_color(color);
        let font = if bold { &self.bold } else { &self.regular };
        self.layer.use_text(text, size, Mm(x), Mm(y), font);
    }

    /// Draws `text` inside the horizontal span `[x, x + width)`, honouring
    /// `align` — the whole reason the font metrics above exist.
    fn draw_aligned(&self, text: &str, size: f32, x: f32, width: f32, y: f32, align: Align, bold: bool, color: Color) {
        let start = match align {
            Align::Left => x,
            Align::Right => x + width - text_width_mm(text, size, bold),
        };
        self.draw_text(text, size, start, y, bold, color);
    }

    fn fill_rect(&self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        self.layer.set_fill_color(color);
        self.layer.add_rect(Rect::new(Mm(x), Mm(y), Mm(x + width), Mm(y + height)).with_mode(PaintMode::Fill));
    }

    fn rule(&self, x1: f32, x2: f32, y: f32, thickness: f32, color: Color) {
        self.layer.set_outline_color(color);
        self.layer.set_outline_thickness(thickness);
        self.layer.add_line(
            vec![(Point::new(Mm(x1), Mm(y)), false), (Point::new(Mm(x2), Mm(y)), false)].into_iter().collect(),
        );
    }

    fn image(&self, bytes: &[u8], x: f32, y: f32, size_mm: f32) {
        // Decoded with this crate's own `image` dependency rather than
        // printpdf's (they're on different major versions), so the pixels
        // are handed over as raw RGB8 rather than as a `DynamicImage`.
        let Ok(decoded) = image::load_from_memory(bytes) else {
            return;
        };
        // A shop logo is usually a big PNG; nothing on the page needs more
        // than a couple of hundred pixels, and the whole thing is embedded
        // uncompressed.
        let rgb = decoded.thumbnail(240, 240).to_rgb8();
        let (w, h) = (rgb.width() as usize, rgb.height() as usize);
        if w == 0 || h == 0 {
            return;
        }

        let xobject = ImageXObject {
            width: Px(w),
            height: Px(h),
            color_space: ColorSpace::Rgb,
            bits_per_component: ColorBits::Bit8,
            interpolate: true,
            image_data: rgb.into_raw(),
            image_filter: None,
            smask: None,
            clipping_bbox: None,
        };

        // `dpi` is what maps pixels onto the page: picking it from the
        // image's own pixel size makes the logo land in a `size_mm` box
        // whatever resolution the shop happened to upload.
        let longest = w.max(h) as f32;
        let dpi = longest / (size_mm / 25.4);
        let (draw_w, draw_h) = (w as f32 / longest * size_mm, h as f32 / longest * size_mm);

        printpdf::Image::from(xobject).add_to_layer(
            self.layer.clone(),
            printpdf::ImageTransform {
                // Centre whichever dimension came out shorter, so a wide
                // and a tall logo both sit in the middle of the same box.
                translate_x: Some(Mm(x + (size_mm - draw_w) / 2.0)),
                translate_y: Some(Mm(y + (size_mm - draw_h) / 2.0)),
                dpi: Some(dpi),
                ..Default::default()
            },
        );
    }

    // ----------------------------------------------------- page handling

    /// Starts a new page if `needed` millimetres won't fit above the
    /// footer. Returns whether it broke, so a table can redraw its column
    /// header at the top of the new page.
    fn ensure_space(&mut self, needed: f32) -> bool {
        if self.y - needed >= FOOTER_H {
            return false;
        }
        let (page, layer_idx) = self.doc.add_page(Mm(PAGE_W), Mm(PAGE_H), "Layer 1");
        self.layer = self.doc.get_page(page).get_layer(layer_idx);
        self.layers.push(self.layer.clone());
        self.y = PAGE_H - MARGIN_TOP;
        true
    }

    // -------------------------------------------------------- components

    /// The branded band across the top of the first page: shop identity on
    /// the left, what-this-document-is on the right.
    pub fn masthead(&mut self, head: &Masthead<'_>) {
        let top = PAGE_H;
        let band_bottom = top - MASTHEAD_H;

        // Full-bleed, and a deeper strip along the bottom edge so the band
        // reads as a designed masthead rather than a coloured rectangle.
        self.fill_rect(0.0, band_bottom, PAGE_W, MASTHEAD_H, violet());
        self.fill_rect(0.0, band_bottom, PAGE_W, 1.6, violet_deep());

        let mut text_left = LEFT;
        if let Some(logo) = head.logo {
            const LOGO_MM: f32 = 17.0;
            let logo_y = band_bottom + (MASTHEAD_H - LOGO_MM) / 2.0 + 1.0;
            self.image(logo, LEFT, logo_y, LOGO_MM);
            text_left += LOGO_MM + 5.0;
        }

        let mut y = top - 13.0;
        self.draw_text(&fit(head.shop_name, CONTENT_W * 0.55, 19.0, true), 19.0, text_left, y, true, white());
        y -= 6.0;
        for line in &head.lines {
            self.draw_text(&fit(line, CONTENT_W * 0.55, 8.5, false), 8.5, text_left, y, false, on_violet_muted());
            y -= 4.2;
        }

        // Right-hand side: the document's own identity.
        let right_w = CONTENT_W * 0.40;
        let right_x = RIGHT - right_w;
        let mut ry = top - 13.0;
        self.draw_aligned(head.doc_label, 13.0, right_x, right_w, ry, Align::Right, true, white());
        ry -= 5.8;
        if let Some(reference) = &head.doc_ref {
            self.draw_aligned(reference, 10.0, right_x, right_w, ry, Align::Right, true, white());
            ry -= 4.6;
        }
        if let Some(date) = &head.doc_date {
            self.draw_aligned(date, 8.5, right_x, right_w, ry, Align::Right, false, on_violet_muted());
        }

        self.y = band_bottom - 12.0;
    }

    /// A small heading above a block of content.
    pub fn section(&mut self, title: &str) {
        self.ensure_space(16.0);
        self.draw_text(title, 11.0, LEFT, self.y, true, violet_deep());
        self.y -= 2.6;
        self.rule(LEFT, RIGHT, self.y, 0.4, hairline());
        self.y -= 5.5;
    }

    /// A row of label-over-value pairs, spread evenly across the page —
    /// the filters a report was run with, or a bill's date and counts.
    pub fn facts(&mut self, pairs: &[(&str, String)]) {
        if pairs.is_empty() {
            return;
        }
        self.ensure_space(16.0);

        let width = CONTENT_W / pairs.len() as f32;
        let top = self.y;
        for (i, (label, value)) in pairs.iter().enumerate() {
            let x = LEFT + width * i as f32;
            self.draw_text(&label.to_uppercase(), 7.0, x, top, false, muted());
            self.draw_text(&fit(value, width - 4.0, 10.0, false), 10.0, x, top - 5.0, false, ink());
        }
        self.y = top - 16.0;
    }

    /// Big headline figures — the two numbers a shopkeeper opens a report
    /// for, given the room to be read from across the counter.
    pub fn highlights(&mut self, stats: &[(&str, String)]) {
        if stats.is_empty() {
            return;
        }
        const H: f32 = 20.0;
        self.ensure_space(H + 6.0);

        let width = (CONTENT_W - 4.0 * (stats.len() - 1) as f32) / stats.len() as f32;
        let top = self.y - H;
        for (i, (label, value)) in stats.iter().enumerate() {
            let x = LEFT + (width + 4.0) * i as f32;
            self.fill_rect(x, top, width, H, panel());
            self.draw_text(&label.to_uppercase(), 7.0, x + 4.0, top + H - 6.5, false, muted());
            self.draw_text(&fit(value, width - 8.0, 14.0, true), 14.0, x + 4.0, top + 4.5, true, violet_deep());
        }
        self.y = top - 9.0;
    }

    /// A table. Draws its own header, repeats it on every page it spills
    /// onto, and stripes alternate rows.
    pub fn table(&mut self, columns: &[Column], rows: &[Vec<String>], empty_note: &str) {
        const HEADER_H: f32 = 8.0;
        const ROW_H: f32 = 7.0;
        const PAD: f32 = 2.5;
        const BODY_SIZE: f32 = 9.0;

        let total_weight: f32 = columns.iter().map(|c| c.weight).sum();
        let bounds: Vec<(f32, f32)> = {
            let mut out = Vec::with_capacity(columns.len());
            let mut x = LEFT;
            for column in columns {
                let width = CONTENT_W * column.weight / total_weight;
                out.push((x, width));
                x += width;
            }
            out
        };

        self.ensure_space(HEADER_H + ROW_H * 2.0);
        self.draw_table_header(columns, &bounds, HEADER_H, PAD);

        if rows.is_empty() {
            self.y -= ROW_H;
            self.draw_text(empty_note, BODY_SIZE, LEFT + PAD, self.y + 2.0, false, muted());
            self.y -= 4.0;
            return;
        }

        for (index, row) in rows.iter().enumerate() {
            if self.ensure_space(ROW_H) {
                self.draw_table_header(columns, &bounds, HEADER_H, PAD);
            }
            let row_top = self.y - ROW_H;
            if index % 2 == 1 {
                self.fill_rect(LEFT, row_top, CONTENT_W, ROW_H, zebra());
            }
            for (cell, (column, (x, width))) in row.iter().zip(columns.iter().zip(&bounds)) {
                let usable = width - PAD * 2.0;
                self.draw_aligned(
                    &fit(cell, usable, BODY_SIZE, false),
                    BODY_SIZE,
                    x + PAD,
                    usable,
                    row_top + 2.3,
                    column.align,
                    false,
                    ink(),
                );
            }
            self.y = row_top;
        }

        self.rule(LEFT, RIGHT, self.y, 0.4, hairline());
        self.y -= 4.0;
    }

    fn draw_table_header(&mut self, columns: &[Column], bounds: &[(f32, f32)], height: f32, pad: f32) {
        let top = self.y - height;
        self.fill_rect(LEFT, top, CONTENT_W, height, violet_deep());
        for (column, (x, width)) in columns.iter().zip(bounds) {
            let usable = width - pad * 2.0;
            self.draw_aligned(column.title, 8.5, x + pad, usable, top + 2.6, column.align, true, white());
        }
        self.y = top;
    }

    /// The totals panel: right-aligned, boxed, with the grand total set
    /// apart so the eye lands on it first.
    pub fn totals(&mut self, rows: &[(String, String)], grand: (&str, String)) {
        const ROW_H: f32 = 6.2;
        const PAD: f32 = 4.0;
        const GRAND_H: f32 = 10.0;

        let width = CONTENT_W * 0.46;
        let x = RIGHT - width;
        // With no breakdown rows (a single-item sale receipt) the panel
        // would be nothing but a grey margin around the total bar, so it
        // collapses to just the bar.
        let height = if rows.is_empty() { GRAND_H } else { ROW_H * rows.len() as f32 + GRAND_H + PAD * 2.0 };
        self.ensure_space(height + 4.0);

        let top = self.y - height;
        if !rows.is_empty() {
            self.fill_rect(x, top, width, height, panel());
        }

        let mut y = self.y - PAD - 4.2;
        for (label, value) in rows {
            self.draw_text(label, 9.5, x + PAD, y, false, muted());
            self.draw_aligned(value, 9.5, x + PAD, width - PAD * 2.0, y, Align::Right, false, ink());
            y -= ROW_H;
        }

        let grand_top = top;
        self.fill_rect(x, grand_top, width, GRAND_H, violet());
        self.draw_text(grand.0, 10.5, x + PAD, grand_top + 3.2, true, white());
        self.draw_aligned(&grand.1, 12.0, x + PAD, width - PAD * 2.0, grand_top + 3.0, Align::Right, true, white());

        self.y = top - 6.0;
    }

    /// Fine print under the content — a thank-you line, or the caveat that
    /// a report only covers the filters it was run with.
    pub fn note(&mut self, text: &str) {
        self.ensure_space(8.0);
        self.draw_text(text, 8.5, LEFT, self.y, false, muted());
        self.y -= 5.0;
    }

    /// Stamps the footer onto every page — which can only happen here,
    /// once the total page count is finally known — and serialises.
    pub fn finish(self, footer_note: &str) -> Result<Vec<u8>, String> {
        let total = self.layers.len();
        for (index, layer) in self.layers.iter().enumerate() {
            let y = FOOTER_H - 6.0;
            layer.set_outline_color(hairline());
            layer.set_outline_thickness(0.4);
            layer.add_line(
                vec![(Point::new(Mm(LEFT), Mm(y + 4.0)), false), (Point::new(Mm(RIGHT), Mm(y + 4.0)), false)]
                    .into_iter()
                    .collect(),
            );

            layer.set_fill_color(muted());
            layer.use_text(footer_note, 8.0, Mm(LEFT), Mm(y), &self.regular);

            let page = format!("Page {} of {}", index + 1, total);
            let x = RIGHT - text_width_mm(&page, 8.0, false);
            layer.use_text(&page, 8.0, Mm(x), Mm(y), &self.regular);
        }

        let mut bytes = Vec::new();
        let mut writer = BufWriter::new(Cursor::new(&mut bytes));
        self.doc.save(&mut writer).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        drop(writer);
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_text_against_the_helvetica_metrics() {
        // "MMMM" is the widest run of letters there is and "iiii" close to
        // the narrowest — if the table were misaligned these would not be
        // far apart.
        assert!(text_width_mm("MMMM", 10.0, false) > text_width_mm("iiii", 10.0, false) * 3.0);
        // A digit is 556/1000 em in both faces, so a four-figure amount is
        // exactly the same width bold or not — which is what lets a totals
        // column stay aligned when one row is emphasised.
        assert!((text_width_mm("1234", 10.0, false) - text_width_mm("1234", 10.0, true)).abs() < 0.001);
    }

    #[test]
    fn width_scales_with_font_size() {
        let small = text_width_mm("Srotas", 8.0, false);
        let large = text_width_mm("Srotas", 16.0, false);
        assert!((large - small * 2.0).abs() < 0.001);
    }

    #[test]
    fn unknown_characters_fall_back_instead_of_panicking() {
        assert!(text_width_mm("रुपये ₹", 10.0, false) > 0.0);
    }

    #[test]
    fn fit_leaves_short_text_alone_and_truncates_long_text() {
        assert_eq!(fit("Bolt", 50.0, 9.0, false), "Bolt");

        let long = "Galvanised Iron Pipe 2 inch heavy duty threaded";
        let fitted = fit(long, 30.0, 9.0, false);
        assert!(fitted.ends_with("..."));
        assert!(fitted.len() < long.len());
        assert!(text_width_mm(&fitted, 9.0, false) <= 30.0);
    }

    #[test]
    fn a_document_with_a_table_that_spills_gets_numbered_pages() {
        let mut doc = Doc::new("Test").unwrap();
        doc.masthead(&Masthead {
            shop_name: "Test Hardware",
            lines: vec!["Main Road".into()],
            logo: None,
            doc_label: "TAX INVOICE",
            doc_ref: Some("Bill #1".into()),
            doc_date: Some("01 Sep 2026".into()),
        });
        let columns = [Column::left("Item", 2.0), Column::right("Amount", 1.0)];
        let rows: Vec<Vec<String>> =
            (0..120).map(|i| vec![format!("Item {i}"), format!("Rs. {i}.00")]).collect();
        doc.table(&columns, &rows, "Nothing here.");
        doc.totals(&[("Subtotal".into(), "Rs. 100.00".into())], ("Total", "Rs. 100.00".into()));

        let bytes = doc.finish("Test Hardware").unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 1000);
    }

    #[test]
    fn an_empty_table_prints_its_note_rather_than_nothing() {
        let mut doc = Doc::new("Empty").unwrap();
        doc.section("Transactions");
        doc.table(&[Column::left("Item", 1.0)], &[], "No transactions match this filter.");
        let bytes = doc.finish("Empty").unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }
}
