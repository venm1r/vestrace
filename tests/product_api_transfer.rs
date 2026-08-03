use vestrace_domain::{id::*, now, product::*};

#[test]
fn test_product_api_transfer_creation() {
    let transfer_id = ProductApiTransferId::new();
    let ws_id = WorkspaceId::new();
    let at = now();

    let transfer = ProductApiTransfer {
        id: transfer_id,
        workspace_id: ws_id,
        artifact_id: None,
        transfer_type: TransferType::Upload,
        status: TransferStatus::Initiated,
        byte_offset: 0,
        total_bytes: 1048576,
        created_at: at,
    };

    assert_eq!(transfer.transfer_type, TransferType::Upload);
    assert_eq!(transfer.total_bytes, 1048576);
}
