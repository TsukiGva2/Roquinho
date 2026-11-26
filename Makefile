PREFIX ?= $(HOME)/.local
BIN    := $(PREFIX)/bin/foco-max
TARGET := target/release/foco-max

.PHONY: all build install restart deploy stop test fmt lint check clean uninstall

all: build

build:
	cargo build --release

# Atomic replace: safe even while foco-max is running (the old process keeps the old code).
install: build
	install -Dm755 $(TARGET) $(BIN)

# Stop the running instance (if any) and start the installed binary detached.
restart: stop
	setsid -f $(BIN) start >/dev/null 2>&1

# Build, install and restart in one go.
deploy: install restart

stop:
	-pkill -x foco-max
	@while pgrep -x foco-max >/dev/null; do sleep 0.1; done

test:
	cargo test --workspace

fmt:
	cargo fmt --all

lint:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings

# Same as CI.
check: lint test

clean:
	cargo clean

uninstall: stop
	rm -f $(BIN)
