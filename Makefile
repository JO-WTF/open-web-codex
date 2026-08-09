.PHONY: mvp deploy deploy-status deploy-stop web-install web-check web-test web-rust-test codex-test contracts-check cargo-cache-status deploy-policy-test codex-upstream-status codex-upstream-sync

mvp:
	./scripts/run-local.sh --background

deploy:
	./scripts/deploy.sh

deploy-status:
	./scripts/deploy.sh --status

deploy-stop:
	./scripts/deploy.sh --stop

web-install:
	cd apps/web && npm ci

web-check:
	cd apps/web && npm run typecheck

web-test:
	cd apps/web && npm test

web-rust-test:
	./scripts/test-web-rust.sh

codex-test:
	./scripts/test-codex.sh

contracts-check:
	cd apps/web && npm run test:codex-harness

cargo-cache-status:
	./scripts/cargo-build-cache-status.sh

deploy-policy-test:
	./scripts/tests/deploy-policy.sh

codex-upstream-status:
	./scripts/codex-upstream-status.sh

codex-upstream-sync:
	./scripts/sync-codex-upstream.sh --apply
