use anyhow::{Context as _, Result, bail};
use clap::Parser;
use serde::Serialize;
use std::time::Duration;
use url::Url;

#[allow(unused_qualifications)]
pub mod proto {
    tonic::include_proto!("bcs.provider.demo.v1");
}

#[derive(Debug, Parser)]
#[command(name = "bcs-provider-demo-client")]
#[command(about = "Call a Provider SDK demo server over gRPC")]
pub struct Cli {
    #[arg(long)]
    pub endpoint: String,

    #[arg(long)]
    pub message: String,
}

pub fn parse_endpoint(value: &str) -> Result<Url> {
    let endpoint = Url::parse(value).with_context(|| format!("invalid endpoint: {value}"))?;
    if !matches!(endpoint.scheme(), "http" | "https") {
        bail!("endpoint must use http:// or https://");
    }
    if endpoint.host_str().is_none() {
        bail!("endpoint must include a host");
    }
    Ok(endpoint)
}

pub fn render_response(response: &proto::InvokeResponse) -> Result<String> {
    #[derive(Serialize)]
    struct JsonResponse<'a> {
        message: &'a str,
        implementation: &'a str,
    }

    serde_json::to_string(&JsonResponse {
        message: &response.message,
        implementation: &response.implementation,
    })
    .context("failed to serialize provider response")
}

pub async fn invoke(endpoint: &Url, message: &str) -> Result<proto::InvokeResponse> {
    let transport = tonic::transport::Endpoint::from_shared(endpoint.as_str().to_owned())
        .context("invalid gRPC endpoint")?
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(5));
    let channel = transport
        .connect()
        .await
        .with_context(|| format!("failed to connect to {endpoint}"))?;
    let mut client = proto::provider_demo_client::ProviderDemoClient::new(channel);
    let response = client
        .invoke(proto::InvokeRequest {
            message: message.to_owned(),
        })
        .await
        .with_context(|| format!("provider invocation failed for {endpoint}"))?;
    Ok(response.into_inner())
}

pub async fn execute(cli: Cli) -> Result<String> {
    let endpoint = parse_endpoint(&cli.endpoint)?;
    let response = invoke(&endpoint, &cli.message).await?;
    render_response(&response)
}

#[cfg(test)]
mod tests {
    use super::proto::provider_demo_server::{ProviderDemo, ProviderDemoServer};
    use super::proto::{InvokeRequest, InvokeResponse};
    use super::{Cli, invoke, parse_endpoint, render_response};
    use clap::Parser;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::{Request, Response, Status};

    struct EchoProvider;

    #[tonic::async_trait]
    impl ProviderDemo for EchoProvider {
        async fn invoke(
            &self,
            request: Request<InvokeRequest>,
        ) -> Result<Response<InvokeResponse>, Status> {
            Ok(Response::new(InvokeResponse {
                message: format!("rust-test: {}", request.into_inner().message),
                implementation: "rust-test".to_owned(),
            }))
        }
    }

    #[test]
    fn endpoint_requires_http_or_https_scheme() {
        assert!(parse_endpoint("127.0.0.1:50051").is_err());
    }

    #[test]
    fn cli_parses_endpoint_and_message() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from([
            "bcs-provider-demo-client",
            "--endpoint",
            "http://127.0.0.1:50051",
            "--message",
            "hello",
        ])?;

        assert_eq!(cli.endpoint, "http://127.0.0.1:50051");
        assert_eq!(cli.message, "hello");
        Ok(())
    }

    #[test]
    fn response_renders_as_one_stable_json_object() {
        let response = InvokeResponse {
            message: "python: hello".to_owned(),
            implementation: "python".to_owned(),
        };

        let rendered = render_response(&response);

        assert_eq!(
            rendered.map_err(|error| error.to_string()),
            Ok(r#"{"message":"python: hello","implementation":"python"}"#.to_owned()),
        );
    }

    #[tokio::test]
    async fn invoke_calls_a_real_grpc_server() -> anyhow::Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let address = listener.local_addr()?;
        let incoming = TcpListenerStream::new(listener);
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let server = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(ProviderDemoServer::new(EchoProvider))
                .serve_with_incoming_shutdown(incoming, async {
                    let _ = shutdown_receiver.await;
                })
                .await
        });

        let endpoint = parse_endpoint(&format!("http://{address}"))?;
        let response = invoke(&endpoint, "hello").await?;

        assert_eq!(response.message, "rust-test: hello");
        assert_eq!(response.implementation, "rust-test");
        let _ = shutdown_sender.send(());
        server.await??;
        Ok(())
    }
}
