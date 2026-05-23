//! Visible signature appearances — Form XObject (AP/N) rendered into a Widget.
//!
//! When `SignOptions::appearance` is set, `sign_pdf` swaps the invisible
//! Widget for one with `/Rect` + `/AP << /N <xobject> >>` so Acrobat /
//! Preview / Foxit render the signature as a visible stamp on the page.

#![allow(dead_code)] // Scaffolding — fleshed out in Task 4 of the v3.11.0 plan.

/// Caller-supplied knobs for rendering a visible signature appearance.
#[derive(Debug, Clone)]
pub struct AppearanceSpec {
    /// 1-indexed page to place the appearance on.
    pub page: u32,
    /// [llx, lly, urx, ury] in PDF user-space units.
    pub rect: [f32; 4],
    /// Font size for the rendered text. Defaults to 9.0pt.
    pub font_size: f32,
    pub show_name: bool,
    pub show_date: bool,
    pub show_reason: bool,
    pub show_location: bool,
    /// Optional PNG/JPEG bytes embedded as a background logo.
    pub image: Option<Vec<u8>>,
}

impl Default for AppearanceSpec {
    fn default() -> Self {
        Self {
            page: 1,
            rect: [50.0, 50.0, 250.0, 120.0],
            font_size: 9.0,
            show_name: true,
            show_date: true,
            show_reason: true,
            show_location: false,
            image: None,
        }
    }
}
