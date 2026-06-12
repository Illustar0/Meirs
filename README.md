# Meirs

An extremely fast network authentication tool for Zhengzhou University.

## Usage

Discover and save portal information first:

```bash
meirs discover --save
```

Log in and log out:

```bash
meirs login
meirs logout
```

For scripts or other non-interactive use, pass the required values explicitly:

```bash
printf '%s\n' "$MEIRS_PASSWORD" | meirs login --account <account> --password-stdin
```

List available ISP suffixes:

```bash
meirs isp list
```

Add `-v`, `-vv`, or `-vvv` to any command for more detailed logs.

## Build

```bash
cargo build --release
```
