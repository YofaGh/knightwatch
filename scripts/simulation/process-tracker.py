#!/usr/bin/env python3
"""
process_test.py — test target for the ProcessTracker.

Phases (each lasts ~10 s by default, configurable via --phase-duration):
  0  Idle root only          — verify root PID tracking
  1  Spawn workers           — ChildrenAppeared events
  2  Add grandchildren       — deeper descendant tracking
  3  Kill grandchildren      — partial disappear events
  4  Kill all workers        — AllChildrenGone / work_done transitions
  5  Memory spike            — test memory snapshot values
  6  CPU spike               — test CPU snapshot values
  7  IO burst                — test IO stats (read_bytes / write_bytes)
  8  FD burst                — open many file descriptors, test FD tracking
  9  Done                    — root exits cleanly

Usage:
    python3 process_test.py
    python3 process_test.py --workers 5 --phase-duration 15
    python3 process_test.py --phase 3          # jump straight to phase 3

Give the printed PID to your tracker:
    cargo run -- --pid <PID>
"""

import argparse
import multiprocessing as mp
import os
import sys
import tempfile
import time

# ── helpers ──────────────────────────────────────────────────────────────────


def banner(phase: int, name: str):
    pid = os.getpid()
    print(f"\n{'=' * 60}", flush=True)
    print(f"  PHASE {phase}: {name}  (root pid={pid})", flush=True)
    print(f"{'=' * 60}", flush=True)


def log(msg: str):
    print(f"  [pid={os.getpid()}] {msg}", flush=True)


# ── child / grandchild workers ───────────────────────────────────────────────


def idle_worker(stop_event, label="worker"):
    """Burns almost no CPU — just proves the child exists."""
    log(f"{label} started")
    while not stop_event.is_set():
        time.sleep(0.5)
    log(f"{label} exiting")


def grandchild_worker(stop_event):
    """Spawned by a worker to create a 2-level subtree."""
    log("grandchild started")
    while not stop_event.is_set():
        time.sleep(0.5)
    log("grandchild exiting")


def worker_with_grandchildren(stop_event, n_grand=2):
    """A worker that itself spawns grandchildren."""
    log(f"worker+grandchildren started (n_grand={n_grand})")
    grand_stop = mp.Event()
    grands = [
        mp.Process(target=grandchild_worker, args=(grand_stop,), daemon=True)
        for _ in range(n_grand)
    ]
    for g in grands:
        g.start()
        log(f"  grandchild pid={g.pid}")
    while not stop_event.is_set():
        time.sleep(0.5)
    grand_stop.set()
    for g in grands:
        g.join(timeout=3)
    log("worker+grandchildren exiting")


def memory_worker(stop_event, mb=200):
    """Allocates ~mb MB and holds it until signalled."""
    log(f"memory worker allocating ~{mb} MB")
    chunk = bytearray(mb * 1024 * 1024)  # keep reference alive
    _ = chunk  # suppress unused warning
    log("memory worker holding allocation")
    while not stop_event.is_set():
        time.sleep(0.5)
    log("memory worker freeing and exiting")


def cpu_worker(stop_event):
    """Spins at 100 % on one core."""
    log("CPU worker spinning")
    while not stop_event.is_set():
        pass  # tight loop
    log("CPU worker exiting")


def io_worker(stop_event, chunk_size_mb=2, iterations=40):
    """Writes and reads back a temp file to generate IO stats."""
    log("IO worker starting")
    data = b"x" * (chunk_size_mb * 1024 * 1024)
    with tempfile.NamedTemporaryFile(delete=True) as f:
        for i in range(iterations):
            if stop_event.is_set():
                break
            f.seek(0)
            f.write(data)
            f.flush()
            f.seek(0)
            _ = f.read()
            time.sleep(0.1)
    log("IO worker exiting")


def fd_worker(stop_event, n_fds=80):
    """Opens many file descriptors and holds them open."""
    log(f"FD worker opening {n_fds} file descriptors")
    fds = []
    for _ in range(n_fds):
        fds.append(open(os.devnull, "rb"))
    log(f"FD worker holding {len(fds)} open FDs")
    while not stop_event.is_set():
        time.sleep(0.5)
    for f in fds:
        f.close()
    log("FD worker exiting")


# ── phase runner ─────────────────────────────────────────────────────────────


def run_phases(n_workers: int, phase_dur: float, start_phase: int):
    root_pid = os.getpid()
    print(f"\n{'*' * 60}", flush=True)
    print("  ProcessTracker test target", flush=True)
    print(f"  ROOT PID = {root_pid}  ← give this to your tracker", flush=True)
    print(
        f"  workers={n_workers}  phase_duration={phase_dur}s  start_phase={start_phase}",
        flush=True,
    )
    print(f"{'*' * 60}\n", flush=True)

    # Give the user a moment to attach the tracker before things get busy.
    if start_phase == 0:
        print("  Waiting 5 s before starting — attach your tracker now.", flush=True)
        time.sleep(20)

    phases = [
        (0, "Idle root only"),
        (1, "Spawn workers"),
        (2, "Add grandchildren"),
        (3, "Kill grandchildren only"),
        (4, "Kill all workers  → AllChildrenGone"),
        (5, "Memory spike"),
        (6, "CPU spike"),
        (7, "IO burst"),
        (8, "FD burst"),
        (9, "Done — root exiting"),
    ]

    workers = {}  # label -> (Process, stop_Event)
    stop_events = {}  # label -> Event

    def spawn(label, target, *extra_args):
        ev = mp.Event()
        p = mp.Process(target=target, args=(ev, *extra_args), daemon=False)
        p.start()
        workers[label] = p
        stop_events[label] = ev
        log(f"spawned {label} pid={p.pid}")
        return p

    def kill_worker(label):
        if label not in workers:
            return
        stop_events[label].set()
        workers[label].join(timeout=5)
        if workers[label].is_alive():
            workers[label].terminate()
        log(f"killed {label} (was pid={workers[label].pid})")
        del workers[label]
        del stop_events[label]

    def kill_all():
        for label in list(workers.keys()):
            kill_worker(label)

    try:
        for phase_num, phase_name in phases:
            if phase_num < start_phase:
                continue

            banner(phase_num, phase_name)

            if phase_num == 0:
                # Root sits alone — tracker should see only the root PID.
                log(f"root pid={root_pid} is alive and idle")
                time.sleep(phase_dur)

            elif phase_num == 1:
                # Spawn a flat set of idle workers.
                for i in range(n_workers):
                    spawn(f"worker-{i}", idle_worker, f"worker-{i}")
                pids = [workers[f"worker-{i}"].pid for i in range(n_workers)]
                log(f"child pids: {pids}")
                time.sleep(phase_dur)

            elif phase_num == 2:
                # Replace workers with ones that have grandchildren.
                kill_all()
                for i in range(n_workers):
                    spawn(f"worker-gc-{i}", worker_with_grandchildren, 2)
                pids = [workers[f"worker-gc-{i}"].pid for i in range(n_workers)]
                log(f"worker-with-grandchildren pids: {pids}")
                time.sleep(phase_dur)

            elif phase_num == 3:
                # Kill the grandchild-carrying workers but immediately respawn
                # plain workers so the tracker still has children.
                log("killing grandchild workers, respawning plain workers")
                kill_all()
                time.sleep(1)  # brief gap so tracker sees children disappear
                for i in range(n_workers):
                    spawn(f"worker-plain-{i}", idle_worker, f"plain-{i}")
                pids = [workers[f"worker-plain-{i}"].pid for i in range(n_workers)]
                log(f"new plain worker pids: {pids}")
                time.sleep(phase_dur)

            elif phase_num == 4:
                # Kill every child → tracker should fire AllChildrenGone / work_done.
                log("killing ALL workers — expecting AllChildrenGone event")
                kill_all()
                time.sleep(phase_dur)

            elif phase_num == 5:
                # Memory spike worker.
                p = spawn("mem-worker", memory_worker, 200)
                log(f"memory worker pid={p.pid}")
                time.sleep(phase_dur)
                kill_worker("mem-worker")

            elif phase_num == 6:
                # CPU spike — one spinner per logical core up to n_workers.
                n_cpu = min(n_workers, mp.cpu_count())
                for i in range(n_cpu):
                    p = spawn(f"cpu-{i}", cpu_worker)
                    log(f"cpu worker {i} pid={p.pid}")
                time.sleep(phase_dur)
                for i in range(n_cpu):
                    kill_worker(f"cpu-{i}")

            elif phase_num == 7:
                p = spawn("io-worker", io_worker)
                log(f"IO worker pid={p.pid}")
                time.sleep(phase_dur)
                kill_worker("io-worker")

            elif phase_num == 8:
                p = spawn("fd-worker", fd_worker, 80)
                log(f"FD worker pid={p.pid}")
                time.sleep(phase_dur)
                kill_worker("fd-worker")

            elif phase_num == 9:
                log("All phases complete. Root process exiting.")
                time.sleep(2)
                break

    except KeyboardInterrupt:
        log("Interrupted — cleaning up children")
    finally:
        kill_all()
        log("root exiting")


# ── entry point ───────────────────────────────────────────────────────────────

if __name__ == "__main__":
    # Ensure subprocesses use 'fork' on Linux (default, but be explicit).
    if sys.platform == "linux":
        mp.set_start_method("fork", force=True)

    parser = argparse.ArgumentParser(description="ProcessTracker test target")
    parser.add_argument(
        "--workers",
        type=int,
        default=3,
        help="Number of child workers to spawn (default: 3)",
    )
    parser.add_argument(
        "--phase-duration",
        type=float,
        default=10.0,
        help="Seconds to hold each phase (default: 10)",
    )
    parser.add_argument(
        "--phase", type=int, default=0, help="Start at this phase number (default: 0)"
    )
    args = parser.parse_args()

    run_phases(
        n_workers=args.workers,
        phase_dur=args.phase_duration,
        start_phase=args.phase,
    )
