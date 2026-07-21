.DEFAULT_GOAL := install

FORK_REPO   ?= https://github.com/AceMythos/kdeconnect-fork.git
FORK_BRANCH ?= v23.08.5-patched
FORK_DIR    ?= build/kdeconnect-fork

NOW := $(shell date +%s)

deps:
	@echo "=== Installing build dependencies ==="
	sudo apt-get install -y cmake extra-cmake-modules libkf5kio-dev \
		libkf5notifications-dev libkf5dbusaddons-dev libkf5config-dev \
		libkf5coreaddons-dev libkf5i18n-dev qtbase5-dev qttools5-dev \
		git build-essential wl-clipboard
	@if ! command -v cargo >/dev/null 2>&1; then \
		echo "=== Installing Rust ==="; \
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y; \
	fi
	@echo "=== Dependencies ready ==="

install: build-fork install-fork build-applet install-applet restart-daemon
	@__end=$$(date +%s); __elapsed=$$((__end - $(NOW))); \
	__min=$$((__elapsed / 60)); __sec=$$((__elapsed % 60)); \
	echo "=== Done ($${__min}m $${__sec}s) ==="

build-fork:
	@__start=$$(date +%s); \
	echo "=== [1/4] Building KDE Connect fork (~2-3 min) ==="; \
	if [ ! -d "$(FORK_DIR)" ]; then \
		mkdir -p build && \
		git clone --progress --branch $(FORK_BRANCH) $(FORK_REPO) $(FORK_DIR); \
	fi; \
	mkdir -p $(FORK_DIR)/build; \
	cd $(FORK_DIR)/build && cmake .. -DCMAKE_INSTALL_PREFIX=/usr && make -j$$(nproc); \
	__end=$$(date +%s); \
	echo "  -> Done ($$((__end - __start))s)"

install-fork:
	@__start=$$(date +%s); \
	echo "=== [2/4] Installing KDE Connect fork ==="; \
	sudo cmake --install $(FORK_DIR)/build; \
	__end=$$(date +%s); \
	echo "  -> Done ($$((__end - __start))s)"

build-applet:
	@__start=$$(date +%s); \
	echo "=== [3/4] Building applet (cargo ~2 min) ==="; \
	cargo build --release; \
	__end=$$(date +%s); \
	echo "  -> Done ($$((__end - __start))s)"

install-applet:
	@__start=$$(date +%s); \
	echo "=== [4/4] Installing applet ==="; \
	pkexec install -m 755 $(CURDIR)/target/release/cosmic-connect /usr/bin/cosmic-connect; \
	pkexec install -m 644 $(CURDIR)/io.github.acemythos.Connect.desktop /usr/share/applications/; \
	__end=$$(date +%s); \
	echo "  -> Done ($$((__end - __start))s)"

restart-daemon:
	@echo "=== Restarting daemon ==="; \
	-sudo killall kdeconnectd 2>/dev/null || true; \
	/usr/lib/x86_64-linux-gnu/libexec/kdeconnectd &

update: pull-fork pull-applet install

pull-fork:
	@if [ -d "$(FORK_DIR)" ]; then \
		cd $(FORK_DIR) && git pull; \
	fi

pull-applet:
	git pull

.PHONY: deps install build-fork install-fork build-applet install-applet restart-daemon update pull-fork pull-applet
