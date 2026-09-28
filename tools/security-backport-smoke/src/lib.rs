//! Executable regression coverage for locally backported dependency fixes.

#[cfg(test)]
mod tests {
    use alloy_dyn_abi::Resolver;
    use ruint::{algorithms::div::reciprocal_mg10, Uint};
    use time::{format_description::well_known::Rfc2822, OffsetDateTime};

    #[test]
    fn alloy_empty_eip712_linearization_returns_error() {
        let resolver = Resolver::default();
        for name in ["bool", "uint256"] {
            let error = resolver
                .encode_type(name)
                .expect_err("must not panic or encode");
            assert!(error.to_string().contains(name));
        }
    }

    fn hickory_024_encode(names: &[String]) -> (Vec<u8>, usize) {
        use hickory_proto_024::{
            op::{Message, Query},
            rr::{Name, RecordType},
        };

        let mut message = Message::new();
        for name in names {
            message.add_query(Query::query(name.parse::<Name>().unwrap(), RecordType::A));
        }
        let bytes = message.to_vec().unwrap();
        let decoded = Message::from_vec(&bytes).unwrap();
        (bytes, decoded.query_count() as usize)
    }

    fn hickory_025_encode(names: &[String]) -> (Vec<u8>, usize) {
        use hickory_proto_025::{
            op::{Message, Query},
            rr::{Name, RecordType},
        };

        let mut message = Message::new();
        for name in names {
            message.add_query(Query::query(name.parse::<Name>().unwrap(), RecordType::A));
        }
        let bytes = message.to_vec().unwrap();
        let decoded = Message::from_vec(&bytes).unwrap();
        (bytes, decoded.query_count() as usize)
    }

    #[test]
    fn hickory_upstream_label_compression_regression_matches_both_lines() {
        // Exact packet from upstream issue #339 and Hickory's retained regression.
        let packet = vec![
            154, 50, 129, 128, 0, 1, 0, 0, 0, 1, 0, 1, 7, 98, 108, 117, 101, 100, 111, 116, 2, 105,
            115, 8, 97, 117, 116, 111, 110, 97, 118, 105, 3, 99, 111, 109, 3, 103, 100, 115, 10,
            97, 108, 105, 98, 97, 98, 97, 100, 110, 115, 3, 99, 111, 109, 0, 0, 28, 0, 1, 192, 36,
            0, 6, 0, 1, 0, 0, 7, 7, 0, 35, 6, 103, 100, 115, 110, 115, 49, 192, 40, 4, 110, 111,
            110, 101, 0, 120, 27, 176, 162, 0, 0, 7, 8, 0, 0, 2, 88, 0, 0, 14, 16, 0, 0, 1, 104, 0,
            0, 41, 2, 0, 0, 0, 0, 0, 0, 0,
        ];
        let message_024 = hickory_proto_024::op::Message::from_vec(&packet).unwrap();
        let message_025 = hickory_proto_025::op::Message::from_vec(&packet).unwrap();
        assert!(message_024.to_vec().is_ok());
        assert!(message_025.to_vec().is_ok());
    }

    #[test]
    fn hickory_normal_wire_corpus_and_many_record_roundtrip_match_both_lines() {
        let normal = vec![
            "www.example.com.".to_owned(),
            "mail.example.com.".to_owned(),
        ];
        let expected = [
            0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 3, b'w', b'w', b'w', 7, b'e', b'x', b'a', b'm',
            b'p', b'l', b'e', 3, b'c', b'o', b'm', 0, 0, 1, 0, 1, 4, b'm', b'a', b'i', b'l', 0xC0,
            16, 0, 1, 0, 1,
        ];
        let (wire_024, queries_024) = hickory_024_encode(&normal);
        let (wire_025, queries_025) = hickory_025_encode(&normal);
        assert_eq!(wire_024, expected);
        assert_eq!(wire_025, expected);
        assert_eq!((queries_024, queries_025), (2, 2));

        let adversarial: Vec<String> = (0..200)
            .map(|index| format!("q{index}.deep.shared.example."))
            .collect();
        let (wire_024, queries_024) = hickory_024_encode(&adversarial);
        let (wire_025, queries_025) = hickory_025_encode(&adversarial);
        assert_eq!((queries_024, queries_025), (200, 200));
        assert_eq!(wire_024, wire_025);
    }

    #[test]
    fn ruint_reciprocal_rejects_invalid_precondition_in_release_mode() {
        assert!(std::panic::catch_unwind(|| reciprocal_mg10(0)).is_err());
    }

    #[test]
    fn ruint_shift_overflow_and_large_rhs_are_not_truncated() {
        let discarded_high_limb = Uint::<128, 2>::from_limbs([0, 1]);
        assert_eq!(discarded_high_limb.overflowing_shl(64), (Uint::ZERO, true));
        assert!(discarded_high_limb.checked_shl(64).is_none());

        let masked_top_bit = Uint::<65, 2>::from_limbs([0, 1]);
        assert_eq!(masked_top_bit.overflowing_shl(1), (Uint::ZERO, true));
        assert_eq!(masked_top_bit.saturating_shl(1), Uint::<65, 2>::MAX);

        let discarded_low_limb = Uint::<128, 2>::from(1u64);
        assert_eq!(discarded_low_limb.overflowing_shr(64), (Uint::ZERO, true));
        assert!(discarded_low_limb.checked_shr(64).is_none());

        type U256 = Uint<256, 4>;
        let over_u64 = U256::from_limbs([0, 1, 0, 0]);
        assert_eq!(U256::ONE << over_u64, U256::ZERO);
        assert_eq!(U256::MAX >> over_u64, U256::ZERO);
    }

    fn rfc2822_with_nested_comment(depth: usize) -> String {
        format!(
            "Mon, {}{}1 Jan 2024 00:00:00 +0000",
            "(".repeat(depth),
            ")".repeat(depth)
        )
    }

    #[test]
    fn time_rfc2822_comment_depth_is_bounded() {
        let accepted = rfc2822_with_nested_comment(31);
        assert!(OffsetDateTime::parse(&accepted, &Rfc2822).is_ok());

        let rejected = rfc2822_with_nested_comment(32);
        assert!(OffsetDateTime::parse(&rejected, &Rfc2822).is_err());
    }
}
