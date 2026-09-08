//! A custom [`RowSource`] pushed through [`RenderSource::Rows`] must render a **grouped** report's
//! group headers and detail rows exactly as it does a flat report's — the render path is identical
//! for both ([`build_and_lay_out`] takes no different branch), so nothing about grouping should be
//! special-cased away.
//!
//! This is the render-visible half of the fix pinned at the decode layer by
//! `database_field_long_name.rs` in `rpt-reader`'s test suite: a `RowSource` built the natural way —
//! one [`Column`] per referenced database field, keyed by [`FieldDef::long_name`] (the model's own
//! "fully qualified name", `table.field`) — used to get `None` back for every field on this fixture,
//! because the reader never resolved a *referenced* field's `long_name` even though the identical
//! qualified name was sitting a few bytes away in the same file's table schema. A `Group`'s
//! `condition_field` is stored in that same qualified form, so every row silently failed to key into
//! its group and the render dropped the group header and the detail row with it — while a flat
//! (ungrouped) report pushed through the very same mechanism rendered its row's values just fine,
//! since nothing there depends on a group key at all. That asymmetry is exactly what made the bug easy
//! to miss: the "grouped reports drop their data via RowSource" symptom looked structural, when the
//! defect was one field decoding to `None` it should never have.

use rpt_data::{Column, Row, RowSource};
use rpt_formula::eval::Value;
use rpt_pages::DrawOp;
use rpt_render::{RenderOptions, RenderSource};
use rpt_test_support::fixture;

/// A `RowSource` over the report's own `data_definition.field_definitions` (`FieldKindData::Database`
/// only), one hand-supplied row — the shape a live/custom integration naturally builds when it has a
/// `Report` in hand but no saved-data batch: one column per field the report actually references,
/// keyed by that field's own fully qualified name.
struct OneRow {
    columns: Vec<Column>,
    row: Row,
}

impl RowSource for OneRow {
    fn columns(&self) -> &[Column] {
        &self.columns
    }
    fn rows(&self) -> Vec<Row> {
        vec![self.row.clone()]
    }
}

fn one_row_source(report: &rpt_reader::model::Report, values: &[(&str, Value)]) -> OneRow {
    let dd = &report.data_definition;
    let mut columns = Vec::new();
    let mut row = Row::default();
    for (f, _) in dd.database_fields() {
        let key = f
            .long_name
            .clone()
            .unwrap_or_else(|| panic!("{:?} has no long_name", f.name));
        columns.push(Column {
            name: key.clone(),
            value_type: f.value_type,
        });
        let value = values
            .iter()
            .find(|(short, _)| *short == f.name)
            .map(|(_, v)| v.clone())
            .unwrap_or(Value::Null);
        row.insert(&key, value);
    }
    OneRow { columns, row }
}

/// Every text run on every page, as `(section, object_name, text)` — enough to check that a specific
/// field's value landed in a specific band, not just that *some* text exists somewhere.
fn text_runs(doc: &rpt_pages::PagedDocument) -> Vec<(String, Option<String>, String)> {
    doc.pages
        .iter()
        .flat_map(|p| &p.ops)
        .filter_map(|op| match op {
            DrawOp::Text(t) => {
                let src = t.source.as_ref()?;
                Some((src.section.clone(), src.object_name.clone(), t.text.clone()))
            }
            _ => None,
        })
        .collect()
}

/// The reported bug, pinned directly: a pushed row for a grouped report must produce the group's
/// header (the group-name field, showing the row's own key) *and* its detail row — not just the
/// report's static bands.
#[test]
fn a_pushed_row_reaches_its_group_header_and_detail_row() {
    let rpt = rpt_reader::Rpt::open(fixture(
        "tests/fixtures/reports/benbrahim777/Customer Orders, Grouped by Country.rpt",
    ))
    .expect("open fixture report");
    let report = rpt.report();

    let source = one_row_source(
        report,
        &[
            ("Customer Name", Value::Str("Acme Corp".into())),
            ("Country", Value::Str("Argentina".into())),
        ],
    );

    let doc = rpt_render::render_with(
        report,
        RenderOptions {
            datasource: RenderSource::Rows(&source),
            ..Default::default()
        },
    );

    assert!(!doc.pages.is_empty(), "the render must produce a page");
    let texts = text_runs(&doc);

    // The group header: the group-name field must show the row's own group key.
    assert!(
        texts
            .iter()
            .any(|(section, _, text)| section.starts_with("GroupHeader") && text == "Argentina"),
        "no GroupHeader text run reads \"Argentina\"; group header was dropped. All text: {texts:?}"
    );
    // The detail row: the row's own field values must show up under Details.
    assert!(
        texts
            .iter()
            .any(|(section, _, text)| section.starts_with("Detail") && text == "Acme Corp"),
        "no Detail text run reads \"Acme Corp\"; detail row was dropped. All text: {texts:?}"
    );
}

/// The control case this bug did NOT affect: the exact same mechanism against a flat (ungrouped)
/// report, so a future regression in the grouped-report path specifically (rather than in
/// `RenderSource::Rows` generally) still shows up as a difference between the two.
#[test]
fn a_pushed_row_reaches_the_detail_row_of_a_flat_report() {
    let rpt = rpt_reader::Rpt::open(fixture(
        "tests/fixtures/reports/benbrahim777/Product Price List.rpt",
    ))
    .expect("open fixture report");
    let report = rpt.report();
    assert!(
        report.data_definition.groups.is_empty(),
        "control fixture must be a flat (ungrouped) report"
    );

    let source = one_row_source(report, &[("Product Name", Value::Str("Widget".into()))]);

    let doc = rpt_render::render_with(
        report,
        RenderOptions {
            datasource: RenderSource::Rows(&source),
            ..Default::default()
        },
    );

    let texts = text_runs(&doc);
    assert!(
        texts
            .iter()
            .any(|(section, _, text)| section.starts_with("Detail") && text == "Widget"),
        "no Detail text run reads \"Widget\". All text: {texts:?}"
    );
}
