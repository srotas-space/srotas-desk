//! A small, generic helper for laying out simple text-and-table PDFs
//! (used for the printable/downloadable report). Deliberately minimal —
//! no rich layout engine, just enough to write left-aligned lines and
//! fixed-column rows on an A4 page, adding new pages automatically when
//! the current one runs out of room.
use printpdf::{BuiltinFont, IndirectFontRef, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference};
use std::io::{BufWriter, Cursor, Write};

const PAGE_WIDTH_MM: f32 = 210.0;
const PAGE_HEIGHT_MM: f32 = 297.0;
const MARGIN_MM: f32 = 15.0;
pub const LEFT_MM: f32 = MARGIN_MM;

pub struct Writer {
    doc: PdfDocumentReference,
    layer: PdfLayerReference,
    regular: IndirectFontRef,
    bold: IndirectFontRef,
    y: f32,
}

impl Writer {
    pub fn new(title: &str) -> Result<Self, String> {
        let (doc, page, layer_idx) = PdfDocument::new(title, Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), "Layer 1");
        let layer = doc.get_page(page).get_layer(layer_idx);
        let regular = doc.add_builtin_font(BuiltinFont::Helvetica).map_err(|e| e.to_string())?;
        let bold = doc.add_builtin_font(BuiltinFont::HelveticaBold).map_err(|e| e.to_string())?;
        Ok(Self { doc, layer, regular, bold, y: PAGE_HEIGHT_MM - MARGIN_MM })
    }

    fn new_page_if_needed(&mut self) {
        if self.y < MARGIN_MM + 6.0 {
            let (page, layer_idx) = self.doc.add_page(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), "Layer 1");
            self.layer = self.doc.get_page(page).get_layer(layer_idx);
            self.y = PAGE_HEIGHT_MM - MARGIN_MM;
        }
    }

    /// Writes one left-aligned line at the left margin and advances down.
    pub fn line(&mut self, text: &str, size: f32, bold: bool) {
        self.new_page_if_needed();
        let font = if bold { &self.bold } else { &self.regular };
        self.layer.use_text(text, size, Mm(LEFT_MM), Mm(self.y), font);
        self.y -= size * 0.5 + 2.0;
    }

    /// Writes several cells at fixed x positions on the same line — used
    /// for the transaction table's header and rows.
    pub fn row(&mut self, cells: &[(&str, f32)], size: f32, bold: bool) {
        self.new_page_if_needed();
        let font = if bold { &self.bold } else { &self.regular };
        for (text, x) in cells {
            self.layer.use_text(*text, size, Mm(*x), Mm(self.y), font);
        }
        self.y -= size * 0.5 + 2.0;
    }

    /// Adds vertical breathing room without writing anything.
    pub fn gap(&mut self, mm: f32) {
        self.y -= mm;
    }

    pub fn finish(self) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        let mut writer = BufWriter::new(Cursor::new(&mut bytes));
        self.doc.save(&mut writer).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        drop(writer);
        Ok(bytes)
    }
}
