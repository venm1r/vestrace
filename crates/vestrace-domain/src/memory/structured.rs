use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "schema_version", rename_all = "snake_case")]
pub enum StructuredMemory {
    #[serde(rename = "v1.assertion")]
    Assertion(AssertionData),
    #[serde(rename = "v1.decision")]
    Decision(DecisionData),
    #[serde(rename = "v1.task")]
    Task(TaskData),
    #[serde(rename = "v1.procedure")]
    Procedure(ProcedureData),
    #[serde(rename = "v1.observation")]
    Observation(ObservationData),
    #[serde(rename = "v1.outcome")]
    Outcome(OutcomeData),
    #[serde(rename = "v1.summary")]
    Summary(SummaryData),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AssertionData {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DecisionData {
    pub title: String,
    pub rationale: String,
    pub alternatives: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskData {
    pub goal: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProcedureData {
    pub name: String,
    pub steps: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ObservationData {
    pub context: String,
    pub finding: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OutcomeData {
    pub action: String,
    pub result: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SummaryData {
    pub topic: String,
    pub body: String,
}
