use pretty_assertions::assert_eq;
use quick_xml::escape::{self, EscapeError, ParseCharRefError};
use std::borrow::Cow;
use std::num::IntErrorKind;

#[test]
fn escape() {
    let unchanged = escape::escape("test");
    // assert_eq does not check that Cow is borrowed, but we explicitly use Cow
    // because it influences diff
    // TODO: use assert_matches! when stabilized and other features will bump MSRV
    assert_eq!(unchanged, Cow::Borrowed("test"));
    assert!(matches!(unchanged, Cow::Borrowed(_)));

    assert_eq!(escape::escape("<&\"'>"), "&lt;&amp;&quot;&apos;&gt;");
    assert_eq!(escape::escape("<test>"), "&lt;test&gt;");
    assert_eq!(escape::escape("\"a\"bc"), "&quot;a&quot;bc");
    assert_eq!(escape::escape("\"a\"b&c"), "&quot;a&quot;b&amp;c");
    assert_eq!(
        escape::escape("prefix_\"a\"b&<>c"),
        "prefix_&quot;a&quot;b&amp;&lt;&gt;c"
    );
    assert_eq!(escape::escape("hello\rworld"), "hello&#13;world");
    assert_eq!(escape::escape("a\r\nb"), "a&#13;\nb");
}

#[test]
fn partial_escape() {
    let unchanged = escape::partial_escape("test");
    // assert_eq does not check that Cow is borrowed, but we explicitly use Cow
    // because it influences diff
    // TODO: use assert_matches! when stabilized and other features will bump MSRV
    assert_eq!(unchanged, Cow::Borrowed("test"));
    assert!(matches!(unchanged, Cow::Borrowed(_)));

    assert_eq!(escape::partial_escape("<&\"'>"), "&lt;&amp;\"'&gt;");
    assert_eq!(escape::partial_escape("<test>"), "&lt;test&gt;");
    assert_eq!(escape::partial_escape("\"a\"bc"), "\"a\"bc");
    assert_eq!(escape::partial_escape("\"a\"b&c"), "\"a\"b&amp;c");
    assert_eq!(
        escape::partial_escape("prefix_\"a\"b&<>c"),
        "prefix_\"a\"b&amp;&lt;&gt;c"
    );
    assert_eq!(escape::partial_escape("hello\rworld"), "hello&#13;world");
}

#[test]
fn minimal_escape() {
    assert_eq!(escape::minimal_escape("test"), Cow::Borrowed("test"));
    assert_eq!(escape::minimal_escape("<&\"'>"), "&lt;&amp;\"'>");
    assert_eq!(escape::minimal_escape("<test>"), "&lt;test>");
    assert_eq!(escape::minimal_escape("\"a\"bc"), "\"a\"bc");
    assert_eq!(escape::minimal_escape("\"a\"b&c"), "\"a\"b&amp;c");
    assert_eq!(
        escape::minimal_escape("prefix_\"a\"b&<>c"),
        "prefix_\"a\"b&amp;&lt;>c"
    );
    assert_eq!(escape::minimal_escape("hello\rworld"), "hello&#13;world");
    // `]]>` is not allowed in text content
    assert_eq!(escape::minimal_escape("a]]>b"), "a]]&gt;b");
    assert_eq!(escape::minimal_escape("a]]]>b]>c] ]>"), "a]]]&gt;b]>c] ]>");
    assert_eq!(escape::minimal_escape("]]<>"), "]]&lt;>");
}

#[test]
fn unescape() {
    let unchanged = escape::unescape("test");
    // assert_eq does not check that Cow is borrowed, but we explicitly use Cow
    // because it influences diff
    // TODO: use assert_matches! when stabilized and other features will bump MSRV
    assert_eq!(unchanged, Ok(Cow::Borrowed("test")));
    assert!(matches!(unchanged, Ok(Cow::Borrowed(_))));

    assert_eq!(
        escape::unescape("&lt;&amp;test&apos;&quot;&gt;"),
        Ok("<&test'\">".into())
    );
    assert_eq!(escape::unescape("&#x30;"), Ok("0".into()));
    assert_eq!(escape::unescape("&#48;"), Ok("0".into()));
    assert_eq!(
        escape::unescape("&foo;"),
        Err(EscapeError::UnrecognizedEntity(1..4, "foo".into()))
    );
}

/// XML allows any number of leading zeroes. That is not explicitly mentioned
/// in the specification, but enforced by the conformance test suite
/// (https://www.w3.org/XML/Test/)
/// 100 digits should be enough to ensure that any artificial restrictions
/// (such as maximal string of u128 representation) does not applied
#[test]
fn unescape_long() {
    assert_eq!(
        escape::unescape(
            "&#0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000048;"
        ),
        Ok("0".into()),
    );
    assert_eq!(
        escape::unescape(
            "&#x0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000030;"
        ),
        Ok("0".into()),
    );

    // Too big numbers for u32 should produce errors
    match escape::unescape(&format!("&#{};", u32::MAX as u64 + 1)) {
        Err(EscapeError::InvalidCharRef(ParseCharRefError::InvalidNumber(err))) => {
            assert_eq!(err.kind(), &IntErrorKind::PosOverflow)
        }
        x => panic!(
            "expected Err(InvalidCharRef(InvalidNumber(PosOverflow))), bug got {:?}",
            x
        ),
    }
    match escape::unescape(&format!("&#x{:x};", u32::MAX as u64 + 1)) {
        Err(EscapeError::InvalidCharRef(ParseCharRefError::InvalidNumber(err))) => {
            assert_eq!(err.kind(), &IntErrorKind::PosOverflow)
        }
        x => panic!(
            "expected Err(InvalidCharRef(InvalidNumber(PosOverflow))), bug got {:?}",
            x
        ),
    }
}

#[test]
fn unescape_sign() {
    assert_eq!(
        escape::unescape("&#+48;"),
        Err(EscapeError::InvalidCharRef(
            ParseCharRefError::UnexpectedSign
        )),
    );
    assert_eq!(
        escape::unescape("&#x+30;"),
        Err(EscapeError::InvalidCharRef(
            ParseCharRefError::UnexpectedSign
        )),
    );

    assert_eq!(
        escape::unescape("&#-48;"),
        Err(EscapeError::InvalidCharRef(
            ParseCharRefError::UnexpectedSign
        )),
    );
    assert_eq!(
        escape::unescape("&#x-30;"),
        Err(EscapeError::InvalidCharRef(
            ParseCharRefError::UnexpectedSign
        )),
    );
}

#[test]
fn unescape_illegal_char() {
    for (raw, code) in [
        ("&#0;", 0x0),
        ("&#x0;", 0x0),
        ("&#65534;", 0xFFFE),
        ("&#xFFFE;", 0xFFFE),
        ("&#65535;", 0xFFFF),
        ("&#xFFFF;", 0xFFFF),
    ] {
        assert_eq!(
            escape::unescape(raw),
            Err(EscapeError::InvalidCharRef(
                ParseCharRefError::IllegalCharacter(code)
            )),
            "{raw}",
        );
    }
    // Neighbours of the excluded code points are still allowed
    assert_eq!(escape::unescape("&#xFFFD;"), Ok("\u{FFFD}".into()));
    assert_eq!(escape::unescape("&#x10000;"), Ok("\u{10000}".into()));
}

#[test]
fn unescape_with() {
    let custom_entities = |ent: &str| match ent {
        "foo" => Some("BAR"),
        _ => None,
    };

    let unchanged = escape::unescape_with("test", custom_entities);
    // assert_eq does not check that Cow is borrowed, but we explicitly use Cow
    // because it influences diff
    // TODO: use assert_matches! when stabilized and other features will bump MSRV
    assert_eq!(unchanged, Ok(Cow::Borrowed("test")));
    assert!(matches!(unchanged, Ok(Cow::Borrowed(_))));

    assert_eq!(
        escape::unescape_with("&lt;", custom_entities),
        Err(EscapeError::UnrecognizedEntity(1..3, "lt".into())),
    );
    assert_eq!(
        escape::unescape_with("&#x30;", custom_entities),
        Ok("0".into())
    );
    assert_eq!(
        escape::unescape_with("&#48;", custom_entities),
        Ok("0".into())
    );
    assert_eq!(
        escape::unescape_with("&foo;", custom_entities),
        Ok("BAR".into())
    );
    assert_eq!(
        escape::unescape_with("&fop;", custom_entities),
        Err(EscapeError::UnrecognizedEntity(1..4, "fop".into()))
    );
}

/// XML allows any number of leading zeroes. That is not explicitly mentioned
/// in the specification, but enforced by the conformance test suite
/// (https://www.w3.org/XML/Test/)
/// 100 digits should be enough to ensure that any artificial restrictions
/// (such as maximal string of u128 representation) does not applied
#[test]
fn unescape_with_long() {
    assert_eq!(
        escape::unescape_with(
            "&#0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000048;",
            |_| None
        ),
        Ok("0".into()),
    );
    assert_eq!(
        escape::unescape_with(
            "&#x0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000030;",
            |_| None
        ),
        Ok("0".into()),
    );

    // Too big numbers for u32 should produce errors
    match escape::unescape_with(&format!("&#{};", u32::MAX as u64 + 1), |_| None) {
        Err(EscapeError::InvalidCharRef(ParseCharRefError::InvalidNumber(err))) => {
            assert_eq!(err.kind(), &IntErrorKind::PosOverflow)
        }
        x => panic!(
            "expected Err(InvalidCharRef(InvalidNumber(PosOverflow))), bug got {:?}",
            x
        ),
    }
    match escape::unescape_with(&format!("&#x{:x};", u32::MAX as u64 + 1), |_| None) {
        Err(EscapeError::InvalidCharRef(ParseCharRefError::InvalidNumber(err))) => {
            assert_eq!(err.kind(), &IntErrorKind::PosOverflow)
        }
        x => panic!(
            "expected Err(InvalidCharRef(InvalidNumber(PosOverflow))), bug got {:?}",
            x
        ),
    }
}

#[test]
fn unescape_with_sign() {
    assert_eq!(
        escape::unescape_with("&#+48;", |_| None),
        Err(EscapeError::InvalidCharRef(
            ParseCharRefError::UnexpectedSign
        )),
    );
    assert_eq!(
        escape::unescape_with("&#x+30;", |_| None),
        Err(EscapeError::InvalidCharRef(
            ParseCharRefError::UnexpectedSign
        )),
    );

    assert_eq!(
        escape::unescape_with("&#-48;", |_| None),
        Err(EscapeError::InvalidCharRef(
            ParseCharRefError::UnexpectedSign
        )),
    );
    assert_eq!(
        escape::unescape_with("&#x-30;", |_| None),
        Err(EscapeError::InvalidCharRef(
            ParseCharRefError::UnexpectedSign
        )),
    );
}

/// Values from <https://html.spec.whatwg.org/entities.json>: at least one entity
/// from each Unicode block that had wrong values, and some neighbours that did not.
#[test]
#[cfg(feature = "escape-html")]
fn resolve_html5_entity() {
    use escape::resolve_html5_entity as resolve;

    // Latin Extended-A and -B
    assert_eq!(resolve("Amacr"), Some("\u{100}"));
    assert_eq!(resolve("scaron"), Some("\u{161}"));
    assert_eq!(resolve("fnof"), Some("\u{192}"));
    // Spacing modifier letters and combining marks
    assert_eq!(resolve("circ"), Some("\u{2C6}"));
    assert_eq!(resolve("DownBreve"), Some("\u{311}"));
    // Greek, including the curly and straight forms of epsilon and phi
    assert_eq!(resolve("alpha"), Some("\u{3B1}"));
    assert_eq!(resolve("Omega"), Some("\u{3A9}"));
    assert_eq!(resolve("epsilon"), Some("\u{3B5}"));
    assert_eq!(resolve("epsi"), Some("\u{3B5}"));
    assert_eq!(resolve("epsiv"), Some("\u{3F5}"));
    assert_eq!(resolve("phi"), Some("\u{3C6}"));
    assert_eq!(resolve("phiv"), Some("\u{3D5}"));
    // Cyrillic
    assert_eq!(resolve("Acy"), Some("\u{410}"));
    assert_eq!(resolve("iocy"), Some("\u{451}"));
    // Letterlike symbols and math operators
    assert_eq!(resolve("ohm"), Some("\u{3A9}"));
    assert_eq!(resolve("angst"), Some("\u{C5}"));
    assert_eq!(resolve("OverBar"), Some("\u{203E}"));
    assert_eq!(resolve("UnderBar"), Some("\u{5F}"));
    assert_eq!(resolve("race"), Some("\u{223D}\u{331}"));
    // Mathematical alphanumeric symbols
    assert_eq!(resolve("Ascr"), Some("\u{1D49C}"));
    assert_eq!(resolve("zfr"), Some("\u{1D537}"));
    assert_eq!(resolve("Aopf"), Some("\u{1D538}"));
    assert_eq!(resolve("zopf"), Some("\u{1D56B}"));

    // Neighbours that already had the right value
    assert_eq!(resolve("macr"), Some("\u{AF}"));
    assert_eq!(resolve("strns"), Some("\u{AF}"));
    assert_eq!(resolve("Copf"), Some("\u{2102}"));
    assert_eq!(resolve("semi"), Some(";"));

    assert_eq!(
        escape::unescape("&alpha; &Acy; &Aopf;").unwrap(),
        "\u{3B1} \u{410} \u{1D538}"
    );
}

/// In-memory string fixtures for the composable custom entity expansion
/// strategy. All inputs are small strings; limits must stop the expansion
/// before it can grow into unbounded memory.
mod entity_expansion {
    use quick_xml::escape::{self, EscapeError};
    use quick_xml::escape::{
        DEFAULT_ENTITY_BYTES, DEFAULT_ENTITY_DEPTH, DEFAULT_ENTITY_REFERENCES, EntityExpansion,
    };
    use std::borrow::Cow;

    /// Simple DTD-like entity table driven by an in-memory string fixture.
    fn table<'a>(
        entities: &'a [(&'a str, &'a str)],
    ) -> impl FnMut(&str) -> Option<&'a str> + use<'a> {
        move |name| {
            entities
                .iter()
                .find(|(entity, _)| *entity == name)
                .map(|(_, value)| *value)
        }
    }

    #[test]
    fn defaults_are_conservative() {
        assert_eq!(DEFAULT_ENTITY_DEPTH, 128);
        assert_eq!(DEFAULT_ENTITY_REFERENCES, 10_000);
        assert_eq!(DEFAULT_ENTITY_BYTES, 10 * 1024 * 1024);
        assert_eq!(
            EntityExpansion::default(),
            EntityExpansion {
                max_depth: Some(DEFAULT_ENTITY_DEPTH),
                max_bytes: Some(DEFAULT_ENTITY_BYTES),
                max_references: Some(DEFAULT_ENTITY_REFERENCES),
            }
        );
        assert_eq!(
            EntityExpansion::UNLIMITED,
            EntityExpansion {
                max_depth: None,
                max_bytes: None,
                max_references: None,
            }
        );
    }

    #[test]
    fn builders_compose() {
        let expansion = EntityExpansion::default()
            .with_max_depth(3)
            .with_max_bytes(42)
            .with_max_references(None);
        assert_eq!(expansion.max_depth, Some(3));
        assert_eq!(expansion.max_bytes, Some(42));
        assert_eq!(expansion.max_references, None);
    }

    #[test]
    fn recursively_expands_nested_entities() {
        let entities = [("a", "&b;&b;"), ("b", "&c;&c;"), ("c", "x")];
        let got = escape::unescape_with_limits("&a;", EntityExpansion::default(), table(&entities))
            .unwrap();
        // a -> 2 * b -> 4 * c -> "xxxx"
        assert_eq!(got, "xxxx");
    }

    #[test]
    fn depth_limit_stops_nesting() {
        let entities = [
            ("e1", "&e2;"),
            ("e2", "&e3;"),
            ("e3", "&e4;"),
            ("e4", "deep"),
        ];
        let err = escape::unescape_with_limits(
            "&e1;",
            EntityExpansion::default().with_max_depth(2),
            table(&entities),
        )
        .unwrap_err();
        assert_eq!(err, EscapeError::EntityDepthLimitExceeded(3));

        // The same chain succeeds when the depth covers all four levels
        let got = escape::unescape_with_limits(
            "&e1;",
            EntityExpansion::default().with_max_depth(4),
            table(&entities),
        )
        .unwrap();
        assert_eq!(got, "deep");
    }

    #[test]
    fn byte_limit_stops_amplification() {
        // Each level doubles the output: a tiny input would grow without the
        // byte budget.
        let entities = [
            ("lol", "abcdefgh"),
            ("lol2", "&lol;&lol;"),
            ("lol3", "&lol2;&lol2;"),
            ("lol4", "&lol3;&lol3;"),
        ];
        let err = escape::unescape_with_limits(
            "&lol4;",
            EntityExpansion::default().with_max_bytes(20),
            table(&entities),
        )
        .unwrap_err();
        assert_eq!(err, EscapeError::EntityByteLimitExceeded(20));

        // Literal text is counted against the same budget
        let err = escape::unescape_with_limits(
            "0123456789&lol;",
            EntityExpansion::default().with_max_bytes(12),
            table(&entities),
        )
        .unwrap_err();
        assert_eq!(err, EscapeError::EntityByteLimitExceeded(12));
    }

    #[test]
    fn reference_limit_stops_many_expansions() {
        let entities = [("x", "a")];
        let input = "&x;&x;&x;&x;&x;";
        let err = escape::unescape_with_limits(
            input,
            EntityExpansion::default().with_max_references(Some(3)),
            table(&entities),
        )
        .unwrap_err();
        assert_eq!(err, EscapeError::EntityReferenceLimitExceeded(3));
    }

    #[test]
    fn direct_and_indirect_cycles_are_detected() {
        let direct = [("loop", "&loop;")];
        assert_eq!(
            escape::unescape_with_limits("&loop;", EntityExpansion::default(), table(&direct))
                .unwrap_err(),
            EscapeError::EntityCycleDetected("loop".to_string())
        );

        let indirect = [("a", "&b;"), ("b", "&a;")];
        assert_eq!(
            escape::unescape_with_limits("&a;", EntityExpansion::default(), table(&indirect))
                .unwrap_err(),
            EscapeError::EntityCycleDetected("a".to_string())
        );

        // Reusing an entity name in sibling references is not a cycle
        let siblings = [("a", "x"), ("b", "&a;&a;")];
        assert_eq!(
            escape::unescape_with_limits("&b;", EntityExpansion::default(), table(&siblings))
                .unwrap(),
            "xx"
        );
    }

    #[test]
    fn unknown_entity_remains_an_error() {
        let entities = [("known", "ok")];
        assert_eq!(
            escape::unescape_with_limits(
                "text &missing; end",
                EntityExpansion::default(),
                table(&entities)
            )
            .unwrap_err(),
            EscapeError::UnrecognizedEntity(6..13, "missing".to_string())
        );
    }

    #[test]
    fn predefined_entities_do_not_count_as_custom() {
        let entities = [("custom", "&amp;")];
        // The custom entity itself costs one level, but the nested `&amp;` is
        // predefined and does not consume another level or reference.
        let got = escape::unescape_with_limits(
            "&lt;&custom;&gt;",
            EntityExpansion::default()
                .with_max_depth(1)
                .with_max_references(Some(1)),
            table(&entities),
        )
        .unwrap();
        assert_eq!(got, "<&>");

        // Purely predefined references need no custom budget at all
        let got = escape::unescape_with_limits(
            "&lt;&amp;&gt;",
            EntityExpansion::default()
                .with_max_depth(0)
                .with_max_references(Some(0)),
            table(&entities),
        )
        .unwrap();
        assert_eq!(got, "<&>");
    }

    #[test]
    fn custom_resolver_can_override_predefined_entity() {
        let entities = [("amp", "OVERRIDE")];
        let got =
            escape::unescape_with_limits("&amp;", EntityExpansion::default(), table(&entities))
                .unwrap();
        assert_eq!(got, "OVERRIDE");
    }

    #[test]
    fn non_recursive_unescape_with_is_unchanged() {
        // The legacy API must not recursively expand and must not apply any
        // limits, preserving its documented behavior.
        let entities = [("a", "&b;"), ("b", "X")];
        let got = escape::unescape_with("&a;", table(&entities)).unwrap();
        assert_eq!(got, "&b;");

        let got = escape::unescape_with("&a;&a;", table(&entities)).unwrap();
        assert_eq!(got, "&b;&b;");
    }

    #[test]
    fn unlimited_allows_deep_nesting() {
        let entities = [
            ("e1", "&e2;"),
            ("e2", "&e3;"),
            ("e3", "&e4;"),
            ("e4", "bottom"),
        ];
        let got =
            escape::unescape_with_limits("&e1;", EntityExpansion::UNLIMITED, table(&entities))
                .unwrap();
        assert_eq!(got, "bottom");
    }

    #[test]
    fn attribute_normalization_shares_the_strategy() {
        use quick_xml::XmlVersion;
        use quick_xml::events::attributes::Attribute;

        let entities = [("e1", "&e2;"), ("e2", "&e3;"), ("e3", "value")];

        let attr = Attribute {
            key: quick_xml::name::QName("a"),
            value: Cow::Borrowed("&e1;"),
        };
        // Depth limit applies through the attribute path
        let err = attr
            .normalized_value_with_limits(
                XmlVersion::Implicit1_0,
                EntityExpansion::default().with_max_depth(1),
                table(&entities),
            )
            .unwrap_err();
        match err {
            quick_xml::Error::Escape(EscapeError::EntityDepthLimitExceeded(2)) => {}
            other => panic!("unexpected error: {other:?}"),
        }

        // Whitespace normalization still happens alongside expansion
        let attr = Attribute {
            key: quick_xml::name::QName("a"),
            value: Cow::Borrowed("\t&e1;\n"),
        };
        let got = attr
            .normalized_value_with_limits(
                XmlVersion::Implicit1_0,
                EntityExpansion::default(),
                table(&entities),
            )
            .unwrap();
        assert_eq!(got, Cow::Borrowed(" value "));
    }

    #[test]
    fn byte_limit_includes_attribute_literal_text() {
        use quick_xml::XmlVersion;
        use quick_xml::events::attributes::Attribute;

        let entities = [("e", "abcdefgh")];
        let attr = Attribute {
            key: quick_xml::name::QName("a"),
            value: Cow::Borrowed("0123456789&e;"),
        };
        let err = attr
            .normalized_value_with_limits(
                XmlVersion::Implicit1_0,
                EntityExpansion::default().with_max_bytes(12),
                table(&entities),
            )
            .unwrap_err();
        match err {
            quick_xml::Error::Escape(EscapeError::EntityByteLimitExceeded(12)) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
