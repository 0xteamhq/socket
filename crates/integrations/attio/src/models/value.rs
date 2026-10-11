//! Attribute values: what a record or a list entry holds for each of its attributes.
//!
//! Attio keeps every attribute as a list of values, each with the time it
//! became active and the time it stopped being so. That is how one shape
//! serves an attribute with a single value, one with several at once, and
//! the history of either. These types keep the list as Attio sends it, and
//! answer the question nearly every caller has: what is the value now?

use std::cmp::Ordering;
use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::Actor;

/// One value of an attribute, now or in the past.
///
/// What every value has is named here. What only its type has (`value` for
/// a number, `option` for a select, `first_name` for a name) is in `fields`,
/// under Attio's own names, and is written beside the rest in JSON, exactly
/// as Attio writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AttributeValue {
    /// When this became the attribute's value.
    pub active_from: Option<String>,
    /// When it stopped being so. Absent while it still is.
    pub active_until: Option<String>,
    pub created_by_actor: Option<Actor>,
    /// The type of the attribute, such as `text`, `select` or `record-reference`.
    pub attribute_type: Option<String>,
    /// What the type itself carries, under Attio's names.
    #[serde(flatten)]
    pub fields: BTreeMap<String, Value>,
}

impl AttributeValue {
    /// True while this is still a value of its attribute: Attio has set no
    /// time at which it stopped being one.
    pub fn is_active(&self) -> bool {
        self.active_until.is_none()
    }

    /// The value by itself, without when it was set or by whom.
    ///
    /// A type that comes down to one thing gives that thing: the text, the
    /// number, the date, the title of a select option or of a status, the
    /// email address, the phone number, the domain, a person's full name.
    /// Any other type (a currency with its code, a reference to a record, a
    /// location) gives its own fields as an object, without those that are
    /// null. So does a type this crate has not met, and a value that lacks
    /// the field its type should have: nothing is dropped for being unknown.
    pub fn plain(&self) -> Value {
        let named = match self.attribute_type.as_deref().unwrap_or_default() {
            "text" | "number" | "checkbox" | "date" | "timestamp" | "rating" => self.fields.get("value"),
            "select" => self.fields.get("option").and_then(|option| option.get("title")),
            "status" => self.fields.get("status").and_then(|status| status.get("title")),
            "domain" => self.fields.get("domain"),
            "email-address" => self.fields.get("email_address"),
            "phone-number" => self.fields.get("phone_number"),
            "personal-name" => self.fields.get("full_name"),
            _ => None,
        };
        match named {
            Some(value) => value.clone(),
            None => Value::Object(
                self.fields
                    .iter()
                    .filter(|(_, value)| !value.is_null())
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect::<Map<String, Value>>(),
            ),
        }
    }
}

/// The values of a record or of a list entry: for each attribute, by its
/// slug, the list Attio keeps for it.
///
/// A record read by itself carries only the values that are active. Attio
/// returns past ones where history is asked for, and they read the same way.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Values(#[serde(deserialize_with = "lists")] pub BTreeMap<String, Vec<AttributeValue>>);

/// Reads each attribute's list, taking a `null` in place of one as no values.
fn lists<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<BTreeMap<String, Vec<AttributeValue>>, D::Error> {
    let sent = BTreeMap::<String, Option<Vec<AttributeValue>>>::deserialize(deserializer)?;
    Ok(sent
        .into_iter()
        .map(|(attribute, values)| (attribute, values.unwrap_or_default()))
        .collect())
}

impl Values {
    /// Every value Attio listed for `attribute`, active or not. Empty for an
    /// attribute that has none, or that is not here.
    pub fn all(&self, attribute: &str) -> &[AttributeValue] {
        self.0.get(attribute).map_or(&[], Vec::as_slice)
    }

    /// The values `attribute` has now, in the order Attio listed them. An
    /// attribute that takes several, such as a multi-select, has them all here.
    pub fn active(&self, attribute: &str) -> Vec<&AttributeValue> {
        self.all(attribute).iter().filter(|value| value.is_active()).collect()
    }

    /// The one active value of `attribute` that became active last, or
    /// `None` when it has no active value.
    ///
    /// For an attribute with a single value this is its value now. For one
    /// with several active values it is only the newest of them, and the
    /// first of those in Attio's list when several became active at once;
    /// [`Values::active`] gives them all, and [`Values::summary`] is what a
    /// record's `current` holds. A time that cannot be read counts as the
    /// earliest.
    pub fn newest(&self, attribute: &str) -> Option<&AttributeValue> {
        self.active(attribute)
            .into_iter()
            .fold(None, |newest, value| match newest {
                Some(newest) if began(value).cmp(&began(newest)) != Ordering::Greater => Some(newest),
                _ => Some(value),
            })
    }

    /// Every attribute's current value by itself, as [`AttributeValue::plain`]
    /// gives it: `null` for an attribute with no active value, the value for
    /// one with a single active value, and a list of them, in Attio's order,
    /// for one with several.
    ///
    /// Whether an attribute could hold several is not something its values
    /// say, so a multi-select with one option chosen reads as that option
    /// and not as a list of one. The attribute's own description
    /// (`is_multiselect`) is where to learn which it is.
    pub fn summary(&self) -> BTreeMap<String, Value> {
        self.0
            .keys()
            .map(|attribute| {
                let mut active: Vec<Value> = self.active(attribute).into_iter().map(AttributeValue::plain).collect();
                let now = match active.len() {
                    0 => Value::Null,
                    1 => active.remove(0),
                    _ => Value::Array(active),
                };
                (attribute.clone(), now)
            })
            .collect()
    }
}

/// When a value became active, in a form that sorts by time: the seconds,
/// then the fraction of a second without its trailing zeros.
///
/// Attio writes its times in UTC with a `Z`, and the seconds are then always
/// nineteen characters, so text order is time order. A time written any
/// other way gives `None`, which sorts before every time that was read.
fn began(value: &AttributeValue) -> Option<(&str, &str)> {
    let time = value.active_from.as_deref()?.strip_suffix('Z')?;
    let (seconds, fraction) = time.split_once('.').unwrap_or((time, ""));
    let well_formed = seconds.len() == 19 && seconds.is_ascii() && fraction.bytes().all(|byte| byte.is_ascii_digit());
    well_formed.then(|| (seconds, fraction.trim_end_matches('0')))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from(time: &str) -> AttributeValue {
        AttributeValue {
            active_from: Some(time.to_owned()),
            ..AttributeValue::default()
        }
    }

    #[test]
    fn times_sort_by_when_they_were_and_not_by_how_they_are_written() {
        let in_order = [
            "2023-01-01T15:00:00Z",
            "2023-01-01T15:00:00.05Z",
            "2023-01-01T15:00:00.4Z",
            "2023-01-01T15:00:00.490000000Z",
            "2023-01-01T15:00:00.5Z",
            "2023-01-01T15:00:01.000000000Z",
            "2024-01-01T00:00:00.000000000Z",
        ];
        for pair in in_order.windows(2) {
            assert!(began(&from(pair[0])) < began(&from(pair[1])), "{pair:?}");
        }
        // The same moment, written with more and with fewer zeros.
        assert_eq!(
            began(&from("2023-01-01T15:00:00.500Z")),
            began(&from("2023-01-01T15:00:00.5Z"))
        );
        assert_eq!(
            began(&from("2023-01-01T15:00:00.000Z")),
            began(&from("2023-01-01T15:00:00Z"))
        );
    }

    #[test]
    fn a_time_that_is_not_written_as_attio_writes_it_is_not_guessed_at() {
        for unread in [
            "",
            "yesterday",
            "2023-01-01",
            "2023-01-01T15:00:00+01:00",
            "2023-01-01T15:00:00",
            "2023-01-01T15:00:00.5xZ",
            "2023-01-01T15:00Z",
        ] {
            assert_eq!(began(&from(unread)), None, "{unread:?}");
        }
        assert_eq!(began(&AttributeValue::default()), None);
        // An unread time sorts before any that was read, so it never wins over one.
        assert!(began(&from("yesterday")) < began(&from("1970-01-01T00:00:00Z")));
    }
}
