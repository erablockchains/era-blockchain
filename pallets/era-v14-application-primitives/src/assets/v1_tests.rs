use super::*;
use alloc::vec;

#[test]
fn golden_struct_field_order_plain_ids_and_result_tags() {
    let asset = AssetV1 {
        id: 0x01020304,
        owner: [1; 32],
        issuer: [2; 32],
        admin: [3; 32],
        freezer: [4; 32],
        supply: 0x0102,
        minimum_balance: 3,
        is_sufficient: false,
        status: AssetStatusV1::Frozen,
        metadata: Some(AssetMetadataV1 {
            name: vec![b'A'].try_into().unwrap(),
            symbol: vec![b'B'].try_into().unwrap(),
            decimals: 18,
            frozen: false,
        }),
    };
    let mut bytes = vec![4, 3, 2, 1];
    for role in 1..=4 {
        bytes.extend_from_slice(&[role; 32]);
    }
    bytes.extend_from_slice(&[2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    bytes.extend_from_slice(&[3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    bytes.extend_from_slice(&[0, 1, 1, 4, b'A', 4, b'B', 18, 0]);
    assert_eq!(asset.encode(), bytes);
    assert_eq!(decode_exact::<AssetV1>(&bytes), Ok(asset.clone()));
    let mut result_bytes = vec![0];
    result_bytes.extend_from_slice(&bytes);
    assert_eq!(
        Ok::<_, AssetApiErrorV1>(asset.clone()).encode(),
        result_bytes
    );
    let page = Page {
        entries: vec![asset].try_into().unwrap(),
        next: Some(0x01020304),
    };
    let mut page_bytes = vec![4];
    page_bytes.extend_from_slice(&bytes);
    page_bytes.extend_from_slice(&[1, 4, 3, 2, 1]);
    assert_eq!(page.encode(), page_bytes);
    assert_eq!(decode_exact::<Page<AssetV1>>(&page_bytes), Ok(page));
    let collection = CollectionV1 {
        id: 7,
        owner: [5; 32],
        issuer: None,
        admin: Some([6; 32]),
        freezer: None,
        item_count: 8,
        metadata: None,
    };
    let mut c = vec![7, 0, 0, 0];
    c.extend_from_slice(&[5; 32]);
    c.extend_from_slice(&[0, 1]);
    c.extend_from_slice(&[6; 32]);
    c.extend_from_slice(&[0, 8, 0, 0, 0, 0]);
    assert_eq!(collection.encode(), c);
    assert_eq!(decode_exact::<CollectionV1>(&c), Ok(collection));
    let item = ItemV1 {
        collection: 7,
        id: 9,
        owner: [10; 32],
        metadata: Some(vec![255].try_into().unwrap()),
    };
    let mut i = vec![7, 0, 0, 0, 9, 0, 0, 0];
    i.extend_from_slice(&[10; 32]);
    i.extend_from_slice(&[1, 4, 255]);
    assert_eq!(item.encode(), i);
    assert_eq!(decode_exact::<ItemV1>(&i), Ok(item));
}

#[test]
fn error_and_status_discriminants_are_exact() {
    use AssetApiErrorV1::*;
    for (index, value) in [
        NotFound,
        InvalidLimit,
        InvalidCursor,
        UnsupportedAsset,
        Unconfigured,
        Arithmetic,
        BackendInvariant,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(value.encode(), vec![index as u8]);
        assert_eq!(Err::<ItemV1, _>(value).encode(), vec![1, index as u8]);
        assert_eq!(decode_exact::<AssetApiErrorV1>(&[index as u8]), Ok(value));
    }
    assert!(decode_exact::<AssetApiErrorV1>(&[7]).is_err());
    for (index, status) in [
        AssetStatusV1::Live,
        AssetStatusV1::Frozen,
        AssetStatusV1::Destroying,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(status.encode(), vec![index as u8]);
    }
    assert!(decode_exact::<AssetStatusV1>(&[3]).is_err());
}

#[test]
fn strict_decoding_rejects_oversize_noncanonical_trailing_truncated_and_invalid_bool() {
    let metadata = AssetMetadataV1 {
        name: vec![255; 64].try_into().unwrap(),
        symbol: vec![254; 16].try_into().unwrap(),
        decimals: 255,
        frozen: false,
    };
    let bytes = metadata.encode();
    assert_eq!(decode_exact::<AssetMetadataV1>(&bytes), Ok(metadata));
    for len in 0..bytes.len() {
        assert!(decode_exact::<AssetMetadataV1>(&bytes[..len]).is_err());
    }
    let mut bad = bytes.clone();
    bad.push(0);
    assert!(decode_exact::<AssetMetadataV1>(&bad).is_err());
    let mut bad = bytes;
    *bad.last_mut().unwrap() = 2;
    assert!(decode_exact::<AssetMetadataV1>(&bad).is_err());
    assert!(decode_exact::<Name>(&vec![0u8; 65].encode()).is_err());
    assert!(decode_exact::<Symbol>(&vec![0u8; 17].encode()).is_err());
    assert!(decode_exact::<Metadata>(&vec![0u8; 129].encode()).is_err());
    assert!(decode_exact::<Name>(&[5, 0, b'A']).is_err()); // noncanonical compact length one
    assert!(decode_exact::<Option<u32>>(&[2]).is_err());
    assert!(decode_exact::<Page<ItemV1>>(&[5, 1]).is_err()); // declared 65 entries
    let item = ItemV1 {
        collection: 1,
        id: 1,
        owner: [1; 32],
        metadata: Some(vec![255; 128].try_into().unwrap()),
    };
    let page = Page {
        entries: vec![item; 64].try_into().unwrap(),
        next: Some(u32::MAX),
    };
    assert_eq!(decode_exact::<Page<ItemV1>>(&page.encode()), Ok(page));
}

#[test]
fn absent_malformed_next_and_exhausted_allocator_state_never_alias() {
    let schema = 1u16.encode();
    assert_eq!(
        AllocatorCursor::decode(None, Some(&[0])),
        Err(CursorError::Missing)
    );
    assert_eq!(
        AllocatorCursor::decode(Some(&schema), None),
        Err(CursorError::Missing)
    );
    for bad in [&[][..], &[2][..], &[1, 0][..], &[0, 0][..]] {
        assert_eq!(
            AllocatorCursor::decode(Some(&schema), Some(bad)),
            Err(CursorError::Malformed)
        );
    }
    assert_eq!(
        AllocatorCursor::decode(Some(&[2, 0]), Some(&[0])),
        Err(CursorError::Version)
    );
    assert_eq!(
        AllocatorCursor::decode(Some(&[1]), Some(&[0])),
        Err(CursorError::Malformed)
    );
    let last = Some(u32::MAX).encode();
    let plan = AllocatorCursor::decode(Some(&schema), Some(&last))
        .unwrap()
        .prepare(IdRange::new(1, u32::MAX).unwrap(), |_| false)
        .unwrap();
    assert_eq!(plan.next, None);
    assert_eq!(
        AllocatorCursor::decode(Some(&schema), Some(&plan.next.encode()))
            .unwrap()
            .prepare(IdRange::new(1, u32::MAX).unwrap(), |_| panic!("no read")),
        Err(CursorError::Allocation(AllocationError::Exhausted))
    );
}

#[test]
fn adopted_namespaces_and_admission_have_no_fallback() {
    let r = IdRange::new(REGISTERED_FIRST, REGISTERED_LAST).unwrap();
    let lp = IdRange::new(LP_FIRST, LP_LAST).unwrap();
    let system = IdRange::new(SYSTEM_FIRST, SYSTEM_LAST).unwrap();
    assert!(!r.overlaps(&lp) && !r.overlaps(&system) && !lp.overlaps(&system));
    assert_eq!(
        r.prepare(Some(REGISTERED_LAST), |_| false).unwrap().next,
        None
    );
    for id in [0, LP_FIRST, LP_LAST, SYSTEM_FIRST, SYSTEM_LAST] {
        assert!(validate_asset_admission(&AssetAdmission {
            id,
            minimum_balance: 1
        })
        .is_err());
    }
    assert!(validate_asset_admission(&AssetAdmission {
        id: 1,
        minimum_balance: 0
    })
    .is_err());
    assert_eq!(
        validate_asset_admission(&AssetAdmission {
            id: REGISTERED_LAST,
            minimum_balance: u128::MAX
        }),
        Ok(())
    );
    for id in [0, 1, REGISTERED_LAST, u32::MAX] {
        assert_eq!(admitted_asset(id), Err(AssetApiErrorV1::UnsupportedAsset));
        assert_eq!(
            admitted_collection(id),
            Err(AssetApiErrorV1::UnsupportedAsset)
        );
    }
    assert_eq!(ADMITTED_CATEGORIES, &[]);
}

#[test]
fn sdk_roles_are_authoritative_and_live_duties_cannot_be_orphaned_or_recovered() {
    let roles = Roles {
        owner: 1,
        issuer: Some(2),
        admin: Some(3),
        freezer: Some(4),
    };
    assert_eq!(AssetOperation::Thaw.authority(), Ok(Authority::Admin));
    assert_eq!(AssetOperation::ThawAsset.authority(), Ok(Authority::Admin));
    assert_eq!(NftOperation::SetMetadata.authority(), Ok(Authority::Admin));
    assert_eq!(
        NftOperation::UnlockItemTransfer.authority(),
        Ok(Authority::Freezer)
    );
    assert_eq!(
        NftOperation::LockCollection.authority(),
        Ok(Authority::Owner)
    );
    for (who, authority) in [
        (1, Authority::Owner),
        (2, Authority::Issuer),
        (3, Authority::Admin),
        (4, Authority::Freezer),
    ] {
        assert_eq!(roles.authorize(&who, authority), Ok(()));
        assert_eq!(
            roles.authorize(&99, authority),
            Err(ContractError::Authority)
        );
    }
    let mut next = Roles {
        owner: 1,
        issuer: Some(5),
        admin: Some(6),
        freezer: Some(7),
    };
    assert_eq!(roles.replacement(&1, &next), Ok(()));
    assert_eq!(roles.replacement(&99, &next), Err(ContractError::Authority));
    next.freezer = None;
    assert!(roles.replacement(&1, &next).is_err());
    assert!(next.replacement(&1, &roles).is_err());
    assert!(AssetOperation::Unsupported.authority().is_err());
    assert!(NftOperation::Unsupported.authority().is_err());
}

#[test]
fn deposit_rounding_bounds_overflow_and_settlement_are_checked() {
    assert_eq!(
        asset_metadata_deposit(64, 16),
        Ok(1_080_000_000_000_000_000)
    );
    assert_eq!(nft_metadata_deposit(128), Ok(1_128_000_000_000_000_000));
    assert_eq!(attribute_deposit(64, 128), Ok(1_192_000_000_000_000_000));
    assert!(asset_metadata_deposit(65, 0).is_err());
    assert!(asset_metadata_deposit(0, 17).is_err());
    assert!(nft_metadata_deposit(129).is_err());
    assert!(attribute_deposit(65, 0).is_err());
    assert!(attribute_deposit(0, 129).is_err());
    assert!(deposit(u128::MAX, 1, [1, 0]).is_err());
    assert!(deposit(0, u128::MAX, [2, 0]).is_err());
    assert!(deposit(0, 0, [u32::MAX, 1]).is_err());
    assert_eq!(validate_destroy(0, [1000; 3], [1000; 3]), Ok(()));
    assert!(validate_destroy(1, [0; 3], [0; 3]).is_err());
    assert!(validate_destroy(0, [1001; 3], [1001; 3]).is_err());
    assert!(validate_destroy(0, [1; 3], [0; 3]).is_err());
    assert_eq!(validate_refund(0, false), Ok(()));
    assert!(validate_refund(1, false).is_err());
    assert!(validate_refund(0, true).is_err());
    assert_eq!(next_item_count(u32::MAX), Err(ContractError::Arithmetic));
}
