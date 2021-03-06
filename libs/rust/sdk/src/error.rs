use std::fmt;

use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use actix_web::ResponseError;
use failure::Backtrace;
use failure::Context;
use failure::Fail;
use uuid::Uuid;

use replicante_util_failure::SerializableFail;

/// Error information returned by functions in case of errors.
#[derive(Debug)]
pub struct Error(Context<ErrorKind>);

impl Error {
    pub fn kind(&self) -> &ErrorKind {
        self.0.get_context()
    }
}

impl Fail for Error {
    fn cause(&self) -> Option<&dyn Fail> {
        self.0.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.0.backtrace()
    }

    fn name(&self) -> Option<&str> {
        self.kind().kind_name()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Error {
        Error(Context::new(kind))
    }
}

impl ResponseError for Error {
    fn status_code(&self) -> StatusCode {
        self.kind().http_status()
    }

    fn error_response(&self) -> HttpResponse {
        let info = SerializableFail::from(self);
        let status = self.status_code();
        HttpResponse::build(status).json(info)
    }
}

// Support conversion from custom ErrorKind to allow agents to define their own kinds that
// can be converted into base agent error kinds and wrapped in an error.
// See the MongoDB agent code for an example of this.
impl<E> From<Context<E>> for Error
where
    E: Into<ErrorKind> + fmt::Display + Sync + Send,
{
    fn from(context: Context<E>) -> Error {
        let context = context.map(Into::into);
        Error(context)
    }
}

/// Exhaustive list of possible errors emitted by this crate.
#[derive(Debug, Fail)]
pub enum ErrorKind {
    #[fail(display = "an action with id '{}' already exists", _0)]
    ActionAlreadyExists(String),

    #[fail(display = "unable to decode action information")]
    ActionDecode,

    #[fail(display = "unable to encode action information")]
    ActionEncode,

    #[fail(display = "actions with kind {} are not available", _0)]
    ActionNotAvailable(String),

    #[fail(display = "invalid configuration: {}", _0)]
    ConfigClash(&'static str),

    #[fail(display = "unable to load configuration")]
    ConfigLoad,

    #[fail(display = "invalid configuration for option {}", _0)]
    ConfigOption(&'static str),

    #[fail(display = "connection error to {} with address '{}'", _0, _1)]
    Connection(&'static str, String),

    #[fail(display = "unable to check external action {} with ID {}", _0, _1)]
    ExternalActionCheck(String, Uuid),

    #[fail(display = "unable to decode check result for external action {}", _0)]
    ExternalActionCheckDecode(Uuid),

    #[fail(
        display = "external action {} check command failed\n--> Standard out:\n{}\n--> Standard error:\n{}",
        _0, _1, _2
    )]
    ExternalActionCheckResult(Uuid, String, String),

    #[fail(
        display = "external action {} start command failed\n--> Standard out:\n{}\n--> Standard error:\n{}",
        _0, _1, _2
    )]
    ExternalActionExec(Uuid, String, String),

    #[fail(display = "external action {} with ID {} failed to start", _0, _1)]
    ExternalActionStart(String, Uuid),

    /// Generic context agents can use if provided contexts are not enough.
    #[fail(display = "{}", _0)]
    FreeForm(String),

    #[fail(display = "agent initialisation error: {}", _0)]
    Initialisation(String),

    #[fail(display = "invalid datastore state: {}", _0)]
    InvalidStoreState(String),

    #[fail(display = "I/O error on file {}", _0)]
    Io(String),

    #[fail(display = "unable to commit transaction to persistent DB")]
    PersistentCommit,

    #[fail(display = "unable to migrate persistent DB")]
    PersistentMigrate,

    #[fail(display = "connection to persistent DB available")]
    PersistentNoConnection,

    #[fail(display = "failed to read {} from persistent store", _0)]
    PersistentRead(&'static str),

    #[fail(display = "failed to write {} to persistent store", _0)]
    PersistentWrite(&'static str),

    #[fail(display = "unable to open persistent DB {}", _0)]
    PersistentOpen(String),

    #[fail(display = "unable to initialse persistent DB connections pool")]
    PersistentPool,

    #[fail(
        display = "could not decode {} response from store for '{}' operation",
        _0, _1
    )]
    ResponseDecode(&'static str, &'static str),

    #[fail(display = "service operation '{}' failed", _0)]
    ServiceOpFailed(&'static str),

    #[fail(display = "datastore operation '{}' failed", _0)]
    StoreOpFailed(&'static str),

    #[fail(display = "unable to spawn '{}' thread", _0)]
    ThreadSpawn(&'static str),
}

impl ErrorKind {
    fn http_status(&self) -> StatusCode {
        match self {
            ErrorKind::ActionAlreadyExists(_) => StatusCode::CONFLICT,
            ErrorKind::ActionEncode => StatusCode::BAD_REQUEST,
            ErrorKind::ActionNotAvailable(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn kind_name(&self) -> Option<&str> {
        let name = match self {
            ErrorKind::ActionAlreadyExists(_) => "ActionAlreadyExists",
            ErrorKind::ActionDecode => "ActionDecode",
            ErrorKind::ActionEncode => "ActionEncode",
            ErrorKind::ActionNotAvailable(_) => "ActionNotAvailable",
            ErrorKind::ConfigClash(_) => "ConfigClash",
            ErrorKind::ConfigLoad => "ConfigLoad",
            ErrorKind::ConfigOption(_) => "ConfigOption",
            ErrorKind::Connection(_, _) => "Connection",
            ErrorKind::ExternalActionCheck(_, _) => "ExternalActionCheck",
            ErrorKind::ExternalActionCheckDecode(_) => "ExternalActionCheckDecode",
            ErrorKind::ExternalActionCheckResult(_, _, _) => "ExternalActionCheckResult",
            ErrorKind::ExternalActionExec(_, _, _) => "ExternalActionExec",
            ErrorKind::ExternalActionStart(_, _) => "ExternalActionStart",
            ErrorKind::FreeForm(_) => "FreeForm",
            ErrorKind::Initialisation(_) => "Initialisation",
            ErrorKind::InvalidStoreState(_) => "InvalidStoreState",
            ErrorKind::Io(_) => "Io",
            ErrorKind::PersistentCommit => "PersistentCommit",
            ErrorKind::PersistentMigrate => "PersistentMigrate",
            ErrorKind::PersistentNoConnection => "PersistentNoConnection",
            ErrorKind::PersistentOpen(_) => "PersistentOpen",
            ErrorKind::PersistentPool => "PersistentPool",
            ErrorKind::PersistentRead(_) => "PersistentRead",
            ErrorKind::PersistentWrite(_) => "PersistentWrite",
            ErrorKind::ResponseDecode(_, _) => "ResponseDecode",
            ErrorKind::ServiceOpFailed(_) => "ServiceOpFailed",
            ErrorKind::StoreOpFailed(_) => "StoreOpFailed",
            ErrorKind::ThreadSpawn(_) => "ThreadSpawn",
        };
        Some(name)
    }
}

/// Short form alias for functions returning `Error`s.
pub type Result<T> = ::std::result::Result<T, Error>;
