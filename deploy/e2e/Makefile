# Blind-relay e2e harness. Requires Docker running.
#   make up    — build + start relay + box
#   make test  — run the reach / anti-bypass / liveness scenarios
#   make logs  — follow logs
#   make down  — stop + remove
.PHONY: up test down logs

up:
	docker compose up -d --build

test:
	./run-tests.sh

logs:
	docker compose logs -f

down:
	docker compose down -v
