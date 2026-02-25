.PHONY: safety-gate

safety-gate:
	./scripts/safety-quality-gate.sh

deploy:
	@bash scripts/block-local-deploy.sh

deploy-prod:
	@bash scripts/block-local-deploy.sh

fly:
	@bash scripts/block-local-deploy.sh
