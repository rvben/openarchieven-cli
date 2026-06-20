# Changelog

All notable changes to this project will be documented in this file.





## [0.4.0](https://github.com/rvben/openarchieven-cli/compare/v0.3.0...v0.4.0) - 2026-05-11

### Added

- agentic output optimizations — compact JSON, ndjson, nested --fields ([0449504](https://github.com/rvben/openarchieven-cli/commit/0449504d0cc81e21d23d06eb7481e743040b0725))
- **openapi**: vendor spec, add drift check, align params, add stats breakdown ([8ffab03](https://github.com/rvben/openarchieven-cli/commit/8ffab03e3103532babb18f1e7f5d40a4f529b9c4))

## [0.3.0](https://github.com/rvben/openarchieven-cli/compare/v0.2.1...v0.3.0) - 2026-04-30

### Added

- **transcripts**: add `transcripts {search,browse,show}` subcommands ([7817c59](https://github.com/rvben/openarchieven-cli/commit/7817c5932c7ac66091999e359da23d841d8feae5))

## [0.2.1](https://github.com/rvben/openarchieven-cli/compare/v0.2.0...v0.2.1) - 2026-04-30

### Added

- **cache**: cache immutable lookups forever, extend archives ttl ([beb0523](https://github.com/rvben/openarchieven-cli/commit/beb0523b2ef5cd04f9fd045148eb550ef95cbe4d))

## [0.2.0](https://github.com/rvben/openarchieven-cli/compare/v0.1.0...v0.2.0) - 2026-04-30

### Added

- **cli**: surface silent truncation, group api flags, harden test fixture ([534c0c7](https://github.com/rvben/openarchieven-cli/commit/534c0c7134106813b46662061acb6331a6a0cbe1))
- **cli**: make api flags work before or after the subcommand ([cb172d7](https://github.com/rvben/openarchieven-cli/commit/cb172d7dd6baf75fe220c5ae4c17b2e90233d2ee))
- **cli**: ship 'oa' as a short alias for openarchieven ([db3bd39](https://github.com/rvben/openarchieven-cli/commit/db3bd392a365bb1dda3d5e4c5e82b16d7206d811))
- **schema**: label integer-coded enums (event-type, sort, richness) ([814092e](https://github.com/rvben/openarchieven-cli/commit/814092e299c327e4cf0a38119a1341e09476df02))
- **yearsago**: echo resolved date to stderr unless --quiet ([03b2222](https://github.com/rvben/openarchieven-cli/commit/03b2222546cf0f1fb751515e0021b8d7dff4b4c5))
- **output**: humanise eventdate and personname cells ([99cbe35](https://github.com/rvben/openarchieven-cli/commit/99cbe359434a51e54ddd908a3caf5bf44120f7b4))
- **cli**: typed Args and examples for all seven stats subcommands ([37373e4](https://github.com/rvben/openarchieven-cli/commit/37373e4545b1669577eed22050ced720f88340a0))
- **cli**: typed Args + examples for show ([2704dfe](https://github.com/rvben/openarchieven-cli/commit/2704dfeab8f8495b4ab7ab8ef0a0f01b557e0477))
- **cli**: typed Args + examples for weather ([080749d](https://github.com/rvben/openarchieven-cli/commit/080749d34fa738bb950c3e787ae966d23e3ea7a4))
- **cli**: typed Args + examples for census ([666bdf7](https://github.com/rvben/openarchieven-cli/commit/666bdf754290f79eb8a5a19a40f79f4c61a8fee7))
- **cli**: typed Args + examples for archives ([8e512d9](https://github.com/rvben/openarchieven-cli/commit/8e512d93e5631fef7b223ceee0f830d90a4231b4))
- **cli**: typed Args + examples for yearsago ([579bfc1](https://github.com/rvben/openarchieven-cli/commit/579bfc1cbfcd1f0509d1bffbf6142c1f82a8a31f))
- **cli**: typed Args + examples for match ([1202ca8](https://github.com/rvben/openarchieven-cli/commit/1202ca857c9f8209f605af0284523c5ce64764da))
- **cli**: typed Args + examples for search ([496051e](https://github.com/rvben/openarchieven-cli/commit/496051e04183c47e1bbfca7cac800e38c982b568))
- **cli**: typed Args + examples for marriages ([f5a7e37](https://github.com/rvben/openarchieven-cli/commit/f5a7e370c290f8d7c7784292c9d5c1f3109783a6))
- **cli**: typed Args + examples for deaths ([5a3b830](https://github.com/rvben/openarchieven-cli/commit/5a3b830a7305234a78853d6c2ee31e7fdc48a4e7))
- **cli**: typed clap::Args for births with examples in --help ([5e01e9d](https://github.com/rvben/openarchieven-cli/commit/5e01e9de4ff83796f6b2d5e4d8a4461aa28ce4fe))

### Fixed

- **cli**: honour NO_COLOR per the no-color.org spec ([9953812](https://github.com/rvben/openarchieven-cli/commit/995381291cb27d833fdc3ffc393bd45616d382b8))
- **events**: post-filter --event-year client-side ([3328641](https://github.com/rvben/openarchieven-cli/commit/332864194c875669c8b3d172bde6b4be2641f445))
- **show**: translate upstream {error_code, error_description} to NotFound ([84d5d9d](https://github.com/rvben/openarchieven-cli/commit/84d5d9d523c85d4be2b5b3d03d6b134c87034b68))
- **weather**: decode upstream array response as list shape ([7a4778a](https://github.com/rvben/openarchieven-cli/commit/7a4778a28717ff7155fbfdac3cb8d2a3666105d9))
- **stats**: firstnames decodes /response/firstnames pointer ([f5f758f](https://github.com/rvben/openarchieven-cli/commit/f5f758fe957015d0fefaceba9ebf13ec02a51921))
- **stats**: firstnames sends eventplace, not place ([838599c](https://github.com/rvben/openarchieven-cli/commit/838599c3fecbdd72a0638dbfb6c0f819782cb7f4))
- **stats**: firstnames sends eventyear, not year ([aed1d70](https://github.com/rvben/openarchieven-cli/commit/aed1d7039d89b314f9ec2be9b8bc0cdeeb1101f8))
- **stats**: decode familynames Google-Charts cols/rows shape ([09e904a](https://github.com/rvben/openarchieven-cli/commit/09e904a4868fce9236ace32ab9f39d376b783121))
- **stats**: decode bare-array responses for records/sources/events/comments ([f2fd2af](https://github.com/rvben/openarchieven-cli/commit/f2fd2af09b91eada5e1adb309f4d85118ce3aa73))
- **archives**: decode upstream bare-array response ([a3a6497](https://github.com/rvben/openarchieven-cli/commit/a3a6497f19ff230b884e042c7b2d444662e75459))

## [Unreleased]

## [0.4.3](https://github.com/rvben/openarchieven-cli/compare/v0.4.2...v0.4.3) - 2026-06-20

### Added

- **schema**: add output_field and arg descriptions throughout ([c1650cd](https://github.com/rvben/openarchieven-cli/commit/c1650cd1417e35671b1f99bc909e74f8ea4b5e31))



## [0.4.2](https://github.com/rvben/openarchieven-cli/compare/v0.4.1...v0.4.2) - 2026-06-20

### Added

- **schema**: adopt clispec v0.2 contract ([1af9813](https://github.com/rvben/openarchieven-cli/commit/1af9813697224bcd31d1638e7da10beff3605846))



## [0.4.1](https://github.com/rvben/openarchieven-cli/compare/v0.4.0...v0.4.1) - 2026-06-11

### Added

- achieve clispec v0.2 full compliance (25/25) ([f218862](https://github.com/rvben/openarchieven-cli/commit/f21886241bddea3eb18be0f110e7eec44f76427f))



## [0.1.0] - 2026-04-30

Initial release.

### Added
- All 17 endpoints of the openarchieven.nl API v1.1: `archives`, `search`,
  `show`, `match`, `births`, `deaths`, `marriages`, `yearsago`, `census`,
  `weather`, and the seven `stats` subcommands.
- Three output formats (`json`, `table`, `markdown`) selected automatically by
  TTY/pipe context, overridable with `--output/-o`.
- `schema` subcommand emits a byte-stable, machine-readable contract for
  scripts and AI agents.
- On-disk response cache with advisory locking and per-endpoint TTLs;
  `cache info`, `cache prune`, and `cache clear --yes` for management.
- Built-in rate limiter (4 req/sec default) and retry with jittered backoff
  that respects `Retry-After`.
- Structured JSON error contract on stderr with stable `kind` enum.
- `--fields` projection for list and single-flat responses.
- Distribution: crates.io, PyPI wheel, and Homebrew tap, all published from
  one tag-driven release workflow.
