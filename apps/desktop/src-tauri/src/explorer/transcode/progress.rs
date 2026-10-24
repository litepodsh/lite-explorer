//! Reads the `-progress pipe:1` output of ffmpeg.

#[derive(Debug, PartialEq)]
pub enum Update {
    /// How much of the output has been written, in seconds.
    Time(f64),
    End,
}

pub fn parse_line(line: &str) -> Option<Update> {
    let (key, value) = line.trim().split_once('=')?;
    match key {
        // `out_time_ms` is really microseconds too; ffmpeg kept the old name.
        "out_time_us" | "out_time_ms" => value
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|micros| *micros >= 0)
            .map(|micros| Update::Time(micros as f64 / 1_000_000.0)),
        "progress" if value.trim() == "end" => Some(Update::End),
        _ => None,
    }
}

/// How far along the job is, between 0 and 1. Unknown without the expected duration.
pub fn fraction(time: f64, total: Option<f64>) -> Option<f64> {
    let total = total.filter(|total| *total > 0.0)?;
    Some((time / total).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_processed_time() {
        assert_eq!(parse_line("out_time_us=1500000"), Some(Update::Time(1.5)));
        assert_eq!(parse_line("out_time_ms=2000000\n"), Some(Update::Time(2.0)));
        assert_eq!(parse_line("  out_time_us=0  "), Some(Update::Time(0.0)));
    }

    #[test]
    fn reads_the_end_marker() {
        assert_eq!(parse_line("progress=end"), Some(Update::End));
        assert_eq!(parse_line("progress=continue"), None);
    }

    #[test]
    fn ignores_everything_else() {
        for line in ["frame=42", "fps=25.0", "speed=1.5x", "bitrate=N/A", "out_time=00:00:01.500000", "", "garbage"] {
            assert_eq!(parse_line(line), None, "{line}");
        }
    }

    #[test]
    fn ignores_unknown_and_negative_times() {
        assert_eq!(parse_line("out_time_us=N/A"), None);
        assert_eq!(parse_line("out_time_us=-9223372036854775808"), None);
    }

    #[test]
    fn the_fraction_stays_between_zero_and_one() {
        assert_eq!(fraction(5.0, Some(10.0)), Some(0.5));
        assert_eq!(fraction(12.0, Some(10.0)), Some(1.0));
        assert_eq!(fraction(0.0, Some(10.0)), Some(0.0));
        assert_eq!(fraction(5.0, None), None);
        assert_eq!(fraction(5.0, Some(0.0)), None);
    }
}
