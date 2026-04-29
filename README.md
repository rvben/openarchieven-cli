# openarchieven

CLI for the [openarchieven.nl](https://www.openarchieven.nl/) Dutch genealogical
API. Designed for humans, scripts, and AI agents per [clispec.dev](https://clispec.dev).

## Install

```bash
cargo install openarchieven
# or
brew install rvben/tap/openarchieven
# or
uvx openarchieven --help
```

## Usage

```bash
openarchieven search "Jan Janssen" --limit 10
openarchieven show beeldbank-amsterdam abc-123
openarchieven schema | jq
```

See `openarchieven schema` for the full machine-readable contract.

## License

MIT
