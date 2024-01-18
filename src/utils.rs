#[allow(unused_imports)]
use std::str::FromStr;

#[macro_export]
macro_rules! assert_json_eq {
    ($a:expr, $b:expr) => {
        assert_eq!(
            serde_json::Value::from_str($a).unwrap(),
            serde_json::Value::from_str($b).unwrap()
        );
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_json_eq() {
        // let a = r#"{"name": "John", "age": 30}"#;
        // let b = r#"{"name": "Jane", "age": 25}"#;

        // assert_json_eq!(a, b);

        let a = r#"{"name": "John", "age": 30}"#;
        let b = r#"{"age": 30, "name": "John"}"#;

        assert_json_eq!(a, b);
    }
}
