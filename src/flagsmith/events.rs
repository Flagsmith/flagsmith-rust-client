use super::models::SDKTrait;
use flagsmith_flag_engine::types::{FlagsmithValue, FlagsmithValueType};
use log::{debug, warn};
use reqwest::header::{self, HeaderMap};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub const DEFAULT_EVENTS_API_URL: &str = "https://events.api.flagsmith.com/";
pub const DEFAULT_EVENTS_FLUSH_INTERVAL_MILLS: u64 = 10 * 1000;
pub const DEFAULT_EVENTS_MAX_BUFFER_SIZE: usize = 1000;
/// How long a request to the events API may go unanswered before it is abandoned and retried.
pub const DEFAULT_EVENTS_REQUEST_TIMEOUT_MILLS: u64 = 3 * 1000;
/// The longest wait before the first retry of a batch, doubling for each later retry. The
/// actual wait is a random one up to it (full jitter).
const RETRY_BACKOFF: Duration = Duration::from_secs(1);
/// Attempts to send a batch on each flush, before it is kept for the next one.
const MAX_SEND_ATTEMPTS: u32 = 3;
/// The event recorded when an identity is exposed to a variant of a feature.
pub const FLAG_EXPOSURE_EVENT: &str = "$flag_exposure";
const EVENTS_ENDPOINT: &str = "v1/events";

/// The optional fields of a tracked event.
#[derive(Clone, Debug, Default)]
pub struct EventOptions {
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
    fn new(
        event: &str,
        feature_name: Option<&str>,
        identifier: Option<&str>,
        options: EventOptions,
    ) -> Event {
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
            identifier: identifier.map(str::to_string),
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

/// Asks the worker to send its events until the deadline, then stop sending; it answers on
/// the sender once it has.
type CloseRequest = (Instant, flume::Sender<()>);

/// What woke the worker.
enum Wake {
    Message(Result<Message, flume::RecvError>),
    Close(Result<CloseRequest, flume::RecvError>),
}

#[derive(Debug, PartialEq)]
enum SendOutcome {
    Delivered,
    /// Worth another attempt: the events API was unreachable, did not answer in time, or
    /// answered with a transient error. `server_error` is a 500, which drops the batch when
    /// its last attempt gets one.
    Failed {
        server_error: bool,
    },
    /// The events API will not accept this batch.
    Rejected,
    /// The events API refuses the environment key, so it will refuse every batch.
    Unauthorised,
}

/// Whether the processor sends events.
#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Sending,
    /// The client was closed.
    Closed,
    /// The events API refused the environment key.
    Refused,
}

/// Events waiting to be sent. Equal exposures add no information, so only the first one
/// waiting is kept.
struct EventBuffer {
    events: VecDeque<Event>,
    exposures: HashSet<ExposureKey>,
    max_size: usize,
    /// The events are a batch that failed, kept for the next flush: a full buffer drops its
    /// oldest event rather than send it again.
    held: bool,
    /// Events dropped since last reported.
    dropped: usize,
}

impl EventBuffer {
    fn new(max_size: usize) -> EventBuffer {
        EventBuffer {
            events: VecDeque::new(),
            exposures: HashSet::new(),
            max_size: max_size.max(1),
            held: false,
            dropped: 0,
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
        if self.is_full() {
            if let Some(key) = self.events.pop_front().and_then(|e| e.exposure_key()) {
                self.exposures.remove(&key);
            }
            self.dropped += 1;
        }
        self.events.push_back(event);
    }

    fn is_full(&self) -> bool {
        self.events.len() >= self.max_size
    }

    fn clear(&mut self) {
        self.events.clear();
        self.exposures.clear();
        self.held = false;
    }
}

/// Buffers tracking and exposure events, and sends them to Flagsmith's events API in
/// batches: on a timer, when an event arrives at a full buffer, on `flush`, on `close`, and
/// when dropped. One batch is in flight at a time. A batch that fails is retried with backoff,
/// then kept for the next flush, ahead of events tracked since; while it is kept, a full
/// buffer drops its oldest event. Tracking never blocks: events arriving faster than they are
/// buffered are dropped.
#[derive(Clone, Debug)]
pub struct EventProcessor {
    tx: flume::Sender<Message>,
    /// Apart from `tx`, so that a full queue of events cannot hold up closing.
    close_tx: flume::Sender<CloseRequest>,
    /// Events dropped because they arrived faster than they could be buffered.
    dropped: Arc<AtomicUsize>,
}

impl EventProcessor {
    /// `request_timeout` is how long a request to the events API may go unanswered before it
    /// is retried; zero uses `DEFAULT_EVENTS_REQUEST_TIMEOUT_MILLS`. A `flush_interval` of zero
    /// uses `DEFAULT_EVENTS_FLUSH_INTERVAL_MILLS`.
    pub fn new(
        environment_key: &str,
        events_api_url: &str,
        request_timeout: Duration,
        flush_interval: Duration,
        max_buffer_size: usize,
    ) -> Self {
        Self::with_retry_backoff(
            environment_key,
            events_api_url,
            request_timeout,
            flush_interval,
            max_buffer_size,
            RETRY_BACKOFF,
        )
    }

    pub(crate) fn with_retry_backoff(
        environment_key: &str,
        events_api_url: &str,
        request_timeout: Duration,
        flush_interval: Duration,
        max_buffer_size: usize,
        retry_backoff: Duration,
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
            .build()
            .unwrap();
        let separator = if events_api_url.ends_with('/') {
            ""
        } else {
            "/"
        };
        let endpoint = format!("{events_api_url}{separator}{EVENTS_ENDPOINT}");
        let request_timeout = if request_timeout.is_zero() {
            warn!("An events request timeout of zero would send no events; using the default");
            Duration::from_millis(DEFAULT_EVENTS_REQUEST_TIMEOUT_MILLS)
        } else {
            request_timeout
        };
        let flush_interval = if flush_interval.is_zero() {
            warn!("An events flush interval of zero would flush without end; using the default");
            Duration::from_millis(DEFAULT_EVENTS_FLUSH_INTERVAL_MILLS)
        } else {
            flush_interval
        };

        let sender = Sender {
            client,
            endpoint,
            request_timeout,
            retry_backoff,
            state: State::Sending,
        };
        let buffer = EventBuffer::new(max_buffer_size);
        let dropped = Arc::new(AtomicUsize::new(0));

        // Room for bursts of events, and for a full buffer of them while a batch is in flight.
        let (tx, rx) = flume::bounded(buffer.max_size.max(DEFAULT_EVENTS_MAX_BUFFER_SIZE));
        let (close_tx, close_rx) = flume::bounded(1);
        let worker = Worker {
            sender,
            buffer,
            rx,
            close_rx,
            closing: None,
            flush_interval,
            last_flushed: Instant::now(),
            dropped: Arc::clone(&dropped),
        };
        thread::Builder::new()
            .name("Event Processor".to_string())
            .spawn(move || worker.run())
            .expect("Failed to start event processor thread");

        EventProcessor {
            tx,
            close_tx,
            dropped,
        }
    }

    pub fn track_event(&self, event: &str, identifier: Option<&str>, options: EventOptions) {
        self.send(Event::new(event, None, identifier, options));
    }

    pub fn track_exposure_event(
        &self,
        feature_name: &str,
        identifier: &str,
        options: EventOptions,
    ) {
        self.send(Event::new(
            FLAG_EXPOSURE_EVENT,
            Some(feature_name),
            Some(identifier),
            options,
        ));
    }

    /// Sends the buffered events, and returns once they have been sent, kept for a retry
    /// or dropped.
    pub fn flush(&self) {
        let (done_tx, done_rx) = flume::bounded(1);
        if self.tx.send(Message::Flush(done_tx)).is_ok() {
            let _ = done_rx.recv();
        }
    }

    /// Sends the buffered events, retries included, and stops sending. Returns by the
    /// timeout; events still unsent then are dropped, as are events tracked afterwards. A
    /// retry waiting out its backoff is made at once; a request already in flight is not cut
    /// short, but is the last one sent.
    pub fn close(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let (done_tx, done_rx) = flume::bounded(1);
        if self
            .close_tx
            .send_deadline((deadline, done_tx), deadline)
            .is_ok()
        {
            let _ = done_rx.recv_deadline(deadline);
        }
    }

    fn send(&self, event: Event) {
        match self.tx.try_send(Message::Track(event)) {
            Ok(()) => {}
            Err(flume::TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(flume::TrySendError::Disconnected(_)) => {
                warn!("Event processor has stopped; dropped an event");
            }
        }
    }
}

/// The event processor's thread: buffers the events it is sent, and sends them on.
struct Worker {
    sender: Sender,
    buffer: EventBuffer,
    rx: flume::Receiver<Message>,
    close_rx: flume::Receiver<CloseRequest>,
    /// A close that arrived while a batch was being retried, to finish once it is done.
    closing: Option<CloseRequest>,
    flush_interval: Duration,
    last_flushed: Instant,
    /// Shared with `EventProcessor`: events it could not hand over.
    dropped: Arc<AtomicUsize>,
}

impl Worker {
    fn run(mut self) {
        loop {
            if let Some(close) = self.closing.take() {
                self.close(close);
                continue;
            }
            // Checked on every message, so steady tracking cannot hold the timer off.
            let until_flush = self
                .flush_interval
                .saturating_sub(self.last_flushed.elapsed());
            if until_flush.is_zero() {
                self.flush(None);
                continue;
            }
            let wake = flume::Selector::new()
                .recv(&self.close_rx, Wake::Close)
                .recv(&self.rx, Wake::Message)
                .wait_timeout(until_flush);
            match wake {
                Err(flume::select::SelectError::Timeout) => {}
                Ok(Wake::Close(Ok(close))) => self.close(close),
                Ok(Wake::Message(Ok(Message::Track(event)))) => self.track(event),
                Ok(Wake::Message(Ok(Message::Flush(done)))) => {
                    self.flush(None);
                    let _ = done.send(());
                }
                // Every `EventProcessor` was dropped.
                Ok(Wake::Message(Err(_)) | Wake::Close(Err(_))) => {
                    debug!("Shutting down event processor");
                    self.take_queued_events();
                    self.flush(None);
                    break;
                }
            }
        }
    }

    fn track(&mut self, event: Event) {
        if self.sender.state != State::Sending {
            self.buffer.dropped += 1;
            return;
        }
        // Send a full buffer rather than drop an event from it, unless it is a batch that
        // failed: that waits for the next flush.
        if self.buffer.is_full() && !self.buffer.held {
            self.flush(None);
        }
        self.buffer.push(event);
    }

    /// Sends the buffered events, and those already queued, until the deadline, then stops
    /// sending. The worker keeps running: the client may still flush, or track events to drop.
    fn close(&mut self, (deadline, done): CloseRequest) {
        self.take_queued_events();
        self.flush(Some(deadline));
        self.buffer.dropped += self.buffer.events.len();
        self.buffer.clear();
        self.report_dropped();
        self.sender.state = State::Closed;
        let _ = done.send(());
    }

    /// Buffers the events queued so far, without sending: a full buffer drops its oldest.
    fn take_queued_events(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            match message {
                Message::Track(event) if self.sender.state == State::Sending => {
                    self.buffer.push(event)
                }
                Message::Track(_) => self.buffer.dropped += 1,
                // Whoever flushes is answered by the send that follows.
                Message::Flush(done) => {
                    let _ = done.send(());
                }
            }
        }
    }

    fn flush(&mut self, deadline: Option<Instant>) {
        self.report_dropped();
        let wait = RetryWait {
            close_rx: &self.close_rx,
            closing: &mut self.closing,
        };
        self.sender.flush(&mut self.buffer, deadline, wait);
        self.last_flushed = Instant::now();
    }

    /// Logs the events dropped since last reported, in one warning.
    fn report_dropped(&mut self) {
        let dropped =
            std::mem::take(&mut self.buffer.dropped) + self.dropped.swap(0, Ordering::Relaxed);
        if dropped > 0 {
            warn!("Dropped {dropped} events: the events buffer was full, or tracking had stopped");
        }
    }
}

/// Waits out a retry's backoff, unless the client is closed meanwhile.
struct RetryWait<'a> {
    close_rx: &'a flume::Receiver<CloseRequest>,
    closing: &'a mut Option<CloseRequest>,
}

impl RetryWait<'_> {
    /// Waits for `wait`, and returns the deadline of a close that cut it short.
    fn wait(&mut self, wait: Duration) -> Option<Instant> {
        match self.close_rx.recv_timeout(wait) {
            Ok(close) => {
                let deadline = close.0;
                *self.closing = Some(close);
                Some(deadline)
            }
            Err(flume::RecvTimeoutError::Timeout) => None,
            Err(flume::RecvTimeoutError::Disconnected) => {
                thread::sleep(wait);
                None
            }
        }
    }
}

/// Sends batches of events to the events API.
struct Sender {
    client: reqwest::blocking::Client,
    endpoint: String,
    request_timeout: Duration,
    retry_backoff: Duration,
    state: State,
}

impl Sender {
    /// Sends the buffered events as one batch, retrying it with backoff until it is delivered,
    /// rejected, or out of attempts, or until the deadline. Then the batch is either cleared
    /// from the buffer or kept in it for the next flush. A close during a backoff retries at
    /// once, within its deadline.
    fn flush(
        &mut self,
        buffer: &mut EventBuffer,
        mut deadline: Option<Instant>,
        mut retry_wait_for: RetryWait,
    ) {
        if self.state != State::Sending || buffer.events.is_empty() {
            return;
        }
        let count = buffer.events.len();
        let mut attempt = 1;
        loop {
            let timeout = match deadline {
                Some(deadline) => self
                    .request_timeout
                    .min(deadline.saturating_duration_since(Instant::now())),
                None => self.request_timeout,
            };
            if timeout.is_zero() {
                buffer.held = true;
                return;
            }
            match self.post(&buffer.events, timeout) {
                SendOutcome::Delivered => return buffer.clear(),
                SendOutcome::Rejected => {
                    warn!("Events API rejected {count} events; dropping them");
                    return buffer.clear();
                }
                SendOutcome::Unauthorised => {
                    warn!("Events API refused the environment key; stopped tracking events");
                    self.state = State::Refused;
                    return buffer.clear();
                }
                SendOutcome::Failed { server_error } if attempt >= MAX_SEND_ATTEMPTS => {
                    if server_error {
                        warn!("Failed to send {count} events; dropping them");
                        buffer.clear();
                    } else {
                        warn!("Failed to send {count} events; keeping them for the next flush");
                        buffer.held = true;
                    }
                    return;
                }
                SendOutcome::Failed { .. } => {}
            }
            let wait = retry_wait(self.retry_backoff, attempt);
            if deadline.is_some_and(|deadline| Instant::now() + wait >= deadline) {
                buffer.held = true;
                return;
            }
            if let Some(close_deadline) = retry_wait_for.wait(wait) {
                deadline = Some(deadline.map_or(close_deadline, |d| d.min(close_deadline)));
            }
            attempt += 1;
        }
    }

    fn post(&self, events: &VecDeque<Event>, timeout: Duration) -> SendOutcome {
        let body = serde_json::json!({ "events": events });
        let response = match self
            .client
            .post(&self.endpoint)
            .timeout(timeout)
            .body(body.to_string())
            .send()
        {
            Ok(response) => response,
            Err(e) => {
                debug!("Error sending events: {e}");
                return SendOutcome::Failed {
                    server_error: false,
                };
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
        debug!("Events API responded {status}");
        outcome_of_status(status)
    }
}

/// What an error status from the events API means for the batch that got it.
fn outcome_of_status(status: reqwest::StatusCode) -> SendOutcome {
    use reqwest::StatusCode;
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => SendOutcome::Unauthorised,
        StatusCode::INTERNAL_SERVER_ERROR => SendOutcome::Failed { server_error: true },
        StatusCode::REQUEST_TIMEOUT
        | StatusCode::TOO_MANY_REQUESTS
        | StatusCode::BAD_GATEWAY
        | StatusCode::SERVICE_UNAVAILABLE
        | StatusCode::GATEWAY_TIMEOUT => SendOutcome::Failed {
            server_error: false,
        },
        _ => SendOutcome::Rejected,
    }
}

/// The longest wait before retry `retry` (from 1): `backoff`, doubling for each later retry.
fn retry_backoff(backoff: Duration, retry: u32) -> Duration {
    backoff.saturating_mul(2u32.saturating_pow(retry.saturating_sub(1)))
}

/// A random wait up to the backoff (full jitter), so that clients failing together do not
/// retry together.
fn retry_wait(backoff: Duration, retry: u32) -> Duration {
    retry_backoff(backoff, retry).mul_f64(fastrand::f64())
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};

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
            Some(identifier),
            EventOptions {
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
        EventProcessor::with_retry_backoff(
            ENVIRONMENT_KEY,
            &server.url("/"),
            Duration::from_secs(10),
            Duration::from_millis(flush_interval_mills),
            max_buffer_size,
            Duration::from_millis(1),
        )
    }

    /// Answers the events API's requests with `statuses` in order, then with 202. A status of
    /// 0 closes the connection without answering.
    fn events_api(statuses: &'static [u16]) -> (std::net::SocketAddr, Arc<Mutex<Vec<String>>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let bodies = Arc::new(Mutex::new(vec![]));
        let received = Arc::clone(&bodies);
        let mut statuses = statuses.iter().copied();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let body = read_request_body(&mut stream);
                received.lock().unwrap().push(body);
                let status = statuses.next().unwrap_or(202);
                if status == 0 {
                    continue;
                }
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (address, bodies)
    }

    fn read_request_body(stream: &mut std::net::TcpStream) -> String {
        let mut reader = BufReader::new(stream);
        let mut content_length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line.trim().is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.eq_ignore_ascii_case("content-length") {
                    content_length = value.trim().parse().unwrap();
                }
            }
        }
        let mut body = vec![0; content_length];
        reader.read_exact(&mut body).unwrap();
        String::from_utf8(body).unwrap()
    }

    fn sent_event_names(bodies: &Arc<Mutex<Vec<String>>>) -> Vec<Vec<String>> {
        bodies
            .lock()
            .unwrap()
            .iter()
            .map(|body| {
                let body: serde_json::Value = serde_json::from_str(body).unwrap();
                body["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|event| event["event"].as_str().unwrap().to_string())
                    .collect()
            })
            .collect()
    }

    fn processor_for(address: std::net::SocketAddr) -> EventProcessor {
        EventProcessor::with_retry_backoff(
            ENVIRONMENT_KEY,
            &format!("http://{address}/"),
            Duration::from_secs(10),
            Duration::from_secs(60),
            100,
            Duration::from_millis(1),
        )
    }

    #[test]
    fn event_carries_its_options_and_the_sdk_version() {
        // Given
        let options = EventOptions {
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
        let event = Event::new("checkout_completed", None, Some("user_123"), options);

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
        buffer.push(Event::new("clicked", None, None, EventOptions::default()));
        buffer.push(Event::new("clicked", None, None, EventOptions::default()));

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
        assert_eq!(buffer.dropped, 2);
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
            Some("user_123"),
            EventOptions::default(),
        );
        processor.track_exposure_event(
            "banner",
            "user_123",
            EventOptions {
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
        processor.track_event("clicked", None, EventOptions::default());
        thread::sleep(Duration::from_millis(300));

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
        processor.track_event("clicked", None, EventOptions::default());

        // When
        drop(processor);
        thread::sleep(Duration::from_millis(100));

        // Then
        events_mock.assert_hits(1);
    }

    #[test]
    fn a_batch_is_retried_until_the_events_api_accepts_it() {
        // Given
        let (address, bodies) = events_api(&[503, 429]);
        let processor = processor_for(address);
        processor.track_event("clicked", None, EventOptions::default());

        // When
        processor.flush();

        // Then
        assert_eq!(sent_event_names(&bodies), vec![vec!["clicked"]; 3]);
    }

    #[test]
    fn a_batch_that_keeps_failing_is_sent_with_the_next_flush_ahead_of_later_events() {
        // Given
        let (address, bodies) = events_api(&[503, 503, 503]);
        let processor = processor_for(address);
        processor.track_event("add_to_cart", None, EventOptions::default());
        processor.flush();
        processor.track_event("checkout", None, EventOptions::default());

        // When
        processor.flush();

        // Then
        let mut expected = vec![vec!["add_to_cart"]; 3];
        expected.push(vec!["add_to_cart", "checkout"]);
        assert_eq!(sent_event_names(&bodies), expected);
    }

    #[test]
    fn a_batch_whose_last_attempt_gets_a_500_is_dropped() {
        // Given
        let (address, bodies) = events_api(&[503, 503, 500]);
        let processor = processor_for(address);
        processor.track_event("add_to_cart", None, EventOptions::default());
        processor.flush();
        processor.track_event("checkout", None, EventOptions::default());

        // When
        processor.flush();

        // Then
        let mut expected = vec![vec!["add_to_cart"]; 3];
        expected.push(vec!["checkout"]);
        assert_eq!(sent_event_names(&bodies), expected);
    }

    #[test]
    fn a_batch_whose_last_attempt_fails_otherwise_is_kept_after_500s() {
        // Given
        let (address, bodies) = events_api(&[500, 500, 503]);
        let processor = processor_for(address);
        processor.track_event("add_to_cart", None, EventOptions::default());
        processor.flush();

        // When
        processor.flush();

        // Then
        let mut expected = vec![vec!["add_to_cart"]; 3];
        expected.push(vec!["add_to_cart"]);
        assert_eq!(sent_event_names(&bodies), expected);
    }

    #[rstest::rstest]
    #[case(&[401])]
    #[case(&[403])]
    fn an_events_api_refusing_the_environment_key_stops_event_tracking(
        #[case] statuses: &'static [u16],
    ) {
        // Given
        let (address, bodies) = events_api(statuses);
        let processor = processor_for(address);
        processor.track_event("add_to_cart", None, EventOptions::default());
        processor.flush();
        processor.track_event("checkout", None, EventOptions::default());

        // When
        processor.flush();

        // Then
        assert_eq!(sent_event_names(&bodies), vec![vec!["add_to_cart"]]);
    }

    #[test]
    fn a_full_buffer_waiting_for_a_retry_drops_its_oldest_event() {
        // Given
        let (address, bodies) = events_api(&[503, 503, 503]);
        let processor = EventProcessor::with_retry_backoff(
            ENVIRONMENT_KEY,
            &format!("http://{address}/"),
            Duration::from_secs(10),
            Duration::from_secs(60),
            2,
            Duration::from_millis(1),
        );
        processor.track_event("event_1", None, EventOptions::default());
        processor.track_event("event_2", None, EventOptions::default());
        processor.flush();
        processor.track_event("event_3", None, EventOptions::default());

        // When
        processor.flush();

        // Then: event_3 dropped event_1 rather than send the failed batch again
        let mut expected = vec![vec!["event_1", "event_2"]; 3];
        expected.push(vec!["event_2", "event_3"]);
        assert_eq!(sent_event_names(&bodies), expected);
    }

    #[test]
    fn a_full_buffer_is_sent_rather_than_drop_an_event() {
        // Given
        let (address, bodies) = events_api(&[]);
        let processor = EventProcessor::with_retry_backoff(
            ENVIRONMENT_KEY,
            &format!("http://{address}/"),
            Duration::from_secs(10),
            Duration::from_secs(60),
            2,
            Duration::from_millis(1),
        );
        processor.track_event("event_1", None, EventOptions::default());
        processor.track_event("event_2", None, EventOptions::default());

        // When
        processor.track_event("event_3", None, EventOptions::default());
        processor.flush();

        // Then
        assert_eq!(
            sent_event_names(&bodies),
            vec![vec!["event_1", "event_2"], vec!["event_3"]]
        );
    }

    #[test]
    fn steady_tracking_does_not_hold_off_the_flush_interval() {
        // Given
        let (address, bodies) = events_api(&[]);
        let processor = EventProcessor::with_retry_backoff(
            ENVIRONMENT_KEY,
            &format!("http://{address}/"),
            Duration::from_secs(10),
            Duration::from_millis(50),
            1000,
            Duration::from_millis(1),
        );

        // When: an event every millisecond, for several flush intervals
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(400) {
            processor.track_event("clicked", None, EventOptions::default());
            thread::sleep(Duration::from_millis(1));
        }

        // Then
        assert!(sent_event_names(&bodies).len() >= 2);
    }

    #[test]
    fn tracking_does_not_block_while_a_batch_is_in_flight() {
        // Given: a batch in flight to an events API that never answers
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let _connections: Vec<_> = listener.incoming().collect();
        });
        let processor = EventProcessor::with_retry_backoff(
            ENVIRONMENT_KEY,
            &format!("http://{address}/"),
            Duration::from_secs(10),
            Duration::from_millis(10),
            10,
            Duration::from_millis(1),
        );
        processor.track_event("clicked", None, EventOptions::default());
        thread::sleep(Duration::from_millis(100));

        // When
        let started = Instant::now();
        for _ in 0..5000 {
            processor.track_event("clicked", None, EventOptions::default());
        }

        // Then: events beyond what the channel holds are dropped, and counted
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(processor.dropped.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn a_closed_connection_is_retried() {
        // Given
        let (address, bodies) = events_api(&[0]);
        let processor = processor_for(address);
        processor.track_event("clicked", None, EventOptions::default());

        // When
        processor.flush();

        // Then
        assert_eq!(sent_event_names(&bodies), vec![vec!["clicked"]; 2]);
    }

    #[test]
    fn a_request_timeout_of_zero_uses_the_default() {
        // Given
        let (address, bodies) = events_api(&[]);
        let processor = EventProcessor::new(
            ENVIRONMENT_KEY,
            &format!("http://{address}/"),
            Duration::ZERO,
            Duration::from_secs(60),
            100,
        );
        processor.track_event("clicked", None, EventOptions::default());

        // When
        processor.flush();

        // Then
        assert_eq!(sent_event_names(&bodies), vec![vec!["clicked"]]);
    }

    #[test]
    fn an_unanswered_request_times_out_and_is_retried() {
        // Given: an events API that never answers
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let connections = Arc::new(Mutex::new(vec![]));
        let accepted = Arc::clone(&connections);
        thread::spawn(move || {
            for stream in listener.incoming() {
                accepted.lock().unwrap().push(stream.unwrap());
            }
        });
        let processor = EventProcessor::with_retry_backoff(
            ENVIRONMENT_KEY,
            &format!("http://{address}/"),
            Duration::from_millis(50),
            Duration::from_secs(60),
            100,
            Duration::from_millis(1),
        );
        processor.track_event("clicked", None, EventOptions::default());

        // When
        let started = Instant::now();
        processor.flush();

        // Then
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(connections.lock().unwrap().len(), 3);
    }

    #[test]
    fn close_sends_the_buffered_events_retries_included() {
        // Given
        let (address, bodies) = events_api(&[503]);
        let processor = processor_for(address);
        processor.track_event("checkout", None, EventOptions::default());

        // When
        processor.close(Duration::from_secs(5));

        // Then
        assert_eq!(sent_event_names(&bodies), vec![vec!["checkout"]; 2]);
    }

    #[test]
    fn close_returns_within_its_timeout_when_the_events_api_does_not_answer() {
        // Given: an events API that never answers
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let _connections: Vec<_> = listener.incoming().collect();
        });
        let processor = processor_for(address);
        processor.track_event("checkout", None, EventOptions::default());

        // When
        let started = Instant::now();
        processor.close(Duration::from_millis(200));

        // Then
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn close_returns_within_its_timeout_while_a_batch_waits_to_be_retried() {
        // Given: a flush backing off before its retry
        let (address, _bodies) = events_api(&[503, 503, 503]);
        let processor = EventProcessor::with_retry_backoff(
            ENVIRONMENT_KEY,
            &format!("http://{address}/"),
            Duration::from_secs(10),
            Duration::from_secs(60),
            100,
            Duration::from_secs(30),
        );
        processor.track_event("checkout", None, EventOptions::default());
        let flushing = processor.clone();
        thread::spawn(move || flushing.flush());
        thread::sleep(Duration::from_millis(100));

        // When
        let started = Instant::now();
        processor.close(Duration::from_millis(200));

        // Then
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn close_returns_within_its_timeout_during_a_timed_flush() {
        // Given: a timed flush waiting on an events API that never answers
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let _connections: Vec<_> = listener.incoming().collect();
        });
        let processor = EventProcessor::with_retry_backoff(
            ENVIRONMENT_KEY,
            &format!("http://{address}/"),
            Duration::from_secs(10),
            Duration::from_millis(50),
            100,
            Duration::from_millis(1),
        );
        processor.track_event("checkout", None, EventOptions::default());
        thread::sleep(Duration::from_millis(150));

        // When
        let started = Instant::now();
        processor.close(Duration::from_millis(200));

        // Then
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn close_stops_sending_even_when_the_queue_is_full() {
        // Given: a timed flush waiting on an events API that never answers, and a full queue
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let connections = Arc::new(Mutex::new(vec![]));
        let accepted = Arc::clone(&connections);
        thread::spawn(move || {
            for stream in listener.incoming() {
                accepted.lock().unwrap().push(stream.unwrap());
            }
        });
        let processor = EventProcessor::with_retry_backoff(
            ENVIRONMENT_KEY,
            &format!("http://{address}/"),
            Duration::from_millis(300),
            Duration::from_millis(10),
            10,
            Duration::from_millis(1),
        );
        processor.track_event("checkout", None, EventOptions::default());
        thread::sleep(Duration::from_millis(100));
        for _ in 0..5000 {
            processor.track_event("checkout", None, EventOptions::default());
        }

        // When
        let started = Instant::now();
        processor.close(Duration::from_millis(100));
        let closed_in = started.elapsed();
        thread::sleep(Duration::from_secs(1));

        // Then: only the request in flight when it closed was sent
        assert!(closed_in < Duration::from_millis(500));
        assert_eq!(connections.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_flush_interval_of_zero_uses_the_default() {
        // Given
        let (address, bodies) = events_api(&[]);
        let processor = EventProcessor::new(
            ENVIRONMENT_KEY,
            &format!("http://{address}/"),
            Duration::from_secs(10),
            Duration::ZERO,
            100,
        );
        processor.track_event("clicked", None, EventOptions::default());

        // When
        processor.flush();

        // Then
        assert_eq!(sent_event_names(&bodies), vec![vec!["clicked"]]);
    }

    #[test]
    fn events_tracked_after_close_are_dropped() {
        // Given
        let (address, bodies) = events_api(&[]);
        let processor = processor_for(address);
        processor.close(Duration::from_secs(1));

        // When
        processor.track_event("checkout", None, EventOptions::default());
        processor.flush();

        // Then
        assert!(sent_event_names(&bodies).is_empty());
    }

    #[test]
    fn statuses_are_retried_rejected_or_stop_tracking() {
        use reqwest::StatusCode;
        let retried = SendOutcome::Failed {
            server_error: false,
        };
        assert_eq!(outcome_of_status(StatusCode::SERVICE_UNAVAILABLE), retried);
        assert_eq!(outcome_of_status(StatusCode::BAD_GATEWAY), retried);
        assert_eq!(outcome_of_status(StatusCode::GATEWAY_TIMEOUT), retried);
        assert_eq!(outcome_of_status(StatusCode::TOO_MANY_REQUESTS), retried);
        assert_eq!(outcome_of_status(StatusCode::REQUEST_TIMEOUT), retried);
        assert_eq!(
            outcome_of_status(StatusCode::INTERNAL_SERVER_ERROR),
            SendOutcome::Failed { server_error: true }
        );
        assert_eq!(
            outcome_of_status(StatusCode::BAD_REQUEST),
            SendOutcome::Rejected
        );
        assert_eq!(
            outcome_of_status(StatusCode::NOT_IMPLEMENTED),
            SendOutcome::Rejected
        );
        assert_eq!(
            outcome_of_status(StatusCode::UNAUTHORIZED),
            SendOutcome::Unauthorised
        );
        assert_eq!(
            outcome_of_status(StatusCode::FORBIDDEN),
            SendOutcome::Unauthorised
        );
    }

    #[test]
    fn retry_backoff_doubles() {
        let backoff = Duration::from_secs(1);
        assert_eq!(retry_backoff(backoff, 1), Duration::from_secs(1));
        assert_eq!(retry_backoff(backoff, 2), Duration::from_secs(2));
        assert_eq!(retry_backoff(backoff, 3), Duration::from_secs(4));
    }

    #[test]
    fn retry_wait_is_at_most_the_backoff() {
        for _ in 0..100 {
            assert!(retry_wait(Duration::from_secs(1), 2) <= Duration::from_secs(2));
        }
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
        processor.track_event("clicked", None, EventOptions::default());
        processor.flush();

        // When
        processor.flush();

        // Then
        events_mock.assert_hits(1);
    }

    #[test]
    fn flush_survives_an_unreachable_events_api() {
        // Given
        let processor = EventProcessor::with_retry_backoff(
            ENVIRONMENT_KEY,
            "http://127.0.0.1:1/",
            Duration::from_millis(100),
            Duration::from_secs(60),
            100,
            Duration::from_millis(1),
        );
        processor.track_event("clicked", None, EventOptions::default());

        // When / Then
        processor.flush();
    }
}
