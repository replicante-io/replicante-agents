#[macro_use(bson, doc)]
extern crate bson;
extern crate config;
extern crate mongodb;
extern crate opentracingrust;
extern crate prometheus;

extern crate serde;
#[macro_use]
extern crate serde_derive;

extern crate replicante_agent;
extern crate replicante_agent_models;

use bson::Bson;
use bson::ordered::OrderedDocument;

use mongodb::Client;
use mongodb::CommandType;
use mongodb::ThreadedClient;
use mongodb::db::ThreadedDatabase;

use opentracingrust::Log;
use opentracingrust::Span;
use opentracingrust::Tracer;
use opentracingrust::utils::FailSpan;

use prometheus::CounterVec;
use prometheus::Opts;
use prometheus::Registry;

use replicante_agent::Agent;
use replicante_agent::Error;
use replicante_agent::Result;

use replicante_agent_models::AgentInfo;
use replicante_agent_models::AgentVersion;
use replicante_agent_models::DatastoreInfo;
use replicante_agent_models::Shard;
use replicante_agent_models::Shards;
use replicante_agent_models::ShardRole;

pub mod settings;
mod error;
mod rs_status;

use self::settings::MongoDBSettings;


/// Agent dealing with MongoDB 3.2+ Replica Sets.
pub struct MongoDBAgent {
    // The client needs to reference mongo settings inside the agent.
    // To implement this, the client is stored in an option that is
    // filled just after the agent is created while in the factory.
    client: Option<Client>,
    settings: MongoDBSettings,

    // Introspection.
    mongo_command_counts: CounterVec,
    registry: Registry,
    tracer: Tracer,
}

impl MongoDBAgent {
    pub fn new(settings: MongoDBSettings, tracer: Tracer) -> Result<MongoDBAgent> {
        // Init metrics.
        let mongo_command_counts = CounterVec::new(
            Opts::new(
                "replicante_mongodb_commands",
                "Counts the commands executed against the MongoDB node"
            ),
            &["command"]
        ).expect("Unable to configure commands counter");
        let registry = Registry::new();
        registry.register(Box::new(mongo_command_counts.clone()))
            .expect("Unable to register commands counter");

        // Init agent.
        let mut agent = MongoDBAgent {
            client: None,
            settings,

            // Introspection.
            mongo_command_counts,
            registry,
            tracer,
        };
        agent.init_client()?;
        Ok(agent)
    }
}

impl MongoDBAgent {
    /// Initialises the MongoDB client instance for the agent.
    fn init_client(&mut self) -> Result<()> {
        let host = &self.settings.host;
        let port = self.settings.port as u16;
        let client = Client::connect(host, port)
            .map_err(self::error::to_agent)?;
        self.client = Some(client);
        Ok(())
    }

    /// Extract the client from the wrapping `Option`.
    fn client(&self) -> &Client {
        self.client.as_ref().unwrap()
    }

    /// Executes the buildInfo command against the DB.
    fn build_info(&self, parent: &mut Span) -> Result<OrderedDocument> {
        let mut span = self.tracer().span("buildInfo").auto_finish();
        span.child_of(parent.context().clone());
        let mongo = self.client();
        span.log(Log::new().log("span.kind", "client-send"));
        self.mongo_command_counts.with_label_values(&["buildInfo"]).inc();
        let info = mongo.db("test").command(
            doc! {"buildInfo" => 1},
            CommandType::BuildInfo,
            None
        ).fail_span(&mut span).map_err(self::error::to_agent)?;
        span.log(Log::new().log("span.kind", "client-receive"));
        Ok(info)
    }

    /// Executes the replSetGetStatus command against the DB.
    fn repl_set_get_status(&self, parent: &mut Span) -> Result<OrderedDocument> {
        let mut span = self.tracer().span("replSetGetStatus").auto_finish();
        span.child_of(parent.context().clone());
        let mongo = self.client();
        span.log(Log::new().log("span.kind", "client-send"));
        self.mongo_command_counts.with_label_values(&["replSetGetStatus"]).inc();
        let status = mongo.db("admin").command(
            doc! {"replSetGetStatus" => 1},
            CommandType::IsMaster,
            None
        ).fail_span(&mut span).map_err(self::error::to_agent)?;
        span.log(Log::new().log("span.kind", "client-receive"));
        Ok(status)
    }
}

impl Agent for MongoDBAgent {
    fn agent_info(&self, _: &mut Span) -> Result<AgentInfo> {
        let version = AgentVersion::new(
            env!("GIT_BUILD_HASH"), env!("CARGO_PKG_VERSION"), env!("GIT_BUILD_TAINT")
        );
        Ok(AgentInfo::new(version))
    }

    fn datastore_info(&self, span: &mut Span) -> Result<DatastoreInfo> {
        let info = self.build_info(span)?;
        let version = info.get("version").ok_or_else(
            || Error::from("Unable to determine MongoDB version")
        )?;
        if let Bson::String(ref version) = *version {
            let status = self.repl_set_get_status(span)?;
            let cluster = rs_status::name(&status)?;
            let node_name = rs_status::node_name(&status)?;
            Ok(DatastoreInfo::new(cluster, "MongoDB", node_name, version.clone()))
        } else {
            Err("Unexpeted version type (should be String)".into())
        }
    }

    fn shards(&self, span: &mut Span) -> Result<Shards> {
        let status = self.repl_set_get_status(span)?;
        let name = rs_status::name(&status)?;
        let role = rs_status::role(&status)?;
        let last_op = rs_status::last_op(&status)?;
        let lag = match role {
            ShardRole::Primary => Some(0),
            _ => match rs_status::lag(&status, last_op) {
                Ok(lag) => Some(lag),
                Err(err) => {
                    // TODO: fix logging
                    println!("Failed to compute lag: {:?}", err);
                    span.tag("lag.error", format!("Failed lag detection: {:?}", err));
                    None
                }
            }
        };
        let shards = vec![Shard::new(name, role, lag, last_op)];
        Ok(Shards::new(shards))
    }

    fn metrics(&self) -> Registry {
        self.registry.clone()
    }

    fn tracer(&self) -> &Tracer {
        &self.tracer
    }
}
