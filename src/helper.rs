use std::sync::atomic::AtomicUsize;

use bevy::remote::BrpRequest;
use ehttp::Response;
use lazy_static::lazy_static;
use serde::{de::DeserializeOwned, Serialize};

lazy_static! {
    static ref COUNTER: AtomicUsize = AtomicUsize::new(1);
}

pub fn create_request<T: Serialize>(value: Option<T>, method: impl ToString) -> BrpRequest {
    let params = match value {
        None => None,
        Some(value) => Some(
            serde_json::to_value(value)
                .expect("Unable to convert query parameters to a valid JSON value"),
        ),
    };
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    BrpRequest {
        jsonrpc: String::from("2.0"),
        method: method.to_string(),
        id: Some(counter.into()),
        params,
    }
}

pub fn make_request<T: Serialize>(
    value: T,
    method: impl ToString,
    url: impl ToString,
) -> ehttp::Request {
    let request = create_request(Some(value), method);
    ehttp::Request {
        method: "GET".to_string(),
        url: url.to_string(),
        body: serde_json::to_string(&request).unwrap().into_bytes(),
        headers: Default::default(),
    }
}

pub fn make_empty_request(method: impl ToString, url: impl ToString) -> ehttp::Request {
    let request = create_request::<String>(None, method);
    ehttp::Request {
        method: "GET".to_string(),
        url: url.to_string(),
        body: serde_json::to_string(&request).unwrap().into_bytes(),
        headers: Default::default(),
    }
}

pub fn parse<T>(response: &Response) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let Some(json) = response.text() else {
        return Err("Cannot parse text".into());
    };
    let result: jsonrpc_types::v2::Response = serde_json::from_str(json).unwrap();
    let jsonrpc_types::v2::Response::Single(result) = result else {
        return Err("NOT ONE".to_string());
    };

    let result: jsonrpc_types::Success = match result {
        jsonrpc_types::Output::Success(result) => result,
        jsonrpc_types::Output::Failure(e) => {
            return Err(e.to_string());
        }
    };
    match serde_json::from_value(result.result) {
        Ok(v) => Ok(v),
        Err(e) => Err(e.to_string()),
    }
}
