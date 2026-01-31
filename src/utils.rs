/// Format bytes into a human-readable size.
pub fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", size as u64, UNITS[unit_idx])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

/// Parse a human-readable size into bytes.
///
/// Supported examples:
/// - `1024`
/// - `10KB`, `10 KB`
/// - `1.5GB`
pub fn parse_size(input: &str) -> anyhow::Result<u64> {
    let input = input.trim();
    if input.is_empty() {
        anyhow::bail!("size is empty");
    }

    let mut split_idx = None;
    for (idx, ch) in input.char_indices() {
        if ch.is_ascii_alphabetic() {
            split_idx = Some(idx);
            break;
        }
    }

    let (number_str, unit_str) = match split_idx {
        Some(idx) => (&input[..idx], &input[idx..]),
        None => (input, ""),
    };

    let number = number_str
        .trim()
        .parse::<f64>()
        .map_err(|_| anyhow::anyhow!("invalid size number: `{}`", number_str.trim()))?;
    if !number.is_finite() || number < 0.0 {
        anyhow::bail!("invalid size number: `{}`", number_str.trim());
    }

    let unit = unit_str.trim().to_ascii_uppercase();
    let unit = unit.strip_suffix('B').unwrap_or(&unit);

    let multiplier: f64 = match unit {
        "" => 1.0,
        "K" | "KI" | "KIB" | "KB" => 1024.0,
        "M" | "MI" | "MIB" | "MB" => 1024.0 * 1024.0,
        "G" | "GI" | "GIB" | "GB" => 1024.0 * 1024.0 * 1024.0,
        "T" | "TI" | "TIB" | "TB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        other => anyhow::bail!("unknown size unit: `{}`", other),
    };

    let bytes = number * multiplier;
    if bytes > (u64::MAX as f64) {
        anyhow::bail!("size is too large");
    }

    Ok(bytes.round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(1048576), "1.00 MB");
        assert_eq!(format_size(1073741824), "1.00 GB");
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("0").unwrap(), 0);
        assert_eq!(parse_size("1024").unwrap(), 1024);
        assert_eq!(parse_size("1KB").unwrap(), 1024);
        assert_eq!(parse_size("1 KB").unwrap(), 1024);
        assert_eq!(parse_size("1MB").unwrap(), 1024 * 1024);
        assert_eq!(
            parse_size("1.5GB").unwrap(),
            (1.5 * 1024.0 * 1024.0 * 1024.0) as u64
        );
    }
}
