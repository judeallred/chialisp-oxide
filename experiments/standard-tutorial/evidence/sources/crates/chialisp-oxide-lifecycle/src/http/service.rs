use super::{
    BODY_TIMEOUT, MAX_BODY, policy,
    session::{ApiError, CommandRequest, Shared},
};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::{
    Method, Request, Response, StatusCode,
    body::Incoming,
    header::{CONTENT_LENGTH, CONTENT_TYPE, ORIGIN},
};
use serde_json::json;
use std::{convert::Infallible, sync::Arc};

type Reply = Response<Full<Bytes>>;

fn response(status: StatusCode, content_type: &'static str, body: impl Into<Bytes>) -> Reply {
    let mut response = Response::new(Full::new(body.into()));
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
    let status = match error.kind {
        "revision_conflict" | "busy" => StatusCode::CONFLICT,
        "simulator_command" => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    failure(status, error)
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
            ApiError::new("forbidden", "The request is not from this loopback origin."),
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
    let path = request.uri().path();
    if request.method() == Method::GET {
        match path {
            "/" => {
                return response(
                    StatusCode::OK,
                    "text/html; charset=utf-8",
                    include_str!("assets/index.html")
                        .replace("__SESSION_CAPABILITY__", &shared.capability),
                );
            }
            "/app.js" => {
                return response(
                    StatusCode::OK,
                    "text/javascript; charset=utf-8",
                    include_str!("assets/app.js"),
                );
            }
            "/app.css" => {
                return response(
                    StatusCode::OK,
                    "text/css; charset=utf-8",
                    include_str!("assets/app.css"),
                );
            }
            _ => (),
        }
    }
    if !matches!(path, "/api/state" | "/api/commands") {
        return failure(
            StatusCode::NOT_FOUND,
            ApiError::new("not_found", "Unknown simulator route."),
        );
    }
    if !policy::authorized(request.headers(), &shared.capability) {
        return failure(
            StatusCode::FORBIDDEN,
            ApiError::new(
                "forbidden",
                "This simulator session capability is required.",
            ),
        );
    }
    if path == "/api/state" && request.method() == Method::GET {
        return match shared.operate(None).await {
            Ok(value) => response(StatusCode::OK, "application/json", value.to_string()),
            Err(error) => api_failure(error),
        };
    }
    if path != "/api/commands" || request.method() != Method::POST {
        return failure(
            StatusCode::METHOD_NOT_ALLOWED,
            ApiError::new("method_not_allowed", "Unsupported simulator API method."),
        );
    }
    if policy::one(request.headers(), ORIGIN.as_str()) != Some(shared.origin.as_str()) {
        return failure(
            StatusCode::FORBIDDEN,
            ApiError::new(
                "forbidden",
                "Simulator commands require the exact browser origin.",
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
                "Commands require unencoded application/json.",
            ),
        );
    }
    if request.headers().contains_key(CONTENT_LENGTH) {
        let Some(length) = policy::one(request.headers(), CONTENT_LENGTH.as_str())
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return failure(
                StatusCode::BAD_REQUEST,
                ApiError::new("invalid_request", "Invalid body length."),
            );
        };
        if length > MAX_BODY as u64 {
            return failure(
                StatusCode::PAYLOAD_TOO_LARGE,
                ApiError::new(
                    "http_body_limit",
                    "Command body exceeds the local byte limit.",
                ),
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
                ApiError::new(
                    "http_body_timeout",
                    "Command body was not received in time.",
                ),
            );
        }
        Ok(Err(error)) if error.is::<http_body_util::LengthLimitError>() => {
            return failure(
                StatusCode::PAYLOAD_TOO_LARGE,
                ApiError::new(
                    "http_body_limit",
                    "Command body exceeded its local byte limit.",
                ),
            );
        }
        Ok(Err(_)) => {
            return failure(
                StatusCode::BAD_REQUEST,
                ApiError::new(
                    "http_body_read",
                    "Command body could not be read. No command was started.",
                ),
            );
        }
        Ok(Ok(body)) => body,
    };
    if body.trailers().is_some() {
        return failure(
            StatusCode::BAD_REQUEST,
            ApiError::new("invalid_request", "Command trailers are unsupported."),
        );
    }
    let command: CommandRequest = match serde_json::from_slice(&body.to_bytes()) {
        Ok(command) => command,
        Err(_) => {
            return failure(
                StatusCode::BAD_REQUEST,
                ApiError::new(
                    "invalid_request",
                    "Expected only a revision and a trusted simulator command.",
                ),
            );
        }
    };
    match shared.operate(Some(command)).await {
        Ok(value) => response(StatusCode::OK, "application/json", value.to_string()),
        Err(error) => api_failure(error),
    }
}
