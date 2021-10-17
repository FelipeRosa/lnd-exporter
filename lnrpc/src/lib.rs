mod gen;

pub use gen::lnrpc::*;
use thiserror::Error;
pub use tonic::transport::Endpoint;

#[derive(Debug, Error)]
pub enum Error {
    #[error("tonic transport error")]
    TonicTransport(#[from] tonic::transport::Error),
}

pub type LndClient = lightning_client::LightningClient<
    tonic::codegen::InterceptedService<tonic::transport::Channel, Interceptor>,
>;

pub async fn new<B1: AsRef<[u8]>, B2: AsRef<[u8]>>(
    tls_cert: Option<B1>,
    macaroon: Option<B2>,
    endpoint: Endpoint,
) -> Result<LndClient, Error> {
    let mut tls_config = tonic::transport::ClientTlsConfig::new();

    if let Some(tls_cert) = tls_cert {
        tls_config = tls_config.ca_certificate(tonic::transport::Certificate::from_pem(tls_cert));
    }

    let transport = endpoint
        .tls_config(tls_config)?
        .connect()
        .await
        .map_err(Error::TonicTransport)?;

    Ok(lightning_client::LightningClient::with_interceptor(
        transport,
        Interceptor {
            macaroon: macaroon.map(|mac| Vec::from(mac.as_ref())),
        },
    ))
}

#[derive(Clone)]
pub struct Interceptor {
    macaroon: Option<Vec<u8>>,
}

impl tonic::service::Interceptor for Interceptor {
    fn call(&mut self, mut req: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        if let Some(macaroon) = &self.macaroon {
            req.metadata_mut().insert(
                "macaroon",
                tonic::metadata::MetadataValue::from_str(hex::encode(macaroon).as_str()).map_err(
                    |_| tonic::Status::internal("failed setting gRPC macaroon metadata"),
                )?,
            );
        }

        Ok(req)
    }
}
