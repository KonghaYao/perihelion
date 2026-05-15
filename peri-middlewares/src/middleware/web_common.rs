use std::net::IpAddr;
use url::Url;

/// URL 安全校验，防止 SSRF
pub(crate) fn validate_url(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|e| format!("无效的 URL: {e}"))?;

    // 仅允许 http/https
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("仅支持 http/https 协议".to_string()),
    }

    // 检查主机名
    match parsed.host() {
        None => return Err("URL 缺少主机名".to_string()),
        Some(url::Host::Domain(_)) => {
            // 域名不做 DNS 解析，直接通过
        }
        Some(url::Host::Ipv4(ip)) => {
            if ip.is_loopback() {
                return Err("禁止访问回环地址".to_string());
            }
            if ip.is_private() {
                return Err("禁止访问私有地址".to_string());
            }
            if ip.is_link_local() {
                return Err("禁止访问链路本地地址".to_string());
            }
            if ip == IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED) {
                return Err("禁止访问未指定地址".to_string());
            }
        }
        Some(url::Host::Ipv6(ip)) => {
            if ip.is_loopback() {
                return Err("禁止访问回环地址".to_string());
            }
            if ip.is_unicast_link_local() {
                return Err("禁止访问链路本地地址".to_string());
            }
            // IPv6 私有地址：fc00::/7 (unique local)
            let segments = ip.segments();
            if (segments[0] & 0xfe00) == 0xfc00 {
                return Err("禁止访问私有地址".to_string());
            }
            if ip == IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED) {
                return Err("禁止访问未指定地址".to_string());
            }
        }
    }

    Ok(parsed)
}

/// HTML 转纯文本（120 列宽度）
pub(crate) fn html_to_text(html: &str) -> String {
    html2text::from_read(html.as_bytes(), 120).unwrap_or_default()
}

/// 按行数截断内容
pub(crate) fn truncate_content(content: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= max_lines {
        content.to_string()
    } else {
        let truncated: String = lines[..max_lines].join("\n");
        format!("{truncated}\n[内容已截断，原始内容共 {} 行]", lines.len())
    }
}

/// 网络来源可信度警告（附在 WebFetch/WebSearch 输出前）
pub(crate) const WEB_CREDIBILITY_WARNING: &str =
    "⚠ Web content may be inaccurate or outdated. Verify critical information before relying on it.\n\n";

/// 响应体大小上限
pub(crate) const MAX_RESPONSE_BYTES: u64 = 10 * 1024 * 1024; // 10MB
