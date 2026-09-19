//! `RenderOptions::object_visibility` reaches the final Page IR through the whole `render_with`
//! orchestration (`render_options` → `build_and_lay_out`/`layout_dataset` → `rpt_layout::layout_scoped`),
//! not just `rpt-layout`'s own direct `layout_scoped` call — the seam
//! `rpt-layout/src/tests/objects.rs`'s `object_visibility_override_hides_one_object_in_a_suppressed_underlay_section`
//! doesn't cover, since it calls `layout_scoped` directly and never goes through this crate's
//! `RenderOptions` plumbing. (Neither `object_visibility` nor `section_visibility` had any coverage at
//! this crate's level before this file — checked via `grep -rl section_visibility crates/rpt-render/tests`.)
//!
//! Hermetic: a hand-built `Report`, no fixture file — the same Suppress + Underlay Following Sections
//! + SuppressIfBlank shape confirmed against a real SAP B1 invoice template's AFIP tax-notice section
//! (a logo field plus two certificate pictures, all underlay-painted), which is the one scenario the
//! feature exists for: `section_visibility` cannot silence a single unwanted object in an
//! underlay-carrier section without also disabling the underlay for everything else sharing it.

use rpt_pages::DrawOp;
use rpt_reader::model::{
    Area, AreaSectionKind, PictureObject, Rect, Report, ReportObject, ReportObjectKind, Section,
    TextObject, Twips,
};
use rpt_render::{render_with, RenderOptions};
use std::collections::BTreeMap;

fn text_object(name: &str, text: &str, top: i32) -> ReportObject {
    let mut o = ReportObject::default();
    o.name = name.to_string();
    o.bounds = Rect {
        left: Twips(100),
        top: Twips(top),
        width: Twips(3000),
        height: Twips(240),
    };
    let mut t = TextObject::default();
    t.text = text.to_string();
    o.kind = ReportObjectKind::Text(t);
    o
}

fn picture_object(name: &str, top: i32) -> ReportObject {
    let mut o = ReportObject::default();
    o.name = name.to_string();
    o.bounds = Rect {
        left: Twips(100),
        top: Twips(top),
        width: Twips(2000),
        height: Twips(1000),
    };
    let mut p = PictureObject::default();
    // A PNG signature so the format sniffs as a browser-renderable raster.
    p.data = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    o.kind = ReportObjectKind::Picture(p);
    o
}

/// A Report Header holding a logo text field, then two certificate pictures: statically suppressed
/// and marked to underlay following sections, so it still paints through despite `suppress`.
fn afip_style_report() -> Report {
    let mut afip = Section::default();
    afip.kind = AreaSectionKind::ReportHeader;
    afip.name = "Afip".into();
    afip.height = Twips(2400);
    afip.objects = vec![
        text_object("Logo", "LOGO_TEXT", 0),
        picture_object("Cert1", 300),
        picture_object("Cert2", 1400),
    ];
    afip.format.base.suppress = true;
    afip.format.underlay_section = true;
    afip.format.suppress_if_blank = true;

    let mut area = Area::default();
    area.kind = AreaSectionKind::ReportHeader;
    area.sections = vec![afip];

    let mut report = Report::default();
    report.report_definition.areas = vec![area];
    report
}

fn has_logo(doc: &rpt_pages::PagedDocument) -> bool {
    doc.pages
        .iter()
        .flat_map(|p| &p.ops)
        .any(|op| matches!(op, DrawOp::Text(t) if t.text == "LOGO_TEXT"))
}

fn image_count(doc: &rpt_pages::PagedDocument) -> usize {
    doc.pages
        .iter()
        .flat_map(|p| &p.ops)
        .filter(|op| matches!(op, DrawOp::Image(_)))
        .count()
}

/// `RenderOptions::object_visibility` reaches the final Page IR: hiding the logo object leaves its
/// sibling certificate pictures — also underlay-painted, inside the same suppressed section —
/// completely untouched, matching the confirmed production render.
#[test]
fn object_visibility_reaches_final_render_output() {
    let report = afip_style_report();

    let without_override = render_with(&report, RenderOptions::default());
    assert!(
        has_logo(&without_override),
        "sanity: the suppressed+underlay section paints its objects absent any override"
    );
    assert_eq!(
        image_count(&without_override),
        2,
        "sanity: both certificate pictures render absent any override"
    );

    let overrides = BTreeMap::from([("Logo".to_string(), false)]);
    let with_override = render_with(
        &report,
        RenderOptions {
            object_visibility: Some(overrides),
            ..Default::default()
        },
    );
    assert!(
        !has_logo(&with_override),
        "RenderOptions::object_visibility=false for the logo must reach the final render and hide it"
    );
    assert_eq!(
        image_count(&with_override),
        2,
        "sibling certificate pictures must still render, untouched by the logo's override"
    );
}
