use crate::IspInfo;
use crate::error::EPortalError;
use crate::model::{EPortalResponse, LoadConfigResponse};
use crate::utils::{JsonPParser, decode_portal_text, portal_client_builder};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::net::IpAddr;
use std::sync::LazyLock;
use tracing::{debug, info, warn};
use url::Url;

static BODY_CONTENT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)var\s+bodyContent\s*=\s*'(.*?)'\s*;"#).expect("valid bodyContent regex")
});

static ISP_OPTION_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(r#"select[name="ISP_select"] option"#)
        .expect("valid ISP select option selector")
});

pub struct EPortalClient {
    base_url: Url,
    user_ip: IpAddr,
    local_address: Option<IpAddr>,
    client: Client,
}

impl EPortalClient {
    pub fn new(
        base_url: Url,
        user_ip: IpAddr,
        local_address: Option<IpAddr>,
    ) -> Result<Self, reqwest::Error> {
        debug!(%base_url, %user_ip, ?local_address, "creating ePortal client");

        let mut builder = portal_client_builder();
        if let Some(address) = &local_address {
            builder = builder.local_address(*address);
        }
        Ok(Self {
            base_url,
            user_ip,
            local_address,
            client: builder.build()?,
        })
    }

    pub fn with_client(base_url: Url, user_ip: IpAddr, client: Client) -> Self {
        debug!(%base_url, %user_ip, "creating ePortal client with injected HTTP client");

        Self {
            base_url,
            user_ip,
            local_address: None,
            client,
        }
    }

    pub async fn login(&self, account: &str, password: &str) -> Result<(), EPortalError> {
        info!(user_ip = %self.user_ip, ?self.local_address, "sending login request");

        let base64_password = BASE64_STANDARD.encode(password.as_bytes());
        let v = rand::random_range(500..15000);
        let url = self
            .base_url
            .join("/eportal/portal/login")
            .expect("joining url should never fail");
        debug!(%url, "prepared login request");

        let params = [
            // 实际上 callback 不重要，JsonP 回调用的
            ("callback", "dr1003".to_string()),
            ("login_method", "1".to_string()),
            ("user_account", format!(",0,{account}")),
            ("user_password", base64_password),
            ("wlan_user_ip", self.user_ip.to_string()),
            ("wlan_user_ipv6", "".to_string()),
            ("wlan_user_mac", "000000000000".to_string()),
            ("wlan_vlan_id", "0".to_string()),
            ("wlan_ac_ip", "".to_string()),
            ("wlan_ac_name", "".to_string()),
            ("authex_enable", "".to_string()),
            ("jsVersion", "4.2.2".to_string()),
            ("terminal_type", "3".to_string()),
            ("lang", "zh".to_string()),
            ("v", v.to_string()),
        ];
        let response_text = self
            .client
            .get(url)
            .query(&params)
            .send()
            .await
            .and_then(|response| response.error_for_status())?
            .text()
            .await?;

        let eportal_response =
            serde_json::from_str::<EPortalResponse>(JsonPParser::new(response_text)?.data())
                .map_err(EPortalError::Json)?;
        if !eportal_response.success() {
            warn!(message = %eportal_response.msg, "login request rejected by portal");
            return Err(EPortalError::PortalRejected(eportal_response.msg));
        }

        info!("login request accepted by portal");
        Ok(())
    }

    pub async fn logout(&self, account: &str) -> Result<(), EPortalError> {
        info!(user_ip = %self.user_ip, ?self.local_address, "sending logout request");

        let v = rand::random_range(500..15000);
        let url = self
            .base_url
            .join("/eportal/portal/mac/unbind")
            .expect("joining url should never fail");
        debug!(%url, "prepared logout request");

        let params = [
            // 实际上 callback 不重要，JsonP 回调用的
            ("callback", "dr1003".to_string()),
            ("unbind_type", "1".to_string()),
            ("user_account", format!(",0,{account}")),
            ("wlan_user_ip", self.user_ip.to_string()),
            ("wlan_user_ipv6", "".to_string()),
            ("wlan_user_mac", "000000000000".to_string()),
            ("wlan_vlan_id", "0".to_string()),
            ("wlan_ac_ip", "".to_string()),
            ("wlan_ac_name", "".to_string()),
            ("authex_enable", "".to_string()),
            ("jsVersion", "4.2.2".to_string()),
            ("terminal_type", "3".to_string()),
            ("lang", "zh".to_string()),
            ("v", v.to_string()),
        ];
        let response_text = self
            .client
            .get(url)
            .query(&params)
            .send()
            .await
            .and_then(|response| response.error_for_status())?
            .text()
            .await?;
        let eportal_response =
            serde_json::from_str::<EPortalResponse>(JsonPParser::new(response_text)?.data())
                .map_err(EPortalError::Json)?;
        if !eportal_response.success() {
            warn!(message = %eportal_response.msg, "logout request rejected by portal");
            return Err(EPortalError::PortalRejected(eportal_response.msg));
        }

        info!("logout request accepted by portal");
        Ok(())
    }

    pub async fn get_isp_info(&self) -> Result<Vec<IspInfo>, EPortalError> {
        info!(user_ip = %self.user_ip, ?self.local_address, "fetching ISP info");

        let base64_user_ip = BASE64_STANDARD.encode(self.user_ip.to_string().as_bytes());
        let v = rand::random_range(500..15000);
        let url = self
            .base_url
            .join("/eportal/portal/page/loadConfig")
            .expect("joining url should never fail");
        debug!(%url, "prepared portal loadConfig request");

        let params = [
            // 实际上 callback 不重要，JsonP 回调用的
            ("callback", "dr1001".to_string()),
            ("wlan_user_ip", base64_user_ip.to_string()),
            ("wlan_user_ipv6", "".to_string()),
            ("wlan_user_mac", "000000000000".to_string()),
            ("wlan_vlan_id", "0".to_string()),
            ("wlan_ac_ip", "".to_string()),
            ("wlan_ac_name", "".to_string()),
            ("jsVersion", "4.X".to_string()),
            ("terminal_type", "3".to_string()),
            ("lang", "zh".to_string()),
            ("v", v.to_string()),
        ];
        let response_text = self
            .client
            .get(url)
            .query(&params)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let config =
            serde_json::from_str::<LoadConfigResponse>(JsonPParser::new(response_text)?.data())?;
        debug!(
            program_index = %config.data.program_index,
            page_index = %config.data.page_index,
            "parsed portal page config"
        );

        let mut hipad_js_url = self
            .base_url
            .join("/eportal/extern/")
            .expect("joining url should never fail");
        let invalid_url = hipad_js_url.to_string();

        hipad_js_url
            .path_segments_mut()
            .map_err(|_| EPortalError::InvalidUrl(invalid_url))?
            .push(&config.data.program_index)
            .push(&config.data.page_index)
            .push("hipad.js");
        debug!(%hipad_js_url, "prepared hipad.js request");

        let hipad_js_bytes = self
            .client
            .get(hipad_js_url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        // 我操你妈你是人吗响应体用 GBK
        let hipad_js_content = decode_portal_text(&hipad_js_bytes);

        let isp_info = parse_isp_info(&hipad_js_content);
        info!(count = isp_info.len(), "parsed ISP info");

        Ok(isp_info)
    }
}

fn parse_isp_info(hipad_js_content: &str) -> Vec<IspInfo> {
    let Some(body_content) = BODY_CONTENT_PATTERN
        .captures(hipad_js_content)
        .and_then(|captures| captures.get(1))
        .map(|body_content| body_content.as_str())
    else {
        warn!("bodyContent not found in hipad.js while parsing ISP info");
        return Vec::new();
    };

    let document = Html::parse_document(body_content);
    document
        .select(&ISP_OPTION_SELECTOR)
        .filter_map(|element| {
            let suffix = element.attr("value")?;
            if suffix == "-1" {
                return None;
            }

            let name = element.text().collect::<String>().trim().to_owned();
            if name.is_empty() {
                return None;
            }

            Some(IspInfo {
                name,
                suffix: suffix.to_owned(),
            })
        })
        .collect()
}
