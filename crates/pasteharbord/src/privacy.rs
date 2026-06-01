const MIN_SECRET_LENGTH: usize = 20;

pub fn should_skip_text(text: &str, marked_sensitive: bool) -> bool {
    if marked_sensitive {
        return true;
    }

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }

    looks_like_secret(trimmed)
}

fn looks_like_secret(text: &str) -> bool {
    if text.len() < MIN_SECRET_LENGTH || text.contains(char::is_whitespace) {
        return false;
    }

    let has_alpha = text.chars().any(|ch| ch.is_ascii_alphabetic());
    let has_digit = text.chars().any(|ch| ch.is_ascii_digit());
    let symbol_count = text
        .chars()
        .filter(|ch| matches!(ch, '-' | '_' | '.' | '/' | '+' | '=' | ':' | '~'))
        .count();

    has_alpha && has_digit && symbol_count >= 2
}

#[cfg(test)]
mod tests {
    use super::should_skip_text;

    #[test]
    fn skips_marked_sensitive_text() {
        assert!(should_skip_text("regular text", true));
    }

    #[test]
    fn keeps_normal_text() {
        assert!(!should_skip_text("a regular copied sentence", false));
    }

    #[test]
    fn skips_token_shaped_text() {
        assert!(should_skip_text("ghp_abc1234567890.def-xyz", false));
    }
}
