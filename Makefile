.DEFAULT_GOAL := install

FORK_REPO   ?= https://github.com/AceMythos/kdeconnect-fork.git
FORK_BRANCH ?= v23.08.5-patched
FORK_DIR    ?= build/kdeconnect-fork

install: build-fork install-fork build-applet install-applet restart-daemon
	@echo "=== Done ==="

build-fork:
	@echo "=== Cloning/building KDE Connect fork ==="
	@if [ ! -d "$(FORK_DIR)" ]; then \
		mkdir -p build && \
		git clone --branch $(FORK_BRANCH) $(FORK_REPO) $(FORK_DIR); \
	fi
	@mkdir -p $(FORK_DIR)/build
	cd $(FORK_DIR)/build && cmake .. -DCMAKE_INSTALL_PREFIX=/usr && make -j$$(nproc)

install-fork:
	@echo "=== Installing KDE Connect fork ==="
	sudo cmake --install $(FORK_DIR)/build

build-applet:
	@echo "=== Building applet ==="
	cargo build --release

install-applet:
	@echo "=== Installing applet ==="
	sudo install -m 755 target/release/cosmic-connect /usr/bin/cosmic-connect
	sudo install -m 644 io.github.acemythos.Connect.desktop /usr/share/applications/

restart-daemon:
	@echo "=== Restarting daemon ==="
	-sudo killall kdeconnectd 2>/dev/null || true
	@/usr/lib/x86_64-linux-gnu/libexec/kdeconnectd &

update: pull-fork pull-applet install

pull-fork:
	@if [ -d "$(FORK_DIR)" ]; then \
		cd $(FORK_DIR) && git pull; \
	fi

pull-applet:
	git pull

.PHONY: install build-fork install-fork build-applet install-applet restart-daemon update pull-fork pull-applet
