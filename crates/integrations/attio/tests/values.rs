//! An attribute's values as Attio lists them, and the question asked of them most: what is the value now?

use serde_json::{Value, json};
use socketkit_attio::models::{AttributeValue, Record, Values};

mod support;
use support::{THEN, active, value};

fn values(of: Value) -> Values {
    serde_json::from_value(of).unwrap()
}

fn text(said: &str, from: &str, until: Option<&str>) -> Value {
    value("text", from, until, json!({ "value": said }))
}

fn option(title: &str, from: &str, until: Option<&str>) -> Value {
    value(
        "select",
        from,
        until,
        json!({ "option": { "id": { "workspace_id": "w", "object_id": "o", "attribute_id": "a", "option_id": title },
                            "title": title, "is_archived": false } }),
    )
}

fn said(value: Option<&AttributeValue>) -> Option<Value> {
    value.map(AttributeValue::plain)
}

#[test]
fn of_several_values_over_time_the_current_one_is_the_one_still_active() {
    // A job title that changed twice, as Attio lists history: oldest first.
    let history = values(json!({ "job_title": [
        text("Analyst", "2021-03-01T09:00:00.000000000Z", Some("2022-06-01T09:00:00.000000000Z")),
        text("Engineer", "2022-06-01T09:00:00.000000000Z", Some("2024-01-15T09:00:00.000000000Z")),
        text("Director", "2024-01-15T09:00:00.000000000Z", None),
    ] }));
    assert_eq!(said(history.newest("job_title")), Some(json!("Director")));
    assert_eq!(history.active("job_title").len(), 1);
    assert_eq!(history.all("job_title").len(), 3, "the history is kept");
    assert_eq!(history.summary()["job_title"], json!("Director"));

    // The order of the list does not decide it: the same history, newest first.
    let reversed = values(json!({ "job_title": [
        text("Director", "2024-01-15T09:00:00.000000000Z", None),
        text("Engineer", "2022-06-01T09:00:00.000000000Z", Some("2024-01-15T09:00:00.000000000Z")),
    ] }));
    assert_eq!(said(reversed.newest("job_title")), Some(json!("Director")));
}

#[test]
fn an_attribute_with_no_values_and_one_that_is_not_there_have_no_current_value() {
    let record = values(json!({ "job_title": [] }));
    assert_eq!(record.newest("job_title"), None);
    assert!(record.active("job_title").is_empty());
    assert!(record.all("job_title").is_empty());
    assert_eq!(record.newest("never_defined"), None);
    assert!(record.all("never_defined").is_empty());
    // The summary says so with a null, and only for attributes Attio listed.
    assert_eq!(
        serde_json::to_value(record.summary()).unwrap(),
        json!({ "job_title": null })
    );
    assert_eq!(serde_json::to_value(Values::default().summary()).unwrap(), json!({}));
}

#[test]
fn a_value_that_was_cleared_leaves_the_attribute_without_a_current_value() {
    // Every value has an end: the attribute was set, then emptied.
    let cleared = values(json!({ "job_title": [
        text("Analyst", "2021-03-01T09:00:00.000000000Z", Some("2022-06-01T09:00:00.000000000Z")),
        text("Engineer", "2022-06-01T09:00:00.000000000Z", Some("2024-01-15T09:00:00.000000000Z")),
    ] }));
    assert_eq!(
        cleared.newest("job_title"),
        None,
        "the last value it had is not its value now"
    );
    assert!(cleared.active("job_title").is_empty());
    assert_eq!(cleared.all("job_title").len(), 2);
    assert_eq!(cleared.summary()["job_title"], Value::Null);
}

#[test]
fn a_multi_select_has_all_its_active_values_and_the_newest_as_its_current_one() {
    let tags = values(json!({ "categories": [
        option("SaaS", "2023-05-01T10:00:00.000000000Z", None),
        option("Legacy", "2020-01-01T10:00:00.000000000Z", Some("2023-05-01T10:00:00.000000000Z")),
        option("Fintech", "2024-02-01T10:00:00.000000000Z", None),
        option("B2B", "2022-01-01T10:00:00.000000000Z", None),
    ] }));
    let active: Vec<Value> = tags
        .active("categories")
        .into_iter()
        .map(AttributeValue::plain)
        .collect();
    assert_eq!(
        active,
        [json!("SaaS"), json!("Fintech"), json!("B2B")],
        "in Attio's order, without the one removed"
    );
    assert_eq!(
        said(tags.newest("categories")),
        Some(json!("Fintech")),
        "the one chosen last"
    );
    assert_eq!(tags.summary()["categories"], json!(["SaaS", "Fintech", "B2B"]));

    // With one option chosen it reads as that option, not as a list of one.
    let one = values(json!({ "categories": [option("SaaS", THEN, None)] }));
    assert_eq!(one.summary()["categories"], json!("SaaS"));
}

#[test]
fn values_that_became_active_together_keep_attios_order() {
    // Two email addresses written in one request carry the same time.
    let together = values(json!({ "tags": [option("First", THEN, None), option("Second", THEN, None)] }));
    assert_eq!(said(together.newest("tags")), Some(json!("First")));

    // A time that cannot be read never wins over one that can, wherever it stands.
    for listed in [
        json!([option("Unread", "last week", None), option("Read", THEN, None)]),
        json!([option("Read", THEN, None), option("Unread", "last week", None)]),
    ] {
        let mixed = values(json!({ "tags": listed }));
        assert_eq!(said(mixed.newest("tags")), Some(json!("Read")), "{mixed:?}");
        assert_eq!(mixed.active("tags").len(), 2, "it is still one of the values");
    }

    // With no time at all on either, the first listed stands.
    let untimed = values(json!({ "tags": [
        { "attribute_type": "select", "option": { "title": "First" } },
        { "attribute_type": "select", "option": { "title": "Second" } }
    ] }));
    assert_eq!(said(untimed.newest("tags")), Some(json!("First")));
}

#[test]
fn a_value_by_itself_is_the_one_thing_its_type_comes_down_to() {
    let plain = |kind: &str, fields: Value| {
        serde_json::from_value::<AttributeValue>(active(kind, fields))
            .unwrap()
            .plain()
    };
    assert_eq!(plain("text", json!({ "value": "Engineer" })), json!("Engineer"));
    assert_eq!(plain("number", json!({ "value": 42.5 })), json!(42.5));
    assert_eq!(plain("checkbox", json!({ "value": false })), json!(false));
    assert_eq!(plain("date", json!({ "value": "2026-10-12" })), json!("2026-10-12"));
    assert_eq!(plain("timestamp", json!({ "value": THEN })), json!(THEN));
    assert_eq!(plain("rating", json!({ "value": 4 })), json!(4));
    assert_eq!(
        plain(
            "select",
            json!({ "option": { "title": "Medium", "is_archived": false } })
        ),
        json!("Medium")
    );
    assert_eq!(
        plain(
            "status",
            json!({ "status": { "title": "In Progress", "is_archived": false } })
        ),
        json!("In Progress")
    );
    assert_eq!(
        plain(
            "domain",
            json!({ "domain": "app.attio.com", "root_domain": "attio.com" })
        ),
        json!("app.attio.com")
    );
    assert_eq!(
        plain(
            "email-address",
            json!({ "original_email_address": "Ada@Example.com", "email_address": "ada@example.com", "email_domain": "example.com" })
        ),
        json!("ada@example.com")
    );
    assert_eq!(
        plain(
            "phone-number",
            json!({ "original_phone_number": "5558675309", "country_code": "US", "phone_number": "+15558675309" })
        ),
        json!("+15558675309")
    );
    assert_eq!(
        plain(
            "personal-name",
            json!({ "first_name": "Ada", "last_name": "Lovelace", "full_name": "Ada Lovelace" })
        ),
        json!("Ada Lovelace")
    );
}

#[test]
fn a_value_with_several_parts_keeps_them_and_leaves_out_when_it_was_set() {
    let plain = |kind: &str, fields: Value| {
        serde_json::from_value::<AttributeValue>(active(kind, fields))
            .unwrap()
            .plain()
    };
    assert_eq!(
        plain("currency", json!({ "currency_value": 99.5, "currency_code": "EUR" })),
        json!({ "currency_value": 99.5, "currency_code": "EUR" })
    );
    assert_eq!(
        plain(
            "record-reference",
            json!({ "target_object": "companies", "target_record_id": "c-1" })
        ),
        json!({ "target_object": "companies", "target_record_id": "c-1" })
    );
    assert_eq!(
        plain(
            "actor-reference",
            json!({ "referenced_actor_type": "workspace-member", "referenced_actor_id": "m-1" })
        ),
        json!({ "referenced_actor_type": "workspace-member", "referenced_actor_id": "m-1" })
    );
    // The parts of an address Attio has no value for are left out.
    assert_eq!(
        plain(
            "location",
            json!({ "line_1": "1 Infinite Loop", "line_2": null, "locality": "Cupertino", "region": null, "country_code": "US", "latitude": null })
        ),
        json!({ "line_1": "1 Infinite Loop", "locality": "Cupertino", "country_code": "US" })
    );
}

#[test]
fn nothing_is_dropped_from_a_type_this_crate_has_not_met() {
    let plain = |kind: &str, fields: Value| {
        serde_json::from_value::<AttributeValue>(active(kind, fields))
            .unwrap()
            .plain()
    };
    assert_eq!(
        plain("hologram", json!({ "value": "x", "depth": 3 })),
        json!({ "value": "x", "depth": 3 })
    );
    // A known type without the field it should have gives what it does have.
    assert_eq!(
        plain("select", json!({ "choice": { "title": "Medium" } })),
        json!({ "choice": { "title": "Medium" } })
    );
    let untyped: AttributeValue = serde_json::from_value(json!({ "value": 7 })).unwrap();
    assert_eq!(untyped.plain(), json!({ "value": 7 }));
    assert!(untyped.is_active());
}

#[test]
fn the_values_are_kept_exactly_as_attio_listed_them() {
    let sent = json!({
        "name": [active("personal-name", json!({ "first_name": "Ada", "last_name": "Lovelace", "full_name": "Ada Lovelace" }))],
        "job_title": [
            text("Analyst", "2021-03-01T09:00:00.000000000Z", Some("2022-06-01T09:00:00.000000000Z")),
            text("Director", "2022-06-01T09:00:00.000000000Z", None),
        ],
        "categories": []
    });
    let read = values(sent.clone());
    assert_eq!(
        serde_json::to_value(&read).unwrap(),
        sent,
        "what is written back is what was read"
    );

    let first = &read.all("job_title")[0];
    assert_eq!(first.active_from.as_deref(), Some("2021-03-01T09:00:00.000000000Z"));
    assert_eq!(first.active_until.as_deref(), Some("2022-06-01T09:00:00.000000000Z"));
    assert!(!first.is_active());
    assert_eq!(first.attribute_type.as_deref(), Some("text"));
    assert_eq!(
        first.created_by_actor.as_ref().and_then(|actor| actor.kind.as_deref()),
        Some("workspace-member")
    );
    assert_eq!(first.fields["value"], "Analyst");
}

#[test]
fn a_record_reads_whether_or_not_attio_sends_everything() {
    let bare: Record = serde_json::from_value(json!({ "id": { "record_id": "r-1" } })).unwrap();
    assert_eq!(bare.id.record_id, "r-1");
    assert_eq!(bare.values, Values::default());
    assert!(bare.current.is_empty());
    // A record's values are a list for each attribute, and nothing else reads as one.
    for wrong in [
        json!({ "values": { "name": "Ada" } }),
        json!({ "values": { "name": [7] } }),
    ] {
        assert!(serde_json::from_value::<Record>(wrong.clone()).is_err(), "{wrong}");
    }
}

#[test]
fn a_null_in_place_of_an_attributes_list_is_an_attribute_with_no_values() {
    let sparse = values(json!({ "job_title": null, "name": [text("Ada", THEN, None)] }));
    assert!(sparse.all("job_title").is_empty());
    assert_eq!(sparse.newest("job_title"), None);
    assert_eq!(
        serde_json::to_value(sparse.summary()).unwrap(),
        json!({ "job_title": null, "name": "Ada" })
    );
    // It is written back as the empty list it was read as, and reads the same again.
    let written = serde_json::to_value(&sparse).unwrap();
    assert_eq!(written["job_title"], json!([]));
    assert_eq!(values(written), sparse);
}

#[test]
fn the_newest_value_and_the_summary_answer_different_questions() {
    // `newest` is one value; the summary, which a record's `current` holds, is all that are active.
    let tags = values(json!({ "categories": [
        option("SaaS", "2023-05-01T10:00:00.000000000Z", None),
        option("Fintech", "2024-02-01T10:00:00.000000000Z", None),
    ] }));
    assert_eq!(said(tags.newest("categories")), Some(json!("Fintech")));
    assert_eq!(tags.summary()["categories"], json!(["SaaS", "Fintech"]));
    let record: Record = serde_json::from_value(json!({ "id": { "record_id": "r-1" }, "values": tags })).unwrap();
    assert_eq!(said(record.values.newest("categories")), Some(json!("Fintech")));
}
