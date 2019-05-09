extern crate clap;
extern crate failure;
#[macro_use]
extern crate lazy_static;
extern crate opentracingrust;
extern crate prometheus;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;
#[macro_use]
extern crate slog;
extern crate zk_4lw;

extern crate replicante_agent;
extern crate replicante_agent_models;
extern crate replicante_util_tracing;

use std::time::Duration;

use clap::App;
use clap::Arg;
use failure::ResultExt;

use replicante_agent::AgentContext;
use replicante_agent::AgentRunner;
use replicante_agent::Result;

use replicante_util_tracing::tracer;
use replicante_util_tracing::TracerExtra;

mod agent;
mod config;
mod error;
mod metrics;
mod zk4lw;

use agent::ZookeeperAgent;
use config::Config;
use error::ErrorKind;

lazy_static! {
    /// Version string.
    pub static ref VERSION: String = format!(
        "{} [{}; {}]",
        env!("CARGO_PKG_VERSION"),
        env!("GIT_BUILD_HASH"),
        env!("GIT_BUILD_TAINT")
    );
}

const DEFAULT_CONFIG_FILE: &str = "agent-zookeeper.yaml";

/// Configure and start the agent.
pub fn run() -> Result<()> {
    // Command line parsing.
    let cli_args = App::new("Zookeeper Replicante Agent")
        .version(VERSION.as_ref())
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .default_value(DEFAULT_CONFIG_FILE)
                .help("Specifies the configuration file to use")
                .takes_value(true),
        )
        .get_matches();

    // Load configuration (default file is allowed to be missing).
    Config::override_defaults();
    let config_location = cli_args.value_of("config").unwrap();
    let config = Config::from_file(config_location)?;
    let config = config.transform();

    // Configure the logger (from the agent context).
    let agent_config = config.agent.clone();
    let (logger, _scope_guard) = AgentContext::logger(&agent_config);

    // Setup and run the tracer.
    let (tracer, mut extra) = tracer(config.agent.tracing.clone(), logger.clone())
        .with_context(|_| ErrorKind::Initialisation("tracer configuration failed".into()))?;
    if let TracerExtra::ReporterThread(ref mut reporter) = extra {
        reporter.stop_delay(Duration::from_secs(2));
    }

    // Setup the agent context.
    let agent_context = AgentContext::new(agent_config, logger, tracer);
    AgentRunner::register_metrics(&agent_context.logger, &agent_context.metrics);
    metrics::register_metrics(&agent_context.logger, &agent_context.metrics);

    // Setup and run the agent.
    let agent = ZookeeperAgent::new(config, agent_context.clone());
    let runner = AgentRunner::new(agent, agent_context);
    runner.run();

    // Cleanup tracer and exit.
    drop(extra);
    Ok(())
}
