//! Adapter that lets the Flagsmith SDK behavioural spec drive this SDK.
//!
//! Protocol: https://github.com/Flagsmith/sdk-spec/blob/main/ADAPTER.md

use std::collections::HashMap;
use std::panic::{self, AssertUnwindSafe};

use flagsmith::flagsmith::models::{Flags, SDKTrait};
use flagsmith::{Flagsmith, FlagsmithOptions};
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

/// The SDK does not expose its version, so read it from its manifest.
const SDK_MANIFEST: &str = include_str!("../../Cargo.toml");

type Reply = (u16, Option<Value>);

#[derive(Default)]
struct Adapter {
    clients: HashMap<String, Flagsmith>,
    next_client_id: u64,
}

fn sdk_version() -> &'static str {
    SDK_MANIFEST
        .lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or("unknown")
}

fn error(status: u16, message: impl ToString) -> Reply {
    (status, Some(json!({ "error": message.to_string() })))
}

fn serialise_flags(flags: Flags) -> Result<Value, String> {
    let flags = flags
        .all_flags()
        .into_iter()
        .map(|flag| {
            // `FlagsmithValue` serialises to a JSON value of its own type.
            let value = serde_json::to_value(&flag.value).map_err(|e| e.to_string())?;
            Ok(json!({
                "featureName": flag.feature_name,
                "enabled": flag.enabled,
                "value": value,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({ "flags": flags }))
}

fn parse_traits(traits: &Value) -> Result<Vec<SDKTrait>, String> {
    let Some(traits) = traits.as_object() else {
        return Ok(vec![]);
    };
    traits
        .iter()
        .map(|(key, value)| {
            // `FlagsmithValue` deserialises from a JSON value, keeping its type.
            serde_json::from_value(json!({ "trait_key": key, "trait_value": value }))
                .map_err(|e| format!("Invalid trait {key:?}: {e}"))
        })
        .collect()
}

impl Adapter {
    fn route(&mut self, method: &Method, path: &str, body: Value) -> Reply {
        match (method, path) {
            (Method::Get, "/health") => {
                return (
                    200,
                    Some(json!({ "sdk": "flagsmith-rust-client", "version": sdk_version() })),
                )
            }
            (Method::Post, "/clients") => return self.create_client(&body),
            _ => {}
        }

        let not_found = || error(404, format!("No route for {method} {path}"));
        let Some(rest) = path.strip_prefix("/clients/") else {
            return not_found();
        };
        let (client_id, action) = rest.split_once('/').unwrap_or((rest, ""));
        let Some(client) = self.clients.get(client_id) else {
            return not_found();
        };

        let flags = match (method, action) {
            (Method::Delete, "") => {
                self.clients.remove(client_id);
                return (204, None);
            }
            (Method::Post, "environment-flags") => client.get_environment_flags(),
            (Method::Post, "identity-flags") => {
                let Some(identifier) = body["identifier"].as_str() else {
                    return error(400, "identifier is required");
                };
                let traits = match parse_traits(&body["traits"]) {
                    Ok(traits) => traits,
                    Err(e) => return error(400, e),
                };
                client.get_identity_flags(identifier, Some(traits), None)
            }
            _ => return not_found(),
        };
        match flags.map_err(|e| e.to_string()).and_then(serialise_flags) {
            Ok(flags) => (200, Some(flags)),
            Err(e) => error(500, e),
        }
    }

    fn create_client(&mut self, body: &Value) -> Reply {
        let (Some(environment_key), Some(api_url)) =
            (body["environmentKey"].as_str(), body["apiUrl"].as_str())
        else {
            return error(400, "environmentKey and apiUrl are required");
        };
        let options = FlagsmithOptions {
            api_url: api_url.to_string(),
            ..Default::default()
        };
        let client = Flagsmith::new(environment_key.to_string(), options);
        self.next_client_id += 1;
        let client_id = self.next_client_id.to_string();
        self.clients.insert(client_id.clone(), client);
        (201, Some(json!({ "clientId": client_id })))
    }
}

fn handle(adapter: &mut Adapter, request: &mut Request) -> Reply {
    let mut raw = String::new();
    if let Err(e) = request.as_reader().read_to_string(&mut raw) {
        return error(400, e);
    }
    let body = if raw.trim().is_empty() {
        Value::Null
    } else {
        match serde_json::from_str(&raw) {
            Ok(body) => body,
            Err(e) => return error(400, format!("Invalid JSON body: {e}")),
        }
    };
    let (method, path) = (request.method().clone(), request.url().to_string());
    // The SDK panics on some errors; report them to the runner instead of exiting.
    panic::catch_unwind(AssertUnwindSafe(|| adapter.route(&method, &path, body)))
        .unwrap_or_else(|_| error(500, "The SDK panicked; see the adapter logs"))
}

fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let server = Server::http(format!("0.0.0.0:{port}")).expect("Failed to start the adapter");
    println!("adapter: listening on :{port}");

    let mut adapter = Adapter::default();
    for mut request in server.incoming_requests() {
        let (status, body) = handle(&mut adapter, &mut request);
        println!("adapter: {} {} {status}", request.method(), request.url());
        let response = match body {
            Some(body) => Response::from_string(body.to_string()).with_header(
                "Content-Type: application/json"
                    .parse::<Header>()
                    .expect("Valid header"),
            ),
            None => Response::from_string(""),
        };
        if let Err(e) = request.respond(response.with_status_code(status)) {
            eprintln!("adapter: failed to respond: {e}");
        }
    }
}
