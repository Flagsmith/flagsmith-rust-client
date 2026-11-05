use super::models::SDKTrait;
use flagsmith_flag_engine::types::{FlagsmithValue, FlagsmithValueType};
use log::{debug, warn};
use reqwest::header::{self, HeaderMap};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::thread;
use std::time::{Duration, Instant};

pub const DEFAULT_EVENTS_API_URL: &str = "https://events.api.flagsmith.com/";
pub const DEFAULT_EVENTS_FLUSH_INTERVAL_MILLS: u64 = 10 * 1000;
pub const DEFAULT_EVENTS_MAX_BUFFER_SIZE: usize = 1000;
/// The event recorded when an identity is exposed to a variant of a feature.
pub const FLAG_EXPOSURE_EVENT: &str = "$flag_exposure";
const EVENTS_ENDPOINT: &str = "v1/events";

/// The optional fields of a tracked event.
#[derive(Clone, Debug, Default)]
pub struct EventOptions {
    /// The identity the event is about. Required for exposure events.
    pub identifier: Option<String>,
    /// The event's value, e.g. an order total, or the variant an identity was exposed to.
    pub value: Option<FlagsmithValue>,
    pub traits: Option<Vec<SDKTrait>>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
struct Event {
    event: String,
    feature_name: Option<String>,
    identifier: Option<String>,
    value: Option<String>,
    traits: Option<HashMap<String, FlagsmithValue>>,
    metadata: HashMap<String, serde_json::Value>,
    timestamp: i64,
}

impl Event {
    fn new(event: &str, feature_name: Option<&str>, options: EventOptions) -> Event {
        let traits = options
            .traits
            .filter(|traits| !traits.is_empty())
            .map(|traits| {
                traits
                    .into_iter()
                    .map(|t| (t.trait_key, t.trait_value))
                    .collect()
            });
        let mut metadata = options.metadata.unwrap_or_default();
        metadata.insert(
            "sdk_version".to_string(),
            serde_json::Value::from(env!("CARGO_PKG_VERSION")),
        );
        Event {
            event: event.to_string(),
            feature_name: feature_name.map(str::to_string),
            identifier: options.identifier,
            value: options
                .value
                .filter(|value| value.value_type != FlagsmithValueType::None)
                .map(|value| value.value),
            traits,
            metadata,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Who saw which variant of which feature, for exposure events only.
    fn exposure_key(&self) -> Option<ExposureKey> {
        (self.event == FLAG_EXPOSURE_EVENT).then(|| ExposureKey {
            feature_name: self.feature_name.clone(),
            identifier: self.identifier.clone(),
            value: self.value.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ExposureKey {
    feature_name: Option<String>,
    identifier: Option<String>,
    value: Option<String>,
}

enum Message {
    Track(Event),
    Flush(flume::Sender<()>),
}

enum SendOutcome {
    Delivered,
    Retryable,
    Rejected,
}

/// Events waiting to be sent. Equal exposures add no information, so only the first one
/// waiting is kept.
struct EventBuffer {
    events: Vec<Event>,
    exposures: HashSet<ExposureKey>,
    max_size: usize,
    /// A send failed: wait for the next timed flush rather than retry on every event.
    held: bool,
}

impl EventBuffer {
    fn new(max_size: usize) -> EventBuffer {
        EventBuffer {
            events: vec![],
            exposures: HashSet::new(),
            max_size: max_size.max(1),
            held: false,
        }
    }

    fn push(&mut self, event: Event) {
        if let Some(key) = event.exposure_key() {
            if !self.exposures.insert(key) {
                debug!(
                    "Skipping duplicate {} event for feature {:?}",
                    FLAG_EXPOSURE_EVENT, event.feature_name
                );
                return;
            }
        }
        if self.events.len() >= self.max_size {
            let oldest = self.events.remove(0);
            if let Some(key) = oldest.exposure_key() {
                self.exposures.remove(&key);
            }
            warn!("Events buffer is full; dropped the oldest event");
        }
        self.events.push(event);
    }

    fn is_full(&self) -> bool {
        self.events.len() >= self.max_size
    }

    fn clear(&mut self) {
        self.events.clear();
        self.exposures.clear();
    }
}

/// Buffers tracking and exposure events, and sends them to Flagsmith's events API in
/// batches: on a timer, when the buffer fills, on `flush`, and when dropped.
#[derive(Clone, Debug)]
pub struct EventProcessor {
    tx: flume::Sender<Message>,
}

impl EventProcessor {
    pub fn new(
        environment_key: &str,
        events_api_url: &str,
        timeout: Duration,
        flush_interval: Duration,
        max_buffer_size: usize,
    ) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Environment-Key",
            header::HeaderValue::from_str(environment_key).unwrap(),
        );
        headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
        let user_agent = header::HeaderValue::from_str(&super::get_user_agent()).unwrap();
        headers.insert(header::USER_AGENT, user_agent.clone());
        // The events API records which SDK sent the events from this header.
        headers.insert("Flagsmith-SDK-User-Agent", user_agent);
        let client = reqwest::blocking::Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()
            .unwrap();
        let separator = if events_api_url.ends_with('/') {
            ""
        } else {
            "/"
        };
        let endpoint = format!("{events_api_url}{separator}{EVENTS_ENDPOINT}");

        let (tx, rx) = flume::unbounded();
        thread::Builder::new()
            .name("Event Processor".to_string())
            .spawn(move || {
                let mut buffer = EventBuffer::new(max_buffer_size);
                let mut last_flushed = Instant::now();
                loop {
                    let message =
                        rx.recv_timeout(flush_interval.saturating_sub(last_flushed.elapsed()));
                    match message {
                        Ok(Message::Track(event)) => {
                            buffer.push(event);
                            if buffer.is_full() && !buffer.held {
                                flush(&client, &endpoint, &mut buffer);
                            }
                        }
                        Ok(Message::Flush(done)) => {
                            flush(&client, &endpoint, &mut buffer);
                            let _ = done.send(());
                        }
                        Err(flume::RecvTimeoutError::Timeout) => {
                            flush(&client, &endpoint, &mut buffer);
                            last_flushed = Instant::now();
                        }
                        Err(flume::RecvTimeoutError::Disconnected) => {
                            debug!("Shutting down event processor");
                            flush(&client, &endpoint, &mut buffer);
                            break;
                        }
                    }
                }
            })
            .expect("Failed to start event processor thread");

        EventProcessor { tx }
    }

    pub fn track_event(&self, event: &str, options: EventOptions) {
        self.send(Event::new(event, None, options));
    }

    pub fn track_exposure_event(&self, feature_name: &str, options: EventOptions) {
        self.send(Event::new(FLAG_EXPOSURE_EVENT, Some(feature_name), options));
    }

    /// Sends the buffered events, and returns once they have been sent, kept for a retry
    /// or dropped.
    pub fn flush(&self) {
        let (done_tx, done_rx) = flume::bounded(1);
        if self.tx.send(Message::Flush(done_tx)).is_ok() {
            let _ = done_rx.recv();
        }
    }

    fn send(&self, event: Event) {
        if self.tx.send(Message::Track(event)).is_err() {
            warn!("Event processor has stopped; dropped an event");
        }
    }
}

fn flush(client: &reqwest::blocking::Client, endpoint: &str, buffer: &mut EventBuffer) {
    buffer.held = false;
    if buffer.events.is_empty() {
        return;
    }
    let count = buffer.events.len();
    match post_events(client, endpoint, &buffer.events) {
        SendOutcome::Delivered => buffer.clear(),
        SendOutcome::Retryable => {
            warn!("Failed to send {count} events; keeping them for the next flush");
            buffer.held = true;
        }
        SendOutcome::Rejected => {
            warn!("Events API rejected {count} events; dropping them");
            buffer.clear();
        }
    }
}

fn post_events(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    events: &[Event],
) -> SendOutcome {
    let body = serde_json::json!({ "events": events });
    let response = match client.post(endpoint).body(body.to_string()).send() {
        Ok(response) => response,
        Err(e) => {
            debug!("Error sending events: {e}");
            return SendOutcome::Retryable;
        }
    };
    let status = response.status();
    if status.is_success() {
        let rejected = response
            .json::<serde_json::Value>()
            .ok()
            .and_then(|body| body["rejected"].as_array().map(Vec::len))
            .unwrap_or(0);
        if rejected > 0 {
            warn!("Events API rejected {rejected} of {} events", events.len());
        }
        return SendOutcome::Delivered;
    }
    if status.is_server_error()
        || status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        debug!("Events API responded {status}");
        return SendOutcome::Retryable;
    }
    SendOutcome::Rejected
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    const ENVIRONMENT_KEY: &str = "ser.test_environment_key";

    fn string_value(value: &str) -> FlagsmithValue {
        FlagsmithValue {
            value: value.to_string(),
            value_type: FlagsmithValueType::String,
        }
    }

    fn exposure(feature_name: &str, identifier: &str, value: &str) -> Event {
        Event::new(
            FLAG_EXPOSURE_EVENT,
            Some(feature_name),
            EventOptions {
                identifier: Some(identifier.to_string()),
                value: Some(string_value(value)),
                ..Default::default()
            },
        )
    }

    fn processor(
        server: &MockServer,
        flush_interval_mills: u64,
        max_buffer_size: usize,
    ) -> EventProcessor {
        EventProcessor::new(
            ENVIRONMENT_KEY,
            &server.url("/"),
            Duration::from_secs(10),
            Duration::from_millis(flush_interval_mills),
            max_buffer_size,
        )
    }

    #[test]
    fn event_carries_its_options_and_the_sdk_version() {
        // Given
        let options = EventOptions {
            identifier: Some("user_123".to_string()),
            value: Some(FlagsmithValue {
                value: "42".to_string(),
                value_type: FlagsmithValueType::Integer,
            }),
            traits: Some(vec![SDKTrait::new(
                "plan".to_string(),
                string_value("premium"),
            )]),
            metadata: Some(HashMap::from([(
                "currency".to_string(),
                serde_json::json!("GBP"),
            )])),
        };

        // When
        let event = Event::new("checkout_completed", None, options);

        // Then
        let mut json = serde_json::to_value(&event).unwrap();
        assert!(json["timestamp"].as_i64().unwrap() > 1_577_836_800_000);
        json.as_object_mut().unwrap().remove("timestamp");
        assert_eq!(
            json,
            serde_json::json!({
                "event": "checkout_completed",
                "feature_name": null,
                "identifier": "user_123",
                "value": "42",
                "traits": {"plan": "premium"},
                "metadata": {"currency": "GBP", "sdk_version": env!("CARGO_PKG_VERSION")},
            })
        );
    }

    #[test]
    fn event_without_value_or_traits_sends_them_as_null() {
        // When
        let event = Event::new(
            "signed_up",
            None,
            EventOptions {
                value: Some(FlagsmithValue::default()),
                traits: Some(vec![]),
                ..Default::default()
            },
        );

        // Then
        assert_eq!(event.value, None);
        assert_eq!(event.traits, None);
        assert_eq!(event.identifier, None);
    }

    #[test]
    fn buffer_keeps_one_of_equal_exposures() {
        // Given
        let mut buffer = EventBuffer::new(10);

        // When
        buffer.push(exposure("banner", "user_123", "treatment"));
        buffer.push(exposure("banner", "user_123", "treatment"));
        buffer.push(exposure("banner", "user_123", "control"));
        buffer.push(exposure("banner", "user_456", "treatment"));

        // Then
        let values: Vec<_> = buffer
            .events
            .iter()
            .map(|e| (e.identifier.clone().unwrap(), e.value.clone().unwrap()))
            .collect();
        assert_eq!(
            values,
            vec![
                ("user_123".to_string(), "treatment".to_string()),
                ("user_123".to_string(), "control".to_string()),
                ("user_456".to_string(), "treatment".to_string()),
            ]
        );
    }

    #[test]
    fn buffer_keeps_every_custom_event() {
        // Given
        let mut buffer = EventBuffer::new(10);

        // When
        buffer.push(Event::new("clicked", None, EventOptions::default()));
        buffer.push(Event::new("clicked", None, EventOptions::default()));

        // Then
        assert_eq!(buffer.events.len(), 2);
    }

    #[test]
    fn full_buffer_drops_its_oldest_event() {
        // Given
        let mut buffer = EventBuffer::new(2);
        buffer.push(exposure("banner", "user_1", "treatment"));
        buffer.push(exposure("banner", "user_2", "treatment"));

        // When
        buffer.push(exposure("banner", "user_3", "treatment"));
        buffer.push(exposure("banner", "user_1", "treatment"));

        // Then
        let identifiers: Vec<_> = buffer
            .events
            .iter()
            .map(|e| e.identifier.clone().unwrap())
            .collect();
        assert_eq!(identifiers, vec!["user_3", "user_1"]);
    }

    #[test]
    fn flush_sends_tracked_events_to_the_events_api() {
        // Given
        let server = MockServer::start();
        let events_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/events")
                .header("X-Environment-Key", ENVIRONMENT_KEY)
                .header("Content-Type", "application/json")
                .header("Flagsmith-SDK-User-Agent", super::super::get_user_agent())
                .json_body_partial(
                    serde_json::json!({"events": [
                        {"event": "checkout_completed", "identifier": "user_123"},
                        {"event": FLAG_EXPOSURE_EVENT, "feature_name": "banner", "value": "treatment"},
                    ]})
                    .to_string(),
                );
            then.status(202)
                .json_body(serde_json::json!({"accepted": 2, "rejected": []}));
        });
        let processor = processor(&server, 60_000, 100);
        processor.track_event(
            "checkout_completed",
            EventOptions {
                identifier: Some("user_123".to_string()),
                ..Default::default()
            },
        );
        processor.track_exposure_event(
            "banner",
            EventOptions {
                identifier: Some("user_123".to_string()),
                value: Some(string_value("treatment")),
                ..Default::default()
            },
        );

        // When
        processor.flush();

        // Then
        events_mock.assert_hits(1);
    }

    #[test]
    fn flush_without_events_sends_nothing() {
        // Given
        let server = MockServer::start();
        let events_mock = server.mock(|when, then| {
            when.method(POST).path("/v1/events");
            then.status(202);
        });
        let processor = processor(&server, 60_000, 100);

        // When
        processor.flush();

        // Then
        events_mock.assert_hits(0);
    }

    #[test]
    fn events_are_sent_on_the_flush_interval() {
        // Given
        let server = MockServer::start();
        let events_mock = server.mock(|when, then| {
            when.method(POST).path("/v1/events");
            then.status(202);
        });
        let processor = processor(&server, 50, 100);

        // When
        processor.track_event("clicked", EventOptions::default());
        thread::sleep(Duration::from_millis(300));

        // Then
        events_mock.assert_hits(1);
    }

    #[test]
    fn events_are_sent_when_the_buffer_fills() {
        // Given
        let server = MockServer::start();
        let events_mock = server.mock(|when, then| {
            when.method(POST).path("/v1/events");
            then.status(202);
        });
        let processor = processor(&server, 60_000, 2);

        // When
        processor.track_event("clicked", EventOptions::default());
        processor.track_event("clicked", EventOptions::default());
        thread::sleep(Duration::from_millis(100));

        // Then
        events_mock.assert_hits(1);
    }

    #[test]
    fn dropping_the_processor_sends_buffered_events() {
        // Given
        let server = MockServer::start();
        let events_mock = server.mock(|when, then| {
            when.method(POST).path("/v1/events");
            then.status(202);
        });
        let processor = processor(&server, 60_000, 100);
        processor.track_event("clicked", EventOptions::default());

        // When
        drop(processor);
        thread::sleep(Duration::from_millis(100));

        // Then
        events_mock.assert_hits(1);
    }

    #[test]
    fn events_are_kept_for_the_next_flush_when_the_events_api_is_unavailable() {
        // Given
        let server = MockServer::start();
        let mut unavailable_mock = server.mock(|when, then| {
            when.method(POST).path("/v1/events");
            then.status(503);
        });
        let processor = processor(&server, 60_000, 100);
        processor.track_event("clicked", EventOptions::default());
        processor.flush();
        unavailable_mock.assert_hits(1);
        unavailable_mock.delete();
        let events_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/events")
                .json_body_partial(r#"{"events": [{"event": "clicked"}]}"#);
            then.status(202);
        });

        // When
        processor.flush();

        // Then
        events_mock.assert_hits(1);
    }

    #[test]
    fn events_rejected_by_the_events_api_are_dropped() {
        // Given
        let server = MockServer::start();
        let events_mock = server.mock(|when, then| {
            when.method(POST).path("/v1/events");
            then.status(400);
        });
        let processor = processor(&server, 60_000, 100);
        processor.track_event("clicked", EventOptions::default());
        processor.flush();

        // When
        processor.flush();

        // Then
        events_mock.assert_hits(1);
    }

    #[test]
    fn flush_survives_an_unreachable_events_api() {
        // Given
        let processor = EventProcessor::new(
            ENVIRONMENT_KEY,
            "http://127.0.0.1:1/",
            Duration::from_millis(100),
            Duration::from_secs(60),
            100,
        );
        processor.track_event("clicked", EventOptions::default());

        // When / Then
        processor.flush();
    }
}
