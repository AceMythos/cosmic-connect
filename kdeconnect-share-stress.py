#!/usr/bin/env python3
"""kdeconnect-fork share signal stress tester.

Monitors D-Bus signals from the patched share plugin and validates:
  - transferStarted always precedes progress/terminal for a given transferId
  - Exactly one terminal event (Finished xor Failed) per transferId
  - Progress throttling >= ~140ms between same-id progress signals
  - Monotonic bytesTransferred and consistent percent values
  - Concurrent transfer isolation (no cross-ID interference)

Usage:
    ./kdeconnect-share-stress.py <device_id> [--timeout SECONDS]
"""

import argparse
import signal as sig
import sys
import time
from collections import defaultdict

try:
    from pydbus import SessionBus
    from gi.repository import GLib
except ImportError as e:
    print(f"Missing dependency: {e}", file=sys.stderr)
    print("Install with: pip install pydbus PyGObject", file=sys.stderr)
    sys.exit(1)

BUS_NAME = "org.kde.kdeconnect"
SHARE_IFACE = "org.kde.kdeconnect.device.share"


class TransferState:
    __slots__ = ("started", "terminal", "last_progress_ts",
                 "last_bytes", "total_bytes", "file_name", "events")

    def __init__(self):
        self.started = False
        self.terminal = None
        self.last_progress_ts = 0.0
        self.last_bytes = 0
        self.total_bytes = 0
        self.file_name = ""
        self.events = []


class StressTest:
    def __init__(self, device_id, timeout=120):
        self.device_id = device_id
        self.timeout = timeout
        self.obj_path = f"/modules/kdeconnect/devices/{device_id}/share"

        self.transfers = {}
        self.violations = []
        self.signal_count = 0
        self.start_mono = time.monotonic()
        self.start_wall = time.time()

        try:
            self.bus = SessionBus()
        except Exception as e:
            print(f"Failed to connect to D-Bus session bus: {e}", file=sys.stderr)
            sys.exit(1)

        self.loop = GLib.MainLoop()

    def _subscribe(self):
        self._match = self.bus.subscribe(
            object=self.obj_path,
            iface=SHARE_IFACE,
            signal_fired=self._on_signal,
        )

    def _fmt(self, ts_mono=None):
        if ts_mono is None:
            ts_mono = time.monotonic()
        elapsed = ts_mono - self.start_mono
        return f"[{elapsed:>8.3f}]"

    def _violation(self, transfer_id, msg):
        elapsed = time.monotonic() - self.start_mono
        full = f"[{elapsed:.3f}] transferId={transfer_id}: {msg}"
        self.violations.append(full)
        print(f"  ** VIOLATION: {full}", file=sys.stderr, flush=True)

    def _on_signal(self, sender, obj, iface, signal_name, params):
        ts_mono = time.monotonic()
        ts_wall = time.time()
        self.signal_count += 1

        timestamp = self._fmt(ts_mono)
        print(f"{timestamp} {signal_name}: {params}", flush=True)

        if not params:
            self._violation("(none)", f"empty params for {signal_name}")
            return

        transfer_id = str(params[0])

        if transfer_id not in self.transfers:
            self.transfers[transfer_id] = TransferState()

        state = self.transfers[transfer_id]
        state.events.append((signal_name, params, ts_mono, ts_wall))

        if signal_name == "transferStarted":
            if state.started:
                self._violation(transfer_id, "duplicate transferStarted")
            state.started = True
            state.total_bytes = params[2]
            state.file_name = str(params[1])

        elif signal_name == "transferProgress":
            if not state.started:
                self._violation(transfer_id, "transferProgress before transferStarted")
            if state.terminal is not None:
                self._violation(transfer_id, f"transferProgress after terminal {state.terminal}")

            bytes_ = params[1]
            total = params[2]
            percent = params[3]

            # --- throttle check ---
            gap = ts_mono - state.last_progress_ts
            if state.last_progress_ts > 0 and gap < 0.135:
                self._violation(
                    transfer_id,
                    f"throttle violation: {gap*1000:.1f}ms between progress signals"
                )

            # --- monotonic bytes ---
            if bytes_ < state.last_bytes:
                self._violation(
                    transfer_id,
                    f"bytesTransferred decreased: {state.last_bytes} -> {bytes_}"
                )

            # --- percent consistency ---
            if total > 0:
                expected_pct = int(bytes_ * 100 / total)
            else:
                expected_pct = 0
            if percent != expected_pct:
                self._violation(
                    transfer_id,
                    f"percent mismatch: got {percent}, "
                    f"expected {expected_pct} (bytes={bytes_}, total={total})"
                )

            state.last_progress_ts = ts_mono
            state.last_bytes = bytes_

        elif signal_name == "transferFinished":
            if not state.started:
                self._violation(transfer_id, "transferFinished before transferStarted")
            if state.terminal is not None:
                self._violation(
                    transfer_id,
                    f"double terminal: transferFinished after {state.terminal}"
                )
            state.terminal = "transferFinished"

        elif signal_name == "transferFailed":
            if not state.started:
                self._violation(transfer_id, "transferFailed before transferStarted")
            if state.terminal is not None:
                self._violation(
                    transfer_id,
                    f"double terminal: transferFailed after {state.terminal}"
                )
            state.terminal = "transferFailed"

        else:
            self._violation(transfer_id, f"unknown signal: {signal_name}")

    def run(self):
        print(f"Bus name:     {BUS_NAME}")
        print(f"Object path:  {self.obj_path}")
        print(f"Interface:    {SHARE_IFACE}")
        print(f"Timeout:      {self.timeout}s  (Ctrl+C to stop early)")
        print()

        try:
            self._subscribe()
        except Exception as e:
            print(f"Failed to subscribe to signals: {e}", file=sys.stderr)
            sys.exit(1)

        GLib.timeout_add_seconds(self.timeout, self._on_timeout)
        sig.signal(sig.SIGINT, self._handle_sigint)

        try:
            self.loop.run()
        except KeyboardInterrupt:
            pass

        self.finish()

    def _handle_sigint(self, signum, frame):
        print("\n\nCaught SIGINT. Generating report...")
        self.loop.quit()

    def _on_timeout(self):
        print(f"\nTimeout ({self.timeout}s) reached.")
        self.loop.quit()
        return False

    def finish(self):
        elapsed = time.monotonic() - self.start_mono
        print()
        print("=" * 62)
        print("  STRESS TEST SUMMARY")
        print("=" * 62)
        print(f"  Duration:          {elapsed:.1f}s")
        print(f"  Total signals:     {self.signal_count}")
        print(f"  Unique transfers:  {len(self.transfers)}")
        print()

        # Check for transfers that started but never reached a terminal event
        # Only flag this if at least 10s has passed (grace period for slow xfers)
        if elapsed > 10:
            for tid, state in list(self.transfers.items()):
                if state.started and state.terminal is None:
                    self._violation(tid, "never reached terminal event (Finished/Failed)")

        print(f"  Per-transfer detail:")
        for tid, state in sorted(self.transfers.items()):
            status = state.terminal if state.terminal else "INCOMPLETE"
            ev_count = len(state.events)
            fname = state.file_name or "?"
            print(f"    {tid:30s}  {status:20s}  {ev_count:3d} events  [{fname}]")

        print()
        if self.violations:
            print(f"  Violations ({len(self.violations)}):")
            for v in self.violations:
                print(f"    {v}")
        else:
            print(f"  Violations: 0  -- all clean!")

        print()
        result = "PASS" if not self.violations else "FAIL"
        print(f"  Result: {result}")
        print("=" * 62)

        self.loop.quit()
        sys.exit(1 if self.violations else 0)


def main():
    parser = argparse.ArgumentParser(
        description="KDE Connect share signal stress tester"
    )
    parser.add_argument("device_id", help="Device ID (e.g., 0123456789abcdef)")
    parser.add_argument(
        "--timeout", "-t", type=int, default=120,
        help="Test duration in seconds (default: 120)"
    )
    args = parser.parse_args()

    test = StressTest(args.device_id, args.timeout)
    test.run()


if __name__ == "__main__":
    main()
