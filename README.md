# Meirs

An extremely fast network authentication tool for Zhengzhou University.

## Usage

Discover and save portal information first:

```bash
meirs discover --save
```

Log in and log out:

```bash
meirs login --account <account> --password <password>
meirs logout --account <account>
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
