use chrono::NaiveDateTime;

pub fn parse_datetime(s: &str) -> Option<NaiveDateTime> {
    if let Ok(d) = chrono::DateTime::parse_from_rfc2822(s) {
        return Some(d.naive_utc());
    }

    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(d.naive_utc());
    }

    if let Ok(d) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(d);
    }
    
    if let Ok(d) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.3f") {
        return Some(d);
    }

    None
}