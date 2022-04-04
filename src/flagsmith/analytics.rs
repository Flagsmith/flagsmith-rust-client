use super::FlagsmithOptions;
use chrono::{DateTime, Duration, Utc, serde};
use reqwest::header::{self, HeaderMap};
use std::{collections::{HashMap, hash_map}};
use serde_json;
use log::{info, trace, warn};

static ANALYTICS_ENDPOINT: &str = "analytics/flags/";
static ANALYTICS_TIMER: i64 = 10;

struct AnalyticsProcessor {
    last_flushed: chrono::DateTime<Utc>,
    analytics_data: HashMap<u32, u32>,
    client: reqwest::blocking::Client,
    analytics_endpoint: String

}

impl AnalyticsProcessor {
    fn new(api_url: String, mut headers: HeaderMap, timeout: std::time::Duration) -> Self {
        // let mut headers = flagsmith_options.custom_headers.clone();//custom_headers.unwrap_or(header::HeaderMap::new());

        headers.insert("Content-Type", "application/json".parse().unwrap());
        let client = reqwest::blocking::Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()
            .unwrap();
        let analytics_endpoint = format!("{}analytics/flags/",api_url);

        return AnalyticsProcessor {
            last_flushed: chrono::Utc::now(),
            analytics_data: HashMap::new(),
            client,
            analytics_endpoint,
        };
    }

    fn flush(&mut self) {
        if self.analytics_data.len() == 0 {
            return;
        }
        let body = serde_json::to_string(&self.analytics_data).unwrap();
        let resp = self.client.post(self.analytics_endpoint.clone()).body(body).send();
        if resp.is_err(){
            warn!("Failed to send analytics data");
        }
       self.analytics_data.clear();
    }
    pub fn track_feature(&mut self, feature_id: u32) {
        self.analytics_data
            .entry(feature_id)
            .and_modify(|e| *e += 1)
            .or_insert(1);
        // self.analytics_data.insert(feature_id, v)
        if chrono::Utc::now() - self.last_flushed > Duration::seconds(ANALYTICS_TIMER) {
            self.flush()
        }
    }
}

#[cfg(test)]
mod tests{
    use super::*;

    #[test]
    fn test_analytics_processor(){
        let mut headers =  header::HeaderMap::new();
        headers.insert(
            "X-Environment-Key",
            header::HeaderValue::from_str("ser.UiYoRr6zUjiFBUXaRwo7b5").unwrap(),
        );
        let mut processor = AnalyticsProcessor::new("http://localhost:8000/api/v1/".to_string(), headers, std::time::Duration::from_secs(10));
        processor.track_feature(10);
        processor.flush();
    }
}
