use std::io::{IsTerminal, Read};

use cliclack::{input, password, select};
use meirs_core::IspInfo;

use crate::error::CliError;

pub(crate) fn ensure_login_can_prompt(
    account: &Option<String>,
    password_stdin: bool,
) -> Result<(), CliError> {
    let Some(options) = missing_login_options(account, password_stdin) else {
        return Ok(());
    };

    require_prompt_terminal("login", options)
}

pub(crate) fn prompt_account(isp_info: &[IspInfo]) -> Result<String, CliError> {
    ensure_isp_info_available(isp_info)?;

    let items: Vec<_> = isp_info
        .iter()
        .enumerate()
        .map(|(index, isp)| (index, isp.name.as_str(), isp.suffix.as_str()))
        .collect();

    let selected_index = select("Select your ISP").items(&items).interact()?;

    let selected_isp = &isp_info[selected_index];
    let raw_account: String = input("Account")
        .placeholder("e.g. 202611451419, without ISP suffix")
        .validate(|input: &String| validate_raw_account(input))
        .interact()?;

    Ok(raw_account + selected_isp.suffix.as_str())
}

pub(crate) fn prompt_password() -> Result<String, CliError> {
    Ok(password("Password").mask('▪').interact()?)
}

pub(crate) fn read_password_stdin() -> Result<String, CliError> {
    let mut password = String::new();
    std::io::stdin().read_to_string(&mut password)?;
    Ok(password.trim_end_matches(['\r', '\n']).to_owned())
}

pub(crate) fn ensure_isp_info_available(isp_info: &[IspInfo]) -> Result<(), CliError> {
    if isp_info.is_empty() {
        return Err(CliError::IspInfoNotFound);
    }

    Ok(())
}

fn missing_login_options(account: &Option<String>, password_stdin: bool) -> Option<&'static str> {
    match (account.is_none(), !password_stdin) {
        (true, true) => Some("--account and --password-stdin"),
        (true, false) => Some("--account"),
        (false, true) => Some("--password-stdin"),
        (false, false) => None,
    }
}

fn require_prompt_terminal(command: &'static str, options: &'static str) -> Result<(), CliError> {
    if std::io::stderr().is_terminal() {
        return Ok(());
    }

    Err(CliError::NonInteractiveMissingOptions { command, options })
}

fn validate_raw_account(input: &str) -> Result<(), &'static str> {
    if input.contains('@') {
        Err("Enter account only, without ISP suffix")
    } else if input.is_empty() {
        Err("Account cannot be empty")
    } else if !input.chars().all(|c| c.is_ascii_digit()) {
        Err("Account must contain digits only")
    } else {
        Ok(())
    }
}
