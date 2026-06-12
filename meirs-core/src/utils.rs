use crate::error::EPortalError;
use crate::model::PortalInfo;
use encoding_rs::GBK;
use regex::Regex;
use reqwest::{Client, ClientBuilder};
use scraper::{Html, Selector};
use std::net::IpAddr;
use std::sync::LazyLock;
use std::time::Duration;
use tracing::{debug, info, warn};
use url::Url;

const PORTAL_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const PORTAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

static JSONP_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(\w+)\((.*)\);?\s*$").expect("valid JSONP regex pattern"));

static A41_JS_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"var\s+(\w+)\s*=\s*(\d+);").expect("valid a41.js regex pattern"));

pub(crate) fn portal_client_builder() -> ClientBuilder {
    Client::builder()
        .connect_timeout(PORTAL_CONNECT_TIMEOUT)
        .tls_danger_accept_invalid_certs(true)
        .timeout(PORTAL_REQUEST_TIMEOUT)
}

fn portal_client() -> Result<Client, reqwest::Error> {
    portal_client_builder().build()
}

pub(crate) struct JsonPParser {
    text: String,
    callback: String,
    data: String,
}

impl JsonPParser {
    pub fn new(text: String) -> Result<Self, EPortalError> {
        let captures = JSONP_PATTERN.captures(&text).and_then(|captures| {
            let callback = captures.get(1)?.as_str().to_owned();
            let data = captures.get(2)?.as_str().to_owned();

            Some((callback, data))
        });

        let Some((callback, data)) = captures else {
            return Err(EPortalError::InvalidJsonp(text));
        };
        Ok(Self {
            text,
            callback,
            data,
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn callback(&self) -> &str {
        &self.callback
    }

    pub fn data(&self) -> &str {
        &self.data
    }
}
fn extract_user_ip(url: &Url) -> Result<IpAddr, EPortalError> {
    let (_, value) = url
        .query_pairs()
        .find(|(key, _)| key == "userip" || key == "wlanuserip")
        .ok_or(EPortalError::InvalidRedirectUrl(url.to_string()))?;
    Ok(value.parse::<IpAddr>()?)
}
fn extract_auth_url(url: &Url) -> Result<Url, EPortalError> {
    url.host_str()
        .ok_or(EPortalError::InvalidRedirectUrl(url.to_string()))?;

    let mut auth_url = url.clone();
    auth_url.set_path("");
    auth_url.set_query(None);
    auth_url.set_fragment(None);

    Ok(auth_url)
}

fn parse_portal_redirect_url(html_content: &str) -> Result<Url, EPortalError> {
    let document = Html::parse_document(html_content);
    let selector = Selector::parse("a[href]").expect("valid href selector");

    let href = document
        .select(&selector)
        .next()
        .and_then(|element| element.attr("href"))
        .ok_or(EPortalError::PortalRedirectUrlNotFound)?;

    Url::parse(href).map_err(|_| EPortalError::InvalidRedirectUrl(href.to_string()))
}
fn parse_ep_ports(js_content: &str) -> (bool, u16, u16) {
    const DEFAULT_ENABLE_HTTPS: bool = false;
    const DEFAULT_HTTP_PORT: u16 = 801;
    const DEFAULT_HTTPS_PORT: u16 = 802;

    let mut enable_https = DEFAULT_ENABLE_HTTPS;
    let mut http_port = DEFAULT_HTTP_PORT;
    let mut https_port = DEFAULT_HTTPS_PORT;

    let mut found_enable_https = false;
    let mut found_http_port = false;
    let mut found_https_port = false;
    for captures in A41_JS_PATTERN.captures_iter(js_content) {
        let key = &captures[1];
        let value = &captures[2];
        match key {
            "enableHttps" => {
                enable_https = value == "1";
                found_enable_https = true;
            }
            "epHTTPPort" => {
                http_port = value.parse().expect("valid epHTTPPort");
                found_http_port = true;
            }
            "enHTTPSPort" => {
                https_port = value.parse().expect("valid enHTTPSPort");
                found_https_port = true;
            }
            _ => {}
        }
    }

    if !found_enable_https {
        warn!(
            "enableHttps not found in JavaScript config, using default value {}. May the content of a41.js changed?",
            DEFAULT_ENABLE_HTTPS
        );
    }

    if !found_http_port {
        warn!(
            "epHTTPPort not found in JavaScript config, using default port {}. May the content of a41.js changed?",
            DEFAULT_HTTP_PORT
        );
    }

    if !found_https_port {
        warn!(
            "enHTTPSPort not found in JavaScript config, using default port {}. May the content of a41.js changed?",
            DEFAULT_HTTPS_PORT
        );
    }

    (enable_https, http_port, https_port)
}
pub async fn check_online(client: Option<Client>) -> Result<bool, EPortalError> {
    let client = match client {
        Some(client) => client,
        None => portal_client()?,
    };
    debug!(url = "http://baidu.com", "sending online detection request");

    let response = match client.get("http://baidu.com").send().await {
        Ok(response) => response,
        Err(error) => {
            debug!(%error, "online detection request failed");
            return Ok(false);
        }
    };
    let response_url = response.url();
    debug!(url = %response_url, "received online detection response");

    Ok(response_url.scheme() == "https"
        && matches!(response_url.host_str(), Some("baidu.com" | "www.baidu.com")))
}

pub async fn discover_portal_info() -> Result<PortalInfo, EPortalError> {
    info!("discovering portal info");

    let client = portal_client()?;
    debug!(url = "http://baidu.com", "sending portal detection request");

    let response = client.get("http://baidu.com").send().await?;
    let response_url = response.url();
    debug!(url = %response_url, "received portal detection response");

    if response_url.scheme() == "https"
        && matches!(response_url.host_str(), Some("baidu.com" | "www.baidu.com"))
    {
        info!("portal discovery detected existing online session");
        return Err(EPortalError::AlreadyOnline);
    }

    let response_url = response.url().clone();

    let auth_url_with_params = if response_url.scheme() == "http"
        && matches!(response_url.host_str(), Some("baidu.com"))
    {
        debug!("parsing portal redirect URL from detection response body");

        let response_text = response.text().await?;
        parse_portal_redirect_url(&response_text)?
    } else {
        response_url
    };

    debug!(url = %auth_url_with_params,"parsed portal redirect URL");
    debug!("extracting user IP from portal redirect URL");
    let user_ip = extract_user_ip(&auth_url_with_params)?;
    debug!("extracting auth URL from portal redirect URL");
    let auth_url = extract_auth_url(&auth_url_with_params)?;
    debug!(%auth_url, %user_ip, "extracted portal auth info");

    let a41_js_url = auth_url
        .join("a41.js")
        .expect("joining a41.js should never fail");
    debug!(url = %a41_js_url, "fetching portal JavaScript config");

    let a41_js_bytes = client.get(a41_js_url.clone()).send().await?.bytes().await?;
    // 我操你妈你是人吗响应体用 GBK
    let a41_js_content = decode_portal_text(&a41_js_bytes);
    let (_enable_https, http_port, https_port) = parse_ep_ports(&a41_js_content);
    debug!(http_port, https_port, "parsed portal ports");

    //let (scheme, port) = if enable_https {
    //    ("https", https_port)
    //} else {
    //    ("http", http_port)
    //};

    // 活爹外包写的活爹代码：var ep_port = window.location.protocol === 'http:' ? epHTTPPort : enHTTPSPort;
    let (scheme, port) = if auth_url_with_params.scheme() == "https" {
        ("https", https_port)
    } else {
        ("http", http_port)
    };

    let mut server_url = auth_url.clone();

    server_url
        .set_scheme(scheme)
        .map_err(|_| EPortalError::InvalidRedirectUrl(server_url.to_string()))?;

    server_url
        .set_port(Some(port))
        .map_err(|_| EPortalError::InvalidRedirectUrl(server_url.to_string()))?;

    server_url.set_path("");
    server_url.set_query(None);
    server_url.set_fragment(None);
    info!(%server_url, "portal server discovered");

    Ok(PortalInfo {
        auth_url,
        server_url,
        user_ip,
    })
}

pub(crate) fn decode_portal_text(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }

    let (text, _, had_errors) = GBK.decode(bytes);

    if had_errors {
        tracing::warn!("portal script decoded as GBK with replacement errors");
    }

    text.into_owned()
}
