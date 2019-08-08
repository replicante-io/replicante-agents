use std::collections::HashMap;

use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use actix_web::ResponseError;
use chrono::DateTime;
use chrono::Utc;
use failure::Fail;
use serde_derive::Deserialize;
use serde_derive::Serialize;
use serde_json::json;
use serde_json::Value as Json;
use uuid::Uuid;

use crate::store::Transaction;
use crate::Result;

/// Abstraction of any action the agent can perform.
///
/// # Action Kinds
/// Action Kinds must be scoped to limit the chance of clashes.
/// Scoping is done using the `<SCOPE>.<ACTION>` format.
/// An action kind can have as many `.`s in it as desired, making Java-like reverse DNS
/// scopes an option that greatly reduces the chances of clashes.
///
/// The only constraint on Action Kindss is some scopes are reserved to replicante use itself.
/// This allows the base agent frameworks to define some standard actions across all agents
/// without clashing with custom or database specific actions.
pub trait Action: Send + Sync + 'static {
    /// Action metadata and attributes.
    fn describe(&self) -> ActionDescriptor;

    /// TODO
    fn invoke(&self, tx: &mut Transaction, record: &ActionRecord) -> Result<()>;

    /// Validate the arguments passed to an action request.
    fn validate_args(&self, args: &Json) -> ActionValidity;
}

/// Container for an action's metadata and other attributes.
///
/// This data is the base of the actions system.
/// Instead of hardcoded knowledge about what actions do,
/// both system and users rely on metadata to call actions.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct ActionDescriptor {
    pub kind: String,
    pub description: String,
}

/// Summary info about an action returned in lists.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct ActionListItem {
    pub action: String,
    pub id: Uuid,
    pub state: ActionState,
}

/// Action state and metadata information.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ActionRecord {
    /// Type ID of the action to run.
    pub action: String,

    /// Version of the agent that last validated the action.
    pub agent_version: String,

    /// Arguments passed to the action when invoked.
    pub args: Json,

    /// Time the agent recorded the action in the DB.
    pub created_ts: DateTime<Utc>,

    /// Additional metadata headers attached to the action.
    pub headers: HashMap<String, String>,

    /// Unique ID of the action.
    pub id: Uuid,

    /// Entity (system or user) requesting the execution of the action.
    pub requester: ActionRequester,

    /// State the action is currently in.
    pub state: ActionState,

    /// Optional payload attached to the current state.
    pub state_payload: Option<Json>,
}

impl ActionRecord {
    pub fn new(action: String, args: Json, requester: ActionRequester) -> ActionRecord {
        ActionRecord {
            action,
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            args,
            created_ts: Utc::now(),
            headers: HashMap::new(),
            id: Uuid::new_v4(),
            requester,
            state: ActionState::New,
            state_payload: None,
        }
    }
}

/// Entity (system, user, ...) that requested the action to be performed.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ActionRequester {
    #[serde(rename = "API")]
    Api,
}

/// Current state of an action execution.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ActionState {
    /// The action ended with an error.
    Failed,

    /// The action has just been sheduled and is not being executed yet.
    New,

    /// The action was started by the agent and is in progress.
    Running,
}

impl ActionState {
    /// True if the action is finished (failed or succeeded).
    pub fn is_finished(&self) -> bool {
        match self {
            ActionState::Failed => true,
            _ => false,
        }
    }
}

/// Result alias for methods that return an ActionValidityError.
pub type ActionValidity<T = ()> = std::result::Result<T, ActionValidityError>;

/// Result of action validation process.
#[derive(Debug, Fail)]
pub enum ActionValidityError {
    #[fail(display = "invalid action arguments: {}", _0)]
    InvalidArgs(String),
}

impl ActionValidityError {
    fn kind(&self) -> &str {
        match self {
            ActionValidityError::InvalidArgs(_) => "InvalidArgs",
        }
    }
}

impl ActionValidityError {
    fn http_status(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }
}

impl ResponseError for ActionValidityError {
    fn error_response(&self) -> HttpResponse {
        let status = self.http_status();
        HttpResponse::build(status).json(json!({
            "error": self.to_string(),
            "kind": self.kind(),
        }))
    }

    fn render_response(&self) -> HttpResponse {
        self.error_response()
    }
}
