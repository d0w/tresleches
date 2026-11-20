.PHONY: dev-mesh dev-mesh-tun dev-mesh-tun-down

dev-mesh:
	@bash scripts/dev-mesh.sh

dev-mesh-tun:
	docker compose -f docker/compose.tun.yaml up --build --abort-on-container-exit

dev-mesh-tun-down:
	docker compose -f docker/compose.tun.yaml down
