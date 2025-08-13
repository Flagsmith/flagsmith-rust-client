use flume;
use log::{debug, warn};
use reqwest::header::HeaderMap;
use serde_json;
use std::{collections::HashMap, thread};

use std::sync::{Arc, RwLock};

use crate::flagsmith::client::client::{ClientLike, ClientRequestBuilder, Method, SafeClient};
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
        let client = SafeClient::new(headers.clone(), timeout);

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
                    let data = rx.try_recv();
                    let mut analytics_data = analytics_data_locked.write().unwrap();
                    match data {
                        // Update the analytics data with feature_id received
                        Ok(feature_name) => {
                            analytics_data
                                .entry(feature_name)
                                .and_modify(|e| *e += 1)
                                .or_insert(1);
                        }
                        Err(flume::TryRecvError::Empty) => {}
                        Err(flume::TryRecvError::Disconnected) => {
                            debug!("Shutting down analytics thread ");
                            break;
                        }
                    };
                    if (chrono::Utc::now() - last_flushed).num_milliseconds() > timer as i64 {
                        flush(&client, &analytics_data, &analytics_endpoint);
                        analytics_data.clear();
                        last_flushed = chrono::Utc::now();
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

fn flush(client: &SafeClient, analytics_data: &HashMap<String, u32>, analytics_endpoint: &str) {
    if analytics_data.len() == 0 {
        return;
    }
    let body = serde_json::to_string(&analytics_data).unwrap();
    let req = client
        .inner
        .request(Method::POST, analytics_endpoint.to_string())
        .with_body(body);
    let resp = req.send();
    if resp.is_err() {
        warn!("Failed to send analytics data");
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use reqwest::header;

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
