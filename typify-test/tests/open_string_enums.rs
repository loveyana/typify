// Copyright 2026 Oxide Computer Company

#[allow(dead_code)]
mod open {
    include!(concat!(env!("OUT_DIR"), "/open_string_enums.rs"));
}

#[allow(dead_code)]
mod closed {
    include!(concat!(env!("OUT_DIR"), "/closed_string_enums.rs"));
}

#[test]
fn unknown_strings_round_trip_without_normalization() {
    for wire in ["future_provider", "", " API_KEY ", "未来供应商", "unknown"] {
        let raw = serde_json::to_string(wire).unwrap();
        let decoded: open::ProviderKind =
            serde_json::from_str(&raw).expect("opt-in enum must accept unknown string values");
        assert_eq!(decoded.to_string(), wire);
        assert_eq!(serde_json::to_string(&decoded).unwrap(), raw);
        let parsed: open::ProviderKind = wire.parse().unwrap();
        assert_eq!(parsed, decoded);
    }
}

#[test]
fn default_stays_closed_and_known_values_stay_known() {
    let known: open::ProviderKind = serde_json::from_str(r#""api_key""#).unwrap();
    assert_eq!(known, open::ProviderKind::ApiKey);
    assert_eq!(open::ProviderKind::default(), known);
    assert!(serde_json::from_str::<closed::ProviderKind>(r#""future_provider""#).is_err());
    assert!("future_provider".parse::<closed::ProviderKind>().is_err());
}

#[test]
fn nested_inline_optional_and_nullable_values_decode() {
    let raw = serde_json::json!({
        "provider": "future_provider",
        "providers": ["api_key", "another_provider"],
        "inline_state": "future_state",
        "optional_provider": "optional_future",
        "nullable_kind": "future_kind",
        "collision": "future_collision"
    });
    let decoded: open::OpenEnumResponse = serde_json::from_value(raw.clone()).unwrap();
    let encoded = serde_json::to_value(&decoded).unwrap();
    for key in raw.as_object().unwrap().keys() {
        assert_eq!(encoded[key], raw[key], "{key}");
    }
    let nulls: open::OpenEnumResponse = serde_json::from_value(serde_json::json!({
        "provider": "api_key",
        "providers": [],
        "inline_state": "done",
        "nullable_kind": null
    }))
    .unwrap();
    assert!(nulls.optional_provider.is_none());
    assert!(nulls.nullable_kind.is_none());
}

#[test]
fn wrong_json_types_and_numeric_enums_remain_rejected() {
    for raw in ["null", "42", "true", "{}", "[]"] {
        assert!(
            serde_json::from_str::<open::ProviderKind>(raw).is_err(),
            "{raw}"
        );
    }
    assert!(serde_json::from_str::<open::Numeric>("3").is_err());
    assert!(serde_json::from_str::<open::Numeric>("1").is_ok());
}

#[test]
fn unknown_strings_still_obey_non_enum_constraints() {
    assert!(serde_json::from_str::<open::Constrained>(r#""future""#).is_ok());
    for raw in [r#""x""#, r#""toolongvalue""#, r#""UPPER""#] {
        assert!(
            serde_json::from_str::<open::Constrained>(raw).is_err(),
            "{raw}"
        );
    }
}

#[test]
fn schema_unknown_variant_names_do_not_collide() {
    for wire in ["unknown", "unknown_value", "future"] {
        let raw = serde_json::to_string(wire).unwrap();
        let decoded: open::Collision = serde_json::from_str(&raw).unwrap();
        assert_eq!(decoded.to_string(), wire);
        assert_eq!(serde_json::to_string(&decoded).unwrap(), raw);
    }
    assert_eq!(
        "unknown".parse::<open::Collision>().unwrap(),
        open::Collision::Unknown
    );
    assert_eq!(
        "unknown_value".parse::<open::Collision>().unwrap(),
        open::Collision::UnknownValue
    );
}

#[test]
fn union_tag_enums_remain_closed_including_referenced_tags() {
    let first = serde_json::json!({"kind": "first", "value": "data"});
    assert!(serde_json::from_value::<open::Choice>(first).is_ok());
    for kind in ["second", "future_kind"] {
        let invalid = serde_json::json!({"kind": kind, "value": "data"});
        assert!(
            serde_json::from_value::<open::Choice>(invalid).is_err(),
            "{kind}"
        );
    }
    assert!("future_kind".parse::<open::FirstKind>().is_err());
}

#[test]
fn merged_string_compositions_remain_closed() {
    assert!(serde_json::from_str::<open::Combined>(r#""combined_a""#).is_ok());
    assert!(serde_json::from_str::<open::Combined>(r#""combined_b""#).is_ok());
    assert!(serde_json::from_str::<open::Combined>(r#""future""#).is_err());
    assert!(serde_json::from_str::<open::Intersection>(r#""shared""#).is_ok());
    assert!(serde_json::from_str::<open::Intersection>(r#""left""#).is_err());
    assert!(serde_json::from_str::<open::Intersection>(r#""future""#).is_err());
}

#[test]
fn protected_constants_and_composing_node_values_remain_closed() {
    assert!(serde_json::from_str::<open::ConstantComposition>(r#""constant_a""#).is_ok());
    assert!(serde_json::from_str::<open::ConstantComposition>(r#""constant_b""#).is_err());
    assert!(serde_json::from_str::<open::NegatedEnum>(r#""negation_a""#).is_ok());
    assert!(serde_json::from_str::<open::NegatedEnum>(r#""negation_b""#).is_err());
}

#[test]
fn anyof_nullable_enum_preserves_unknown_and_null() {
    let value: open::AnyNullable = serde_json::from_str(r#""future_nullable""#).unwrap();
    assert_eq!(
        serde_json::to_string(&value).unwrap(),
        r#""future_nullable""#
    );
    let value: open::AnyNullable = serde_json::from_str("null").unwrap();
    assert_eq!(serde_json::to_string(&value).unwrap(), "null");
    assert!(serde_json::from_str::<closed::AnyNullable>(r#""future_nullable""#).is_err());
}
