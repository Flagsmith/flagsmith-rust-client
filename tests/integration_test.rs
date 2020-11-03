use bullettrain;

#[test]
fn test_get_features() {
    let client = bullettrain::Client{
        api_key: String::from("MgfUaRCvvZMznuQyqjnQKt"),
        config: bullettrain::Config{
            base_uri: String::from("https://api.bullet-train.io/api/v1/"),
        },
    };
    let features = client.get_features();
    match features {
        Err(e) => panic!("Problem with remote call: {:?}", e),
        Ok(features) => {
            assert_eq!(features.len(), 4);
            for f in features.iter() {
                assert!(f.feature.name != "");
            }
        }
    }
}
