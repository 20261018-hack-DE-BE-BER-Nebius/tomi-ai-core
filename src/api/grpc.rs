use crate::{
    AppState,
    api::{authorized, models, types::ChatInput},
    application::chat,
};
use futures_util::{Stream, StreamExt};
use std::pin::Pin;
use tokio::sync::mpsc;
use tonic::{Request, Response, Status};

pub mod proto {
    tonic::include_proto!("tomi.ai.v1");
}
use proto::{
    ChatRequest, ChatResponse, GetModelsRequest, GetModelsResponse,
    ai_gateway_service_server::{AiGatewayService, AiGatewayServiceServer},
};

#[derive(Clone)]
struct Gateway(AppState);
impl Gateway {
    fn authorize<T>(&self, request: &Request<T>) -> Result<(), Status> {
        if authorized(
            request
                .metadata()
                .get("authorization")
                .and_then(|v| v.to_str().ok()),
            &self.0.config.core_token,
        ) {
            Ok(())
        } else {
            Err(Status::unauthenticated("Internal authentication required"))
        }
    }
}

#[tonic::async_trait]
impl AiGatewayService for Gateway {
    type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatResponse, Status>> + Send>>;
    async fn chat(
        &self,
        request: Request<ChatRequest>,
    ) -> Result<Response<Self::ChatStream>, Status> {
        self.authorize(&request)?;
        let input = request.into_inner();
        let input = ChatInput {
            session_id: input.session_id,
            request_id: input.request_id,
            text: input.text,
            model: input.model,
            language: input.language,
            context: vec![],
        };
        chat::validate(&input).map_err(|e| Status::invalid_argument(e.message))?;
        let (tx, rx) = mpsc::channel(128);
        let state = self.0.clone();
        let task = tokio::spawn(chat::run(
            state,
            input,
            uuid::Uuid::new_v4().to_string(),
            tx,
        ));
        let guard = crate::infrastructure::task::AbortOnDrop(task.abort_handle());
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(move |e| {
            let _keep_task = &guard;
            Ok(ChatResponse {
                r#type: e.kind,
                run_id: e.run_id,
                session_id: e.session_id,
                timestamp: e.timestamp,
                data_json: e.data.to_string(),
            })
        });
        Ok(Response::new(Box::pin(stream)))
    }
    async fn get_models(
        &self,
        request: Request<GetModelsRequest>,
    ) -> Result<Response<GetModelsResponse>, Status> {
        self.authorize(&request)?;
        Ok(Response::new(GetModelsResponse {
            catalog_json: models::catalog(&self.0).await.to_string(),
        }))
    }
}

pub async fn serve(state: AppState) -> Result<(), tonic::transport::Error> {
    let addr = state.config.grpc_addr;
    let (reporter, health) = tonic_health::server::health_reporter();
    reporter
        .set_serving::<AiGatewayServiceServer<Gateway>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health)
        .add_service(AiGatewayServiceServer::new(Gateway(state)))
        .serve(addr)
        .await
}

pub fn http_routes(state: AppState) -> axum::Router {
    tonic::service::Routes::new(AiGatewayServiceServer::new(Gateway(state)))
        .into_axum_router()
        .reset_fallback()
}
