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
