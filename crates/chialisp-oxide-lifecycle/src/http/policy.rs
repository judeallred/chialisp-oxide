use hyper::{
    HeaderMap, Request, Version,
    body::Incoming,
    header::{HOST, ORIGIN},
};
use subtle::ConstantTimeEq;

pub(super) fn one<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let first = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    Some(first)
}

pub(super) fn same_origin(request: &Request<Incoming>, host: &str, origin: &str) -> bool {
    if request.version() != Version::HTTP_11
        || request.uri().scheme().is_some()
        || request.uri().authority().is_some()
        || request.uri().query().is_some()
        || one(request.headers(), HOST.as_str()) != Some(host)
    {
        return false;
    }
    if request.headers().contains_key(ORIGIN)
        && one(request.headers(), ORIGIN.as_str()) != Some(origin)
    {
        return false;
    }
    if request.headers().contains_key("sec-fetch-site")
        && !matches!(
            one(request.headers(), "sec-fetch-site"),
            Some("same-origin" | "none")
        )
    {
        return false;
    }
    true
}

pub(super) fn authorized(headers: &HeaderMap, capability: &str) -> bool {
    let Some(candidate) = one(headers, "x-oxide-session") else {
        return false;
    };
    candidate.len() == capability.len()
        && bool::from(candidate.as_bytes().ct_eq(capability.as_bytes()))
}
