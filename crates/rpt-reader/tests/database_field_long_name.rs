//! A *referenced* database field (`data_definition.field_definitions`, `FieldKindData::Database`)
//! must carry the same qualified `long_name` (`table.field`) and owning `table_alias` its
//! table-schema counterpart (`database.tables[].data_fields`) already does.
//!
//! The two are decoded from different streams — `Contents` (`0x0073 FieldDef`, one per field the
//! report actually references) and the separately-encrypted `QESession` (`0x0004 QeField`, the full
//! table schema) — that share one global field-id space. `0x0073` carries that id (`field_id`) but,
//! until fixed, the decoder never used it to resolve back into the schema: every referenced field's
//! `long_name` read `None` and its `DbField::table_alias`/`unique_id` read empty, even though the
//! schema-side field a few bytes away in the same file had the real values all along.
//!
//! That gap bites hardest on a **grouped** report: a `Group::condition_field` is stored as the
//! qualified `table.field` form (e.g. `"Customer.Country"`), so any consumer that builds a custom
//! [`rpt_data::RowSource`] by keying its rows off `FieldDef::long_name` — the field the model
//! documents as "the fully qualified name", the natural and SDK-faithful thing to key by — silently
//! got `None` for every field and so had no way to line a row's value up with the group it belongs to.
//! `rows_source_grouped.rs` in `rpt-render`'s test suite exercises the render consequence directly;
//! this file pins the decode fact underneath it.

use rpt_model::FieldKindData;
use rpt_reader::Rpt;
use rpt_test_support::fixture;

/// Every `FieldKindData::Database` field definition in `data_definition.field_definitions` resolves
/// to a `long_name`/`table_alias` that names a real table+field in `database.tables`, and the two
/// agree on the qualified name — for a grouped report and a flat one alike (the gap was not specific
/// to either shape; it was that `long_name` was never populated for any referenced database field).
#[test]
fn referenced_database_fields_carry_their_qualified_name() {
    for report_path in [
        "tests/fixtures/reports/benbrahim777/Customer Orders, Grouped by Country.rpt",
        "tests/fixtures/reports/benbrahim777/Product Price List.rpt",
    ] {
        let rpt = Rpt::open(fixture(report_path)).expect("open fixture report");
        let report = rpt.report();

        let mut checked = 0;
        for (f, _) in report.data_definition.database_fields() {
            let long_name = f
                .long_name
                .as_deref()
                .unwrap_or_else(|| panic!("{report_path}: {:?} has no long_name", f.name));
            let FieldKindData::Database(db) = &f.kind else {
                unreachable!("database_fields() only yields Database-kind fields");
            };
            assert!(
                !db.table_alias.is_empty(),
                "{report_path}: {:?} has no table_alias",
                f.name
            );
            assert_eq!(
                long_name,
                format!("{}.{}", db.table_alias, f.name),
                "{report_path}: {:?}'s long_name must be its table_alias qualifying its own name",
                f.name
            );

            // And it must actually resolve to a real table+field in the schema, not just look
            // plausible — the whole point is joining back to the table the field truly reads from.
            let table = report
                .database
                .tables
                .iter()
                .find(|t| t.alias == db.table_alias)
                .unwrap_or_else(|| {
                    panic!("{report_path}: {:?} names no known table", db.table_alias)
                });
            assert!(
                table.data_fields.iter().any(|df| df.name == f.name),
                "{report_path}: table {:?} has no field named {:?}",
                db.table_alias,
                f.name
            );
            checked += 1;
        }
        assert!(
            checked > 0,
            "{report_path}: no database fields found to check"
        );
    }
}

/// The specific fact the grouped report's bug hinges on: the group's `condition_field` (stored as the
/// qualified `table.field` form) matches *exactly* one referenced field's `long_name` — the join a
/// `RowSource` built by keying off `long_name` needs in order to land a row in the right group.
#[test]
fn a_groups_condition_field_matches_a_referenced_fields_long_name() {
    let rpt = Rpt::open(fixture(
        "tests/fixtures/reports/benbrahim777/Customer Orders, Grouped by Country.rpt",
    ))
    .expect("open fixture report");
    let report = rpt.report();

    assert_eq!(report.data_definition.groups.len(), 1, "one group level");
    let condition_field = &report.data_definition.groups[0].condition_field;
    assert_eq!(condition_field, "Customer.Country");

    let matches = report
        .data_definition
        .database_fields()
        .filter(|(f, _)| f.long_name.as_deref() == Some(condition_field.as_str()))
        .count();
    assert_eq!(
        matches, 1,
        "exactly one referenced field's long_name must match the group's condition_field"
    );
}
