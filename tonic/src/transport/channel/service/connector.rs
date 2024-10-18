use super::BoxedIo;
#[cfg(feature = "tls")]
use super::TlsConnector;
use crate::transport::channel::BoxFuture;
use crate::ConnectError;
use http::Uri;
use std::task::{Context, Poll};

use hyper::rt;

#[cfg(feature = "tls")]
use hyper_util::rt::TokioIo;
use tower_service::Service;

pub(crate) struct Connector<C> {
    inner: C,
    #[cfg(feature = "tls")]
    tls: Option<TlsConnector>,
}

impl<C> Connector<C> {
    pub(crate) fn new(inner: C, #[cfg(feature = "tls")] tls: Option<TlsConnector>) -> Self {
        Self {
            inner,
            #[cfg(feature = "tls")]
            tls,
        }
    }
}

impl<C> Service<Uri> for Connector<C>
where
    C: Service<Uri>,
    C::Response: rt::Read + rt::Write + Unpin + Send + 'static,
    C::Future: Send + 'static,
    crate::BoxError: From<C::Error> + Send + 'static,
{
    type Response = BoxedIo;
    type Error = ConnectError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner
            .poll_ready(cx)
            .map_err(|err| ConnectError(From::from(err)))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        #[cfg(feature = "tls")]
        let tls = self.tls.clone();

        #[cfg(feature = "tls")]
        let is_https = uri.scheme_str() == Some("https");
        let connect = self.inner.call(uri);

        Box::pin(async move {
            async {
                let io = connect.await?;

                #[cfg(feature = "tls")]
                if let Some(tls) = tls {
                    if is_https {
                        let io = tls.connect(TokioIo::new(io)).await?;
                        return Ok(io);
                    }
                }

                Ok::<_, crate::BoxError>(BoxedIo::new(io))
            }
            .await
            .map_err(ConnectError)
        })
    }
}
