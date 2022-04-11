use log::{warn, debug};
use reqwest::header::HeaderMap;
use serde_json;
use std::sync::mpsc;
use std::sync::mpsc:: Sender;
use std::{
    collections::HashMap,
    thread,
};

static ANALYTICS_TIMER: u64 = 1;

#[derive(Clone, Debug)]
pub struct AnalyticsProcessor {
    pub tx: Sender<u32>,
}

impl AnalyticsProcessor {
    pub fn new(api_url: String, headers: HeaderMap, timeout: std::time::Duration) -> Self {
        let (tx, rx) = mpsc::channel::<u32>();
        let client = reqwest::blocking::Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()
            .unwrap();
        let analytics_endpoint = format!("{}analytics/flags/", api_url);
        thread::Builder::new()
            .name("Analytics Processor".to_string())
            .spawn(move || {
                let mut last_flushed = chrono::Utc::now();
                let analytics_data: &mut HashMap<u32, u32> = &mut HashMap::new();
                loop {
                    let data = rx.recv();
                    if data.is_err(){
                        debug!("Shutting down analytics thread ");
                        break;
                    }
                    analytics_data
                        .entry(data.unwrap())
                        .and_modify(|e| *e += 1)
                        .or_insert(1);
                    if (chrono::Utc::now() - last_flushed).num_seconds() > ANALYTICS_TIMER as i64 {
                        flush(&client, analytics_data, &analytics_endpoint);
                        last_flushed = chrono::Utc::now();
                    }
                    thread::sleep(std::time::Duration::from_secs(ANALYTICS_TIMER));
                }
            }).expect("Failed to start analytics thread");

        return AnalyticsProcessor { tx };
    }
    pub fn track_feature(&self, feature_id: u32) {
        self.tx.send(feature_id).unwrap();
    }
}

fn flush(
    client: &reqwest::blocking::Client,
    analytics_data: &HashMap<u32, u32>,
    analytics_endpoint: &str,
) {
    println!("sending analytics data");
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

    #[test]
    fn test_analytics_processor() {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            "X-Environment-Key",
            header::HeaderValue::from_str("ser.UiYoRr6zUjiFBUXaRwo7b5").unwrap(),
        );
        let processor = AnalyticsProcessor::new(
            "http://localhost:8000/api/v1/".to_string(),
            headers,
            std::time::Duration::from_secs(10),
        );
        processor.tx.send(32).unwrap();

        thread::sleep(std::time::Duration::from_secs(2));
        processor.track_feature(10);
        thread::sleep(std::time::Duration::from_secs(2));
        processor.track_feature(10);
    }
}
