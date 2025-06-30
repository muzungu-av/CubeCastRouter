use super::message::Validate;
use actix_web::{dev::Payload, error::ErrorBadRequest, web::Json, Error, FromRequest, HttpRequest};
use futures_core::future::LocalBoxFuture;

pub struct ValidatedJson<T>(pub T);

impl<T> FromRequest for ValidatedJson<T>
where
    T: serde::de::DeserializeOwned + Validate + 'static,
{
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let fut = Json::<T>::from_request(req, payload);
        Box::pin(async move {
            let Json(inner) = fut.await.map_err(ErrorBadRequest)?;
            inner.validate().map_err(ErrorBadRequest)?;
            Ok(ValidatedJson(inner))
        })
    }
}
