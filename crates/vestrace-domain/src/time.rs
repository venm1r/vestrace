pub type Timestamp = chrono::DateTime<chrono::Utc>;

pub fn now() -> Timestamp {
    chrono::Utc::now()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::now;

    #[test]
    fn now_returns_a_current_utc_timestamp() {
        let before = Utc::now();
        let timestamp = now();
        let after = Utc::now();

        assert!(timestamp >= before);
        assert!(timestamp <= after);
    }
}
