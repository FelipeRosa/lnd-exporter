mod gen;

pub use gen::lnrpc::*;
use thiserror::Error;
pub use tonic::transport::Endpoint;

#[derive(Debug, Error)]
pub enum Error {
    #[error("openssl error")]
    Ssl(openssl::error::ErrorStack),
    #[error("hyper openssl error")]
    HyperSsl(openssl::error::ErrorStack),
    #[error("tonic transport error")]
    TonicTransport(#[from] tonic::transport::Error),
}

pub type LndClient = lightning_client::LightningClient<
    tonic::codegen::InterceptedService<tonic::transport::Channel, Interceptor>,
>;

pub async fn new<B: AsRef<[u8]>>(
    tls_cert: B,
    macaroon: B,
    endpoint: Endpoint,
) -> Result<LndClient, Error> {
    let connector = {
        let ssl_cert = openssl::x509::X509::from_pem(tls_cert.as_ref()).expect("pem");

        let mut ssl_connector = openssl::ssl::SslConnector::builder(openssl::ssl::SslMethod::tls())
            .map_err(Error::Ssl)?;

        ssl_connector
            .cert_store_mut()
            .add_cert(ssl_cert)
            .map_err(Error::Ssl)?;

        ssl_connector
            .set_alpn_protos(b"\x02h2")
            .map_err(Error::Ssl)?;

        let mut http_connector = hyper::client::HttpConnector::new();
        http_connector.enforce_http(false);

        hyper_openssl::HttpsConnector::with_connector(http_connector, ssl_connector)
            .map_err(Error::HyperSsl)?
    };

    let transport = endpoint
        .connect_with_connector(connector)
        .await
        .map_err(Error::TonicTransport)?;

    Ok(lightning_client::LightningClient::with_interceptor(
        transport,
        Interceptor {
            macaroon: Vec::from(macaroon.as_ref()),
        },
    ))
}

#[derive(Clone)]
pub struct Interceptor {
    macaroon: Vec<u8>,
}

impl tonic::service::Interceptor for Interceptor {
    fn call(&mut self, mut req: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        req.metadata_mut().insert(
            "macaroon",
            tonic::metadata::MetadataValue::from_str(hex::encode(&self.macaroon).as_str())
                .map_err(|_| tonic::Status::internal("failed setting gRPC macaroon metadata"))?,
        );

        Ok(req)
    }
}
