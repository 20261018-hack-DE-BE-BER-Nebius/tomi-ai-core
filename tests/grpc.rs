mod support;
use support::fixture;
use tomi_ai_core::{AppState, api};

#[tokio::test]
async fn grpc_requires_token_and_streams_same_contract() {
    use api::grpc::proto::{
        ChatRequest, GetModelsRequest, ai_gateway_service_client::AiGatewayServiceClient,
    };
    let (state, fixture_server) = fixture().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let mut configured = (*state.config).clone();
    configured.grpc_addr = address;
    let server = tokio::spawn(api::grpc::serve(AppState::new(configured).unwrap()));
    let mut client = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match AiGatewayServiceClient::connect(format!("http://{address}")).await {
                Ok(client) => break client,
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .expect("gRPC listener starts");
    assert_eq!(
        client
            .get_models(GetModelsRequest {})
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unauthenticated
    );
    let mut request = tonic::Request::new(ChatRequest {
        session_id: "grpc-session".into(),
        request_id: "r1".into(),
        text: "Compare air".into(),
        model: "gpt-6-astra".into(),
        language: "en".into(),
    });
    request.metadata_mut().insert(
        "authorization",
        "Bearer fixture-internal-token-with-32-characters"
            .parse()
            .unwrap(),
    );
    let mut stream = client.chat(request).await.unwrap().into_inner();
    let mut completed = false;
    while let Some(event) = stream.message().await.unwrap() {
        if event.r#type == "answer.completed" {
            completed = true;
            assert!(event.data_json.contains("Source evidence"));
        }
    }
    assert!(completed);
    server.abort();
    fixture_server.abort();
}

#[tokio::test]
async fn grpc_works_on_shared_http2_port_without_changing_http_errors() {
    use api::grpc::proto::{GetModelsRequest, ai_gateway_service_client::AiGatewayServiceClient};
    let (state, fixture_server) = fixture().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, api::router(state)).await.unwrap() });
    let mut client = AiGatewayServiceClient::connect(format!("http://{address}"))
        .await
        .unwrap();
    assert_eq!(
        client
            .get_models(GetModelsRequest {})
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unauthenticated
    );
    let mut request = tonic::Request::new(GetModelsRequest {});
    request.metadata_mut().insert(
        "authorization",
        "Bearer fixture-internal-token-with-32-characters"
            .parse()
            .unwrap(),
    );
    let catalog = client
        .get_models(request)
        .await
        .unwrap()
        .into_inner()
        .catalog_json;
    assert!(catalog.contains("gpt-6-astra"));
    assert_eq!(
        reqwest::get(format!("http://{address}/missing"))
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::NOT_FOUND
    );
    server.abort();
    fixture_server.abort();
}
