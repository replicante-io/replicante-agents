use bson;
use bson::Bson;
use bson::TimeStamp;

use mongodb::Client;
use mongodb::CommandType;
use mongodb::ThreadedClient;
use mongodb::db::ThreadedDatabase;

use opentracingrust::Log;
use opentracingrust::Span;
use opentracingrust::utils::FailSpan;

use replicante_agent::Agent;
use replicante_agent::AgentContext;
use replicante_agent::Error;
use replicante_agent::Result;
use replicante_agent::ResultExt;

use replicante_agent_models::AgentInfo;
use replicante_agent_models::DatastoreInfo;
use replicante_agent_models::Shard;
use replicante_agent_models::Shards;
use replicante_agent_models::ShardRole;

use super::super::errors;

use super::super::metrics::MONGODB_OPS_COUNT;
use super::super::metrics::MONGODB_OPS_DURATION;
use super::super::metrics::MONGODB_OP_ERRORS_COUNT;

use super::AGENT_VERSION;


/// Section of the buildInfo command that we care about.
#[derive(Deserialize)]
pub struct BuildInfo {
    pub version: String,
}


/// MongoDB 3.2 client interface.
pub struct ReplicaSet {
    client: Client,
    context: AgentContext,
}

impl ReplicaSet {
    pub fn new(client: Client, context: AgentContext) -> ReplicaSet {
        ReplicaSet { 
            client,
            context,
        }
    }

    /// Executes the buildInfo command against the DB.
    fn build_info(&self, parent: &mut Span) -> Result<BuildInfo> {
        let mut span = self.context.tracer.span("buildInfo").auto_finish();
        span.child_of(parent.context().clone());
        span.log(Log::new().log("span.kind", "client-send"));
        MONGODB_OPS_COUNT.with_label_values(&["buildInfo"]).inc();
        let timer = MONGODB_OPS_DURATION.with_label_values(&["buildInfo"]).start_timer();
        let info = self.client.db("test").command(
            doc! {"buildInfo" => 1},
            CommandType::BuildInfo,
            None
        ).fail_span(&mut span).map_err(|error| {
            MONGODB_OP_ERRORS_COUNT.with_label_values(&["buildInfo"]).inc();
            errors::to_agent(error)
        }).chain_err(|| Error::from("BuildInfo command failed"))?;
        timer.observe_duration();
        span.log(Log::new().log("span.kind", "client-receive"));
        let info = bson::from_bson(Bson::Document(info))
            .map_err(errors::to_agent)
            .chain_err(|| Error::from("Unable to parse buildInfo response"))?;
        Ok(info)
    }

    /// Executes the replSetGetStatus command against the DB.
    fn repl_set_get_status(&self, parent: &mut Span) -> Result<ReplSetStatus> {
        let mut span = self.context.tracer.span("replSetGetStatus").auto_finish();
        span.child_of(parent.context().clone());
        span.log(Log::new().log("span.kind", "client-send"));
        MONGODB_OPS_COUNT.with_label_values(&["replSetGetStatus"]).inc();
        let timer = MONGODB_OPS_DURATION.with_label_values(&["replSetGetStatus"]).start_timer();
        let status = self.client.db("admin").command(
            doc! {"replSetGetStatus" => 1},
            CommandType::IsMaster,
            None
        ).fail_span(&mut span).map_err(|error| {
            MONGODB_OP_ERRORS_COUNT.with_label_values(&["replSetGetStatus"]).inc();
            errors::to_agent(error)
        }).chain_err(|| Error::from("ReplSetGetStatus command failed"))?;
        timer.observe_duration();
        span.log(Log::new().log("span.kind", "client-receive"));
        let status = bson::from_bson(Bson::Document(status))
            .map_err(errors::to_agent)
            .chain_err(|| Error::from("Unable to parse replSetGetStatus response"))?;
        Ok(status)
    }
}

impl Agent for ReplicaSet {
    fn agent_info(&self, span: &mut Span) -> Result<AgentInfo> {
        span.log(Log::new().log("span.kind", "server-receive"));
        let info = AgentInfo::new(AGENT_VERSION.clone());
        span.log(Log::new().log("span.kind", "server-send"));
        Ok(info)
    }

    fn datastore_info(&self, span: &mut Span) -> Result<DatastoreInfo> {
        let info = self.build_info(span)?;
        let status = self.repl_set_get_status(span)?;
        let node_name = status.node_name()?;
        let cluster = status.set;
        Ok(DatastoreInfo::new(cluster, "MongoDB", node_name, info.version))
    }

    fn shards(&self, span: &mut Span) -> Result<Shards> {
        let status = self.repl_set_get_status(span)?;
        let last_op = status.last_op()?;
        let role = status.role()?;
        let lag = match role {
            ShardRole::Primary => Some(0),
            _ => match status.primary_optime() {
                Ok(head) => Some(head - last_op),
                Err(error) => {
                    error!(self.context.logger, "Failed to compute lag"; "error" => ?error);
                    span.tag("lag.error", format!("Failed lag computation: {:?}", error));
                    None
                }
            }
        };
        let name = status.set;
        let shards = vec![Shard::new(name, role, lag, last_op)];
        Ok(Shards::new(shards))
    }
}


/// Section of the replSetGetStatus command that we care about.
#[derive(Debug, Deserialize)]
pub struct ReplSetStatus {
    pub members: Vec<ReplSetStatusMember>,
    #[serde(rename = "myState")]
    pub my_state: i32,
    pub set: String,
}

impl ReplSetStatus {
    /// Extracts the timestamp (in seconds) of the latest operation.
    pub fn last_op(&self) -> Result<i64> {
        for member in &self.members {
            if member.is_self {
                return Ok(member.optime.ts.t as i64);
            }
        }
        Err("Unable to find self in members list".into())
    }

    /// Extracts the node's name from the output of replSetGetStatus.
    pub fn node_name(&self) -> Result<String> {
        for member in &self.members {
            if member.is_self {
                return Ok(member.name.clone());
            }
        }
        Err("Unable to find self in members list".into())
    }

    /// Extracts the optime (in seconds) of the primary.
    pub fn primary_optime(&self) -> Result<i64> {
        for member in &self.members {
            if member.state == 1 {
                return Ok(member.optime.ts.t as i64);
            }
        }
        Err("Unable to find primary node in members list".into())
    }

    /// Extracts the node's role in the Replica Set.
    pub fn role(&self) -> Result<ShardRole> {
        match self.my_state {
            0 => Ok(ShardRole::Unknown(String::from("STARTUP"))),
            1 => Ok(ShardRole::Primary),
            2 => Ok(ShardRole::Secondary),
            3 => Ok(ShardRole::Unknown(String::from("RECOVERING"))),
            5 => Ok(ShardRole::Unknown(String::from("STARTUP2"))),
            6 => Ok(ShardRole::Unknown(String::from("UNKNOWN"))),
            7 => Ok(ShardRole::Unknown(String::from("ARBITER"))),
            8 => Ok(ShardRole::Unknown(String::from("DOWN"))),
            9 => Ok(ShardRole::Unknown(String::from("ROLLBACK"))),
            10 => Ok(ShardRole::Unknown(String::from("REMOVED"))),
            _ => Err("Unkown MongoDB node state".into())
        }
    }
}


/// Section of the replSetGetStatus member that we care about.
#[derive(Debug, Deserialize)]
pub struct ReplSetStatusMember {
    #[serde(rename = "self")]
    pub is_self: bool,
    pub name: String,
    pub optime: RepliSetOptime,
    pub state: i32,
}


/// Section of replSetGetStatus optime information that we care about.
#[derive(Debug, Deserialize)]
pub struct RepliSetOptime {
    pub ts: TimeStamp,
}


#[cfg(test)]
mod tests {
    use bson;
    use bson::Bson;

    use replicante_agent::Error;
    use replicante_agent::ErrorKind;
    use replicante_agent_models::ShardRole;

    use super::ReplSetStatus;

    lazy_static! {
        static ref MONGO_TIMESTAMP_ONE: Bson = {
            let ts = 1514677701_u32.to_le();
            Bson::TimeStamp((ts as i64) << 32)
        };

        static ref MONGO_TIMESTAMP_TWO: Bson = {
            let ts = 1514677698_u32.to_le();
            Bson::TimeStamp((ts as i64) << 32)
        };
    }

    fn make_rs() -> Bson {
        Bson::Document(doc! {
            "set": "test-rs",
            "members": [{
                "_id": 0,
                "name": "host0",
                "optime": {
                    "ts": MONGO_TIMESTAMP_ONE.clone(),
                },
                "self": false,
                "state": 1,
            }, {
                "_id": 1,
                "name": "host1",
                "optime": {
                    "ts": MONGO_TIMESTAMP_TWO.clone(),
                },
                "self": true,
                "state": 2,
            }],
            "myState": 1,
        })
    }

    #[test]
    fn last_op() {
        let rs: ReplSetStatus = bson::from_bson(make_rs()).unwrap();
        let last_op = rs.last_op().unwrap();
        assert_eq!(last_op, 1514677698);
    }

    #[test]
    fn last_op_without_self_fails() {
        let rs = Bson::Document(doc! {
            "set": "test-rs",
            "members": [{
                "_id": 0,
                "name": "host0",
                "optime": {
                    "ts": MONGO_TIMESTAMP_ONE.clone(),
                },
                "self": false,
                "state": 2,
            }],
            "myState": 1,
        });
        let rs: ReplSetStatus = bson::from_bson(rs).unwrap();
        match rs.last_op() {
            Err(Error(ErrorKind::Msg(ref msg), _)) => assert_eq!(
                "Unable to find self in members list", msg
            ),
            Err(error) => panic!("Unexpected error {:?}", error),
            Ok(result) => panic!("Unexpected success {:?}", result),
        };
    }

    #[test]
    fn node_name() {
        let rs: ReplSetStatus = bson::from_bson(make_rs()).unwrap();
        let node_name = rs.node_name().unwrap();
        assert_eq!("host1", node_name);
    }

    #[test]
    fn node_name_without_self_fails() {
        let rs = Bson::Document(doc! {
            "set": "test-rs",
            "members": [{
                "_id": 0,
                "name": "host0",
                "optime": {
                    "ts": MONGO_TIMESTAMP_ONE.clone(),
                },
                "self": false,
                "state": 2,
            }],
            "myState": 1,
        });
        let rs: ReplSetStatus = bson::from_bson(rs).unwrap();
        match rs.node_name() {
            Err(Error(ErrorKind::Msg(ref msg), _)) => assert_eq!(
                "Unable to find self in members list", msg
            ),
            Err(error) => panic!("Unexpected error {:?}", error),
            Ok(result) => panic!("Unexpected success {:?}", result),
        };
    }

    #[test]
    fn primary_optime() {
        let rs: ReplSetStatus = bson::from_bson(make_rs()).unwrap();
        let primary_optime = rs.primary_optime().unwrap();
        assert_eq!(1514677701, primary_optime);
    }

    #[test]
    fn primary_optime_without_primary() {
        let rs = Bson::Document(doc! {
            "set": "test-rs",
            "members": [{
                "_id": 0,
                "name": "host0",
                "optime": {
                    "ts": MONGO_TIMESTAMP_ONE.clone(),
                },
                "self": false,
                "state": 2,
            }],
            "myState": 1,
        });
        let rs: ReplSetStatus = bson::from_bson(rs).unwrap();
        match rs.primary_optime() {
            Err(Error(ErrorKind::Msg(ref msg), _)) => assert_eq!(
                "Unable to find primary node in members list", msg
            ),
            Err(error) => panic!("Unexpected error {:?}", error),
            Ok(result) => panic!("Unexpected success {:?}", result),
        };
    }

    #[test]
    fn role_primary() {
        let rs = Bson::Document(doc! {
            "set": "test-rs",
            "members": [],
            "myState": 1,
        });
        let rs: ReplSetStatus = bson::from_bson(rs).unwrap();
        let role = rs.role().unwrap();
        assert_eq!(ShardRole::Primary, role);
    }

    #[test]
    fn role_not_supported() {
        let rs = Bson::Document(doc! {
            "set": "test-rs",
            "members": [],
            "myState": 22,
        });
        let rs: ReplSetStatus = bson::from_bson(rs).unwrap();
        match rs.role() {
            Err(Error(ErrorKind::Msg(ref msg), _)) => assert_eq!(
                "Unkown MongoDB node state", msg
            ),
            Err(error) => panic!("Unexpected error {:?}", error),
            Ok(result) => panic!("Unexpected success {:?}", result),
        };
    }
}
