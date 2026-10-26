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
