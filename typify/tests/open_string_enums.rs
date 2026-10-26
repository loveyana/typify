// Copyright 2026 Oxide Computer Company

use serde_json::json;
use typify::{TypeDetails, TypeEnumVariant, TypeSpace, TypeSpaceSettings};

#[test]
fn late_composition_closes_an_earlier_referenced_enum() {
    let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
    types
        .add_ref_types([(
            "Kind",
            serde_json::from_value(json!({"type": "string", "enum": ["first"]})).unwrap(),
        )])
        .unwrap();
    let has_unknown = |types: &TypeSpace| {
        types.iter_types().any(|ty| {
            if let TypeDetails::Enum(e) = ty.details() {
                return e
                    .variants()
                    .any(|(_, variant)| matches!(variant, TypeEnumVariant::Tuple(_)));
            }
            false
        })
    };
    assert!(has_unknown(&types));
    types
        .add_type_with_name(
            &serde_json::from_value(json!({
                "oneOf": [
                    {"type": "object", "properties": {"kind": {"$ref": "#/definitions/Kind"}}},
                    {"type": "integer"}
                ]
            }))
            .unwrap(),
            Some("Choice".into()),
        )
        .unwrap();
    let kind = types.iter_types().find(|ty| ty.name() == "Kind").unwrap();
    let TypeDetails::Enum(kind) = kind.details() else {
        panic!("Kind must remain an enum")
    };
    assert!(kind
        .variants()
        .all(|(_, variant)| matches!(variant, TypeEnumVariant::Simple)));
}

#[test]
fn invalid_defaults_do_not_become_unknown_variants() {
    for default in [json!("future"), json!({"Unknown": "future"})] {
        let schema = json!({
            "title": "Kind",
            "type": "string",
            "enum": ["known"],
            "default": default
        });
        let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
        assert!(types
            .add_root_schema(serde_json::from_value(schema).unwrap())
            .is_err());
    }
}

fn enum_has_fallback(types: &TypeSpace, name: &str) -> bool {
    let ty = types.iter_types().find(|ty| ty.name() == name).unwrap();
    let TypeDetails::Enum(details) = ty.details() else {
        panic!("{name} must be an enum")
    };
    let has_fallback = details
        .variants()
        .any(|(_, variant)| matches!(variant, TypeEnumVariant::Tuple(_)));
    has_fallback
}

#[test]
fn unrelated_literal_sets_do_not_close_plain_enums() {
    let schema = json!({
        "title":"Response", "type":"object", "properties":{
            "status":{"$ref":"#/definitions/Status"},
            "choice":{"$ref":"#/definitions/Choice"}
        },
        "definitions":{
            "Status":{"type":"string", "enum":["draft","active","disabled"]},
            "Tag":{"type":"string", "enum":["draft","active","disabled"]},
            "Choice":{"oneOf":[
                {"type":"object", "properties":{"kind":{"$ref":"#/definitions/Tag"}}},
                {"type":"integer"}
            ]}
        }
    });
    let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
    types
        .add_root_schema(serde_json::from_value(schema).unwrap())
        .unwrap();
    assert!(
        enum_has_fallback(&types, "Status"),
        "ordinary status must not inherit tag fencing by values"
    );
    assert!(
        !enum_has_fallback(&types, "Tag"),
        "actual composition reference remains strict"
    );
}

#[test]
fn late_reference_fence_does_not_close_an_unrelated_same_value_enum() {
    let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
    let schema: schemars::schema::Schema =
        serde_json::from_value(json!({"type":"string", "enum":["first"]})).unwrap();
    types
        .add_ref_types([("Tag", schema.clone()), ("Plain", schema)])
        .unwrap();
    types.add_type_with_name(&serde_json::from_value(json!({
        "oneOf":[{"type":"object", "properties":{"kind":{"$ref":"#/definitions/Tag"}}},{"type":"integer"}]
    })).unwrap(),Some("Choice".into())).unwrap();
    assert!(!enum_has_fallback(&types, "Tag"));
    assert!(enum_has_fallback(&types, "Plain"));
}

#[test]
fn late_alias_object_fence_closes_inline_descendants_only() {
    let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
    let defs = json!({
        "Plain":{"type":"string","enum":["first"]},
        "Object":{"type":"object","properties":{
            "kind":{"type":"string","enum":["first"]},
            "items":{"type":"array","items":{"type":"string","enum":["first"]}}
        }},
        "Alias":{"$ref":"#/definitions/Object"}
    });
    types
        .add_ref_types(defs.as_object().unwrap().iter().map(|(name, value)| {
            (
                name.as_str(),
                serde_json::from_value::<schemars::schema::Schema>(value.clone()).unwrap(),
            )
        }))
        .unwrap();
    assert!(enum_has_fallback(&types, "ObjectKind"));
    types
        .add_type_with_name(
            &serde_json::from_value(json!({
                "oneOf":[{"$ref":"#/definitions/Alias"},{"type":"integer"}]
            }))
            .unwrap(),
            Some("Choice".into()),
        )
        .unwrap();
    assert!(!enum_has_fallback(&types, "ObjectKind"));
    assert!(!enum_has_fallback(&types, "ObjectItemsItem"));
    assert!(enum_has_fallback(&types, "Plain"));
}

#[test]
fn incompatible_open_closed_generated_name_reuse_is_rejected() {
    let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
    let plain =
        serde_json::from_value(json!({"title":"Shared","type":"string","enum":["first"]})).unwrap();
    types.add_type(&plain).unwrap();
    let protected = serde_json::from_value(json!({"title":"Container", "oneOf":[
        {"type":"object","properties":{"kind":{"title":"Shared","type":"string","enum":["first"]}}},
        {"type":"integer"}
    ]}))
    .unwrap();
    assert!(
        types.add_type(&protected).is_err(),
        "closed frame cannot reuse unrelated open named enum"
    );
    assert!(
        enum_has_fallback(&types, "Shared"),
        "reject rather than close unrelated origin"
    );
    let next =
        serde_json::from_value(json!({"title":"AfterError","type":"string","enum":["next"]}))
            .unwrap();
    types.add_type(&next).unwrap();
    assert!(
        enum_has_fallback(&types, "AfterError"),
        "error must restore conversion context"
    );
}

#[test]
fn nullable_named_enum_reuse_checks_before_inner_assignment() {
    let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
    types
        .add_type(
            &serde_json::from_value(json!({"title":"Shared","type":"string","enum":["first"]}))
                .unwrap(),
        )
        .unwrap();
    let protected = json!({"title":"Container", "oneOf":[
        {"type":"object","properties":{"kind":{"title":"Shared","type":["string","null"],"enum":["first",null]}}},
        {"type":"integer"}
    ]});
    assert!(types
        .add_type(&serde_json::from_value(protected).unwrap())
        .is_err());
    assert!(enum_has_fallback(&types, "Shared"));
}

#[test]
fn closure_reuse_mismatch_is_rejected_in_reverse_direction() {
    let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
    types.add_type(&serde_json::from_value(json!({"title":"Container", "oneOf":[
        {"type":"object","properties":{"kind":{"title":"Shared","type":"string","enum":["first"]}}},
        {"type":"integer"}
    ]})).unwrap()).unwrap();
    assert!(!enum_has_fallback(&types, "Shared"));
    let plain =
        serde_json::from_value(json!({"title":"Shared","type":"string","enum":["first"]})).unwrap();
    assert!(
        types.add_type(&plain).is_err(),
        "open frame cannot silently receive a closed unrelated type"
    );
}

#[test]
fn late_reference_fence_visits_map_and_cyclic_object_descendants() {
    let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
    let defs = json!({
        "Object":{"type":"object","properties":{
            "kind":{"type":"string","enum":["first"]},
            "next":{"$ref":"#/definitions/Object"},
            "map":{"type":"object","additionalProperties":{"type":"string","enum":["first"]}}
        }},
        "Plain":{"type":"string","enum":["first"]}
    });
    types
        .add_ref_types(defs.as_object().unwrap().iter().map(|(name, value)| {
            (
                name.as_str(),
                serde_json::from_value::<schemars::schema::Schema>(value.clone()).unwrap(),
            )
        }))
        .unwrap();
    types
        .add_type(
            &serde_json::from_value(json!({"title":"Choice","oneOf":[
                {"$ref":"#/definitions/Object"},{"type":"integer"}
            ]}))
            .unwrap(),
        )
        .unwrap();
    assert!(!enum_has_fallback(&types, "ObjectKind"));
    assert!(!enum_has_fallback(&types, "ObjectMapValue"));
    assert!(enum_has_fallback(&types, "Plain"));
}

#[test]
fn disjoint_object_inheritance_does_not_fence_plain_fields() {
    let schema = json!({"title":"Root", "type":"object", "definitions":{
        "Status":{"type":"string","enum":["draft","active","disabled"]},
        "Base":{"type":"object","properties":{"status":{"$ref":"#/definitions/Status"}}},
        "Extended":{"allOf":[{"$ref":"#/definitions/Base"},{"type":"object","properties":{"id":{"type":"string"}}}]}
    }});
    let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
    types
        .add_root_schema(serde_json::from_value(schema).unwrap())
        .unwrap();
    assert!(
        enum_has_fallback(&types, "Status"),
        "object extension is not enum branch selection"
    );
}

#[test]
fn object_extension_exception_preserves_cross_property_constraints() {
    for constraint in [
        json!({"additionalProperties":false}),
        json!({"additionalProperties":{"type":"string","enum":["first"]}}),
    ] {
        let mut right = json!({"type":"object","properties":{"id":{"type":"string"}}});
        right
            .as_object_mut()
            .unwrap()
            .extend(constraint.as_object().unwrap().clone());
        let schema = json!({"title":"Root","type":"object","definitions":{
            "Status":{"type":"string","enum":["first"]},
            "Extended":{"allOf":[{"type":"object","properties":{"status":{"$ref":"#/definitions/Status"}}},right]}
        }});
        let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
        // Some combinations are impossible; either generator rejection or a
        // strict enum is acceptable, never widened unknown acceptance.
        if types
            .add_root_schema(serde_json::from_value(schema).unwrap())
            .is_ok()
        {
            assert!(
                !enum_has_fallback(&types, "Status"),
                "cross-property {constraint}"
            );
        }
    }
}

#[test]
fn pure_object_extension_inside_real_union_remains_protected() {
    let schema = json!({"title":"Root","type":"object","definitions":{
        "Kind":{"type":"string","enum":["first"]},
        "Plain":{"type":"string","enum":["first"]},
        "Choice":{"oneOf":[{"allOf":[
            {"type":"object","properties":{"kind":{"$ref":"#/definitions/Kind"}}},
            {"type":"object","properties":{"id":{"type":"string"}}}
        ]},{"type":"integer"}]}
    }});
    let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
    types
        .add_root_schema(serde_json::from_value(schema).unwrap())
        .unwrap();
    assert!(!enum_has_fallback(&types, "Kind"));
    assert!(enum_has_fallback(&types, "Plain"));
}

#[test]
fn overlapping_object_enum_intersection_stays_strict() {
    let schema = json!({"title":"Root","type":"object","definitions":{
        "Kind":{"type":"string","enum":["first","second"]},
        "Extended":{"allOf":[
            {"type":"object","properties":{"kind":{"$ref":"#/definitions/Kind"}}},
            {"type":"object","properties":{"kind":{"type":"string","enum":["first"]}}}
        ]}
    }});
    let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
    types
        .add_root_schema(serde_json::from_value(schema).unwrap())
        .unwrap();
    assert!(!enum_has_fallback(&types, "Kind"));
}

#[test]
fn singleton_object_wrapper_keeps_denied_additional_properties_without_enum_fencing() {
    let schema = json!({"title":"Root","type":"object","definitions":{
        "Kind":{"type":"string","enum":["first"]},
        "Body":{"type":"object","additionalProperties":false,"properties":{"kind":{"$ref":"#/definitions/Kind"}}},
        "Wrapped":{"allOf":[{"$ref":"#/definitions/Body"}]}
    }});
    let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
    types
        .add_root_schema(serde_json::from_value(schema).unwrap())
        .unwrap();
    assert!(enum_has_fallback(&types, "Kind"));
}

#[test]
fn nullable_hinted_collision_is_not_swallowed_by_union_probe() {
    for keyword in ["anyOf", "oneOf"] {
        let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
        types.add_type(&serde_json::from_value(json!({"title":"Container","oneOf":[
            {"type":"object","properties":{"kind":{"title":"Shared","type":"string","enum":["first"]}}},
            {"type":"integer"}
        ]})).unwrap()).unwrap();
        let nullable = json!({keyword:[{"type":"string","enum":["first"]},{"type":"null"}]});
        let error = types
            .add_type_with_name(
                &serde_json::from_value(nullable).unwrap(),
                Some("Shared".into()),
            )
            .expect_err("confirmed nullable conversion must propagate incompatible reuse");
        assert!(matches!(error, typify::Error::InvalidSchema { .. }));
        types
            .add_type(
                &serde_json::from_value(
                    json!({"title":"AfterError","type":"string","enum":["next"]}),
                )
                .unwrap(),
            )
            .unwrap();
        assert!(enum_has_fallback(&types, "AfterError"));
    }
}

#[test]
fn constrained_reference_collision_returns_error_and_restores_scope() {
    let mut types = TypeSpace::new(TypeSpaceSettings::default().with_open_string_enums(true));
    types
        .add_ref_types([(
            "Base",
            serde_json::from_value(json!({"type":"string","enum":["first"]})).unwrap(),
        )])
        .unwrap();
    types.add_type(&serde_json::from_value(json!({"title":"Container","oneOf":[
        {"type":"object","properties":{"kind":{"title":"Shared","type":"string","enum":["first"]}}},
        {"type":"integer"}
    ]})).unwrap()).unwrap();
    let error = types
        .add_type_with_name(
            &serde_json::from_value(json!({"$ref":"#/definitions/Base", "minLength":1})).unwrap(),
            Some("Shared".into()),
        )
        .expect_err("reference-with-siblings collision must be a Result, not panic");
    assert!(matches!(error, typify::Error::InvalidSchema { .. }));
    types
        .add_type(
            &serde_json::from_value(
                json!({"title":"AfterRefError","type":"string","enum":["next"]}),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(enum_has_fallback(&types, "AfterRefError"));
}
