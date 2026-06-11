pub mod error;

pub mod model;

mod utils;

mod eportal_client;

pub use eportal_client::EPortalClient;
pub use error::EPortalError;
pub use model::{IspInfo, PortalInfo};
pub use utils::{discover_portal_info,check_online};
