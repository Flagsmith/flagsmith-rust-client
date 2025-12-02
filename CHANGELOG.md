# Changelog

## [2.1.0](https://github.com/Flagsmith/flagsmith-rust-client/compare/v2.0.0...v2.1.0) (2025-12-02)


### Features

* add User-Agent header to all outbound HTTP requests ([#37](https://github.com/Flagsmith/flagsmith-rust-client/issues/37)) ([8606c34](https://github.com/Flagsmith/flagsmith-rust-client/commit/8606c34ce2ddfff0ff02b11e88bcdce706b11566))
* migrate to new flag engine with context value support ([#39](https://github.com/Flagsmith/flagsmith-rust-client/issues/39)) ([19a235b](https://github.com/Flagsmith/flagsmith-rust-client/commit/19a235b5a7e5f0bdb93a12c3b8be84f036aadcf2))


### CI

* add release please configuration ([#27](https://github.com/Flagsmith/flagsmith-rust-client/issues/27)) ([1656952](https://github.com/Flagsmith/flagsmith-rust-client/commit/16569521e20bafdec1e9889da1f57c3fd5d78687))


### Docs

* removing hero image from SDK readme ([#29](https://github.com/Flagsmith/flagsmith-rust-client/issues/29)) ([2b60f22](https://github.com/Flagsmith/flagsmith-rust-client/commit/2b60f22ce1b61262de4b17c7e6e253d7d317fe77))


### Dependency Updates

* bump flagsmith-flag-engine to 0.5.1 ([#41](https://github.com/Flagsmith/flagsmith-rust-client/issues/41)) ([2f8fe7e](https://github.com/Flagsmith/flagsmith-rust-client/commit/2f8fe7e807592ca8d862f27f511eb1ef145fefc8))


### Other

* add root CODEOWNERS ([#35](https://github.com/Flagsmith/flagsmith-rust-client/issues/35)) ([288364a](https://github.com/Flagsmith/flagsmith-rust-client/commit/288364a294538630f81c560cc8a6b9bdb01521ab))

<a id="v2.0.0"></a>
## [v2.0.0](https://github.com/Flagsmith/flagsmith-rust-client/releases/tag/v2.0.0) - 2024-10-22

## What's Changed
* feat!: Support transient identities and traits by [@khvn26](https://github.com/khvn26) in [#23](https://github.com/Flagsmith/flagsmith-rust-client/pull/23)

**Full Changelog**: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.5.0...v2.0.0

[Changes][v2.0.0]


<a id="v1.5.0"></a>
## [v1.5.0](https://github.com/Flagsmith/flagsmith-rust-client/releases/tag/v1.5.0) - 2024-04-19

## What's Changed
* feat: Identity overrides in local evaluation mode by [@khvn26](https://github.com/khvn26) in [#20](https://github.com/Flagsmith/flagsmith-rust-client/pull/20)
* chore: remove examples by [@dabeeeenster](https://github.com/dabeeeenster) in [#19](https://github.com/Flagsmith/flagsmith-rust-client/pull/19)

## New Contributors
* [@dabeeeenster](https://github.com/dabeeeenster) made their first contribution in [#19](https://github.com/Flagsmith/flagsmith-rust-client/pull/19)

**Full Changelog**: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.4.0...v1.5.0

[Changes][v1.5.0]


<a id="v1.4.0"></a>
## [v1.4.0](https://github.com/Flagsmith/flagsmith-rust-client/releases/tag/v1.4.0) - 2024-01-30

## What's Changed
* feat(offline-mode): Add support for offline handler by [@gagantrivedi](https://github.com/gagantrivedi) in [#16](https://github.com/Flagsmith/flagsmith-rust-client/pull/16)


**Full Changelog**: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.3.0...v1.4.0

[Changes][v1.4.0]


<a id="v1.3.0"></a>
## [v1.3.0](https://github.com/Flagsmith/flagsmith-rust-client/releases/tag/v1.3.0) - 2023-07-20

## What's Changed
* feat: bump flagsmith-flag-engine to enable `IN` operator by [@khvn26](https://github.com/khvn26) in [#15](https://github.com/Flagsmith/flagsmith-rust-client/pull/15)

## New Contributors
* [@khvn26](https://github.com/khvn26) made their first contribution in [#15](https://github.com/Flagsmith/flagsmith-rust-client/pull/15)

**Full Changelog**: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.2.0...v1.3.0

[Changes][v1.3.0]


<a id="v1.2.0"></a>
## [v1.2.0](https://github.com/Flagsmith/flagsmith-rust-client/releases/tag/v1.2.0) - 2022-10-20

## What's Changed
* chore: bump flag-engine to support new segment operators by [@gagantrivedi](https://github.com/gagantrivedi) in [#14](https://github.com/Flagsmith/flagsmith-rust-client/pull/14)
* Release v1.2.0 by [@gagantrivedi](https://github.com/gagantrivedi) in [#13](https://github.com/Flagsmith/flagsmith-rust-client/pull/13)


**Full Changelog**: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.1.0...v1.2.0

[Changes][v1.2.0]


<a id="v1.1.0"></a>
## [v1.1.0](https://github.com/Flagsmith/flagsmith-rust-client/releases/tag/v1.1.0) - 2022-10-03

## What's Changed
* Update API url by [@matthewelwell](https://github.com/matthewelwell) in [#10](https://github.com/Flagsmith/flagsmith-rust-client/pull/10)
* fix(analytics): use feature name instead of feature id by [@gagantrivedi](https://github.com/gagantrivedi) in [#9](https://github.com/Flagsmith/flagsmith-rust-client/pull/9)
* feat(flagmith): make client <Send + Sync> by [@gagantrivedi](https://github.com/gagantrivedi) in [#11](https://github.com/Flagsmith/flagsmith-rust-client/pull/11)
* Release/v1.1.0 by [@gagantrivedi](https://github.com/gagantrivedi) in [#12](https://github.com/Flagsmith/flagsmith-rust-client/pull/12)


**Full Changelog**: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.0.1...v1.1.0

[Changes][v1.1.0]


<a id="v1.0.1"></a>
## [v1.0.1](https://github.com/Flagsmith/flagsmith-rust-client/releases/tag/v1.0.1) - 2022-10-03

## What's Changed
* patch release v1.0.1 by [@gagantrivedi](https://github.com/gagantrivedi) in [#8](https://github.com/Flagsmith/flagsmith-rust-client/pull/8)


**Full Changelog**: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.0.0...v1.0.1

[Changes][v1.0.1]


<a id="v1.0.0"></a>
## [v1.0.0](https://github.com/Flagsmith/flagsmith-rust-client/releases/tag/v1.0.0) - 2022-06-07

## What's Changed
* Allow null feature type by [@matthewelwell](https://github.com/matthewelwell) in [#1](https://github.com/Flagsmith/flagsmith-rust-client/pull/1)
* Rebrand: bullettrain to flagsmith by [@gagantrivedi](https://github.com/gagantrivedi) in [#2](https://github.com/Flagsmith/flagsmith-rust-client/pull/2)
* ci(deploy-workflow): Add workflow to publish crate on crates.io by [@gagantrivedi](https://github.com/gagantrivedi) in [#5](https://github.com/Flagsmith/flagsmith-rust-client/pull/5)
* Release 1.0.0 by [@gagantrivedi](https://github.com/gagantrivedi) in [#4](https://github.com/Flagsmith/flagsmith-rust-client/pull/4)

## New Contributors
* [@matthewelwell](https://github.com/matthewelwell) made their first contribution in [#1](https://github.com/Flagsmith/flagsmith-rust-client/pull/1)

**Full Changelog**: https://github.com/Flagsmith/flagsmith-rust-client/commits/v1.0.0

[Changes][v1.0.0]


[v2.0.0]: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.5.0...v2.0.0
[v1.5.0]: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.4.0...v1.5.0
[v1.4.0]: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.3.0...v1.4.0
[v1.3.0]: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.2.0...v1.3.0
[v1.2.0]: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.1.0...v1.2.0
[v1.1.0]: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.0.1...v1.1.0
[v1.0.1]: https://github.com/Flagsmith/flagsmith-rust-client/compare/v1.0.0...v1.0.1
[v1.0.0]: https://github.com/Flagsmith/flagsmith-rust-client/tree/v1.0.0

<!-- Generated by https://github.com/rhysd/changelog-from-release v3.9.0 -->
