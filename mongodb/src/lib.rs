#[macro_use(bson, doc)]
extern crate bson;
extern crate failure;
#[macro_use]
extern crate lazy_static;
extern crate mongodb;
extern crate opentracingrust;
extern crate prometheus;
extern crate semver;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;
#[macro_use]
extern crate slog;

extern crate replicante_agent;
extern crate replicante_agent_models;
extern crate replicante_util_failure;
extern crate replicante_util_tracing;

use replicante_agent::Result;
use replicante_agent::VersionedAgent;

mod config;
mod error;
mod metrics;
mod version;

use config::Config;
use version::MongoDBFactory;

lazy_static! {
    static ref RELEASE: String = format!("repliagent-officials@{}", env!("GIT_BUILD_HASH"));
    pub static ref VERSION: String = format!(
        "{} [{}; {}]",
        env!("CARGO_PKG_VERSION"),
        env!("GIT_BUILD_HASH"),
        env!("GIT_BUILD_TAINT"),
    );
}

const DEFAULT_CONFIG_FILE: &str = "agent-mongodb.yaml";

/// Configure and start the agent.
pub fn run() -> Result<bool> {
    // Command line parsing.
    let cli_args = ::replicante_agent::process::clap(
        "MongoDB Replicante Agent",
        VERSION.as_ref(),
        env!("CARGO_PKG_DESCRIPTION"),
        DEFAULT_CONFIG_FILE,
    )
    .get_matches();

    // Load configuration.
    Config::override_defaults();
    let config_location = cli_args.value_of("config").unwrap();
    let config = Config::from_file(config_location)?;
    let config = config.transform();

    // Run the agent using the provided default helper.
    let agent_conf = config.agent.clone();
    let release = RELEASE.as_str();
    ::replicante_agent::process::run(agent_conf, release, |context, _, _| {
        metrics::register_metrics(context);
        let factory = MongoDBFactory::with_config(config, context.clone())?;
        let agent = VersionedAgent::new(context.clone(), factory);
        Ok(agent)
    })
}
