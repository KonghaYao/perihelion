use axum::http::StatusCode;
use subtle::ConstantTimeEq;

/// 使用常量时间比较防止 timing attack。
/// 注：长度不同时短路返回 false，攻击者可得知长度差异，但不影响 token 内容安全。
pub fn validate_token(provided: Option<&str>, expected: &str) -> Result<(), StatusCode> {
    match provided {
        Some(token) => {
            let ok: bool = token.as_bytes().ct_eq(expected.as_bytes()).into();
            if ok { Ok(()) } else { Err(StatusCode::UNAUTHORIZED) }
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn test_validate_token_correct() {
        assert_eq!(validate_token(Some("abc"), "abc"), Ok(()));
    }

    #[test]
    fn test_validate_token_wrong() {
        assert_eq!(validate_token(Some("xyz"), "abc"), Err(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn test_validate_token_none() {
        assert_eq!(validate_token(None, "abc"), Err(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn test_validate_token_empty_string() {
        assert_eq!(validate_token(Some(""), "abc"), Err(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn test_validate_token_correct_unicode() {
        let token = "tok-\u{4e2d}\u{6587}-\u{1f600}";
        assert_eq!(validate_token(Some(token), token), Ok(()));
        assert_eq!(validate_token(Some("other"), token), Err(StatusCode::UNAUTHORIZED));
    }
}
