use tonic_health::pb::{
    HealthCheckRequest, health_check_response::ServingStatus, health_client::HealthClient,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = std::env::args()
        .find_map(|arg| arg.strip_prefix("-addr=").map(String::from))
        .ok_or("usage: grpc_health_probe -addr=HOST:PORT")?;
    let channel = tonic::transport::Channel::from_shared(format!("http://{address}"))?
        .connect()
        .await?;
    let mut client = HealthClient::new(channel);
    let response = client
        .check(HealthCheckRequest {
            service: "tomi.ai.v1.AiGatewayService".into(),
        })
        .await?;
    if response.into_inner().status != ServingStatus::Serving as i32 {
        return Err("gRPC service not serving".into());
    }
    Ok(())
}
