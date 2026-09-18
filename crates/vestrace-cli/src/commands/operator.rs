use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

pub fn plan(finding_ids: &[Uuid]) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "operator": "health",
            "phase": "plan",
            "read_only": true,
            "finding_ids": finding_ids,
            "execution_started": false,
            "contract": "derive an immutable RepairPlan from HealthFinding evidence"
        }))?
    );
    Ok(())
}

pub fn repair(plan_id: Uuid, current_state_ref: &str) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "operator": "health",
            "phase": "repair",
            "read_only": false,
            "plan_id": plan_id,
            "current_state_ref": current_state_ref,
            "execution_started": false,
            "requires": ["immutable_repair_plan", "capability_policy_authorization", "fresh_preconditions", "verification"],
            "contract": "execution adapter must submit the exact plan; no ad-hoc rebuild is accepted"
        }))?
    );
    Ok(())
}
