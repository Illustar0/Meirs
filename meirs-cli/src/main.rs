use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;

mod error;

use crate::error::CliError;
use clap::builder::styling::AnsiColor;
use clap::builder::Styles;
use clap::{ArgAction, Args, Parser, Subcommand};
use directories::BaseDirs;
use meirs_core::{discover_portal_info, EPortalClient, EPortalError, IspInfo, PortalInfo};
use serde::{Deserialize, Serialize};
use tabled::settings::Style;
use tabled::Table;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;
use url::Url;

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
    version,
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
    #[arg(long, value_name = "ACCOUNT", help = "Portal account")]
    account: String,
    #[arg(long, value_name = "PASSWORD", help = "Portal password")]
    password: String,
    #[arg(long, value_name = "URL", help = "Portal server base URL")]
    portal_url: Option<Url>,
    #[arg(long, value_name = "IP", help = "User IP address")]
    user_ip: Option<IpAddr>,
    #[arg(long, value_name = "IP", help = "Local address for bind")]
    local_addr: Option<IpAddr>,
}


#[derive(Debug, Args)]
struct LogoutArgs {
    #[arg(long, value_name = "ACCOUNT", help = "Portal account")]
    account: String,
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

fn print_error(error: &CliError) {
    error!(%error, "command failed");

    match error {
        CliError::PortalInfoNotFound(path) => {
            eprintln!("Portal info not found: {}", path.display());
            eprintln!("Run `meirs discover --save` first, or specify portal info manually.");
        }
        _ => eprintln!("Error: {error}"),
    }
}

async fn login(args: LoginArgs) -> Result<(), CliError> {
    info!("starting login command");
    debug!(
        has_portal_url = args.portal_url.is_some(),
        has_user_ip = args.user_ip.is_some(),
        has_local_addr = args.local_addr.is_some(),
        "resolving login portal info"
    );

    let portal_info = resolve_portal_info(args.portal_url, args.user_ip)?;
    let client = EPortalClient::new(portal_info.server_url, portal_info.user_ip, args.local_addr)
        .map_err(EPortalError::from)?;
    client.login(&args.account, &args.password).await?;
    println!("Login completed");
    Ok(())
}

async fn logout(args: LogoutArgs) -> Result<(), CliError> {
    info!("starting logout command");
    debug!(
        has_portal_url = args.portal_url.is_some(),
        has_user_ip = args.user_ip.is_some(),
        has_local_addr = args.local_addr.is_some(),
        "resolving logout portal info"
    );

    let portal_info = resolve_portal_info(args.portal_url, args.user_ip)?;
    let client = EPortalClient::new(portal_info.server_url, portal_info.user_ip, args.local_addr)
        .map_err(EPortalError::from)?;
    client.logout(&args.account).await?;
    println!("Logout completed");
    Ok(())
}

async fn discover(args: DiscoverArgs) -> Result<(), CliError> {
    info!(save = args.save, "starting discover command");

    let portal_info = discover_portal_info().await?;
    print_portal_info(&portal_info);

    if args.save {
        let path = save_portal_info(&portal_info)?;
        println!("Saved portal info to {}", path.display());
    }

    Ok(())
}

async fn isp(args: IspArgs) -> Result<(), CliError> {
    match args.command {
        IspCommand::List(args) => list_isp(args).await?,
    }

    Ok(())
}


async fn list_isp(args: IspListArgs) -> Result<(), CliError> {
    info!("starting ISP list command");
    debug!(
        has_portal_url = args.portal_url.is_some(),
        has_user_ip = args.user_ip.is_some(),
        has_local_addr = args.local_addr.is_some(),
        "resolving ISP portal info"
    );

    let portal_info = resolve_portal_info(args.portal_url, args.user_ip)?;
    let client = EPortalClient::new(portal_info.server_url, portal_info.user_ip, args.local_addr)
        .map_err(EPortalError::from)?;
    let isp_info = client.get_isp_info().await?;
    print_isp_info(&isp_info);
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

fn print_portal_info(info: &PortalInfo) {
    println!("Portal server discovered");
    let mut portal_table = Table::new(vec![info]);
    portal_table.with(Style::modern_rounded());
    println!("{}", portal_table)
}

fn print_isp_info(isp_info: &[IspInfo]) {
    if isp_info.is_empty() {
        return;
    }

    let mut isp_info_table = Table::new(isp_info);
    isp_info_table.with(Style::modern_rounded());
    println!("{}", isp_info_table);
    println!("Usage: <account>[suffix]");
    println!("Example: 2026114514@cmcc");
}