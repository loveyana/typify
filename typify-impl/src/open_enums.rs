// Copyright 2026 Oxide Computer Company

use std::collections::{BTreeMap, BTreeSet};

use schemars::schema::{InstanceType, Schema, SingleOrVec, SubschemaValidation};
use serde_json::Value;

use crate::{util::ref_key, RefKey};

pub(crate) fn string_values(values: &[Value]) -> Vec<String> {
    values
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

// Enums used inside compositions remain closed, including referenced types.
// The value fence is conservative: enums whose values are all used in
// compositions also remain closed, including unions/intersections synthesized
// by the existing schema merger, rather than weakening branch selection.
pub(crate) fn collect_closed_enums(
    schema: &Schema,
    definitions: &BTreeMap<RefKey, Schema>,
    closed: &mut BTreeSet<String>,
) {
    visit(schema, definitions, closed, false, &mut BTreeSet::new());
}

fn visit(
    schema: &Schema,
    definitions: &BTreeMap<RefKey, Schema>,
    closed: &mut BTreeSet<String>,
    protected: bool,
    visited: &mut BTreeSet<(RefKey, bool)>,
) {
    let Schema::Object(object) = schema else {
        return;
    };
    let nullable = object
        .subschemas
        .as_deref()
        .is_some_and(is_nullable_wrapper);
    let protected = protected
        || object.extensions.contains_key("discriminator")
        || object.subschemas.as_deref().is_some_and(|subschemas| {
            !nullable
                && (subschemas.all_of.is_some()
                    || subschemas.any_of.is_some()
                    || subschemas.one_of.is_some()
                    || subschemas.not.is_some()
                    || subschemas.if_schema.is_some()
                    || subschemas.then_schema.is_some()
                    || subschemas.else_schema.is_some())
        });
    if let Some(reference) = &object.reference {
        let key = ref_key(reference);
        if visited.insert((key.clone(), protected)) {
            if let Some(referenced) = definitions.get(&key) {
                visit(referenced, definitions, closed, protected, visited);
            }
        }
    }
    if protected {
        if let Some(values) = &object.enum_values {
            closed.extend(string_values(values));
        }
        if let Some(Value::String(value)) = &object.const_value {
            closed.insert(value.clone());
        }
    }
    if let Some(subschemas) = &object.subschemas {
        for branches in [&subschemas.all_of, &subschemas.any_of, &subschemas.one_of]
            .into_iter()
            .flatten()
        {
            for branch in branches {
                visit(branch, definitions, closed, protected || !nullable, visited);
            }
        }
        for branch in [
            &subschemas.not,
            &subschemas.if_schema,
            &subschemas.then_schema,
            &subschemas.else_schema,
        ]
        .into_iter()
        .flatten()
        {
            visit(branch, definitions, closed, true, visited);
        }
    }
    if let Some(object) = &object.object {
        for child in object
            .properties
            .values()
            .chain(object.pattern_properties.values())
            .chain(object.additional_properties.as_deref())
            .chain(object.property_names.as_deref())
        {
            visit(child, definitions, closed, protected, visited);
        }
    }
    if let Some(array) = &object.array {
        if let Some(items) = &array.items {
            match items {
                SingleOrVec::Single(item) => visit(item, definitions, closed, protected, visited),
                SingleOrVec::Vec(items) => {
                    for item in items {
                        visit(item, definitions, closed, protected, visited);
                    }
                }
            }
        }
        for child in [&array.additional_items, &array.contains]
            .into_iter()
            .flatten()
        {
            visit(child, definitions, closed, protected, visited);
        }
    }
}

fn is_nullable_wrapper(subschemas: &SubschemaValidation) -> bool {
    subschemas.one_of.as_ref().is_some_and(|branches| {
        branches.len() == 2 && branches.iter().filter(|s| is_null(s)).count() == 1
    }) && subschemas.all_of.is_none()
        && subschemas.any_of.is_none()
        && subschemas.not.is_none()
        && subschemas.if_schema.is_none()
        && subschemas.then_schema.is_none()
        && subschemas.else_schema.is_none()
}

fn is_null(schema: &Schema) -> bool {
    matches!(schema, Schema::Object(object)
        if object.instance_type == Some(InstanceType::Null.into()))
}
