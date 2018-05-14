use opentracingrust::Span;
use opentracingrust::Tracer;
use prometheus::Registry;

use replicante_agent_models::AgentVersion;
use replicante_agent_models::DatastoreInfo;
use replicante_agent_models::Shard;

use super::AgentResult;


/// Trait to share common agent code and features.
///
/// Agents should be implemented as structs that implement `BaseAgent`.
pub trait Agent : Send + Sync {
    //*** Methods to access datastore model requirements ***//
    /// Fetches the agent version information.
    fn agent_version(&self, span: &mut Span) -> AgentResult<AgentVersion>;

    /// Fetches the datastore information.
    fn datastore_info(&self, span: &mut Span) -> AgentResult<DatastoreInfo>;

    /// Fetches all shards and details on the managed datastore node.
    fn shards(&self, span: &mut Span) -> AgentResult<Vec<Shard>>;


    //*** Methods needed for agent introspection and diagnostics ***//
    /// Acess the agent's metrics [`Registry`].
    ///
    /// Agents MUST register their metrics at creation time and as part of the same [`Registry`].
    ///
    /// [`Registry`]: https://docs.rs/prometheus/0.3.13/prometheus/struct.Registry.html
    fn metrics(&self) -> Registry;

    /// Access the agent's [`Tracer`].
    ///
    /// This is the agent's way to access the optional opentracing compatible tracer.
    ///
    /// [`Tracer`]: https://docs.rs/opentracingrust/0.3.0/opentracingrust/struct.Tracer.html
    fn tracer(&self) -> &Tracer;
}