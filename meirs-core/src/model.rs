use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use tabled::Tabled;
use url::Url;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct EPortalResponse {
    pub(crate) result: i8,
    pub(crate) msg: String,
}

impl EPortalResponse {
    pub fn success(&self) -> bool {
        self.result == 1
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct LoadConfigResponse {
    pub(crate) data: LoadConfigData,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LoadConfigData {
    pub(crate) program_index: String,
    pub(crate) page_index: String,
}

#[derive(Debug, Clone, Tabled)]
pub struct PortalInfo {
    pub auth_url: Url,
    pub server_url: Url,
    pub user_ip: IpAddr,
}

#[derive(Debug, Clone, PartialEq, Eq, Tabled)]
pub struct IspInfo {
    pub name: String,
    pub suffix: String,
}
