//! The event contract only grows: compares the checked-in schema (`schema/events/v1.json`) with
//! the one on `main` and names every change a consumer or a replayed old event would trip on.
//! New event types, optional fields and enum values are fine; removing or retyping anything, or
//! making a field required that old events don't carry, is a new version instead.
use serde_json::{Map, Value};

/// Every way `new` stops accepting what `base` accepted, as readable paths; empty when additive.
pub fn breaking_changes(base: &Value, new: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let roots = Roots { base, new };
    roots.compare(base, new, "$", &mut out);
    out
}

struct Roots<'a> {
    base: &'a Value,
    new: &'a Value,
}

fn obj(v: &Value) -> Option<&Map<String, Value>> {
    v.as_object()
}

/// Keywords `compare` checks itself, plus annotations no consumer validates against. A change to
/// any other keyword (`pattern`, `additionalProperties`, `allOf`…) is reported: fail closed.
const MODELLED: &[&str] = &[
    "type",
    "format",
    "const",
    "enum",
    "oneOf",
    "anyOf",
    "properties",
    "required",
    "$defs",
    "$schema",
    "$id",
    "$comment",
    "title",
    "description",
    "examples",
];

/// `#/$defs/Name` resolved in its own document, following chains of refs.
fn resolve<'a>(root: &'a Value, mut v: &'a Value) -> &'a Value {
    // ponytail: refs are local and their siblings ignored; bounded so a ref cycle cannot hang CI.
    for _ in 0..32 {
        match v
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|r| r.strip_prefix('#'))
            .and_then(|p| root.pointer(p))
        {
            Some(target) => v = target,
            None => break,
        }
    }
    v
}

/// An event type's name, for readable paths and for matching alternatives across versions.
fn event_type(v: &Value) -> Option<&str> {
    v.pointer("/properties/type/const").and_then(Value::as_str)
}

impl Roots<'_> {
    fn compare(&self, base: &Value, new: &Value, path: &str, out: &mut Vec<String>) {
        let (base, new) = (resolve(self.base, base), resolve(self.new, new));
        for key in ["type", "format", "const"] {
            if base.get(key) != new.get(key) {
                out.push(format!(
                    "{path}: {key} changed from {:?} to {:?}",
                    base.get(key),
                    new.get(key)
                ));
            }
        }
        let keys = |v: &Value| {
            obj(v)
                .map(|m| m.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        };
        let mut unmodelled: Vec<String> = keys(base).into_iter().chain(keys(new)).collect();
        unmodelled.sort();
        unmodelled.dedup();
        for key in unmodelled
            .iter()
            .filter(|k| !MODELLED.contains(&k.as_str()))
        {
            if base.get(key) != new.get(key) {
                out.push(format!(
                    "{path}: {key} changed, and the check cannot tell if that is additive"
                ));
            }
        }
        if let Some(values) = base.get("enum").and_then(Value::as_array) {
            let now = new.get("enum").and_then(Value::as_array);
            for v in values {
                if !now.is_some_and(|n| n.contains(v)) {
                    out.push(format!("{path}: enum value {v} removed"));
                }
            }
        }
        for key in ["oneOf", "anyOf"] {
            self.alternatives(base, new, key, path, out);
        }
        let props = |v: &Value| {
            v.get("properties")
                .and_then(obj)
                .cloned()
                .unwrap_or_default()
        };
        let (bp, np) = (props(base), props(new));
        for (name, b) in &bp {
            match np.get(name) {
                Some(n) => self.compare(b, n, &format!("{path}.{name}"), out),
                None => out.push(format!("{path}.{name}: removed")),
            }
        }
        // Events already on the bus or in the outbox lack a field that is newly required.
        let required = |v: &Value| -> Vec<String> {
            v.get("required")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let base_required = required(base);
        for r in required(new) {
            if !base_required.contains(&r) {
                out.push(format!("{path}.{r}: newly required"));
            }
        }
    }

    /// Each alternative `base` allowed must still be allowed: event types by name, others by
    /// any compatible alternative.
    fn alternatives(
        &self,
        base: &Value,
        new: &Value,
        key: &str,
        path: &str,
        out: &mut Vec<String>,
    ) {
        let Some(old) = base.get(key).and_then(Value::as_array) else {
            return;
        };
        let now: Vec<&Value> = new
            .get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().collect())
            .unwrap_or_default();
        for (i, b) in old.iter().enumerate() {
            match event_type(b) {
                Some(name) => match now.iter().find(|n| event_type(n) == Some(name)) {
                    Some(n) => self.compare(b, n, &format!("{path}[{name}]"), out),
                    None => out.push(format!("{path}[{name}]: event type removed")),
                },
                None => {
                    let fits = now.iter().any(|n| {
                        let mut errs = Vec::new();
                        self.compare(b, n, path, &mut errs);
                        errs.is_empty()
                    });
                    if !fits {
                        out.push(format!("{path}.{key}[{i}]: no longer accepted"));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema(events: Value, defs: Value) -> Value {
        json!({
            "type": "object",
            "properties": {"id": {"type": "string", "format": "uuid"}},
            "required": ["id"],
            "oneOf": events,
            "$defs": defs,
        })
    }

    fn event(name: &str, data: Value, required: &[&str]) -> Value {
        json!({
            "type": "object",
            "properties": {
                "type": {"type": "string", "const": name},
                "data": {"type": "object", "properties": data, "required": required},
            },
            "required": ["type", "data"],
        })
    }

    fn decision(values: &[&str]) -> Value {
        json!({"Decision": {"type": "string", "enum": values}})
    }

    fn base() -> Value {
        schema(
            json!([event(
                "approval.decided.v1",
                json!({"approval": {"type": "string", "format": "uuid"}, "decision": {"$ref": "#/$defs/Decision"}}),
                &["approval", "decision"],
            )]),
            decision(&["approved", "denied"]),
        )
    }

    #[test]
    fn the_same_schema_and_additions_are_fine() {
        assert!(breaking_changes(&base(), &base()).is_empty());
        let grown = schema(
            json!([
                event(
                    "approval.decided.v1",
                    json!({
                        "approval": {"type": "string", "format": "uuid"},
                        "decision": {"$ref": "#/$defs/Decision"},
                        "note": {"type": "string"},
                    }),
                    &["approval", "decision"],
                ),
                event(
                    "grant.created.v1",
                    json!({"grant": {"type": "string"}}),
                    &["grant"]
                ),
            ]),
            decision(&["approved", "denied", "expired"]),
        );
        assert_eq!(breaking_changes(&base(), &grown), Vec::<String>::new());
    }

    #[test]
    fn removing_or_retyping_anything_is_breaking() {
        let removed_type = schema(json!([]), decision(&["approved", "denied"]));
        assert_eq!(
            breaking_changes(&base(), &removed_type),
            ["$[approval.decided.v1]: event type removed"]
        );
        let changed = schema(
            json!([event(
                "approval.decided.v1",
                json!({"approval": {"type": "integer"}, "decision": {"$ref": "#/$defs/Decision"}}),
                &["approval", "decision"],
            )]),
            decision(&["approved"]),
        );
        let found = breaking_changes(&base(), &changed);
        assert!(
            found.iter().any(|e| e.contains(".approval: type changed")),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|e| e.contains(".approval: format changed")),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|e| e.contains("enum value \"denied\" removed")),
            "{found:?}"
        );
        let dropped = schema(
            json!([event(
                "approval.decided.v1",
                json!({"approval": {"type": "string", "format": "uuid"}}),
                &["approval"],
            )]),
            decision(&["approved", "denied"]),
        );
        assert_eq!(
            breaking_changes(&base(), &dropped),
            ["$[approval.decided.v1].data.decision: removed"]
        );
    }

    #[test]
    fn a_newly_required_field_is_breaking_because_old_events_lack_it() {
        let stricter = schema(
            json!([event(
                "approval.decided.v1",
                json!({
                    "approval": {"type": "string", "format": "uuid"},
                    "decision": {"$ref": "#/$defs/Decision"},
                    "by": {"type": "string"},
                }),
                &["approval", "decision", "by"],
            )]),
            decision(&["approved", "denied"]),
        );
        assert_eq!(
            breaking_changes(&base(), &stricter),
            ["$[approval.decided.v1].data.by: newly required"]
        );
    }

    #[test]
    fn a_keyword_the_check_does_not_model_fails_closed() {
        let with = |key: &str, v: Value| {
            let mut s = base();
            s["oneOf"][0]["properties"]["data"][key] = v;
            s
        };
        for (key, v) in [
            ("additionalProperties", json!(false)),
            ("pattern", json!("^a")),
            ("maxLength", json!(3)),
            ("minimum", json!(1)),
            ("items", json!({"type": "string"})),
            ("allOf", json!([{"required": ["approval"]}])),
        ] {
            assert_eq!(
                breaking_changes(&base(), &with(key, v)),
                [format!(
                    "$[approval.decided.v1].data: {key} changed, and the check cannot tell if that is additive"
                )],
            );
        }
        // Annotations change nothing a consumer sees.
        assert!(breaking_changes(&base(), &with("description", json!("why"))).is_empty());
    }

    #[test]
    fn a_ref_to_a_ref_is_followed() {
        let chained = |values: &[&str]| {
            let mut s = base();
            s["$defs"]["Decision"] = json!({"$ref": "#/$defs/Inner"});
            s["$defs"]["Inner"] = json!({"type": "string", "enum": values});
            s
        };
        assert_eq!(
            breaking_changes(&chained(&["approved", "denied"]), &chained(&["approved"])),
            ["$[approval.decided.v1].data.decision: enum value \"denied\" removed"]
        );
    }

    #[test]
    fn the_checked_in_schema_has_not_broken_the_one_on_main() {
        // CI sets this to `main`'s schema/events/v1.json; locally the test has nothing to compare.
        let Ok(path) = std::env::var("MANDATE_EVENTS_BASE_SCHEMA") else {
            return;
        };
        let read = |p: &str| -> Value {
            serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
        };
        let current = concat!(env!("CARGO_MANIFEST_DIR"), "/../schema/events/v1.json");
        let found = breaking_changes(&read(&path), &read(current));
        assert!(
            found.is_empty(),
            "event schema changes must be additive; make a new version instead:\n{}",
            found.join("\n")
        );
    }
}
