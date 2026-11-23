//! Outbound web access for the assistant: search and page fetching, behind an
//! SSRF guard. Only public internet addresses are reachable from here.

use crate::error::{Error, Result};
use futures_util::StreamExt;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use url::Url;

const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;
const TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

/// True when `ip` is a public, routable address. Everything else — loopback,
/// private ranges, link-local, CGNAT, multicast, documentation, unspecified —
/// is refused so a web page can never steer requests into the local network.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || o[0] == 0
                || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64.0.0/10 CGNAT
                || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24
                || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18.0.0/15
                || o[0] >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // unique local
                || (s[0] & 0xffc0) == 0xfe80 // link-local
                || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
                || (s[0] == 0x64 && s[1] == 0xff9b)) // NAT64
        }
    }
}

/// Parses and validates a URL for outbound fetching (scheme and host only).
pub fn check_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw.trim()).map_err(|_| Error::Invalid("url".into()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::Blocked("only http and https are allowed".into()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::Blocked("credentials in url".into()));
    }
    match url.host() {
        None => Err(Error::Invalid("url host".into())),
        Some(url::Host::Ipv4(ip)) if !is_public_ip(IpAddr::V4(ip)) => {
            Err(Error::Blocked("private address".into()))
        }
        Some(url::Host::Ipv6(ip)) if !is_public_ip(IpAddr::V6(ip)) => {
            Err(Error::Blocked("private address".into()))
        }
        Some(_) => Ok(url),
    }
}

/// Resolves the URL's host and returns one public address, refusing the
/// request if any resolved address is not public.
async fn resolve_public(url: &Url) -> Result<SocketAddr> {
    let host = url
        .host_str()
        .ok_or_else(|| Error::Invalid("url host".into()))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| Error::Invalid("url port".into()))?;
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| Error::Net(e.to_string()))?
        .collect();
    if addrs.is_empty() {
        return Err(Error::Net("host did not resolve".into()));
    }
    if addrs.iter().any(|a| !is_public_ip(a.ip())) {
        return Err(Error::Blocked("host resolves to a private address".into()));
    }
    Ok(addrs[0])
}

/// Fetches a public URL with size, time and redirect limits. Every hop is
/// re-validated and the connection is pinned to the address that was checked,
/// so DNS rebinding cannot swap in a private target.
pub async fn fetch(raw: &str) -> Result<(Url, String)> {
    let mut url = check_url(raw)?;
    for _ in 0..=MAX_REDIRECTS {
        let addr = resolve_public(&url).await?;
        let host = url.host_str().unwrap_or_default().to_string();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(TIMEOUT)
            .user_agent(USER_AGENT)
            .resolve(&host, addr)
            .build()
            .map_err(|e| Error::Net(e.to_string()))?;
        let resp = client
            .get(url.clone())
            .header("Accept", "text/html,text/plain;q=0.9,*/*;q=0.5")
            .send()
            .await
            .map_err(|e| Error::Net(e.to_string()))?;
        if resp.status().is_redirection() {
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| Error::Net("redirect without location".into()))?;
            let next = url
                .join(location)
                .map_err(|_| Error::Invalid("redirect url".into()))?;
            url = check_url(next.as_str())?;
            continue;
        }
        if !resp.status().is_success() {
            return Err(Error::Net(format!(
                "http status {}",
                resp.status().as_u16()
            )));
        }
        let mut body = Vec::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Error::Net(e.to_string()))?;
            let room = MAX_BODY.saturating_sub(body.len());
            body.extend_from_slice(&chunk[..chunk.len().min(room)]);
            if body.len() >= MAX_BODY {
                break;
            }
        }
        return Ok((url, String::from_utf8_lossy(&body).into_owned()));
    }
    Err(Error::Net("too many redirects".into()))
}

/// Reduces an HTML document to readable text.
pub fn html_to_text(html: &str, max_chars: usize) -> String {
    let doc = scraper::Html::parse_document(html);
    let mut out = String::new();
    collect_text(doc.root_element(), &mut out);
    let collapsed = collapse_whitespace(&out);
    truncate_chars(&collapsed, max_chars)
}

fn collect_text(el: scraper::ElementRef<'_>, out: &mut String) {
    const SKIP: [&str; 9] = [
        "script", "style", "noscript", "svg", "nav", "footer", "header", "form", "iframe",
    ];
    const BLOCK: [&str; 12] = [
        "p", "div", "br", "li", "h1", "h2", "h3", "h4", "tr", "section", "article", "pre",
    ];
    for child in el.children() {
        if let Some(text) = child.value().as_text() {
            out.push_str(text);
        } else if let Some(child_el) = scraper::ElementRef::wrap(child) {
            let name = child_el.value().name();
            if SKIP.contains(&name) {
                continue;
            }
            collect_text(child_el, out);
            if BLOCK.contains(&name) {
                out.push('\n');
            }
        }
    }
}

fn collapse_whitespace(s: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in s.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines.join("\n")
}

pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push_str("\n[truncated]");
        out
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Parses a DuckDuckGo HTML results page.
pub fn parse_search(html: &str) -> Vec<SearchHit> {
    let doc = scraper::Html::parse_document(html);
    let (Ok(result), Ok(link), Ok(snippet)) = (
        scraper::Selector::parse(".result"),
        scraper::Selector::parse("a.result__a"),
        scraper::Selector::parse(".result__snippet"),
    ) else {
        return Vec::new();
    };
    doc.select(&result)
        .filter_map(|r| {
            let a = r.select(&link).next()?;
            let url = unwrap_ddg_redirect(a.value().attr("href")?)?;
            Some(SearchHit {
                title: a.text().collect::<String>().trim().to_string(),
                url,
                snippet: r
                    .select(&snippet)
                    .next()
                    .map(|s| s.text().collect::<String>().trim().to_string())
                    .unwrap_or_default(),
            })
        })
        .take(8)
        .collect()
}

/// DuckDuckGo wraps result links as `//duckduckgo.com/l/?uddg=<target>`.
fn unwrap_ddg_redirect(href: &str) -> Option<String> {
    let absolute = if href.starts_with("//") {
        format!("https:{href}")
    } else {
        href.to_string()
    };
    let url = Url::parse(&absolute).ok()?;
    if url
        .host_str()
        .is_some_and(|h| h.ends_with("duckduckgo.com"))
        && url.path() == "/l/"
    {
        let target = url.query_pairs().find(|(k, _)| k == "uddg")?.1.into_owned();
        return check_url(&target).ok().map(|u| u.to_string());
    }
    check_url(&absolute).ok().map(|u| u.to_string())
}

pub async fn web_search(query: &str) -> Result<Vec<SearchHit>> {
    web_search_recent(query, "any").await
}

/// Like [`web_search`], limited to results from the last `day`, `week`,
/// `month` or `year` (anything else: no limit).
pub async fn web_search_recent(query: &str, recency: &str) -> Result<Vec<SearchHit>> {
    let query = query.trim();
    if query.is_empty() || query.len() > 300 {
        return Err(Error::Invalid("search query".into()));
    }
    let mut url =
        Url::parse("https://html.duckduckgo.com/html/").map_err(|e| Error::Net(e.to_string()))?;
    url.query_pairs_mut().append_pair("q", query);
    let df = match recency {
        "day" => "d",
        "week" => "w",
        "month" => "m",
        "year" => "y",
        _ => "",
    };
    if !df.is_empty() {
        url.query_pairs_mut().append_pair("df", df);
    }
    let (_, html) = fetch(url.as_str()).await?;
    Ok(parse_search(&html))
}

/// The `<title>` of an HTML page, cleaned up.
pub fn page_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open_end = start + lower[start..].find('>')? + 1;
    let close = open_end + lower[open_end..].find("</title>")?;
    let title = html_to_text(&html[open_end..close], 200);
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    (!title.is_empty()).then_some(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn private_and_special_addresses_are_refused() {
        for bad in [
            "127.0.0.1",
            "10.0.0.5",
            "172.16.3.1",
            "192.168.1.1",
            "169.254.169.254",
            "0.0.0.0",
            "100.64.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "198.18.0.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd12::1",
            "::ffff:127.0.0.1",
            "::ffff:192.168.0.1",
            "64:ff9b::7f00:1",
        ] {
            assert!(!is_public_ip(ip(bad)), "{bad} must be refused");
        }
        for good in [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.216.34",
            "2606:4700:4700::1111",
        ] {
            assert!(is_public_ip(ip(good)), "{good} must be allowed");
        }
    }

    #[test]
    fn url_checks() {
        assert!(check_url("https://example.com/a?b=1").is_ok());
        assert!(check_url("file:///etc/passwd").is_err());
        assert!(check_url("ftp://example.com").is_err());
        assert!(check_url("http://127.0.0.1:11434/api/tags").is_err());
        assert!(check_url("http://[::1]/").is_err());
        assert!(check_url("http://192.168.0.1/admin").is_err());
        assert!(check_url("https://user:pw@example.com/").is_err());
        assert!(check_url("http://2130706433/").is_err()); // decimal 127.0.0.1
        assert!(check_url("not a url").is_err());
    }

    #[tokio::test]
    async fn localhost_name_is_refused_after_resolution() {
        let err = fetch("http://localhost:1/").await.unwrap_err();
        assert!(matches!(err, Error::Blocked(_)), "{err}");
    }

    #[test]
    fn html_text_extraction() {
        let html = "<html><head><style>x{}</style><script>evil()</script></head>
            <body><nav>menu</nav><h1>Title</h1><p>Hello   <b>world</b></p><footer>f</footer></body></html>";
        assert_eq!(html_to_text(html, 1000), "Title\nHello world");
        assert!(html_to_text(&"<p>abcdef</p>".repeat(10), 5).starts_with("abcde\n[truncated]"));
    }

    #[test]
    fn search_parsing() {
        let html = r#"<div class="result"><a class="result__a"
            href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage&rut=x">Example</a>
            <a class="result__snippet">A snippet</a></div>
            <div class="result"><a class="result__a" href="//duckduckgo.com/l/?uddg=http%3A%2F%2F10.0.0.1%2F">Bad</a></div>"#;
        let hits = parse_search(html);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://example.com/page");
        assert_eq!(hits[0].title, "Example");
        assert_eq!(hits[0].snippet, "A snippet");
    }
}
