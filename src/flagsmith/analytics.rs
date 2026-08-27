use flume;
use log::{debug, warn};
use reqwest::header::HeaderMap;
use serde_json;
use std::{collections::HashMap, thread};

use std::sync::{Arc, RwLock};
static ANALYTICS_TIMER_IN_MILLI: u64 = 10 * 1000;

#[derive(Clone, Debug)]
pub struct AnalyticsProcessor {
    pub tx: flume::Sender<String>,
    _analytics_data: Arc<RwLock<HashMap<String, u32>>>,
}

impl AnalyticsProcessor {
    pub fn new(
        api_url: String,
        headers: HeaderMap,
        timeout: std::time::Duration,
        timer: Option<u64>,
    ) -> Self {
        let (tx, rx) = flume::unbounded();
        let client = reqwest::blocking::Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()
            .unwrap();
        let analytics_endpoint = format!("{}analytics/flags/", api_url);
        let timer = timer.unwrap_or(ANALYTICS_TIMER_IN_MILLI);

        let analytics_data_arc: Arc<RwLock<HashMap<String, u32>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let analytics_data_locked = Arc::clone(&analytics_data_arc);
        thread::Builder::new()
            .name("Analytics Processor".to_string())
            .spawn(move || {
                let mut last_flushed = chrono::Utc::now();
                loop {
                    // Block until either a feature is tracked or the next flush is due
                    let elapsed = (chrono::Utc::now() - last_flushed)
                        .num_milliseconds()
                        .max(0) as u64;
                    // `saturating_sub` covers a flush that overran its window
                    let data = rx.recv_timeout(std::time::Duration::from_millis(
                        timer.saturating_sub(elapsed),
                    ));

                    let disconnected = match data {
                        // Update the analytics data with feature_id received
                        Ok(feature_name) => {
                            let mut analytics_data = analytics_data_locked.write().unwrap();
                            analytics_data
                                .entry(feature_name)
                                .and_modify(|e| *e += 1)
                                .or_insert(1);
                            false
                        }
                        Err(flume::RecvTimeoutError::Timeout) => false,
                        Err(flume::RecvTimeoutError::Disconnected) => {
                            debug!("Shutting down analytics thread ");
                            true
                        }
                    };

                    // Flush when due, or on shutdown
                    let flush_due = (chrono::Utc::now() - last_flushed).num_milliseconds()
                        > timer as i64
                        || disconnected;
                    if flush_due {
                        let mut analytics_data = analytics_data_locked.write().unwrap();
                        flush(&client, &analytics_data, &analytics_endpoint);
                        analytics_data.clear();
                        last_flushed = chrono::Utc::now();
                    }

                    if disconnected {
                        break;
                    }
                }
            })
            .expect("Failed to start analytics thread");

        return AnalyticsProcessor {
            tx,
            _analytics_data: Arc::clone(&analytics_data_arc),
        };
    }
    pub fn track_feature(&self, feature_name: &str) {
        self.tx.send(feature_name.to_string()).unwrap();
    }
}

fn flush(
    client: &reqwest::blocking::Client,
    analytics_data: &HashMap<String, u32>,
    analytics_endpoint: &str,
) {
    if analytics_data.len() == 0 {
        return;
    }
    let body = serde_json::to_string(&analytics_data).unwrap();
    let resp = client.post(analytics_endpoint).body(body).send();
    if resp.is_err() {
        warn!("Failed to send analytics data");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use reqwest::header;

    #[test]
    fn dropping_processor_flushes_pending_data_and_shuts_down() {
        // Given
        let feature_1 = "feature_1";
        let server = MockServer::start();
        let shutdown_flush_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/analytics/flags/")
                .json_body(serde_json::json!({feature_1: 2}));
            then.status(200);
        });

        let processor = AnalyticsProcessor::new(
            server.url("/api/v1/"),
            header::HeaderMap::new(),
            std::time::Duration::from_secs(10),
            Some(60_000), // deliberately never due during this test
        );
        processor.track_feature(feature_1);
        processor.track_feature(feature_1);
        thread::sleep(std::time::Duration::from_millis(50));

        // Nothing should have been sent yet: the timer is 60s away.
        shutdown_flush_mock.assert_hits(0);

        // When
        drop(processor);
        thread::sleep(std::time::Duration::from_millis(50));

        // Then
        shutdown_flush_mock.assert_hits(1);
    }

    #[test]
    fn flush_survives_unreachable_endpoint() {
        // Given
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(100))
            .build()
            .unwrap();
        let mut analytics_data = HashMap::new();
        analytics_data.insert("feature_1".to_string(), 1);

        // When / Then
        flush(
            &client,
            &analytics_data,
            "http://127.0.0.1:1/analytics/flags/",
        );
    }

    #[test]
    fn track_feature_updates_analytics_data() {
        // Given
        let feature_1 = "feature_1";
        let processor = AnalyticsProcessor::new(
            "http://localhost".to_string(),
            header::HeaderMap::new(),
            std::time::Duration::from_secs(10),
            Some(10000),
        );
        // Now, let's make tracking calls
        processor.track_feature(feature_1);
        processor.track_feature(feature_1);
        // Wait a little for it to receive the message
        thread::sleep(std::time::Duration::from_millis(50));
        let analytics_data = processor._analytics_data.read().unwrap();
        // Then, verify that analytics_data was updated correctly
        assert_eq!(analytics_data[feature_1], 2);
    }

    #[test]
    fn test_analytics_processor() {
        // Given
        let feature_1 = "feature_1";
        let feature_2 = "feature_2";
        let server = MockServer::start();
        let first_invocation_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/analytics/flags/")
                .header("X-Environment-Key", "ser.UiYoRr6zUjiFBUXaRwo7b5")
                .json_body(serde_json::json!({feature_1:10, feature_2:10}));
            then.status(200).header("content-type", "application/json");
        });
        let mut headers = header::HeaderMap::new();
        headers.insert(
            "X-Environment-Key",
            header::HeaderValue::from_str("ser.UiYoRr6zUjiFBUXaRwo7b5").unwrap(),
        );
        let url = server.url("/api/v1/");

        let processor = AnalyticsProcessor::new(
            url.to_string(),
            headers,
            std::time::Duration::from_secs(10),
            Some(10),
        );
        // Now, let's update the analytics data
        let mut analytics_data = processor._analytics_data.write().unwrap();
        analytics_data.insert(feature_1.to_string(), 10);
        analytics_data.insert(feature_2.to_string(), 10);
        // drop the analytics data to release the lock
        drop(analytics_data);
        // Next, let's sleep a little to let the processor flush the data
        thread::sleep(std::time::Duration::from_millis(50));

        // Finally, let's assert that the mock was called
        first_invocation_mock.assert();
        // and, analytics data is now empty
        let analytics_data = processor._analytics_data.read().unwrap();
        assert_eq!(true, analytics_data.is_empty())
    }
}
