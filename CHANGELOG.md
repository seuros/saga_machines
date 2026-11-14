# Changelog

## [0.3.0](https://github.com/seuros/saga_machines/compare/saga_machines/v0.2.0...saga_machines/v0.3.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* read ids, actions and snapshots natively by exact class, issue next_action clone-then-keep

### Features

* read ids, actions and snapshots natively by exact class, issue next_action clone-then-keep ([f670ac0](https://github.com/seuros/saga_machines/commit/f670ac0abc79ebaa71224fc5d1f5c4162709e36b))

## [0.2.0](https://github.com/seuros/saga_machines/compare/saga_machines/v0.1.5...saga_machines/v0.2.0) (2026-11-13)


### ⚠ BREAKING CHANGES

* converted to the fleet conventions. One exhaustive Refusal per crate, Result from every operation that can refuse, a refusal changes nothing, provable idempotent repeats are honoured, Ruby mirrors the core. No compatibility aliases. Requires dedup-machines >= 0.5. Saga::new takes a SagaId; claim is claim_in; restore is restored(snapshot, scope) and refuses old-style snapshots.

### Features

* one Refusal, scoped attempt ids, honoured repeats and a restore that accepts exactly the reachable snapshots ([1b71528](https://github.com/seuros/saga_machines/commit/1b715281fc8866b87d7086f1eeba9589aca16289))

## [0.1.5](https://github.com/seuros/saga_machines/compare/saga_machines/v0.1.4...saga_machines/v0.1.5) (2026-11-06)


### Bug Fixes

* build against dedup-machines 0.3 ([fa36da9](https://github.com/seuros/saga_machines/commit/fa36da96c3aab595ff4daaa421a1f3e0d7c02fe9))
* require dedup-machines 0.3 ([e192ae3](https://github.com/seuros/saga_machines/commit/e192ae3fb7c33882826dfc56c3582cd2e685e6a3))

## [0.1.4](https://github.com/seuros/saga_machines/compare/saga_machines/v0.1.3...saga_machines/v0.1.4) (2026-11-06)


### Features

* restore a saga from persisted progress ([6c98ddd](https://github.com/seuros/saga_machines/commit/6c98ddd8fc44b880dc8bf195f8d5a8e32048b423))

## [0.1.3](https://github.com/seuros/saga_machines/compare/saga_machines/v0.1.2...saga_machines/v0.1.3) (2026-11-06)


### Bug Fixes

* crates.io keywords and categories for the core ([660b649](https://github.com/seuros/saga_machines/commit/660b649c5b0229ab715571ecb0b97270dcaacfab))

## [0.1.2](https://github.com/seuros/saga_machines/compare/saga_machines/v0.1.1...saga_machines/v0.1.2) (2026-11-06)


### Bug Fixes

* keep test sources out of the packaged gem and crate ([38c7ba8](https://github.com/seuros/saga_machines/commit/38c7ba8523f9ad783b258df3df15e2e2b5f3b6a0))

## [0.1.1](https://github.com/seuros/saga_machines/compare/saga_machines-v0.1.0...saga_machines/v0.1.1) (2026-11-06)


### Features

* initial extraction ([5c83b4d](https://github.com/seuros/saga_machines/commit/5c83b4d32f62e4e8074f5cf1e08f9756bbc3ccfe))


### Bug Fixes

* depend on the published dedup-machines crate ([d3dc0aa](https://github.com/seuros/saga_machines/commit/d3dc0aaee5b40d7e969ab8973aac098c67b3411d))
* name the gemspec block spec so the release workflow detects the extension ([fe76b8d](https://github.com/seuros/saga_machines/commit/fe76b8d64e1ba6677df891aa9037a57b030c64c9))
