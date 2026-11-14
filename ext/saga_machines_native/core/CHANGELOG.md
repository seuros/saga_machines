# Changelog

## [0.6.0](https://github.com/seuros/saga_machines/compare/saga-machines-v0.5.0...saga-machines-v0.6.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* adopt matryona-machines 0.2: exact field reads and compiler-checked symbol tables

### Features

* adopt matryona-machines 0.2: exact field reads and compiler-checked symbol tables ([6f4f73a](https://github.com/seuros/saga_machines/commit/6f4f73a37a2d9f6a48a93490d244aa7a172fb58d))

## [0.5.0](https://github.com/seuros/saga_machines/compare/saga-machines-v0.4.0...saga-machines-v0.5.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* read ids, actions and snapshots natively by exact class, issue next_action clone-then-keep

### Features

* read ids, actions and snapshots natively by exact class, issue next_action clone-then-keep ([f670ac0](https://github.com/seuros/saga_machines/commit/f670ac0abc79ebaa71224fc5d1f5c4162709e36b))

## [0.4.0](https://github.com/seuros/saga_machines/compare/saga-machines-v0.3.1...saga-machines-v0.4.0) (2026-11-13)


### ⚠ BREAKING CHANGES

* converted to the fleet conventions. One exhaustive Refusal per crate, Result from every operation that can refuse, a refusal changes nothing, provable idempotent repeats are honoured, Ruby mirrors the core. No compatibility aliases. Requires dedup-machines >= 0.5. Saga::new takes a SagaId; claim is claim_in; restore is restored(snapshot, scope) and refuses old-style snapshots.

### Features

* one Refusal, scoped attempt ids, honoured repeats and a restore that accepts exactly the reachable snapshots ([1b71528](https://github.com/seuros/saga_machines/commit/1b715281fc8866b87d7086f1eeba9589aca16289))

## [0.3.1](https://github.com/seuros/saga_machines/compare/saga-machines-v0.3.0...saga-machines-v0.3.1) (2026-11-06)


### Bug Fixes

* build against dedup-machines 0.3 ([fa36da9](https://github.com/seuros/saga_machines/commit/fa36da96c3aab595ff4daaa421a1f3e0d7c02fe9))

## [0.3.0](https://github.com/seuros/saga_machines/compare/saga-machines-v0.2.2...saga-machines-v0.3.0) (2026-11-06)


### Features

* restore a saga from persisted progress ([6c98ddd](https://github.com/seuros/saga_machines/commit/6c98ddd8fc44b880dc8bf195f8d5a8e32048b423))

## [0.2.2](https://github.com/seuros/saga_machines/compare/saga-machines-v0.2.1...saga-machines-v0.2.2) (2026-11-06)


### Bug Fixes

* crates.io keywords and categories for the core ([660b649](https://github.com/seuros/saga_machines/commit/660b649c5b0229ab715571ecb0b97270dcaacfab))

## [0.2.1](https://github.com/seuros/saga_machines/compare/saga-machines-v0.2.0...saga-machines-v0.2.1) (2026-11-06)


### Bug Fixes

* keep test sources out of the packaged gem and crate ([38c7ba8](https://github.com/seuros/saga_machines/commit/38c7ba8523f9ad783b258df3df15e2e2b5f3b6a0))

## [0.2.0](https://github.com/seuros/saga_machines/compare/saga-machines-v0.1.0...saga-machines-v0.2.0) (2026-11-06)


### Features

* initial extraction ([5c83b4d](https://github.com/seuros/saga_machines/commit/5c83b4d32f62e4e8074f5cf1e08f9756bbc3ccfe))
