use super::{
    Shared,
    worker::{ApiError, LiveAction},
};
use crate::http::{BODY_TIMEOUT, MAX_BODY, policy};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::{
    Method, Request, Response, StatusCode,
    body::Incoming,
    header::{CONTENT_LENGTH, CONTENT_TYPE, ORIGIN},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{convert::Infallible, sync::Arc};

const MAX_REPLY: usize = 64 * 1024;
type Reply = Response<Full<Bytes>>;

fn response(status: StatusCode, content_type: &'static str, body: impl Into<Bytes>) -> Reply {
    let body = body.into();
    let (status, content_type, body) = if body.len() <= MAX_REPLY {
        (status, content_type, body)
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "application/json", Bytes::from_static(b"{\"error\":{\"kind\":\"response_limit\",\"message\":\"Complete response exceeded its bound.\"}}"))
    };
    let mut response = Response::new(Full::new(body));
    *response.status_mut() = status;
    for (key, value) in [
        ("content-type", content_type),
        ("cache-control", "no-store"),
        (
            "content-security-policy",
            "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'",
        ),
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("cross-origin-resource-policy", "same-origin"),
        ("referrer-policy", "no-referrer"),
    ] {
        response
            .headers_mut()
            .insert(key, hyper::header::HeaderValue::from_static(value));
    }
    response
}
fn failure(status: StatusCode, error: ApiError) -> Reply {
    response(
        status,
        "application/json",
        json!({"error":error}).to_string(),
    )
}
fn api_failure(error: ApiError) -> Reply {
    let status = match error.kind.as_str() {
        "busy" | "revision_conflict" | "reload_required" => StatusCode::CONFLICT,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    failure(status, error)
}
fn json_response(status: StatusCode, value: &impl Serialize) -> Reply {
    struct Bounded(Vec<u8>);
    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_REPLY - self.0.len() {
                return Err(std::io::Error::other("reply bound"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded(vec![]);
    if serde_json::to_writer(&mut writer, value).is_err() {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            ApiError::new(
                "response_limit",
                "The complete public response could not be rendered within its bound.",
            ),
        );
    }
    response(status, "application/json", writer.0)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionRequest {
    action: LiveAction,
    revision: String,
}

pub(super) async fn handle(
    request: Request<Incoming>,
    shared: Arc<Shared>,
) -> Result<Reply, Infallible> {
    Ok(route(request, shared).await)
}
async fn route(request: Request<Incoming>, shared: Arc<Shared>) -> Reply {
    if !policy::same_origin(&request, &shared.host, &shared.origin) {
        return failure(
            StatusCode::FORBIDDEN,
            ApiError::new(
                "forbidden",
                "This request is not from the exact live loopback origin.",
            ),
        );
    }
    if request.method() == Method::GET
        && (request.headers().contains_key("transfer-encoding")
            || (request.headers().contains_key(CONTENT_LENGTH)
                && policy::one(request.headers(), CONTENT_LENGTH.as_str()) != Some("0")))
    {
        return failure(
            StatusCode::BAD_REQUEST,
            ApiError::new("invalid_request", "GET requests cannot carry a body."),
        );
    }
    if request.method() == Method::GET {
        let asset = match request.uri().path() {
            "/" => Some((
                "text/html; charset=utf-8",
                include_str!("assets/index.html")
                    .replace("__SESSION_CAPABILITY__", &shared.capability),
            )),
            "/app.js" => Some((
                "text/javascript; charset=utf-8",
                include_str!("assets/app.js").to_owned(),
            )),
            "/app.css" => Some((
                "text/css; charset=utf-8",
                include_str!("assets/style.css").to_owned(),
            )),
            _ => None,
        };
        if let Some((kind, body)) = asset {
            return response(StatusCode::OK, kind, body);
        }
    }
    let path = request.uri().path();
    if !matches!(path, "/api/state" | "/api/action") {
        return failure(
            StatusCode::NOT_FOUND,
            ApiError::new("not_found", "Unknown live loopback route."),
        );
    }
    if !policy::authorized(request.headers(), &shared.capability) {
        return failure(
            StatusCode::FORBIDDEN,
            ApiError::new("forbidden", "This live session capability is required."),
        );
    }
    if path == "/api/state" && request.method() == Method::GET {
        return match shared.worker.snapshot() {
            Ok(snapshot) => json_response(StatusCode::OK, &snapshot),
            Err(error) => api_failure(error),
        };
    }
    if path != "/api/action" || request.method() != Method::POST {
        return failure(
            StatusCode::METHOD_NOT_ALLOWED,
            ApiError::new("method_not_allowed", "Unsupported live API method."),
        );
    }
    if policy::one(request.headers(), ORIGIN.as_str()) != Some(shared.origin.as_str()) {
        return failure(
            StatusCode::FORBIDDEN,
            ApiError::new(
                "forbidden",
                "Live actions require the exact browser Origin.",
            ),
        );
    }
    if policy::one(request.headers(), CONTENT_TYPE.as_str()) != Some("application/json")
        || request.headers().contains_key("content-encoding")
    {
        return failure(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ApiError::new(
                "invalid_request",
                "Live actions require unencoded application/json.",
            ),
        );
    }
    if request.headers().contains_key(CONTENT_LENGTH) {
        let Some(length) = policy::one(request.headers(), CONTENT_LENGTH.as_str())
            .and_then(|length| length.parse::<u64>().ok())
        else {
            return failure(
                StatusCode::BAD_REQUEST,
                ApiError::new("invalid_request", "Invalid action body length."),
            );
        };
        if length > MAX_BODY as u64 {
            return failure(
                StatusCode::PAYLOAD_TOO_LARGE,
                ApiError::new("http_body_limit", "Action body exceeds the local limit."),
            );
        }
    }
    let body = match tokio::time::timeout(
        BODY_TIMEOUT,
        Limited::new(request.into_body(), MAX_BODY).collect(),
    )
    .await
    {
        Err(_) => {
            return failure(
                StatusCode::REQUEST_TIMEOUT,
                ApiError::new("http_body_timeout", "Action body was not received in time."),
            );
        }
        Ok(Err(error)) if error.is::<http_body_util::LengthLimitError>() => {
            return failure(
                StatusCode::PAYLOAD_TOO_LARGE,
                ApiError::new("http_body_limit", "Action body exceeded the local limit."),
            );
        }
        Ok(Err(_)) => {
            return failure(
                StatusCode::BAD_REQUEST,
                ApiError::new(
                    "http_body_read",
                    "Action body could not be read. No operation was queued.",
                ),
            );
        }
        Ok(Ok(body)) => body,
    };
    if body.trailers().is_some() {
        return failure(
            StatusCode::BAD_REQUEST,
            ApiError::new("invalid_request", "Action trailers are unsupported."),
        );
    }
    let command: ActionRequest = match serde_json::from_slice(&body.to_bytes()) {
        Ok(command) => command,
        Err(_) => {
            return failure(
                StatusCode::BAD_REQUEST,
                ApiError::new(
                    "invalid_request",
                    "Expected only one closed action and exact outbox revision.",
                ),
            );
        }
    };
    if command.revision.len() != 64
        || !command
            .revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return failure(
            StatusCode::BAD_REQUEST,
            ApiError::new(
                "invalid_request",
                "Outbox revision must be 32 bytes of lowercase hex.",
            ),
        );
    }
    let mut revision = [0; 32];
    if hex::decode_to_slice(&command.revision, &mut revision).is_err() {
        return failure(
            StatusCode::BAD_REQUEST,
            ApiError::new("invalid_request", "Malformed outbox revision."),
        );
    }
    match shared.worker.accept(command.action, revision) {
        Ok(id) => json_response(StatusCode::ACCEPTED, &json!({"job_id":id})),
        Err(error) => api_failure(error),
    }
}
