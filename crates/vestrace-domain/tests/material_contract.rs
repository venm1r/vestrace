use static_assertions::assert_not_impl_any;
use vestrace_domain::{
    AssociatedData, CredentialKeyCreationIntentState, ErasureReceipt, IntentNonce,
    MaterialKeyCreationIntentState, MaterialKeyId, SizeClass, VaultReceipt, ZeroizingDek,
    size_class_for,
};

#[test]
fn size_class_boundaries() {
    let cases = [
        (0, SizeClass::FourKiB),
        (1, SizeClass::FourKiB),
        (2, SizeClass::FourKiB),
        (4095, SizeClass::FourKiB),
        (4096, SizeClass::FourKiB),
        (4097, SizeClass::EightKiB),
    ];

    for (byte_len, expected) in cases {
        let actual = size_class_for(byte_len);
        assert_eq!(
            actual, expected,
            "{byte_len} bytes must map to its padded class"
        );
        assert!(actual.minimum_bytes() >= 4096);
    }
}

#[test]
fn size_class_does_not_reveal_exact_length() {
    let one_byte = size_class_for(1);
    let almost_four_kib = size_class_for(4095);

    assert_eq!(one_byte, almost_four_kib);
    assert_eq!(format!("{one_byte:?}"), format!("{almost_four_kib:?}"));
    assert_eq!(
        serde_json::to_string(&one_byte).unwrap(),
        serde_json::to_string(&almost_four_kib).unwrap(),
    );
}

#[test]
fn zeroizing_dek_has_no_revealing_debug() {
    let dek = ZeroizingDek::new([0xA5; 32]);
    let debug = format!("{dek:?}");

    assert!(debug.contains("REDACTED"));
    assert!(!debug.contains("A5"));
    assert!(!debug.contains("165"));
}

assert_not_impl_any!(ZeroizingDek: Clone, serde::Serialize);

#[test]
fn material_contract_types_are_constructible_without_persistence() {
    let key_id = MaterialKeyId::new();
    let nonce = IntentNonce::new();
    let vault_receipt = VaultReceipt::new();
    let erasure_receipt = ErasureReceipt::new();
    let associated_data = AssociatedData::new(b"material-contract".to_vec());

    assert_ne!(key_id, MaterialKeyId::new());
    assert_ne!(nonce, IntentNonce::new());
    assert_ne!(vault_receipt, VaultReceipt::new());
    assert_ne!(erasure_receipt, ErasureReceipt::new());
    assert_eq!(associated_data.as_bytes(), b"material-contract");
    assert_eq!(
        MaterialKeyCreationIntentState::Reserved,
        MaterialKeyCreationIntentState::Reserved,
    );
    assert_eq!(
        CredentialKeyCreationIntentState::Reserved,
        CredentialKeyCreationIntentState::Reserved,
    );
}
