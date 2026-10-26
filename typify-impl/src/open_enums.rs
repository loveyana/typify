// Copyright 2026 Oxide Computer Company

use std::collections::{BTreeMap, BTreeSet};

use schemars::schema::{InstanceType, Schema, SchemaObject, SingleOrVec, SubschemaValidation};

use crate::{util::ref_key, RefKey};

// Protection follows schema references, not matching literal values. Inline
// and synthesized enums inherit the scope of their composing conversion.
pub(crate) fn collect_closed_enums(
    schema: &Schema,
    definitions: &BTreeMap<RefKey, Schema>,
    closed: &mut BTreeSet<RefKey>,
) {
    visit(schema, definitions, closed, false, &mut BTreeSet::new());
}

fn visit(
    schema: &Schema,
    definitions: &BTreeMap<RefKey, Schema>,
    closed: &mut BTreeSet<RefKey>,
    protected: bool,
    visited: &mut BTreeSet<(RefKey, bool)>,
) {
    let Schema::Object(object) = schema else {
        return;
    };
    let protected = protected || requires_closed_context(schema, definitions);
    if let Some(reference) = &object.reference {
        let key = ref_key(reference);
        if protected {
            closed.insert(key.clone());
        }
        if visited.insert((key.clone(), protected)) {
            if let Some(referenced) = definitions.get(&key) {
                visit(referenced, definitions, closed, protected, visited);
            }
        }
    }
    if let Some(subschemas) = &object.subschemas {
        for branches in [&subschemas.all_of, &subschemas.any_of, &subschemas.one_of]
            .into_iter()
            .flatten()
        {
            for branch in branches {
                visit(branch, definitions, closed, protected, visited);
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

pub(crate) fn requires_closed_context(
    schema: &Schema,
    definitions: &BTreeMap<RefKey, Schema>,
) -> bool {
    let Schema::Object(object) = schema else {
        return false;
    };
    object.extensions.contains_key("discriminator")
        || object.subschemas.as_deref().is_some_and(|subschemas| {
            !is_nullable_wrapper(subschemas)
                && !is_object_extension(object, definitions)
                && (subschemas.all_of.is_some()
                    || subschemas.any_of.is_some()
                    || subschemas.one_of.is_some()
                    || subschemas.not.is_some()
                    || subschemas.if_schema.is_some()
                    || subschemas.then_schema.is_some()
                    || subschemas.else_schema.is_some())
        })
}

// Disjoint object-only allOf extends a record; it does not select a branch or
// narrow an enum. Cross-property constraints stay on the conservative path.
fn is_object_extension(object: &SchemaObject, definitions: &BTreeMap<RefKey, Schema>) -> bool {
    let Some(subschemas) = &object.subschemas else {
        return false;
    };
    let Some(branches) = &subschemas.all_of else {
        return false;
    };
    if branches.is_empty()
        || subschemas.any_of.is_some()
        || subschemas.one_of.is_some()
        || subschemas.not.is_some()
        || subschemas.if_schema.is_some()
        || subschemas.then_schema.is_some()
        || subschemas.else_schema.is_some()
    {
        return false;
    }
    // allOf of one branch with no outer constraints is just a wrapper,
    // including an object which denies additional fields.
    if branches.len() == 1
        && object.instance_type.is_none()
        && object.object.is_none()
        && object.enum_values.is_none()
        && object.const_value.is_none()
        && object.reference.is_none()
        && object.number.is_none()
        && object.string.is_none()
        && object.array.is_none()
        && object.format.is_none()
    {
        return true;
    }
    let mut names = BTreeSet::new();
    let outer = Schema::Object(SchemaObject {
        subschemas: None,
        ..object.clone()
    });
    // An absent outer body contributes no object constraints.
    if object.object.is_some() || object.instance_type.is_some() {
        if !add_object_properties(&outer, definitions, &mut names, &mut BTreeSet::new()) {
            return false;
        }
    } else if object.enum_values.is_some()
        || object.const_value.is_some()
        || object.reference.is_some()
        || object.number.is_some()
        || object.string.is_some()
        || object.array.is_some()
        || object.format.is_some()
    {
        return false;
    }
    branches
        .iter()
        .all(|branch| add_object_properties(branch, definitions, &mut names, &mut BTreeSet::new()))
}

fn add_object_properties(
    schema: &Schema,
    definitions: &BTreeMap<RefKey, Schema>,
    names: &mut BTreeSet<String>,
    visited: &mut BTreeSet<RefKey>,
) -> bool {
    let Schema::Object(object) = schema else {
        return false;
    };
    if object.enum_values.is_some()
        || object.const_value.is_some()
        || object.subschemas.is_some()
        || object.extensions.contains_key("discriminator")
        || object.number.is_some()
        || object.string.is_some()
        || object.array.is_some()
        || object.format.is_some()
    {
        return false;
    }
    if let Some(reference) = &object.reference {
        // A reference with structural siblings is not a pure alias.
        if object.instance_type.is_some() || object.object.is_some() {
            return false;
        }
        let key = ref_key(reference);
        if !visited.insert(key.clone()) {
            return false;
        }
        return definitions
            .get(&key)
            .is_some_and(|target| add_object_properties(target, definitions, names, visited));
    }
    if object.instance_type != Some(InstanceType::Object.into())
        && !(object.instance_type.is_none() && object.object.is_some())
    {
        return false;
    }
    let Some(fields) = &object.object else {
        return true;
    };
    fields.max_properties.is_none()
        && fields.min_properties.is_none()
        && fields.pattern_properties.is_empty()
        && fields.property_names.is_none()
        && matches!(
            fields.additional_properties.as_deref(),
            None | Some(Schema::Bool(true))
        )
        && fields
            .properties
            .keys()
            .all(|name| names.insert(name.clone()))
}

fn is_nullable_wrapper(subschemas: &SubschemaValidation) -> bool {
    let branches = match (&subschemas.one_of, &subschemas.any_of) {
        (Some(branches), None) | (None, Some(branches)) => branches,
        _ => return false,
    };
    branches.len() == 2
        && branches.iter().filter(|s| is_null(s)).count() == 1
        && subschemas.all_of.is_none()
        && subschemas.not.is_none()
        && subschemas.if_schema.is_none()
        && subschemas.then_schema.is_none()
        && subschemas.else_schema.is_none()
}

fn is_null(schema: &Schema) -> bool {
    matches!(schema, Schema::Object(object)
        if object.instance_type == Some(InstanceType::Null.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_extension_requires_pure_resolved_refs_and_outer_body() {
        let mut defs = BTreeMap::new();
        defs.insert(
            RefKey::Def("Base".into()),
            serde_json::from_value(json!({
                "type":"object", "properties":{"status":{"type":"string","enum":["first"]}}
            }))
            .unwrap(),
        );
        let pure = json!({"allOf":[{"$ref":"#/definitions/Base"},{"type":"object","properties":{"id":{"type":"string"}}}]});
        assert!(!requires_closed_context(
            &serde_json::from_value(pure.clone()).unwrap(),
            &defs
        ));
        let mut outer = pure.clone();
        outer["type"] = json!("object");
        outer["properties"] = json!({"status":{"type":"string"}});
        assert!(requires_closed_context(
            &serde_json::from_value(outer).unwrap(),
            &defs
        ));
        let mut sibling = pure.clone();
        sibling["allOf"][0]["additionalProperties"] = json!(false);
        assert!(requires_closed_context(
            &serde_json::from_value(sibling).unwrap(),
            &defs
        ));
        for constraint in [
            json!({"patternProperties":{".*":{"type":"string","enum":["first"]}}}),
            json!({"propertyNames":{"enum":["status"]}}),
        ] {
            let mut constrained = pure.clone();
            constrained["allOf"][1]
                .as_object_mut()
                .unwrap()
                .extend(constraint.as_object().unwrap().clone());
            assert!(requires_closed_context(
                &serde_json::from_value(constrained).unwrap(),
                &defs
            ));
        }
        defs.insert(
            RefKey::Def("Base".into()),
            serde_json::from_value(json!({"$ref":"#/definitions/Base"})).unwrap(),
        );
        assert!(requires_closed_context(
            &serde_json::from_value(pure).unwrap(),
            &defs
        ));
    }
}
