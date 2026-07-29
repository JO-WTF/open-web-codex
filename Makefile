.PHONY: mvp deploy deploy-status deploy-stop web-install web-check web-test web-rust-test codex-test contracts-check cargo-cache-status cargo-target-status cargo-target-gc build-cache-test codex-upstream-status codex-upstream-sync

mvp:
	./scripts/start-all.sh

deploy:
	./scripts/deploy.sh

deploy-status:
	./scripts/deploy.sh --status

deploy-stop:
	./scripts/deploy.sh --stop

web-install:
	cd apps/web && npm ci

web-check:
	cd apps/web && npm run typecheck && npm run check:codex-contracts

web-test:
	cd apps/web && npm test

web-rust-test:
	./scripts/test-web-rust.sh

codex-test:
	./scripts/test-codex.sh

contracts-check:
	cd apps/web && npm run test:codex-capabilities && npm run test:codex-fixtures && npm run test:codex-harness

cargo-cache-status:
	./scripts/cargo-build-cache-status.sh

cargo-target-status:
	./scripts/cargo-target-gc.sh --status

cargo-target-gc:
	./scripts/cargo-target-gc.sh

build-cache-test:
	./scripts/tests/cargo-build-retention.sh

codex-upstream-status:
	./scripts/codex-upstream-status.sh

codex-upstream-sync:
	./scripts/sync-codex-upstream.sh --apply
