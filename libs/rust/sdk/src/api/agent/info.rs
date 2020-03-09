use std::sync::Arc;

use actix_web::HttpRequest;
use actix_web::HttpResponse;
use actix_web::Responder;
use actix_web::Result;
use opentracingrust::Log;

use replicante_util_actixweb::request_span;
use replicante_util_tracing::fail_span;

use crate::Agent;

/// API interface to Agent::agent_info
pub fn agent(request: HttpRequest) -> Result<impl Responder> {
    let mut exts = request.extensions_mut();
    let mut span = request_span(&mut exts);
    span.log(Log::new().log("span.kind", "server-receive"));
    let info = request
        .app_data::<Arc<dyn Agent>>()
        .expect("dyn Agent must be available as App::data")
        .agent_info(&mut span)
        .map_err(|error| fail_span(error, &mut *span))?;
    let response = HttpResponse::Ok().json(info);
    span.log(Log::new().log("span.kind", "server-send"));
    Ok(response)
}

/// API interface to Agent::datastore_info
pub struct DatastoreInfo {
    agent: Arc<dyn Agent>,
    cluster_display_name_override: Option<String>,
}

impl DatastoreInfo {
    pub fn new(
        agent: Arc<dyn Agent>,
        cluster_display_name_override: Option<String>,
    ) -> DatastoreInfo {
        DatastoreInfo {
            agent,
            cluster_display_name_override,
        }
    }
}

impl Responder for DatastoreInfo {
    type Error = actix_web::Error;
    type Future = Result<HttpResponse, actix_web::Error>;

    fn respond_to(self, request: &HttpRequest) -> Self::Future {
        let mut exts = request.extensions_mut();
        let mut span = request_span(&mut exts);
        span.log(Log::new().log("span.kind", "server-receive"));
        let mut info = self
            .agent
            .datastore_info(&mut span)
            .map_err(|error| fail_span(error, &mut *span))?;

        // Inject the cluster_display_name override if configured.
        info.cluster_display_name = self
            .cluster_display_name_override
            .or(info.cluster_display_name);

        let response = HttpResponse::Ok().json(info);
        span.log(Log::new().log("span.kind", "server-send"));
        Ok(response)
    }
}