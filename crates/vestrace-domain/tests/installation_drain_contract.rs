use vestrace_domain::{
    CredentialKeyCreationIntentState, InstallationDrainRequest, InstallationDrainRequestId,
    MaterialKeyCreationIntentState, is_credential_pre_quiescing, is_material_pre_quiescing,
};

#[test]
fn a_fresh_request_is_draining() {
    let request = InstallationDrainRequest::request(InstallationDrainRequestId::new());
    assert!(!request.is_frozen());
}

#[test]
fn completing_a_request_freezes_it() {
    let request = InstallationDrainRequest::request(InstallationDrainRequestId::new());
    let frozen = request.complete();
    assert!(frozen.is_frozen());
}

#[test]
fn every_material_state_has_an_exact_pre_quiescing_answer() {
    use MaterialKeyCreationIntentState::*;
    let pre_quiescing = [
        Reserved,
        ProvisionalCreated,
        ProvisionalReceipted,
        ContentPrepared,
        ResultPrepared,
    ];
    let not_pre_quiescing = [
        ContentAbandonPrepared,
        PrePreparedAbandonPrepared,
        Bound,
        Live,
        ErasurePrepared,
        Tombstoned,
        Abandoned,
    ];
    for state in pre_quiescing {
        assert!(
            is_material_pre_quiescing(state),
            "{state:?} must be pre-Quiescing"
        );
    }
    for state in not_pre_quiescing {
        assert!(
            !is_material_pre_quiescing(state),
            "{state:?} must not be pre-Quiescing"
        );
    }
}

#[test]
fn every_credential_state_has_an_exact_pre_quiescing_answer() {
    use CredentialKeyCreationIntentState::*;
    let pre_quiescing = [
        Reserved,
        ProvisionalCreated,
        ProvisionalReceipted,
        CredentialPrepared,
    ];
    let not_pre_quiescing = [
        CredentialAbandonPrepared,
        Bound,
        Candidate,
        ErasurePrepared,
        Destroyed,
        Abandoned,
    ];
    for state in pre_quiescing {
        assert!(
            is_credential_pre_quiescing(state),
            "{state:?} must be pre-Quiescing"
        );
    }
    for state in not_pre_quiescing {
        assert!(
            !is_credential_pre_quiescing(state),
            "{state:?} must not be pre-Quiescing"
        );
    }
}
