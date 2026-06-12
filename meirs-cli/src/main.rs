use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::OnceLock;

mod error;
mod prompt;

use crate::error::CliError;
use crate::prompt::{
    ensure_isp_info_available, ensure_login_can_prompt, prompt_account, prompt_password,
    read_password_stdin,
};
use clap::builder::Styles;
use clap::builder::styling::AnsiColor;
use clap::{ArgAction, Args, Parser, Subcommand};
use cliclack::{intro, log, outro, spinner};
use directories::BaseDirs;
use meirs_core::{EPortalClient, EPortalError, IspInfo, PortalInfo, discover_portal_info};
use serde::{Deserialize, Serialize};
use shadow_rs::shadow;
use tabled::Table;
use tabled::settings::Style;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;
use url::Url;

shadow!(build);
const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::Red.on_default().bold())
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Yellow.on_default());

#[derive(Debug, Parser)]
#[command(
    name = "meirs",
    version = version(),
    about = "An extremely fast network authentication tool for Zhengzhou University.",
    arg_required_else_help = true,
    styles = STYLES,
)]
struct Cli {
    #[arg(
        short,
        long,
        global = true,
        action = ArgAction::Count,
        help = "Increase log verbosity (-v, -vv, -vvv). RUST_LOG overrides this"
    )]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Authenticate the current device")]
    Login(LoginArgs),
    #[command(about = "End the current session")]
    Logout(LogoutArgs),
    #[command(about = "Discover portal server information")]
    Discover(DiscoverArgs),
    #[command(about = "Manage ISP information")]
    Isp(IspArgs),
}

#[derive(Debug, Args)]
struct DiscoverArgs {
    #[arg(long, help = "Save discovered portal information")]
    save: bool,
}

#[derive(Debug, Args)]
struct IspArgs {
    #[command(subcommand)]
    command: IspCommand,
}

#[derive(Debug, Subcommand)]
enum IspCommand {
    #[command(about = "List available ISPs")]
    List(IspListArgs),
}

#[derive(Debug, Args)]
struct IspListArgs {
    #[arg(long, value_name = "URL", help = "Portal server base URL")]
    portal_url: Option<Url>,
    #[arg(long, value_name = "IP", help = "User IP address")]
    user_ip: Option<IpAddr>,
    #[arg(long, value_name = "IP", help = "Local address for bind")]
    local_addr: Option<IpAddr>,
}

#[derive(Debug, Args)]
struct LoginArgs {
    #[arg(
        long,
        value_name = "ACCOUNT",
        help = "Portal account, with possible isp suffix, e.g. 2026114514@cmcc"
    )]
    account: Option<String>,
    #[arg(long, help = "Read portal password from standard input")]
    password_stdin: bool,
    #[arg(long, value_name = "URL", help = "Portal server base URL")]
    portal_url: Option<Url>,
    #[arg(long, value_name = "IP", help = "User IP address")]
    user_ip: Option<IpAddr>,
    #[arg(long, value_name = "IP", help = "Local address for bind")]
    local_addr: Option<IpAddr>,
}

#[derive(Debug, Args)]
struct LogoutArgs {
    #[arg(
        long,
        value_name = "ACCOUNT",
        help = "Portal account, with possible isp suffix, e.g. 2026114514@cmcc"
    )]
    account: Option<String>,
    #[arg(long, value_name = "URL", help = "Portal  server base URL")]
    portal_url: Option<Url>,
    #[arg(long, value_name = "IP", help = "User IP address")]
    user_ip: Option<IpAddr>,
    #[arg(long, value_name = "IP", help = "Local address for bind")]
    local_addr: Option<IpAddr>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PortalInfoFile {
    auth_url: Url,
    server_url: Url,
    user_ip: IpAddr,
}

impl From<&PortalInfo> for PortalInfoFile {
    fn from(info: &PortalInfo) -> Self {
        Self {
            auth_url: info.auth_url.clone(),
            server_url: info.server_url.clone(),
            user_ip: info.user_ip,
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            print_error(&error);
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), CliError> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    debug!(command = command_name(&cli.command), "parsed CLI command");

    match cli.command {
        Command::Login(args) => login(args).await?,
        Command::Logout(args) => logout(args).await?,
        Command::Discover(args) => discover(args).await?,
        Command::Isp(args) => isp(args).await?,
    }

    Ok(())
}

fn init_tracing(verbose: u8) {
    let default_filter = match verbose {
        0 => "off",
        1 => "meirs_cli=info,meirs_core=info",
        2 => "meirs_cli=debug,meirs_core=debug",
        _ => "meirs_cli=trace,meirs_core=trace",
    };

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .without_time()
        .compact()
        .init();
}

fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Login(_) => "login",
        Command::Logout(_) => "logout",
        Command::Discover(_) => "discover",
        Command::Isp(_) => "isp",
    }
}

fn version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();

    VERSION
        .get_or_init(|| {
            format!(
                "{} ({} {} {})",
                build::PKG_VERSION,
                build::SHORT_COMMIT,
                build_date(),
                build::BUILD_TARGET
            )
        })
        .as_str()
}

fn build_date() -> &'static str {
    build::BUILD_TIME_3339
        .split_once('T')
        .map(|(date, _)| date)
        .unwrap_or(build::BUILD_TIME)
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();

    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn print_error(error: &CliError) {
    error!(%error, "command failed");

    match error {
        CliError::PortalInfoNotFound(path) => {
            let _ = log::error(format!("Portal info not found: {}", path.display()));
            let _ =
                log::info("Run `meirs discover --save` first, or specify portal info manually.");
        }
        _ => {
            let _ = log::error(capitalize_first(&error.to_string()));
        }
    }

    let _ = outro("Wipeout! Please check the error above.");
}

async fn login(args: LoginArgs) -> Result<(), CliError> {
    intro("Login")?;

    let spinner = spinner();
    spinner.start("Preparing for login...");

    let LoginArgs {
        account,
        password_stdin,
        portal_url,
        user_ip,
        local_addr,
    } = args;

    ensure_login_can_prompt(&account, password_stdin)?;

    info!("starting login command");
    debug!(
        has_portal_url = portal_url.is_some(),
        has_user_ip = user_ip.is_some(),
        has_local_addr = local_addr.is_some(),
        "resolving login portal info"
    );
    let portal_info = resolve_portal_info(portal_url, user_ip)?;
    let client = EPortalClient::new(portal_info.server_url, portal_info.user_ip, local_addr)
        .map_err(EPortalError::from)?;
    spinner.stop("Preparation complete");

    let account = match account {
        Some(account) => account,
        None => {
            let isp_info = client.get_isp_info().await?;
            prompt_account(&isp_info)?
        }
    };

    let password = if password_stdin {
        read_password_stdin()?
    } else {
        prompt_password()?
    };

    let spinner = cliclack::spinner();
    spinner.start("Logging in...");
    match client.login(&account, &password).await {
        Ok(_) => spinner.stop("Logged in"),

        Err(EPortalError::AlreadyOnline) => spinner.stop(format!(
            "Login skipped \nIP: {} is already online.",
            portal_info.user_ip
        )),

        Err(error) => return Err(CliError::from(error)),
    };

    outro("You're online. Surf the open Internet!")?;
    Ok(())
}

async fn logout(args: LogoutArgs) -> Result<(), CliError> {
    intro("Logout")?;

    let spinner = spinner();
    spinner.start("Preparing for logout...");

    let LogoutArgs {
        account: _,
        portal_url,
        user_ip,
        local_addr,
    } = args;

    info!("starting logout command");
    debug!(
        has_portal_url = portal_url.is_some(),
        has_user_ip = user_ip.is_some(),
        has_local_addr = local_addr.is_some(),
        "resolving logout portal info"
    );

    let portal_info = resolve_portal_info(portal_url, user_ip)?;
    let client = EPortalClient::new(portal_info.server_url, portal_info.user_ip, local_addr)
        .map_err(EPortalError::from)?;
    spinner.stop("Preparation complete");

    //let account = match account {
    //    Some(account) => account,
    //    None => {
    //        let isp_info = client.get_isp_info().await?;
    //        prompt_account(&isp_info)?
    //    }
    //};
    let spinner = cliclack::spinner();
    spinner.start("Logging out...");
    client.logout(None).await?;
    spinner.stop("Logged out");

    outro("Back to shore. See you next time!")?;
    Ok(())
}

async fn discover(args: DiscoverArgs) -> Result<(), CliError> {
    intro("Discover")?;

    info!(save = args.save, "starting discover command");

    let spin = spinner();
    spin.start("Discovering portal server...");
    let portal_info = discover_portal_info().await?;
    spin.stop("Portal server discovered");

    print_portal_info(&portal_info)?;

    log::success("Portal discovered")?;
    if args.save {
        let path = save_portal_info(&portal_info)?;
        log::info(format!("Saved portal info \n{} ", path.display()))?;
    }

    outro("Gateway found. Ready to surf!")?;
    Ok(())
}

async fn isp(args: IspArgs) -> Result<(), CliError> {
    match args.command {
        IspCommand::List(args) => list_isp(args).await?,
    }

    Ok(())
}

async fn list_isp(args: IspListArgs) -> Result<(), CliError> {
    intro("List ISPs")?;

    info!("starting ISP list command");
    debug!(
        has_portal_url = args.portal_url.is_some(),
        has_user_ip = args.user_ip.is_some(),
        has_local_addr = args.local_addr.is_some(),
        "resolving ISP portal info"
    );
    let spin = spinner();
    spin.start("Getting ISP info...");
    let portal_info = resolve_portal_info(args.portal_url, args.user_ip)?;
    let client = EPortalClient::new(portal_info.server_url, portal_info.user_ip, args.local_addr)
        .map_err(EPortalError::from)?;
    let isp_info = client.get_isp_info().await?;
    spin.stop("ISP info retrieved");

    print_isp_info(&isp_info)?;
    log::success("ISPs listed")?;
    outro("Providers on deck.")?;
    Ok(())
}

fn resolve_portal_info(
    portal_url: Option<Url>,
    user_ip: Option<IpAddr>,
) -> Result<PortalInfoFile, CliError> {
    match (portal_url, user_ip) {
        (Some(server_url), Some(user_ip)) => {
            debug!("using portal info from command arguments");

            Ok(PortalInfoFile {
                auth_url: server_url.clone(),
                server_url,
                user_ip,
            })
        }
        (portal_url, user_ip) => {
            debug!("loading saved portal info");
            let mut portal_info = load_portal_info()?;
            if let Some(portal_url) = portal_url {
                debug!("overriding saved portal URL from command arguments");
                portal_info.server_url = portal_url;
            }
            if let Some(user_ip) = user_ip {
                debug!("overriding saved user IP from command arguments");
                portal_info.user_ip = user_ip;
            }
            Ok(portal_info)
        }
    }
}

fn save_portal_info(info: &PortalInfo) -> Result<PathBuf, CliError> {
    let path = portal_info_path()?;
    debug!(path = %path.display(), "saving portal info");

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(&PortalInfoFile::from(info))?;
    fs::write(&path, content)?;
    Ok(path)
}

fn load_portal_info() -> Result<PortalInfoFile, CliError> {
    let path = portal_info_path()?;
    debug!(path = %path.display(), "loading portal info");

    if !path.try_exists()? {
        return Err(CliError::PortalInfoNotFound(path));
    }

    let content = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content)?)
}

fn portal_info_path() -> Result<PathBuf, CliError> {
    let base_dirs = BaseDirs::new().ok_or(CliError::ConfigDirUnavailable)?;
    Ok(base_dirs
        .config_dir()
        .join("meirs")
        .join("portal-info.json"))
}

fn print_portal_info(portal_info: &PortalInfo) -> Result<(), CliError> {
    let mut portal_table = Table::new(vec![portal_info]);
    portal_table.with(Style::modern());
    Ok(log::info(format!("{}", portal_table))?)
}

fn print_isp_info(isp_info: &[IspInfo]) -> Result<(), CliError> {
    ensure_isp_info_available(isp_info)?;

    let mut isp_info_table = Table::new(isp_info);
    isp_info_table.with(Style::modern());
    log::info(format!("{}", isp_info_table))?;
    log::info("Usage: <account>[suffix] \ne.g. 202611451419@cmcc")?;
    Ok(())
}
